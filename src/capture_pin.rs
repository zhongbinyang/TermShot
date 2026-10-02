use crate::annotation::{AnnotKind, AnnotationSession};
use crate::bitmap::Bitmap;
use crate::geom::{Point, Rect};
use crate::native::{
    capture, cursor_arrow, cursor_cross, cursor_hand, dpi_scale_at, dpi_scale_hwnd, fill_rect_hdc,
    hinstance, place_topmost, release_capture, sc, shift_down, work_area_from_point,
};
use crate::settings::PostCaptureAction;
use crate::theme;
use crate::toolbar::{ActionToolbar, ToolbarResult};
use crate::util::wide;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, InvalidateRect, SelectObject, SetBkColor, SetTextColor, SetWindowOrgEx,
    HBRUSH, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_1, VK_2, VK_3, VK_4, VK_A, VK_B, VK_C, VK_E, VK_ESCAPE, VK_H, VK_L, VK_M, VK_O, VK_P, VK_S,
    VK_T, VK_TAB, VK_X, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, RegisterClassExW, SetWindowLongPtrW, CS_DBLCLKS, CS_DROPSHADOW, GWLP_USERDATA,
    MSG, WM_CTLCOLOREDIT, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
    WM_RBUTTONDOWN, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

pub fn run(bmp: Bitmap, screen_rect: Rect) -> Option<(PostCaptureAction, Bitmap, AnnotationSession)> {
    let mut state = Box::new(PinAsk {
        bmp,
        screen_rect,
        hwnd: HWND::default(),
        toolbar: ActionToolbar::default(),
        ann: AnnotationSession::default(),
        scale: 1.0,
        image_client: Rect::default(),
        done: None,
        edit: HWND::default(),
        text_at: Point::default(),
        capturing: false,
        lit: None,
        prev_tip_rect: None,
        edit_brush: unsafe { CreateSolidBrush(COLORREF(theme::INPUT_BG.colorref())) },
    });
    state.ann.attach(&state.bmp);
    state.ann.select(AnnotKind::Arrow);
    state.scale = dpi_scale_at(Point::new(screen_rect.x, screen_rect.y)).max(1.0);
    state.ann.width_index = 1;

    unsafe {
        let class = wide("TermShotCapturePin");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS | CS_DROPSHADOW,
            lpfnWndProc: Some(wndproc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let work = work_area_from_point(Point::new(screen_rect.x, screen_rect.y));
        let layout = layout(&state.bmp, screen_rect, work, state.scale);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShot"),
            WS_POPUP | WS_VISIBLE,
            layout.0.x,
            layout.0.y,
            layout.0.w,
            layout.0.h,
            None,
            None,
            hinstance(),
            Some(state.as_mut() as *mut PinAsk as *mut _),
        )
        .unwrap_or_default();
        state.hwnd = hwnd;
        state.image_client = layout.1;
        place_topmost(hwnd, layout.0, true);

        let mut msg = MSG::default();
        while state.done.is_none() {
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                break;
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !hwnd.is_invalid() {
            let _ = DestroyWindow(hwnd);
        }
    }

    unsafe {
        if !state.edit_brush.is_invalid() {
            let _ = DeleteObject(state.edit_brush);
            state.edit_brush = HBRUSH::default();
        }
    }
    match state.done {
        Some(Some(action)) => {
            let mut bmp = state.bmp;
            let source = bmp.clone();
            state.ann.stamp(&mut bmp, Point::new(0, 0), Some(&source));
            Some((action, bmp, state.ann))
        }
        _ => None,
    }
}

struct PinAsk {
    bmp: Bitmap,
    #[allow(dead_code)]
    screen_rect: Rect,
    hwnd: HWND,
    toolbar: ActionToolbar,
    ann: AnnotationSession,
    scale: f32,
    image_client: Rect,
    done: Option<Option<PostCaptureAction>>,
    edit: HWND,
    text_at: Point,
    capturing: bool,
    lit: Option<Bitmap>,
    prev_tip_rect: Option<Rect>,
    edit_brush: HBRUSH,
}

fn layout(bmp: &Bitmap, screen: Rect, work: Rect, scale: f32) -> (Rect, Rect) {
    let tool_h = (96.0 * scale).round() as i32;
    let gap = (8.0 * scale).round() as i32;
    let margin = 8;
    let max_w = (work.w - margin * 2).max(120);
    let max_h = (work.h - margin * 2 - tool_h - gap).max(80);
    let fit = (1.0f32)
        .min(max_w as f32 / bmp.width as f32)
        .min(max_h as f32 / bmp.height as f32);
    let img_w = ((bmp.width as f32 * fit).round() as i32).max(1);
    let img_h = ((bmp.height as f32 * fit).round() as i32).max(1);
    let tool_w = (560.0 * scale).round() as i32;
    let mut form_w = img_w.max(tool_w);
    let outside = screen.y + img_h + gap + tool_h <= work.bottom() - margin;
    let mut form_h = if outside { img_h + gap + tool_h } else { img_h };
    let mut x = screen.x;
    let mut y = screen.y;
    if x + form_w > work.right() - margin {
        x = work.right() - form_w - margin;
    }
    if y + form_h > work.bottom() - margin {
        y = work.bottom() - form_h - margin;
    }
    x = x.max(work.x + margin);
    y = y.max(work.y + margin);
    let inset = 1;
    form_w += inset * 2;
    form_h += inset;
    let img = Rect::new((form_w - img_w) / 2, inset, img_w, img_h);
    (Rect::new(x, y, form_w, form_h), img)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PinAsk;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    (*ptr).handle(hwnd, msg, wparam, lparam)
}

fn lp_pt(lp: LPARAM) -> Point {
    let v = lp.0 as u32;
    Point::new((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}

impl PinAsk {
    fn handle(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                self.paint(hwnd);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                self.on_move(hwnd, lp_pt(lparam));
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                self.on_down(hwnd, lp_pt(lparam));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                self.on_up(hwnd, lp_pt(lparam));
                LRESULT(0)
            }
            WM_RBUTTONDOWN => {
                if self.ann.has_draft() {
                    self.ann.cancel_draft();
                    self.capturing = false;
                    release_capture();
                    self.inv(hwnd);
                } else {
                    self.done = Some(None);
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                self.on_key(hwnd, wparam.0 as u32);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_CTLCOLOREDIT => unsafe {
                let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
                let _ = SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
                let _ = SetBkColor(hdc, COLORREF(theme::INPUT_BG.colorref()));
                LRESULT(self.edit_brush.0 as isize)
            },
            WM_DESTROY => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn inv(&self, hwnd: HWND) {
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
    }

    fn inv_toolbar(&mut self, hwnd: HWND) {
        let tb = self.toolbar.bounds;
        let pad = sc(6, self.scale);
        let mut dirty = tb.inflate(pad, pad);
        if let Some((tip_r, _)) = self.toolbar.hover_tip(self.scale) {
            dirty = dirty.union(tip_r.inflate(pad, pad));
        }
        if let Some(prev) = self.prev_tip_rect {
            dirty = dirty.union(prev.inflate(pad, pad));
        }
        if let Some((tip_r, _)) = self.toolbar.hover_tip(self.scale) {
            self.prev_tip_rect = Some(tip_r);
        } else {
            self.prev_tip_rect = None;
        }
        let rc = RECT {
            left: dirty.x,
            top: dirty.y,
            right: dirty.right(),
            bottom: dirty.bottom(),
        };
        unsafe {
            let _ = InvalidateRect(hwnd, Some(&rc), false);
        }
    }

    fn client_to_bmp(&self, p: Point) -> Point {
        let ic = self.image_client;
        let x = (p.x - ic.x) as f32 * self.bmp.width as f32 / ic.w.max(1) as f32;
        let y = (p.y - ic.y) as f32 * self.bmp.height as f32 / ic.h.max(1) as f32;
        Point::new(
            (x as i32).clamp(0, self.bmp.width - 1),
            (y as i32).clamp(0, self.bmp.height - 1),
        )
    }

    fn on_move(&mut self, hwnd: HWND, p: Point) {
        if self.ann.has_draft() {
            self.ann.move_to(self.client_to_bmp(p), shift_down());
            self.inv(hwnd);
            return;
        }
        if self.toolbar.set_hover(p) {
            self.inv_toolbar(hwnd);
        }
        let hit = self.toolbar.hit(p);
        if hit == ToolbarResult::Color || hit == ToolbarResult::Width || self.toolbar.hover_index >= 0 {
            cursor_hand();
        } else if self.ann.tool_active() && self.image_client.contains(p) {
            cursor_cross();
        } else {
            cursor_arrow();
        }
    }

    fn on_down(&mut self, hwnd: HWND, p: Point) {
        let hit = self.toolbar.hit(p);
        if hit == ToolbarResult::Color {
            let c = self.toolbar.hit_color(p);
            if c >= 0 {
                self.ann.color_index = c as usize;
            }
            self.inv(hwnd);
            return;
        }
        if hit == ToolbarResult::Width {
            let w = self.toolbar.hit_width(p);
            if w >= 0 {
                self.ann.set_width_index(w as usize);
            }
            self.inv(hwnd);
            return;
        }
        if hit != ToolbarResult::Miss {
            self.end_text(true);
            self.toolbar.pressed_index = self.toolbar.hit_test(p);
            self.inv(hwnd);
            return;
        }
        self.end_text(true);
        if !self.image_client.contains(p) {
            return;
        }
        let at = self.client_to_bmp(p);
        if self.ann.is_text_tool() {
            self.begin_text(hwnd, at, p);
            return;
        }
        if self.ann.can_draw() {
            self.ann.begin(at);
            capture(hwnd);
            self.capturing = true;
            self.inv(hwnd);
        }
    }

    fn on_up(&mut self, hwnd: HWND, p: Point) {
        if self.ann.has_draft() {
            self.ann.move_to(self.client_to_bmp(p), shift_down());
            self.ann.commit_draft();
            self.rebuild_lit();
            if self.capturing {
                release_capture();
                self.capturing = false;
            }
            self.inv(hwnd);
            return;
        }
        let hit = self.toolbar.hit_test(p);
        let pressed = self.toolbar.pressed_index;
        self.toolbar.pressed_index = -1;
        if pressed >= 0 && hit == pressed {
            self.apply(self.toolbar.result_at(pressed as usize));
        } else {
            self.inv(hwnd);
        }
    }

    fn on_key(&mut self, hwnd: HWND, vk: u32) {
        if vk == VK_ESCAPE.0 as u32 {
            if !self.edit.is_invalid() {
                self.end_text(false);
                self.inv(hwnd);
                return;
            }
            if self.ann.has_draft() {
                self.ann.cancel_draft();
                release_capture();
                self.inv(hwnd);
                return;
            }
            self.done = Some(None);
            return;
        }
        if !self.edit.is_invalid() {
            return;
        }
        match vk {
            k if k == VK_TAB.0 as u32 => self.ann.cycle_tab(),
            k if k == VK_A.0 as u32 => self.ann.toggle(AnnotKind::Arrow),
            k if k == VK_B.0 as u32 => self.ann.toggle(AnnotKind::Pencil),
            k if k == VK_H.0 as u32 => self.ann.toggle(AnnotKind::Marker),
            k if k == VK_M.0 as u32 => self.ann.toggle(AnnotKind::Mosaic),
            k if k == VK_X.0 as u32 => self.ann.toggle(AnnotKind::Text),
            k if k == VK_E.0 as u32 => self.ann.toggle(AnnotKind::Eraser),
            k if k == VK_Z.0 as u32 => {
                self.ann.undo();
                self.rebuild_lit();
            }
            k if k == VK_1.0 as u32 => self.ann.set_width_index(0),
            k if k == VK_2.0 as u32 => self.ann.set_width_index(1),
            k if k == VK_3.0 as u32 => self.ann.set_width_index(2),
            k if k == VK_4.0 as u32 => self.ann.set_width_index(3),
            k if k == VK_T.0 as u32 => self.choose(PostCaptureAction::Pin),
            k if k == VK_S.0 as u32 => self.choose(PostCaptureAction::SaveImage),
            k if k == VK_C.0 as u32 => self.choose(PostCaptureAction::CopyImage),
            k if k == VK_P.0 as u32 => self.choose(PostCaptureAction::CopyPath),
            k if k == VK_O.0 as u32 => self.choose(PostCaptureAction::CopyText),
            k if k == VK_L.0 as u32 => self.choose(PostCaptureAction::Translate),
            _ => {}
        }
        self.inv(hwnd);
    }

    fn apply(&mut self, r: ToolbarResult) {
        match r {
            ToolbarResult::Shape => self.ann.toggle_shape(),
            ToolbarResult::Stroke => self.ann.toggle_stroke(),
            ToolbarResult::Pencil => self.ann.toggle(AnnotKind::Pencil),
            ToolbarResult::Marker => self.ann.toggle(AnnotKind::Marker),
            ToolbarResult::Mosaic => self.ann.toggle(AnnotKind::Mosaic),
            ToolbarResult::AnnotText => self.ann.toggle(AnnotKind::Text),
            ToolbarResult::Eraser => self.ann.toggle(AnnotKind::Eraser),
            ToolbarResult::Undo => {
                self.ann.undo();
                self.rebuild_lit();
            }
            ToolbarResult::Close => self.done = Some(None),
            other => {
                if let Some(a) = other.to_action() {
                    self.choose(a);
                }
            }
        }
        self.inv(self.hwnd);
    }

    fn choose(&mut self, a: PostCaptureAction) {
        self.end_text(true);
        let _ = self.ann.commit_draft();
        self.done = Some(Some(a));
    }

    fn begin_text(&mut self, hwnd: HWND, at: Point, client: Point) {
        self.end_text(true);
        self.text_at = at;
        unsafe {
            let edit = CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                windows::core::w!("EDIT"),
                windows::core::w!(""),
                windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                    windows::Win32::UI::WindowsAndMessaging::WS_CHILD.0
                        | windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE.0
                        | 0x80,
                ),
                client.x,
                client.y,
                sc(220, dpi_scale_hwnd(hwnd).max(1.0)),
                sc(28, dpi_scale_hwnd(hwnd).max(1.0)),
                hwnd,
                None,
                crate::native::hinstance(),
                None,
            )
            .unwrap_or_default();
            self.edit = edit;
            release_capture();
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(edit);
        }
    }

    fn end_text(&mut self, commit: bool) {
        if self.edit.is_invalid() {
            return;
        }
        unsafe {
            let mut buf = [0u16; 512];
            windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(self.edit, &mut buf);
            let text = crate::util::from_wide(&buf);
            let _ = DestroyWindow(self.edit);
            self.edit = HWND::default();
            if commit && !text.trim().is_empty() {
                self.ann.add_text(self.text_at, &text);
                self.rebuild_lit();
            }
        }
    }

    #[allow(dead_code)]
    fn map_bmp_rect(&self, r: Rect) -> Rect {
        let ic = self.image_client;
        if self.bmp.width <= 0 || self.bmp.height <= 0 {
            return r;
        }
        let x1 = ic.x + r.x * ic.w / self.bmp.width;
        let y1 = ic.y + r.y * ic.h / self.bmp.height;
        let x2 = ic.x + r.right() * ic.w / self.bmp.width;
        let y2 = ic.y + r.bottom() * ic.h / self.bmp.height;
        Rect::from_ltrb(x1, y1, x2.max(x1 + 1), y2.max(y1 + 1))
    }

    fn rebuild_lit(&mut self) {
        if !self.ann.has_marks() {
            self.lit = None;
            return;
        }
        let mut lit = self.bmp.clone();
        let src = Rect::new(0, 0, lit.width, lit.height);
        self.ann.paint_committed(&mut lit, src, Some(&self.bmp));
        self.lit = Some(lit);
    }

    fn paint(&mut self, hwnd: HWND) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let rc = ps.rcPaint;
            let rw = (rc.right - rc.left).max(1);
            let rh = (rc.bottom - rc.top).max(1);

            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, rw, rh);
            let old_bmp = SelectObject(mem_dc, mem_bmp);
            let _ = SetWindowOrgEx(mem_dc, rc.left, rc.top, None);

            let mut client_rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rc);
            let client = Rect::from_ltrb(client_rc.left, client_rc.top, client_rc.right, client_rc.bottom);
            fill_rect_hdc(mem_dc, client, theme::WIN_BG);
            let src = self.lit.as_ref().unwrap_or(&self.bmp);
            src.blit_to_hdc(mem_dc, self.image_client);
            let ic = self.image_client;
            let bw = self.bmp.width.max(1);
            let bh = self.bmp.height.max(1);
            let ws = ic.w as f32 / bw as f32;
            self.ann.paint_draft_hdc(
                mem_dc,
                |p| Point::new(ic.x + p.x * ic.w / bw, ic.y + p.y * ic.h / bh),
                ws,
            );
            let scale = dpi_scale_hwnd(hwnd).max(self.scale).max(1.0);
            self.scale = scale;
            self.toolbar.sync(&self.ann);
            self.toolbar.undo_enabled =
                self.ann.has_marks() || self.ann.has_draft() || !self.edit.is_invalid();
            self.toolbar.relayout(self.image_client, client, scale);
            let tb = self.toolbar.bounds;
            if tb.w > 8 && tb.h > 8 {
                let mut chrome = Bitmap::new(tb.w, tb.h);
                self.toolbar.paint_at(&mut chrome, scale, Point::new(tb.x, tb.y));
                chrome.blit_to_hdc(mem_dc, tb);
            }
            if let Some((tip_r, text)) = self.toolbar.hover_tip(scale) {
                crate::draw::paint_chip_hdc(mem_dc, tip_r, &text, scale);
            }
            let _ = BitBlt(hdc, rc.left, rc.top, rw, rh, mem_dc, rc.left, rc.top, SRCCOPY);
            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(mem_bmp);
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
        }
    }
}
