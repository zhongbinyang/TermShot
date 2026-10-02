use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use crate::native::{
    dpi_scale_at, dpi_scale_hwnd, fill_rect_hdc, hinstance, key_down, place_topmost, sc,
    stroke_rect_hdc, virtual_screen,
};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CombineRgn, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreatePen, CreateRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CENTER, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID,
    RGN_OR, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetWindowLongPtrW,
    LoadCursorW, PeekMessageW, RegisterClassExW, SetCursor, SetWindowLongPtrW, TranslateMessage,
    CS_DBLCLKS, GWLP_USERDATA, IDC_HAND, MSG, PM_REMOVE, WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEMOVE,
    WM_PAINT, WM_RBUTTONDOWN, WM_SETCURSOR, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

pub const MAX_HEIGHT: i32 = 32000;

pub enum HudDecision {
    Finish,
    Cancel,
}

struct Hud {
    region: Rect,
    vs: Rect,
    scale: f32,
    height: i32,
    hole: Rect,
    bar: Rect,
    done_btn: Rect,
    cancel_btn: Rect,
    hover: i32, // -1: none, 0: done_btn, 1: cancel_btn
    decision: Option<HudDecision>,
}

pub fn run_scroll(region: Rect) -> Option<Bitmap> {
    let vs = virtual_screen();
    let region = region.intersect(vs);
    if region.w < 8 || region.h < 8 {
        return None;
    }

    let center = Point::new(region.x + region.w / 2, region.y + region.h / 2);
    let initial_scale = dpi_scale_at(center).max(1.0);

    let mut hud = Box::new(Hud {
        region,
        vs,
        scale: initial_scale,
        height: region.h,
        hole: Rect::default(),
        bar: Rect::default(),
        done_btn: Rect::default(),
        cancel_btn: Rect::default(),
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

        // Calculate layout and window region once at initialization, avoiding SetWindowRgn inside WM_PAINT
        hud.layout(hwnd);
        place_topmost(hwnd, vs, false);
        crate::native::sleep_ms(120);

        let first = crate::bitmap::capture_rect(region).ok()?;
        let mut session = Stitcher::new();
        session.begin(&first);
        hud.height = session.height();
        let _ = InvalidateRect(hwnd, None, false);

        // Drain any lingering initial keys
        while key_down(VK_RETURN.0 as i32) || key_down(VK_ESCAPE.0 as i32) || key_down(0x52) {
            crate::native::sleep_ms(20);
            pump();
        }

        let mut last_capture_time = std::time::Instant::now();

        loop {
            pump();
            if hwnd.is_invalid() {
                break;
            }

            // Handle HUD decisions from user clicks
            match hud.decision.take() {
                Some(HudDecision::Cancel) => {
                    let _ = DestroyWindow(hwnd);
                    return None;
                }
                Some(HudDecision::Finish) => break,
                None => {}
            }

            // Keyboard shortcuts
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

            // Capture and incrementally stitch frame
            if last_capture_time.elapsed() >= std::time::Duration::from_millis(50) {
                last_capture_time = std::time::Instant::now();
                if let Ok(frame) = crate::bitmap::capture_rect(region) {
                    if session.append(&frame) {
                        hud.height = session.height();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
            }

            crate::native::sleep_ms(15);
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
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let scale = hud.scale.max(1.0);
            let border_w = sc(2, scale).max(1);

            // Selection border and mask
            fill_rect_hdc(hdc, hud.hole.inflate(border_w + 1, border_w + 1), Color::argb(220, 8, 10, 14));
            stroke_rect_hdc(hdc, hud.hole, theme::ACCENT, border_w);

            if hud.bar.w > 4 && hud.bar.h > 4 {
                let mem_dc = CreateCompatibleDC(hdc);
                let mem_bmp = CreateCompatibleBitmap(hdc, hud.bar.w, hud.bar.h);
                let old_bmp = SelectObject(mem_dc, mem_bmp);

                let card_round = sc(8, scale);
                let btn_round = sc(6, scale);

                // 1. Draw HUD Container Card (Modern rounded card with border)
                let bar_brush = CreateSolidBrush(COLORREF(Color::rgb(20, 26, 36).colorref()));
                let bar_pen = CreatePen(PS_SOLID, 1, COLORREF(Color::rgb(46, 58, 76).colorref()));
                let old_brush = SelectObject(mem_dc, bar_brush);
                let old_pen = SelectObject(mem_dc, bar_pen);
                let _ = RoundRect(mem_dc, 0, 0, hud.bar.w, hud.bar.h, card_round, card_round);

                // 2. Status text (Vertically centered, YaHei UI, ClearType)
                let font_title = CreateFontW(
                    -sc(13, scale),
                    0, 0, 0,
                    FW_NORMAL.0 as i32,
                    0, 0, 0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    CLEARTYPE_QUALITY.0 as u32,
                    0,
                    windows::core::w!("Microsoft YaHei UI"),
                );
                let font_btn = CreateFontW(
                    -sc(12, scale),
                    0, 0, 0,
                    FW_BOLD.0 as i32,
                    0, 0, 0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    CLEARTYPE_QUALITY.0 as u32,
                    0,
                    windows::core::w!("Microsoft YaHei UI"),
                );

                SetBkMode(mem_dc, TRANSPARENT);
                SetTextColor(mem_dc, COLORREF(theme::TEXT.colorref()));
                let old_font = SelectObject(mem_dc, font_title);

                let status_msg = if hud.height >= MAX_HEIGHT {
                    format!("已达到高度上限  ·  {} px  ·  请完成", hud.height)
                } else {
                    format!("已捕获 {} px  ·  滚轮继续  ·  Enter 完成", hud.height)
                };
                let mut wt_status = wide(&status_msg);
                let text_max_right = (hud.done_btn.x - hud.bar.x - sc(12, scale)).max(sc(100, scale));
                let mut status_rc = RECT {
                    left: sc(16, scale),
                    top: 0,
                    right: text_max_right,
                    bottom: hud.bar.h,
                };
                DrawTextW(
                    mem_dc,
                    &mut wt_status,
                    &mut status_rc,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // 3. [ ✓ 完成 ] Button (CTA Accent)
                SelectObject(mem_dc, font_btn);
                let (done_bg, done_pen_col, done_fg) = if hud.hover == 0 {
                    (theme::ACCENT_HI, theme::ACCENT_HI, Color::rgb(0x06, 0x22, 0x1B))
                } else {
                    (theme::ACCENT, theme::ACCENT, Color::rgb(0x06, 0x22, 0x1B))
                };
                let done_brush = CreateSolidBrush(COLORREF(done_bg.colorref()));
                let done_pen = CreatePen(PS_SOLID, 1, COLORREF(done_pen_col.colorref()));
                SelectObject(mem_dc, done_brush);
                SelectObject(mem_dc, done_pen);

                let done_rx = hud.done_btn.x - hud.bar.x;
                let done_ry = hud.done_btn.y - hud.bar.y;
                let mut done_rc = RECT {
                    left: done_rx,
                    top: done_ry,
                    right: done_rx + hud.done_btn.w,
                    bottom: done_ry + hud.done_btn.h,
                };
                let _ = RoundRect(
                    mem_dc,
                    done_rc.left,
                    done_rc.top,
                    done_rc.right,
                    done_rc.bottom,
                    btn_round,
                    btn_round,
                );
                SetTextColor(mem_dc, COLORREF(done_fg.colorref()));
                let mut wt_done = wide("✓ 完成");
                DrawTextW(
                    mem_dc,
                    &mut wt_done,
                    &mut done_rc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // 4. [ ✕ 取消 ] Button
                let (cancel_bg, cancel_pen_col, cancel_fg) = if hud.hover == 1 {
                    (theme::HOVER_BG, Color::rgb(0x42, 0x54, 0x6E), theme::TEXT)
                } else {
                    (theme::PANEL, Color::rgb(0x35, 0x43, 0x58), theme::TEXT)
                };
                let cancel_brush = CreateSolidBrush(COLORREF(cancel_bg.colorref()));
                let cancel_pen = CreatePen(PS_SOLID, 1, COLORREF(cancel_pen_col.colorref()));
                SelectObject(mem_dc, cancel_brush);
                SelectObject(mem_dc, cancel_pen);

                let cancel_rx = hud.cancel_btn.x - hud.bar.x;
                let cancel_ry = hud.cancel_btn.y - hud.bar.y;
                let mut cancel_rc = RECT {
                    left: cancel_rx,
                    top: cancel_ry,
                    right: cancel_rx + hud.cancel_btn.w,
                    bottom: cancel_ry + hud.cancel_btn.h,
                };
                let _ = RoundRect(
                    mem_dc,
                    cancel_rc.left,
                    cancel_rc.top,
                    cancel_rc.right,
                    cancel_rc.bottom,
                    btn_round,
                    btn_round,
                );
                SetTextColor(mem_dc, COLORREF(cancel_fg.colorref()));
                let mut wt_cancel = wide("✕ 取消");
                DrawTextW(
                    mem_dc,
                    &mut wt_cancel,
                    &mut cancel_rc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // 5. Blit to Screen DC
                let _ = BitBlt(
                    hdc,
                    hud.bar.x,
                    hud.bar.y,
                    hud.bar.w,
                    hud.bar.h,
                    mem_dc,
                    0,
                    0,
                    SRCCOPY,
                );

                // Cleanup GDI objects
                SelectObject(mem_dc, old_brush);
                SelectObject(mem_dc, old_pen);
                SelectObject(mem_dc, old_font);
                SelectObject(mem_dc, old_bmp);
                let _ = DeleteObject(bar_brush);
                let _ = DeleteObject(bar_pen);
                let _ = DeleteObject(done_brush);
                let _ = DeleteObject(done_pen);
                let _ = DeleteObject(cancel_brush);
                let _ = DeleteObject(cancel_pen);
                let _ = DeleteObject(font_title);
                let _ = DeleteObject(font_btn);
                let _ = DeleteObject(mem_bmp);
                let _ = DeleteDC(mem_dc);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = lp(lparam);
            let h = if hud.done_btn.contains(p) {
                0
            } else if hud.cancel_btn.contains(p) {
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
            if hud.done_btn.contains(p) {
                hud.decision = Some(HudDecision::Finish);
            } else if hud.cancel_btn.contains(p) {
                hud.decision = Some(HudDecision::Cancel);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            hud.decision = Some(HudDecision::Cancel);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cur = crate::native::cursor_pos();
            let mut pt = windows::Win32::Foundation::POINT { x: cur.x, y: cur.y };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
            let p = Point::new(pt.x, pt.y);
            if hud.done_btn.contains(p) || hud.cancel_btn.contains(p) {
                let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                SetCursor(cursor);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
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

        let center = Point::new(self.region.x + self.region.w / 2, self.region.y + self.region.h / 2);
        let scale = crate::native::dpi_scale_at(center)
            .max(dpi_scale_hwnd(hwnd))
            .max(1.0);
        self.scale = scale;

        let bar_h = sc(42, scale);
        let pad = sc(10, scale);
        let btn_w = sc(84, scale);
        let bar_w = sc(420, scale);
        let mut x = self.hole.x + (self.hole.w - bar_w) / 2;
        let mut y = self.hole.bottom() + pad;
        if y + bar_h > ch - 4 {
            y = self.hole.y - bar_h - pad;
        }
        x = x.clamp(4, (cw - bar_w - 4).max(4));
        y = y.clamp(4, (ch - bar_h - 4).max(4));
        self.bar = Rect::new(x, y, bar_w, bar_h);

        let by = self.bar.y + sc(6, scale);
        let bh = bar_h - sc(12, scale);
        self.cancel_btn = Rect::new(self.bar.right() - pad - btn_w, by, btn_w, bh);
        self.done_btn = Rect::new(self.cancel_btn.x - sc(8, scale) - btn_w, by, btn_w, bh);

        let border_w = sc(3, scale).max(3);
        unsafe {
            let outer = CreateRectRgn(
                self.hole.x - border_w,
                self.hole.y - border_w,
                self.hole.right() + border_w,
                self.hole.bottom() + border_w,
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

pub(crate) struct Pix {
    buf: Vec<u8>,
    stride: i32,
    w: i32,
    h: i32,
}

pub struct Stitcher {
    canvas: Option<Bitmap>,
    last: Option<Bitmap>,
    footer: i32,
    stagnant_count: usize,
}

impl Default for Stitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Stitcher {
    pub fn new() -> Self {
        Self {
            canvas: None,
            last: None,
            footer: 0,
            stagnant_count: 0,
        }
    }

    pub fn height(&self) -> i32 {
        self.canvas.as_ref().map(|c| c.height).unwrap_or(0)
    }

    pub fn begin(&mut self, first: &Bitmap) {
        self.canvas = Some(first.clone());
        self.last = Some(first.clone());
        self.footer = 0;
        self.stagnant_count = 0;
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

        // If frame is identical to last captured, nothing has scrolled yet
        if almost_equal(&incoming, &committed) {
            return false;
        }

        let skip_top = detect_sticky(&committed, &incoming, true);
        let skip_bot = detect_sticky(&committed, &incoming, false);
        let dy = find_delta(&committed, &incoming, skip_top, skip_bot);

        if dy <= 0 {
            self.stagnant_count += 1;
            // If stagnant for multiple captures, user may have scrolled past max_dy.
            // Reset reference to incoming frame so stitching can recover smoothly.
            if self.stagnant_count > 8 {
                self.last = Some(frame.clone());
                self.stagnant_count = 0;
            }
            return false;
        }

        self.stagnant_count = 0;

        if self.footer == 0 && skip_bot > 0 && skip_bot < frame.height / 4 {
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
    let step_y = (a.h / 50).max(2);
    let step_x = (a.w / 50).max(2);
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
    n > 0 && (sum / n) < 6
}

fn detect_sticky(a: &Pix, b: &Pix, from_top: bool) -> i32 {
    let max = (a.h / 5).max(4);
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
    if run >= 6 {
        run
    } else {
        0
    }
}

fn row_close(a: &Pix, ay: i32, b: &Pix, by: i32) -> bool {
    let sample_w = if a.w > 100 { a.w - 18 } else { a.w };
    let step = (sample_w / 60).max(1);
    let cols = (sample_w + step - 1) / step;
    diff(a, ay, b, by, 1, sample_w, step) < cols as i64 * 14
}

pub fn find_delta(a: &Pix, b: &Pix, skip_top: i32, skip_bot: i32) -> i32 {
    let h = a.h;
    let top = skip_top.clamp(0, h / 3);
    let bot = (h - skip_bot).clamp(top + 24, h);
    let content_h = bot - top;
    if content_h < 24 {
        return 0;
    }

    // Require at least 25% overlap
    let min_overlap = (content_h / 4).max(16);
    let max_dy = content_h - min_overlap;
    if max_dy < 1 {
        return 0;
    }

    // Exclude scrollbar noise on the right edge
    let right_margin = if a.w > 100 { 18 } else { 0 };
    let sample_w = a.w - right_margin;
    let step_x = (sample_w / 60).max(1);
    let cols = (sample_w + step_x - 1) / step_x;

    let mut best_norm = i64::MAX;
    let mut best_dy = 0;

    for dy in 1..=max_dy {
        let rows = content_h - dy;
        let d = diff(a, top + dy, b, top, rows, sample_w, step_x);
        let norm = d / rows as i64;
        if norm < best_norm {
            best_norm = norm;
            best_dy = dy;
        }
    }

    // Average channel difference threshold per sampled pixel
    let thresh = cols as i64 * 45;
    if best_dy <= 0 || best_norm > thresh {
        return 0;
    }

    best_dy
}

fn diff(a: &Pix, ay: i32, b: &Pix, by: i32, rows: i32, sample_w: i32, step_x: i32) -> i64 {
    let mut sum = 0i64;
    let step_y = if rows > 100 { 2 } else { 1 };
    let mut row = 0;
    while row < rows {
        let oa = (ay + row) * a.stride;
        let ob = (by + row) * b.stride;
        let mut x = 0;
        while x < sample_w {
            let ia = (oa + x * 4) as usize;
            let ib = (ob + x * 4) as usize;
            sum += (a.buf[ia] as i64 - b.buf[ib] as i64).abs()
                + (a.buf[ia + 1] as i64 - b.buf[ib + 1] as i64).abs()
                + (a.buf[ia + 2] as i64 - b.buf[ib + 2] as i64).abs();
            x += step_x;
        }
        row += step_y;
    }
    if step_y > 1 {
        sum = sum * rows as i64 / ((rows + step_y - 1) / step_y) as i64;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Color;

    fn create_test_pattern(w: i32, h: i32) -> Bitmap {
        let mut bmp = Bitmap::new(w, h);
        // Fill white
        for y in 0..h {
            for x in 0..w {
                bmp.set(x, y, Color::rgb(255, 255, 255));
            }
        }
        // Draw distinct non-periodic lines with varied positions and colors
        let line_y = [25, 65, 115, 180, 260, 350, 450, 560];
        for (idx, &y) in line_y.iter().enumerate() {
            if y + 2 < h {
                for x in 10..w - 10 {
                    let r = ((idx * 37) % 256) as u8;
                    let g = ((idx * 73) % 256) as u8;
                    bmp.set(x, y, Color::rgb(r, g, 40));
                    bmp.set(x, y + 1, Color::rgb(r, g, 100));
                }
            }
        }
        bmp
    }

    #[test]
    fn test_find_delta_exact_shift() {
        let w = 200;
        let h = 300;
        let full = create_test_pattern(w, 500);

        // Frame A is rows 0..300
        let mut frame_a = Bitmap::new(w, h);
        copy_rows(&mut frame_a, 0, &full, 0, h);

        // Frame B is rows 45..345 (shifted down by 45 pixels)
        let mut frame_b = Bitmap::new(w, h);
        copy_rows(&mut frame_b, 0, &full, 45, h);

        let pix_a = copy_pix(&frame_a);
        let pix_b = copy_pix(&frame_b);

        let dy = find_delta(&pix_a, &pix_b, 0, 0);
        assert_eq!(dy, 45, "Should accurately detect 45px scroll shift");
    }

    #[test]
    fn test_whitespace_does_not_collapse_to_1px() {
        let w = 200;
        let h = 300;
        let mut full = Bitmap::new(w, 500);
        // Large whitespace with only two lines
        for y in 0..500 {
            for x in 0..w {
                full.set(x, y, Color::rgb(255, 255, 255));
            }
        }
        for x in 20..180 {
            full.set(x, 50, Color::rgb(0, 0, 0));
            full.set(x, 200, Color::rgb(0, 0, 0));
            full.set(x, 350, Color::rgb(0, 0, 0));
        }

        let mut frame_a = Bitmap::new(w, h);
        copy_rows(&mut frame_a, 0, &full, 0, h);

        let mut frame_b = Bitmap::new(w, h);
        copy_rows(&mut frame_b, 0, &full, 60, h);

        let pix_a = copy_pix(&frame_a);
        let pix_b = copy_pix(&frame_b);

        let dy = find_delta(&pix_a, &pix_b, 0, 0);
        assert_eq!(dy, 60, "Should detect 60px shift instead of collapsing to 1px on whitespace");
    }

    #[test]
    fn test_stitcher_continuous_growth() {
        let w = 160;
        let h = 200;
        let full = create_test_pattern(w, 600);

        let mut stitcher = Stitcher::new();

        let mut f0 = Bitmap::new(w, h);
        copy_rows(&mut f0, 0, &full, 0, h);
        stitcher.begin(&f0);
        assert_eq!(stitcher.height(), 200);

        // Scroll step 1: +35px
        let mut f1 = Bitmap::new(w, h);
        copy_rows(&mut f1, 0, &full, 35, h);
        let stitched1 = stitcher.append(&f1);
        assert!(stitched1, "Step 1 should stitch");
        assert_eq!(stitcher.height(), 235);

        // Scroll step 2: +50px
        let mut f2 = Bitmap::new(w, h);
        copy_rows(&mut f2, 0, &full, 85, h);
        let stitched2 = stitcher.append(&f2);
        assert!(stitched2, "Step 2 should stitch");
        assert_eq!(stitcher.height(), 285);

        // Take result
        let result = stitcher.take();
        assert_eq!(result.width, w);
        assert_eq!(result.height, 285);
    }
}
