use crate::bitmap::Bitmap;
use crate::draw::fill_rect;
use crate::geom::{Color, Point, Rect};
use crate::native::{dpi_scale_hwnd, fill_rect_hdc, hinstance, key_down, place_topmost, sc, stroke_rect_hdc, virtual_screen};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, DeleteObject, EndPaint, InvalidateRect, SetWindowRgn,
    PAINTSTRUCT, RGN_OR,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetWindowLongPtrW,
    PeekMessageW, RegisterClassExW, SetWindowLongPtrW, TranslateMessage, CS_DBLCLKS, GWLP_USERDATA,
    MSG, PM_REMOVE, WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN, WNDCLASSEXW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

pub const MAX_HEIGHT: i32 = 32000;

pub enum HudDecision {
    Finish,
    Cancel,
}

struct Hud {
    region: Rect,
    vs: Rect,
    height: i32,
    hole: Rect,
    bar: Rect,
    done: Rect,
    cancel: Rect,
    hover: i32,
    decision: Option<HudDecision>,
}

pub fn run_scroll(region: Rect) -> Option<Bitmap> {
    let vs = virtual_screen();
    let region = region.intersect(vs);
    if region.w < 8 || region.h < 8 {
        return None;
    }

    let mut hud = Box::new(Hud {
        region,
        vs,
        height: region.h,
        hole: Rect::default(),
        bar: Rect::default(),
        done: Rect::default(),
        cancel: Rect::default(),
        hover: -1,
        decision: None,
    });

    unsafe {
        let class = wide("TermShotScrollHud");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(hud_proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!(""),
            WS_POPUP | WS_VISIBLE,
            vs.x,
            vs.y,
            vs.w,
            vs.h,
            None,
            None,
            hinstance(),
            Some(hud.as_mut() as *mut Hud as *mut _),
        )
        .unwrap_or_default();
        place_topmost(hwnd, vs, false);
        crate::native::sleep_ms(160);

        let first = crate::bitmap::capture_rect(region).ok()?;
        let mut session = Stitcher::new();
        session.begin(&first);
        hud.height = session.height();
        let _ = InvalidateRect(hwnd, None, false);

        while key_down(VK_RETURN.0 as i32) || key_down(VK_ESCAPE.0 as i32) || key_down(0x52) {
            crate::native::sleep_ms(20);
            pump();
        }

        loop {
            pump();
            if hwnd.is_invalid() {
                break;
            }
            match &hud.decision {
                Some(HudDecision::Cancel) => {
                    let _ = DestroyWindow(hwnd);
                    return None;
                }
                Some(HudDecision::Finish) => break,
                None => {}
            }
            if key_down(VK_ESCAPE.0 as i32) {
                let _ = DestroyWindow(hwnd);
                return None;
            }
            if key_down(VK_RETURN.0 as i32) {
                break;
            }
            if session.height() >= MAX_HEIGHT {
                break;
            }
            if let Ok(frame) = crate::bitmap::capture_rect(region) {
                if session.append(&frame) {
                    hud.height = session.height();
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            crate::native::sleep_ms(80);
        }
        let _ = DestroyWindow(hwnd);
        Some(session.take())
    }
}

fn pump() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn hud_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Hud;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let hud = &mut *ptr;
    match msg {
        WM_PAINT => {
            hud.layout(hwnd);
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let scale = dpi_scale_hwnd(hwnd).max(1.0);
            fill_rect_hdc(hdc, hud.hole.inflate(4, 4), Color::argb(220, 8, 10, 14));
            stroke_rect_hdc(hdc, hud.hole, theme::ACCENT, sc(2, scale).max(1));
            if hud.bar.w > 4 && hud.bar.h > 4 {
                let mut frame = Bitmap::new(hud.bar.w, hud.bar.h);
                fill_rect(
                    &mut frame,
                    0,
                    0,
                    hud.bar.w,
                    hud.bar.h,
                    Color::argb(242, 18, 22, 30),
                );
                crate::draw::draw_text(
                    &mut frame,
                    Point::new(sc(10, scale), (hud.bar.h - sc(16, scale)) / 2),
                    &format!("用滚轮向下滚  高度 {} px  ·  Enter 完成  ·  Esc 取消", hud.height),
                    theme::TEXT,
                    sc(13, scale),
                );
                let done_c = if hud.hover == 0 {
                    theme::ACCENT_HI
                } else {
                    theme::TEXT
                };
                crate::draw::draw_text(
                    &mut frame,
                    Point::new(hud.done.x - hud.bar.x + sc(12, scale), hud.done.y - hud.bar.y + sc(6, scale)),
                    "完成",
                    done_c,
                    sc(13, scale),
                );
                crate::draw::draw_text(
                    &mut frame,
                    Point::new(hud.cancel.x - hud.bar.x + sc(12, scale), hud.cancel.y - hud.bar.y + sc(6, scale)),
                    "取消",
                    theme::TEXT,
                    sc(13, scale),
                );
                frame.blit_to_hdc(hdc, hud.bar);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = lp(lparam);
            let h = if hud.done.contains(p) {
                0
            } else if hud.cancel.contains(p) {
                1
            } else {
                -1
            };
            if h != hud.hover {
                hud.hover = h;
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let p = lp(lparam);
            if hud.done.contains(p) {
                hud.decision = Some(HudDecision::Finish);
            } else if hud.cancel.contains(p) {
                hud.decision = Some(HudDecision::Cancel);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            hud.decision = Some(HudDecision::Cancel);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn lp(lparam: LPARAM) -> Point {
    let v = lparam.0 as u32;
    Point::new((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}

impl Hud {
    fn layout(&mut self, hwnd: HWND) {
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut rc);
        }
        let cw = (rc.right - rc.left).max(1);
        let ch = (rc.bottom - rc.top).max(1);
        let vw = self.vs.w.max(1);
        let vh = self.vs.h.max(1);
        let x1 = (self.region.x - self.vs.x) * cw / vw;
        let y1 = (self.region.y - self.vs.y) * ch / vh;
        let x2 = (self.region.right() - self.vs.x) * cw / vw;
        let y2 = (self.region.bottom() - self.vs.y) * ch / vh;
        self.hole = Rect::from_ltrb(x1, y1, x2.max(x1 + 1), y2.max(y1 + 1));
        let scale = dpi_scale_hwnd(hwnd).max(1.0);
        let bar_h = sc(36, scale);
        let pad = sc(8, scale);
        let btn_w = sc(68, scale);
        let bar_w = sc(420, scale);
        let mut x = self.hole.x + (self.hole.w - bar_w) / 2;
        let mut y = self.hole.bottom() + pad;
        if y + bar_h > ch - 4 {
            y = self.hole.y - bar_h - pad;
        }
        x = x.clamp(4, (cw - bar_w - 4).max(4));
        y = y.clamp(4, (ch - bar_h - 4).max(4));
        self.bar = Rect::new(x, y, bar_w, bar_h);
        self.done = Rect::new(self.bar.right() - pad - btn_w, self.bar.y + 4, btn_w, bar_h - 8);
        self.cancel = Rect::new(self.done.x - 6 - btn_w, self.done.y, btn_w, self.done.h);
        unsafe {
            let outer = CreateRectRgn(
                self.hole.x - 4,
                self.hole.y - 4,
                self.hole.right() + 4,
                self.hole.bottom() + 4,
            );
            let inner = CreateRectRgn(self.hole.x, self.hole.y, self.hole.right(), self.hole.bottom());
            let bar = CreateRectRgn(self.bar.x, self.bar.y, self.bar.right(), self.bar.bottom());
            let _ = CombineRgn(outer, outer, inner, windows::Win32::Graphics::Gdi::RGN_DIFF);
            let _ = CombineRgn(outer, outer, bar, RGN_OR);
            SetWindowRgn(hwnd, outer, true);
            let _ = DeleteObject(inner);
            let _ = DeleteObject(bar);
        }
    }
}

struct Pix {
    buf: Vec<u8>,
    stride: i32,
    w: i32,
    h: i32,
}

pub struct Stitcher {
    canvas: Option<Bitmap>,
    last: Option<Bitmap>,
    pending: Option<Bitmap>,
    footer: i32,
}

impl Stitcher {
    pub fn new() -> Self {
        Self {
            canvas: None,
            last: None,
            pending: None,
            footer: 0,
        }
    }

    pub fn height(&self) -> i32 {
        self.canvas.as_ref().map(|c| c.height).unwrap_or(0)
    }

    pub fn begin(&mut self, first: &Bitmap) {
        self.canvas = Some(first.clone());
        self.last = Some(first.clone());
        self.pending = None;
        self.footer = 0;
    }

    pub fn append(&mut self, frame: &Bitmap) -> bool {
        let Some(canvas) = self.canvas.as_ref() else {
            return false;
        };
        let Some(last) = self.last.as_ref() else {
            return false;
        };
        if frame.width != last.width || frame.height != last.height {
            return false;
        }
        if canvas.height >= MAX_HEIGHT {
            return false;
        }
        let incoming = copy_pix(frame);
        let committed = copy_pix(last);
        if almost_equal(&incoming, &committed) {
            return false;
        }
        if self.pending.is_none() {
            self.pending = Some(frame.clone());
            return false;
        }
        let pending = copy_pix(self.pending.as_ref().unwrap());
        if !almost_equal(&incoming, &pending) {
            self.pending = Some(frame.clone());
            return false;
        }
        self.pending = None;
        let skip_top = detect_sticky(&committed, &incoming, true);
        let skip_bot = detect_sticky(&committed, &incoming, false);
        let dy = find_delta(&committed, &incoming, skip_top, skip_bot);
        if dy <= 0 {
            return false;
        }
        if self.footer == 0 {
            self.footer = skip_bot;
        }
        let mut add = dy.min(MAX_HEIGHT - canvas.height);
        if add <= 0 {
            return false;
        }
        let mut src_y = frame.height - self.footer - add;
        if src_y < skip_top {
            src_y = skip_top;
        }
        add = add.min(frame.height - self.footer - src_y);
        if add <= 0 {
            return false;
        }
        let body_h = (canvas.height - self.footer).max(0);
        let mut grown = Bitmap::new(canvas.width, body_h + add + self.footer);
        copy_rows(&mut grown, 0, canvas, 0, body_h);
        copy_rows(&mut grown, body_h, frame, src_y, add);
        if self.footer > 0 {
            copy_rows(
                &mut grown,
                body_h + add,
                frame,
                frame.height - self.footer,
                self.footer,
            );
        }
        self.canvas = Some(grown);
        self.last = Some(frame.clone());
        true
    }

    pub fn take(self) -> Bitmap {
        self.canvas.unwrap_or_else(|| Bitmap::new(1, 1))
    }
}

fn copy_rows(dest: &mut Bitmap, dest_y: i32, src: &Bitmap, src_y: i32, rows: i32) {
    for y in 0..rows {
        for x in 0..src.width {
            dest.set(x, dest_y + y, src.get(x, src_y + y));
        }
    }
}

fn copy_pix(bmp: &Bitmap) -> Pix {
    let stride = bmp.width * 4;
    let mut buf = vec![0u8; (stride * bmp.height) as usize];
    for y in 0..bmp.height {
        for x in 0..bmp.width {
            let c = bmp.get(x, y);
            let i = (y * stride + x * 4) as usize;
            buf[i] = c.b;
            buf[i + 1] = c.g;
            buf[i + 2] = c.r;
            buf[i + 3] = c.a;
        }
    }
    Pix {
        buf,
        stride,
        w: bmp.width,
        h: bmp.height,
    }
}

fn almost_equal(a: &Pix, b: &Pix) -> bool {
    if a.w != b.w || a.h != b.h {
        return false;
    }
    let step_y = (a.h / 80).max(1);
    let step_x = (a.w / 80).max(1);
    let mut sum = 0i64;
    let mut n = 0i64;
    let mut y = 0;
    while y < a.h {
        let mut x = 0;
        while x < a.w {
            let ia = (y * a.stride + x * 4) as usize;
            let ib = (y * b.stride + x * 4) as usize;
            sum += (a.buf[ia] as i64 - b.buf[ib] as i64).abs()
                + (a.buf[ia + 1] as i64 - b.buf[ib + 1] as i64).abs()
                + (a.buf[ia + 2] as i64 - b.buf[ib + 2] as i64).abs();
            n += 1;
            x += step_x;
        }
        y += step_y;
    }
    n > 0 && sum / n < 8
}

fn detect_sticky(a: &Pix, b: &Pix, from_top: bool) -> i32 {
    let max = (a.h / 4).max(4);
    let mut run = 0;
    if from_top {
        for y in 0..max {
            if !row_close(a, y, b, y) {
                break;
            }
            run += 1;
        }
    } else {
        for y in (a.h - max..a.h).rev() {
            if !row_close(a, y, b, y) {
                break;
            }
            run += 1;
        }
    }
    if run >= 4 {
        run
    } else {
        0
    }
}

fn row_close(a: &Pix, ay: i32, b: &Pix, by: i32) -> bool {
    let step = (a.w / 80).max(1);
    let cols = (a.w + step - 1) / step;
    diff(a, ay, b, by, 1) < cols as i64 * 12
}

fn find_delta(a: &Pix, b: &Pix, skip_top: i32, skip_bot: i32) -> i32 {
    let h = a.h;
    let top = skip_top;
    let bot = h - skip_bot;
    let content_h = bot - top;
    if content_h < 24 {
        return 0;
    }
    let min_overlap = (content_h / 4).max(16);
    let max_dy = content_h - min_overlap;
    if max_dy < 1 {
        return 0;
    }
    let step = (a.w / 80).max(1);
    let cols = (a.w + step - 1) / step;
    let thresh = cols as i64 * 20;
    let mut best_norm = i64::MAX;
    let mut best_dy = 0;
    for dy in 1..=max_dy {
        let rows = content_h - dy;
        let norm = diff(a, top + dy, b, top, rows) / rows as i64;
        if norm < best_norm {
            best_norm = norm;
            best_dy = dy;
        }
    }
    if best_dy <= 0 || best_norm > thresh {
        return 0;
    }
    let allow = best_norm * 115 / 100 + 3;
    for dy in 1..best_dy {
        let rows = content_h - dy;
        let norm = diff(a, top + dy, b, top, rows) / rows as i64;
        if norm <= allow {
            return dy;
        }
    }
    best_dy
}

fn diff(a: &Pix, ay: i32, b: &Pix, by: i32, rows: i32) -> i64 {
    let step = (a.w / 80).max(1);
    let mut sum = 0i64;
    for row in 0..rows {
        let oa = (ay + row) * a.stride;
        let ob = (by + row) * b.stride;
        let mut x = 0;
        while x < a.w {
            let ia = (oa + x * 4) as usize;
            let ib = (ob + x * 4) as usize;
            sum += (a.buf[ia] as i64 - b.buf[ib] as i64).abs()
                + (a.buf[ia + 1] as i64 - b.buf[ib + 1] as i64).abs()
                + (a.buf[ia + 2] as i64 - b.buf[ib + 2] as i64).abs();
            x += step;
        }
    }
    sum
}
