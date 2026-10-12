use crate::annotation::{AnnotKind, AnnotationSession};
use crate::bitmap::{Bitmap, GdiBitmap};
use crate::draw::fill_rect;
use crate::geom::{Color, Point, Rect};
use crate::native::{
    alt_down, apply_cursor_cross, capture, ctrl_down, cursor_arrow, cursor_cross, cursor_hand,
    cursor_pos, cursor_sizeall, cursor_sizenesw, cursor_sizens, cursor_sizenwse, cursor_sizewe,
    dpi_scale_at, dpi_scale_hwnd, hinstance, key_down, monitor_from_point, place_topmost,
    release_capture, sc, set_foreground, shift_down, stroke_rect_hdc, virtual_screen, WM_SETCURSOR,
};
use crate::settings::PostCaptureAction;
use crate::theme;
use crate::toolbar::{ActionToolbar, ToolbarResult};
use crate::util::{clamp_i32, wide};
use crate::windows_enum::{hit_test, WindowInfo};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, InvalidateRect, SelectObject, SetBkColor, SetTextColor, SetWindowOrgEx,
    HBRUSH, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_1, VK_2, VK_3, VK_4, VK_A, VK_ADD, VK_B, VK_C, VK_DOWN, VK_E, VK_ESCAPE, VK_H, VK_L,
    VK_LEFT, VK_M, VK_O, VK_OEM_MINUS, VK_OEM_PLUS, VK_P, VK_R, VK_RETURN, VK_RIGHT, VK_S,
    VK_SPACE, VK_SUBTRACT, VK_T, VK_TAB, VK_UP, VK_X, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_SIZE,
    CS_DBLCLKS, GWLP_USERDATA, MSG, SW_SHOW, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_CTLCOLOREDIT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const DRAG_THRESHOLD: i32 = 4;
const MAG_SRC: i32 = 13;
const MAG_ZOOM: i32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    None,
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Inside,
}

pub struct OverlayResult {
    pub selected: Option<Rect>,
    pub scroll: bool,
    pub action: Option<PostCaptureAction>,
    pub session: AnnotationSession,
}

struct OverlayState {
    shot: Bitmap,
    veiled: Bitmap,
    vs: Rect,
    windows: Vec<WindowInfo>,
    prev: HWND,
    ask_after: bool,
    scroll_after: bool,
    hwnd: HWND,
    cursor_bmp: Point,
    drag_start: Point,
    dragging: bool,
    drag_confirmed: bool,
    awaiting: bool,
    locked: Option<Rect>,
    hover: Option<WindowInfo>,
    toolbar: ActionToolbar,
    ann: AnnotationSession,
    done: Option<Outcome>,
    edit: HWND,
    text_at: Point,
    composed: Option<Bitmap>,
    shot_gdi: Option<GdiBitmap>,
    veiled_gdi: Option<GdiBitmap>,
    prev_tip_rect: Option<Rect>,
    edit_brush: HBRUSH,
    resizing: Option<ResizeHandle>,
    resize_origin: Point,
    locked_origin: Rect,
}

struct Outcome {
    selected: Option<Rect>,
    scroll: bool,
    action: Option<PostCaptureAction>,
}

pub fn run(
    shot: Bitmap,
    windows: Vec<WindowInfo>,
    prev: HWND,
    ask_after: bool,
    scroll_after: bool,
) -> OverlayResult {
    let vs = virtual_screen();
    let veiled = veil_shot(&shot);
    let shot_gdi = GdiBitmap::from_bitmap(&shot);
    let veiled_gdi = GdiBitmap::from_bitmap(&veiled);
    let mut state = Box::new(OverlayState {
        shot,
        veiled,
        vs,
        windows,
        prev,
        ask_after,
        scroll_after,
        hwnd: HWND::default(),
        cursor_bmp: Point::new(cursor_pos().x - vs.x, cursor_pos().y - vs.y),
        drag_start: Point::default(),
        dragging: false,
        drag_confirmed: false,
        awaiting: false,
        locked: None,
        hover: None,
        toolbar: {
            let mut t = ActionToolbar::default();
            t.show_scroll = true;
            t
        },
        ann: AnnotationSession::default(),
        done: None,
        edit: HWND::default(),
        text_at: Point::default(),
        composed: None,
        shot_gdi,
        veiled_gdi,
        prev_tip_rect: None,
        edit_brush: unsafe { CreateSolidBrush(COLORREF(theme::INPUT_BG.colorref())) },
        resizing: None,
        resize_origin: Point::default(),
        locked_origin: Rect::default(),
    });
    if state.veiled_gdi.is_some() {
        state.veiled = Bitmap::new(1, 1);
    }
    state.hover = hit_test(&state.windows, cursor_pos()).cloned();
    state.ann.attach(&state.shot);

    unsafe {
        let class = wide("TermShotOverlay");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_CROSS,
            )
            .unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShot"),
            WS_POPUP | WS_VISIBLE,
            vs.x,
            vs.y,
            state.shot.width,
            state.shot.height,
            None,
            None,
            hinstance(),
            Some(state.as_mut() as *mut OverlayState as *mut _),
        )
        .unwrap_or_default();
        state.hwnd = hwnd;
        place_topmost(
            hwnd,
            Rect::new(vs.x, vs.y, state.shot.width, state.shot.height),
            true,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        apply_cursor_cross();
        capture(hwnd);

        let mut msg = MSG::default();
        while state.done.is_none() {
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                break;
            }
            if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                PostQuitMessage(msg.wParam.0 as i32);
                break;
            }
            if !state.edit.is_invalid() && msg.message == WM_KEYDOWN {
                if msg.wParam.0 as u32 == VK_RETURN.0 as u32 {
                    state.end_text(hwnd, true);
                    state.invalidate(hwnd);
                    continue;
                } else if msg.wParam.0 as u32 == VK_ESCAPE.0 as u32 {
                    state.end_text(hwnd, false);
                    state.invalidate(hwnd);
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !hwnd.is_invalid() {
            let _ = DestroyWindow(hwnd);
        }
    }

    let outcome = state.done.take().unwrap_or(Outcome {
        selected: None,
        scroll: false,
        action: None,
    });
    if is_window_ok(state.prev) {
        set_foreground(state.prev);
    }
    unsafe {
        if !state.edit_brush.is_invalid() {
            let _ = DeleteObject(state.edit_brush);
            state.edit_brush = HBRUSH::default();
        }
    }
    OverlayResult {
        selected: outcome.selected,
        scroll: outcome.scroll,
        action: outcome.action,
        session: state.ann,
    }
}

fn is_window_ok(h: HWND) -> bool {
    crate::native::is_window(h)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
        as *mut OverlayState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    (*ptr).hwnd = hwnd;
    (*ptr).handle(hwnd, msg, wparam, lparam)
}

fn apply_resize_cursor(h: ResizeHandle) {
    match h {
        ResizeHandle::TopLeft | ResizeHandle::BottomRight => cursor_sizenwse(),
        ResizeHandle::TopRight | ResizeHandle::BottomLeft => cursor_sizenesw(),
        ResizeHandle::Top | ResizeHandle::Bottom => cursor_sizens(),
        ResizeHandle::Left | ResizeHandle::Right => cursor_sizewe(),
        ResizeHandle::Inside => cursor_sizeall(),
        ResizeHandle::None => {}
    }
}

impl OverlayState {
    fn handle(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                self.paint(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_CTLCOLOREDIT => unsafe {
                let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
                let _ = SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
                let _ = SetBkColor(hdc, COLORREF(theme::INPUT_BG.colorref()));
                LRESULT(self.edit_brush.0 as isize)
            },
            WM_MOUSEMOVE => {
                let p = lparam_point(lparam);
                self.on_move(hwnd, p);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                self.on_down(hwnd, lparam_point(lparam), true);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                self.on_up(hwnd, lparam_point(lparam));
                LRESULT(0)
            }
            WM_RBUTTONDOWN => {
                if self.awaiting && self.ann.has_draft() {
                    self.ann.cancel_draft();
                    self.invalidate(hwnd);
                } else {
                    self.cancel();
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                self.on_key(hwnd, wparam.0 as u32);
                LRESULT(0)
            }
            WM_SETCURSOR => {
                if self.awaiting {
                    if let Some(h) = self.resizing {
                        apply_resize_cursor(h);
                    }
                    LRESULT(1)
                } else if self.dragging && key_down(VK_SPACE.0 as i32) {
                    cursor_sizeall();
                    LRESULT(1)
                } else {
                    apply_cursor_cross();
                    LRESULT(1)
                }
            }
            WM_SIZE | WM_DPICHANGED => {
                self.restore_bounds(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn restore_bounds(&self, hwnd: HWND) {
        let want = Rect::new(self.vs.x, self.vs.y, self.shot.width, self.shot.height);
        let c = self.client_size(hwnd);
        if c.w != want.w || c.h != want.h {
            place_topmost(hwnd, want, true);
        }
    }

    fn dest_for_bmp(&self, hwnd: HWND, r: Rect) -> Rect {
        let c = self.client_size(hwnd);
        if c.w == self.shot.width && c.h == self.shot.height {
            r
        } else {
            self.map_bmp_rect(hwnd, r)
        }
    }

    fn blit_shot_region(
        &self,
        hdc: windows::Win32::Graphics::Gdi::HDC,
        dest: Rect,
        src: Rect,
    ) {
        if let Some(g) = &self.shot_gdi {
            g.blt(hdc, dest, src);
        } else {
            self.shot.blit_region_to_hdc(hdc, dest, src);
        }
    }

    fn invalidate(&self, hwnd: HWND) {
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
    }

    fn invalidate_toolbar(&mut self, hwnd: HWND) {
        let scale = if let Some(hr) = self.highlight_bmp() {
            self.ui_scale(hwnd, hr)
        } else {
            1.0
        };
        let tb = self.dest_for_bmp(hwnd, self.toolbar.bounds);
        let pad = sc(6, scale);
        let mut dirty = tb.inflate(pad, pad);
        if let Some((tip_r, _)) = self.toolbar.hover_tip(scale) {
            let tip_dest = self.dest_for_bmp(hwnd, tip_r);
            dirty = dirty.union(tip_dest.inflate(pad, pad));
        }
        if let Some(prev) = self.prev_tip_rect {
            dirty = dirty.union(prev.inflate(pad, pad));
        }
        if let Some((tip_r, _)) = self.toolbar.hover_tip(scale) {
            self.prev_tip_rect = Some(self.dest_for_bmp(hwnd, tip_r));
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

    fn client_size(&self, hwnd: HWND) -> Rect {
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut rc);
        }
        Rect::from_ltrb(rc.left, rc.top, rc.right, rc.bottom)
    }

    fn client_to_bmp(&self, hwnd: HWND, p: Point) -> Point {
        let c = self.client_size(hwnd);
        if c.w <= 0 || c.h <= 0 {
            return p;
        }
        Point::new(
            clamp_i32(p.x * self.shot.width / c.w, 0, self.shot.width - 1),
            clamp_i32(p.y * self.shot.height / c.h, 0, self.shot.height - 1),
        )
    }

    #[allow(dead_code)]
    fn bmp_to_client(&self, hwnd: HWND, p: Point) -> Point {
        let c = self.client_size(hwnd);
        if self.shot.width <= 0 || self.shot.height <= 0 {
            return p;
        }
        Point::new(
            p.x * c.w / self.shot.width,
            p.y * c.h / self.shot.height,
        )
    }

    #[allow(dead_code)]
    fn bmp_rect_to_client(&self, hwnd: HWND, r: Rect) -> Rect {
        let a = self.bmp_to_client(hwnd, Point::new(r.x, r.y));
        let b = self.bmp_to_client(hwnd, Point::new(r.right(), r.bottom()));
        Rect::from_ltrb(a.x, a.y, b.x.max(a.x + 1), b.y.max(a.y + 1))
    }

    fn highlight_bmp(&self) -> Option<Rect> {
        if let Some(l) = self.locked {
            return Some(l);
        }
        if self.drag_confirmed {
            return Some(self.normalized_drag());
        }
        self.hover.as_ref().map(|w| self.screen_to_bmp(w.bounds))
    }

    fn screen_to_bmp(&self, r: Rect) -> Rect {
        let x = r.x - self.vs.x;
        let y = r.y - self.vs.y;
        Rect::new(x, y, r.w, r.h).intersect(Rect::new(0, 0, self.shot.width, self.shot.height))
    }

    fn bmp_to_screen(&self, r: Rect) -> Rect {
        Rect::new(r.x + self.vs.x, r.y + self.vs.y, r.w, r.h)
    }

    fn bmp_pt_to_screen(&self, p: Point) -> Point {
        Point::new(p.x + self.vs.x, p.y + self.vs.y)
    }

    fn normalized_drag(&self) -> Rect {
        let x1 = self.drag_start.x.min(self.cursor_bmp.x).clamp(0, self.shot.width - 1);
        let y1 = self.drag_start.y.min(self.cursor_bmp.y).clamp(0, self.shot.height - 1);
        let x2 = self.drag_start.x.max(self.cursor_bmp.x).clamp(0, self.shot.width);
        let y2 = self.drag_start.y.max(self.cursor_bmp.y).clamp(0, self.shot.height);
        Rect::from_ltrb(x1, y1, (x2).max(x1 + 1), (y2).max(y1 + 1))
    }

    fn clamp_sel(&self, p: Point) -> Point {
        if let Some(r) = self.locked {
            Point::new(
                p.x.clamp(r.x, (r.right() - 1).max(r.x)),
                p.y.clamp(r.y, (r.bottom() - 1).max(r.y)),
            )
        } else {
            p
        }
    }

    fn hit_resize_handle(&self, hwnd: HWND, client: Point) -> ResizeHandle {
        let Some(hr) = self.locked else {
            return ResizeHandle::None;
        };
        let dest = self.dest_for_bmp(hwnd, hr);
        let scale = self.ui_scale(hwnd, hr);
        let handle_radius = sc(8, scale).max(6);
        let r2 = handle_radius * handle_radius;

        let dist_sq = |px: i32, py: i32| (client.x - px) * (client.x - px) + (client.y - py) * (client.y - py);

        // 1. Check 4 corners first
        if dist_sq(dest.x, dest.y) <= r2 {
            return ResizeHandle::TopLeft;
        }
        if dist_sq(dest.right(), dest.y) <= r2 {
            return ResizeHandle::TopRight;
        }
        if dist_sq(dest.right(), dest.bottom()) <= r2 {
            return ResizeHandle::BottomRight;
        }
        if dist_sq(dest.x, dest.bottom()) <= r2 {
            return ResizeHandle::BottomLeft;
        }

        // 2. Check 4 midpoints if enough space
        if dest.w >= sc(40, scale) {
            if dist_sq(dest.x + dest.w / 2, dest.y) <= r2 {
                return ResizeHandle::Top;
            }
            if dist_sq(dest.x + dest.w / 2, dest.bottom()) <= r2 {
                return ResizeHandle::Bottom;
            }
        }
        if dest.h >= sc(40, scale) {
            if dist_sq(dest.x, dest.y + dest.h / 2) <= r2 {
                return ResizeHandle::Left;
            }
            if dist_sq(dest.right(), dest.y + dest.h / 2) <= r2 {
                return ResizeHandle::Right;
            }
        }

        // 3. Check 4 edges
        let edge_tol = sc(5, scale).max(4);
        let in_x_range = client.x >= dest.x - edge_tol && client.x <= dest.right() + edge_tol;
        let in_y_range = client.y >= dest.y - edge_tol && client.y <= dest.bottom() + edge_tol;

        if in_x_range && (client.y - dest.y).abs() <= edge_tol {
            return ResizeHandle::Top;
        }
        if in_x_range && (client.y - dest.bottom()).abs() <= edge_tol {
            return ResizeHandle::Bottom;
        }
        if in_y_range && (client.x - dest.x).abs() <= edge_tol {
            return ResizeHandle::Left;
        }
        if in_y_range && (client.x - dest.right()).abs() <= edge_tol {
            return ResizeHandle::Right;
        }

        // 4. Inside selection (when no annotation tool is active, can drag whole rect)
        if dest.contains(client) && !self.ann.tool_active() {
            return ResizeHandle::Inside;
        }

        ResizeHandle::None
    }

    fn handle_resize_drag(&mut self, hwnd: HWND, client: Point, h: ResizeHandle) {
        let cur = self.client_to_bmp(hwnd, client);
        let dx = cur.x - self.resize_origin.x;
        let dy = cur.y - self.resize_origin.y;
        let orig = self.locked_origin;
        let min_size = 8;

        let mut l = orig.x;
        let mut t = orig.y;
        let mut r = orig.right();
        let mut b = orig.bottom();

        match h {
            ResizeHandle::TopLeft => {
                l = (orig.x + dx).min(orig.right() - min_size);
                t = (orig.y + dy).min(orig.bottom() - min_size);
            }
            ResizeHandle::Top => {
                t = (orig.y + dy).min(orig.bottom() - min_size);
            }
            ResizeHandle::TopRight => {
                r = (orig.right() + dx).max(orig.x + min_size);
                t = (orig.y + dy).min(orig.bottom() - min_size);
            }
            ResizeHandle::Right => {
                r = (orig.right() + dx).max(orig.x + min_size);
            }
            ResizeHandle::BottomRight => {
                r = (orig.right() + dx).max(orig.x + min_size);
                b = (orig.bottom() + dy).max(orig.y + min_size);
            }
            ResizeHandle::Bottom => {
                b = (orig.bottom() + dy).max(orig.y + min_size);
            }
            ResizeHandle::BottomLeft => {
                l = (orig.x + dx).min(orig.right() - min_size);
                b = (orig.bottom() + dy).max(orig.y + min_size);
            }
            ResizeHandle::Left => {
                l = (orig.x + dx).min(orig.right() - min_size);
            }
            ResizeHandle::Inside => {
                let max_dx = (self.shot.width - orig.right()).max(0);
                let min_dx = -orig.x;
                let clamped_dx = dx.clamp(min_dx, max_dx);

                let max_dy = (self.shot.height - orig.bottom()).max(0);
                let min_dy = -orig.y;
                let clamped_dy = dy.clamp(min_dy, max_dy);

                l = orig.x + clamped_dx;
                t = orig.y + clamped_dy;
                r = l + orig.w;
                b = t + orig.h;
            }
            ResizeHandle::None => return,
        }

        l = l.clamp(0, (self.shot.width - min_size).max(0));
        t = t.clamp(0, (self.shot.height - min_size).max(0));
        r = r.clamp(l + min_size, self.shot.width);
        b = b.clamp(t + min_size, self.shot.height);

        self.locked = Some(Rect::from_ltrb(l, t, r, b));
        apply_resize_cursor(h);
        self.rebuild_composed();
        self.invalidate(hwnd);
    }

    fn paint_handles(&self, hdc: windows::Win32::Graphics::Gdi::HDC, dest: Rect, scale: f32) {
        let size = sc(7, scale).max(5);
        let half = size / 2;
        let mut pts = Vec::with_capacity(8);

        pts.push(Point::new(dest.x, dest.y));
        pts.push(Point::new(dest.right(), dest.y));
        pts.push(Point::new(dest.right(), dest.bottom()));
        pts.push(Point::new(dest.x, dest.bottom()));

        if dest.w >= sc(40, scale) {
            pts.push(Point::new(dest.x + dest.w / 2, dest.y));
            pts.push(Point::new(dest.x + dest.w / 2, dest.bottom()));
        }
        if dest.h >= sc(40, scale) {
            pts.push(Point::new(dest.x, dest.y + dest.h / 2));
            pts.push(Point::new(dest.right(), dest.y + dest.h / 2));
        }

        for p in pts {
            let r = Rect::new(p.x - half, p.y - half, size, size);
            crate::native::fill_rect_hdc(hdc, r, Color::rgb(255, 255, 255));
            crate::native::stroke_rect_hdc(hdc, r, Color::argb(220, 10, 14, 20), 1);
        }
    }

    fn nudge_or_expand(&mut self, hwnd: HWND, vk: u32) -> bool {
        let Some(orig) = self.locked else {
            return false;
        };
        let ctrl = ctrl_down();
        let alt = alt_down();
        let shift = shift_down();
        let step = if shift { 10 } else { 1 };
        let min_size = 8;

        let mut l = orig.x;
        let mut t = orig.y;
        let mut r = orig.right();
        let mut b = orig.bottom();

        if vk == VK_LEFT.0 as u32 {
            if ctrl {
                l -= step;
            } else if alt {
                l = (l + step).min(r - min_size);
            } else {
                l -= step;
                r -= step;
            }
        } else if vk == VK_RIGHT.0 as u32 {
            if ctrl {
                r += step;
            } else if alt {
                r = (r - step).max(l + min_size);
            } else {
                l += step;
                r += step;
            }
        } else if vk == VK_UP.0 as u32 {
            if ctrl {
                t -= step;
            } else if alt {
                t = (t + step).min(b - min_size);
            } else {
                t -= step;
                b -= step;
            }
        } else if vk == VK_DOWN.0 as u32 {
            if ctrl {
                b += step;
            } else if alt {
                b = (b - step).max(t + min_size);
            } else {
                t += step;
                b += step;
            }
        } else if vk == VK_OEM_PLUS.0 as u32 || vk == VK_ADD.0 as u32 {
            let pad = if shift { 16 } else { 8 };
            l -= pad;
            t -= pad;
            r += pad;
            b += pad;
        } else if vk == VK_OEM_MINUS.0 as u32 || vk == VK_SUBTRACT.0 as u32 {
            let pad = if shift { 16 } else { 8 };
            l = (l + pad).min(orig.center().x - min_size / 2);
            t = (t + pad).min(orig.center().y - min_size / 2);
            r = (r - pad).max(l + min_size);
            b = (b - pad).max(t + min_size);
        } else {
            return false;
        }

        l = l.clamp(0, (self.shot.width - min_size).max(0));
        t = t.clamp(0, (self.shot.height - min_size).max(0));
        r = r.clamp(l + min_size, self.shot.width);
        b = b.clamp(t + min_size, self.shot.height);

        self.locked = Some(Rect::from_ltrb(l, t, r, b));
        self.rebuild_composed();
        self.invalidate(hwnd);
        true
    }

    fn on_move(&mut self, hwnd: HWND, client: Point) {
        self.cursor_bmp = self.client_to_bmp(hwnd, client);
        if self.awaiting {
            let bmp_pt = self.client_to_bmp(hwnd, client);
            if self.ann.has_draft() {
                self.ann.move_to(self.clamp_sel(bmp_pt), shift_down());
                self.invalidate(hwnd);
                return;
            }
            if let Some(h) = self.resizing {
                self.handle_resize_drag(hwnd, client, h);
                return;
            }
            let dirty = self.toolbar.set_hover(bmp_pt);
            let tb_hit = self.toolbar.hit(bmp_pt);
            if tb_hit == ToolbarResult::Color
                || tb_hit == ToolbarResult::Width
                || self.toolbar.hover_index >= 0
            {
                cursor_hand();
            } else if tb_hit == ToolbarResult::Chrome {
                cursor_arrow();
            } else {
                let handle = self.hit_resize_handle(hwnd, client);
                if handle != ResizeHandle::None {
                    apply_resize_cursor(handle);
                } else if self.ann.tool_active() {
                    cursor_cross();
                } else {
                    cursor_arrow();
                }
            }
            if dirty {
                self.invalidate_toolbar(hwnd);
            }
            return;
        }
        if self.dragging {
            let p = self.client_to_bmp(hwnd, client);
            if key_down(VK_SPACE.0 as i32) {
                cursor_sizeall();
                let dx = p.x - self.cursor_bmp.x;
                let dy = p.y - self.cursor_bmp.y;
                self.drag_start.x = (self.drag_start.x + dx).clamp(0, self.shot.width);
                self.drag_start.y = (self.drag_start.y + dy).clamp(0, self.shot.height);
                self.cursor_bmp = p;
            } else {
                self.cursor_bmp = p;
                let dx = self.cursor_bmp.x - self.drag_start.x;
                let dy = self.cursor_bmp.y - self.drag_start.y;
                if dx.abs() >= DRAG_THRESHOLD || dy.abs() >= DRAG_THRESHOLD {
                    self.drag_confirmed = true;
                }
            }
        } else {
            self.hover = hit_test(&self.windows, Point::new(self.cursor_bmp.x + self.vs.x, self.cursor_bmp.y + self.vs.y))
                .cloned();
        }
        self.invalidate(hwnd);
    }

    fn on_down(&mut self, hwnd: HWND, client: Point, left: bool) {
        if !left {
            return;
        }
        if self.awaiting {
            let bmp_pt = self.client_to_bmp(hwnd, client);
            let hit = self.toolbar.hit(bmp_pt);
            if hit == ToolbarResult::Color {
                let c = self.toolbar.hit_color(bmp_pt);
                if c >= 0 {
                    self.ann.color_index = c as usize;
                }
                self.invalidate(hwnd);
                return;
            }
            if hit == ToolbarResult::Width {
                let w = self.toolbar.hit_width(bmp_pt);
                if w >= 0 {
                    self.ann.set_width_index(w as usize);
                }
                self.invalidate(hwnd);
                return;
            }
            if hit != ToolbarResult::Miss {
                self.end_text(hwnd, true);
                self.toolbar.pressed_index = self.toolbar.hit_test(bmp_pt);
                self.invalidate(hwnd);
                return;
            }
            self.end_text(hwnd, true);

            // Check resize handle / edge / inside drag
            let handle = self.hit_resize_handle(hwnd, client);
            if handle != ResizeHandle::None {
                self.resizing = Some(handle);
                self.resize_origin = self.client_to_bmp(hwnd, client);
                self.locked_origin = self.locked.unwrap_or_default();
                apply_resize_cursor(handle);
                return;
            }

            self.cursor_bmp = bmp_pt;
            let at = self.clamp_sel(self.cursor_bmp);
            if self.ann.is_text_tool() {
                self.begin_text(hwnd, at, client);
                return;
            }
            if self.ann.can_draw() {
                self.ann.begin(at);
                cursor_cross();
                self.invalidate(hwnd);
            }
            return;
        }
        self.dragging = true;
        self.drag_confirmed = false;
        self.drag_start = self.client_to_bmp(hwnd, client);
        self.cursor_bmp = self.drag_start;
        self.invalidate(hwnd);
    }

    fn on_up(&mut self, hwnd: HWND, client: Point) {
        if self.awaiting {
            if self.resizing.is_some() {
                self.resizing = None;
                self.rebuild_composed();
                self.invalidate(hwnd);
                return;
            }
            if self.ann.has_draft() {
                self.ann.move_to(self.clamp_sel(self.client_to_bmp(hwnd, client)), shift_down());
                self.ann.commit_draft();
                self.rebuild_composed();
                self.invalidate(hwnd);
                return;
            }
            let bmp_pt = self.client_to_bmp(hwnd, client);
            let hit = self.toolbar.hit_test(bmp_pt);
            let pressed = self.toolbar.pressed_index;
            self.toolbar.pressed_index = -1;
            if pressed >= 0 && hit == pressed {
                self.apply_toolbar(hwnd, self.toolbar.result_at(pressed as usize));
            } else {
                self.invalidate(hwnd);
            }
            return;
        }
        if self.dragging {
            self.cursor_bmp = self.client_to_bmp(hwnd, client);
            if self.drag_confirmed {
                let r = self.normalized_drag();
                if r.w >= 1 && r.h >= 1 {
                    self.complete(self.bmp_to_screen(r));
                } else {
                    self.dragging = false;
                }
            } else if let Some(w) = self.hover.clone() {
                self.complete(w.bounds);
            } else {
                self.dragging = false;
                self.drag_confirmed = false;
            }
            self.invalidate(hwnd);
        }
    }

    fn on_key(&mut self, hwnd: HWND, vk: u32) {
        if vk == VK_ESCAPE.0 as u32 {
            if !self.edit.is_invalid() {
                self.end_text(hwnd, false);
                self.invalidate(hwnd);
                return;
            }
            if self.awaiting && self.ann.has_draft() {
                self.ann.cancel_draft();
                self.invalidate(hwnd);
                return;
            }
            self.cancel();
            return;
        }
        if !self.edit.is_invalid() {
            return;
        }
        if vk == VK_R.0 as u32 {
            self.try_scroll();
            return;
        }
        if !self.awaiting {
            if vk == VK_RETURN.0 as u32 {
                if self.drag_confirmed {
                    self.complete(self.bmp_to_screen(self.normalized_drag()));
                } else if let Some(w) = self.hover.clone() {
                    self.complete(w.bounds);
                }
            }
            return;
        }
        if self.nudge_or_expand(hwnd, vk) {
            return;
        }
        match vk {
            k if k == VK_TAB.0 as u32 => {
                self.ann.cycle_tab();
                self.invalidate(hwnd);
            }
            k if k == VK_A.0 as u32 => {
                self.ann.toggle(AnnotKind::Arrow);
                self.invalidate(hwnd);
            }
            k if k == VK_B.0 as u32 => {
                self.ann.toggle(AnnotKind::Pencil);
                self.invalidate(hwnd);
            }
            k if k == VK_H.0 as u32 => {
                self.ann.toggle(AnnotKind::Marker);
                self.invalidate(hwnd);
            }
            k if k == VK_M.0 as u32 => {
                self.ann.toggle(AnnotKind::Mosaic);
                self.invalidate(hwnd);
            }
            k if k == VK_X.0 as u32 => {
                self.ann.toggle(AnnotKind::Text);
                self.invalidate(hwnd);
            }
            k if k == VK_E.0 as u32 => {
                self.ann.toggle(AnnotKind::Eraser);
                self.invalidate(hwnd);
            }
            k if k == VK_Z.0 as u32 => {
                if self.ann.undo() {
                    self.rebuild_composed();
                    self.invalidate(hwnd);
                }
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
        if (VK_1.0 as u32..=VK_4.0 as u32).contains(&vk) {
            self.invalidate(hwnd);
        }
    }

    fn apply_toolbar(&mut self, hwnd: HWND, r: ToolbarResult) {
        match r {
            ToolbarResult::Shape => {
                self.end_text(hwnd, true);
                self.ann.toggle_shape();
            }
            ToolbarResult::Stroke => {
                self.end_text(hwnd, true);
                self.ann.toggle_stroke();
            }
            ToolbarResult::Pencil => {
                self.end_text(hwnd, true);
                self.ann.toggle(AnnotKind::Pencil);
            }
            ToolbarResult::Marker => {
                self.end_text(hwnd, true);
                self.ann.toggle(AnnotKind::Marker);
            }
            ToolbarResult::Mosaic => {
                self.end_text(hwnd, true);
                self.ann.toggle(AnnotKind::Mosaic);
            }
            ToolbarResult::AnnotText => {
                self.end_text(hwnd, true);
                self.ann.toggle(AnnotKind::Text);
            }
            ToolbarResult::Eraser => {
                self.end_text(hwnd, true);
                self.ann.toggle(AnnotKind::Eraser);
            }
            ToolbarResult::Undo => {
                self.end_text(hwnd, false);
                self.ann.undo();
                self.rebuild_composed();
            }
            ToolbarResult::Close => self.cancel(),
            ToolbarResult::Scroll => self.choose_scroll(None),
            other => {
                if let Some(a) = other.to_action() {
                    self.choose(a);
                }
            }
        }
        self.invalidate(hwnd);
    }

    fn complete(&mut self, mut screen: Rect) {
        screen = screen.intersect(self.vs);
        if screen.w < 1 || screen.h < 1 {
            return;
        }
        if self.scroll_after {
            self.done = Some(Outcome {
                selected: Some(screen),
                scroll: true,
                action: None,
            });
            return;
        }
        if self.ask_after {
            self.enter_ask(screen);
            return;
        }
        self.done = Some(Outcome {
            selected: Some(screen),
            scroll: false,
            action: None,
        });
    }

    fn enter_ask(&mut self, screen: Rect) {
        self.awaiting = true;
        self.dragging = false;
        self.drag_confirmed = false;
        self.locked = Some(self.screen_to_bmp(screen));
        self.ann.width_index = 1;
        self.ann.select(AnnotKind::Arrow);
        self.rebuild_composed();
        self.invalidate(self.hwnd);
    }

    fn choose(&mut self, action: PostCaptureAction) {
        self.end_text(self.hwnd, true);
        let _ = self.ann.commit_draft();
        let sel = self.locked.map(|r| self.bmp_to_screen(r));
        self.done = Some(Outcome {
            selected: sel,
            scroll: false,
            action: Some(action),
        });
    }

    fn try_scroll(&mut self) {
        if self.awaiting {
            self.choose_scroll(None);
            return;
        }
        if self.drag_confirmed {
            self.choose_scroll(Some(self.bmp_to_screen(self.normalized_drag())));
            return;
        }
        if let Some(w) = self.hover.clone() {
            self.choose_scroll(Some(w.bounds));
        }
    }

    fn choose_scroll(&mut self, screen: Option<Rect>) {
        self.end_text(self.hwnd, false);
        let mut r = screen.or_else(|| self.locked.map(|x| self.bmp_to_screen(x)));
        if let Some(ref mut rr) = r {
            *rr = rr.intersect(self.vs);
            if rr.w < 1 || rr.h < 1 {
                return;
            }
        } else {
            return;
        }
        self.done = Some(Outcome {
            selected: r,
            scroll: true,
            action: None,
        });
    }

    fn cancel(&mut self) {
        self.end_text(self.hwnd, false);
        self.done = Some(Outcome {
            selected: None,
            scroll: false,
            action: None,
        });
    }

    fn begin_text(&mut self, hwnd: HWND, bmp_pt: Point, client: Point) {
        self.end_text(hwnd, true);
        self.text_at = bmp_pt;
        unsafe {
            let edit = CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                windows::core::w!("EDIT"),
                windows::core::w!(""),
                windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                    windows::Win32::UI::WindowsAndMessaging::WS_CHILD.0
                        | windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE.0
                        | 0x0080, // ES_AUTOHSCROLL
                ),
                client.x.clamp(8, 4000),
                client.y.clamp(8, 4000),
                sc(220, dpi_scale_hwnd(hwnd).max(1.0)),
                sc(28, dpi_scale_hwnd(hwnd).max(1.0)),
                hwnd,
                None,
                hinstance(),
                None,
            )
            .unwrap_or_default();
            self.edit = edit;
            release_capture();
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(edit);
        }
    }

    fn end_text(&mut self, hwnd: HWND, commit: bool) {
        if self.edit.is_invalid() {
            return;
        }
        unsafe {
            let mut buf = [0u16; 512];
            windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(self.edit, &mut buf);
            let text = crate::util::from_wide(&buf);
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.edit);
            self.edit = HWND::default();
            if commit && !text.trim().is_empty() {
                self.ann.add_text(self.text_at, &text);
                self.rebuild_composed();
            }
            capture(hwnd);
        }
    }

    fn rebuild_composed(&mut self) {
        let Some(hr) = self.locked else {
            self.composed = None;
            return;
        };
        let mut lit = self.shot.crop(hr);
        self.ann.paint_committed(&mut lit, hr, Some(&self.shot));
        self.composed = Some(lit);
    }

    fn ui_scale(&self, hwnd: HWND, hr: Rect) -> f32 {
        let at = self.bmp_pt_to_screen(hr.center());
        dpi_scale_at(at)
            .max(dpi_scale_hwnd(hwnd))
            .max(1.0)
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

            let client = self.client_size(hwnd);
            let full = Rect::new(0, 0, self.shot.width, self.shot.height);
            if let Some(g) = &self.veiled_gdi {
                g.blt(mem_dc, full, full);
            } else {
                self.veiled.blit_to_hdc(mem_dc, full);
            }
            if let Some(hr) = self.highlight_bmp() {
                let dest = self.dest_for_bmp(hwnd, hr);
                let scale = self.ui_scale(hwnd, hr);
                if self.awaiting {
                    if let Some(lit) = &self.composed {
                        lit.blit_to_hdc(mem_dc, dest);
                    } else {
                        self.blit_shot_region(mem_dc, dest, hr);
                    }
                    self.ann.paint_draft_hdc(
                        mem_dc,
                        |p| {
                            let r = self.dest_for_bmp(hwnd, Rect::new(p.x, p.y, 1, 1));
                            Point::new(r.x, r.y)
                        },
                        1.0,
                    );
                } else {
                    self.blit_shot_region(mem_dc, dest, hr);
                }
                stroke_rect_hdc(
                    mem_dc,
                    dest,
                    Color::argb(220, 8, 10, 14),
                    sc(3, scale),
                );
                stroke_rect_hdc(mem_dc, dest, theme::ACCENT, 1);
                if self.awaiting {
                    self.paint_handles(mem_dc, dest, scale);
                }
                let size_text = format!("{} × {} px", hr.w.max(1), hr.h.max(1));
                let size_px = sc(12, scale);
                let (size_w, size_h) = crate::draw::measure_text(&size_text, size_px);
                let badge_w = size_w + sc(16, scale);
                let badge_h = size_h + sc(8, scale);
                let badge_y = if dest.y - badge_h - sc(6, scale) >= 0 {
                    dest.y - badge_h - sc(6, scale)
                } else {
                    dest.y + sc(6, scale)
                };
                paint_chip(
                    mem_dc,
                    Rect::new(dest.x, badge_y, badge_w, badge_h),
                    &size_text,
                    scale,
                );
                if self.awaiting {
                    self.toolbar.sync(&self.ann);
                    let confine = {
                        let c = self.bmp_pt_to_screen(Point::new(hr.x + hr.w / 2, hr.y + hr.h / 2));
                        let mon = monitor_from_point(c);
                        self.screen_to_bmp(mon)
                    };
                    let confine = if confine.w < 8 {
                        Rect::new(0, 0, self.shot.width, self.shot.height)
                    } else {
                        confine
                    };
                    self.toolbar.relayout(hr, confine, scale);
                    self.toolbar.undo_enabled =
                        self.ann.has_marks() || self.ann.has_draft() || !self.edit.is_invalid();
                    let tb = self.toolbar.bounds;
                    if tb.w > 8 && tb.h > 8 {
                        let mut chrome = Bitmap::new(tb.w, tb.h);
                        self.toolbar.paint_at(&mut chrome, scale, Point::new(tb.x, tb.y));
                        chrome.blit_to_hdc(mem_dc, self.dest_for_bmp(hwnd, tb));
                    }
                    if let Some((tip_r, text)) = self.toolbar.hover_tip(scale) {
                        paint_chip(mem_dc, self.dest_for_bmp(hwnd, tip_r), &text, scale);
                    }
                }
            }
            if !self.awaiting {
                let scale = dpi_scale_at(self.bmp_pt_to_screen(self.cursor_bmp))
                    .max(dpi_scale_hwnd(hwnd))
                    .max(1.0);
                let c = self.dest_for_bmp(hwnd, Rect::new(self.cursor_bmp.x, self.cursor_bmp.y, 1, 1));
                gdi_line(
                    mem_dc,
                    0,
                    c.y,
                    client.w.max(self.shot.width),
                    c.y,
                    theme::ACCENT.with_alpha(200),
                    scale.max(1.0) as i32,
                );
                gdi_line(
                    mem_dc,
                    c.x,
                    0,
                    c.x,
                    client.h.max(self.shot.height),
                    theme::ACCENT.with_alpha(200),
                    scale.max(1.0) as i32,
                );
                paint_magnifier_hdc(
                    mem_dc,
                    &self.shot,
                    self.cursor_bmp,
                    self.bmp_pt_to_screen(self.cursor_bmp),
                    scale,
                    full,
                );
                paint_capture_hint(mem_dc, self, hwnd, scale);
            }
            let _ = BitBlt(hdc, rc.left, rc.top, rw, rh, mem_dc, rc.left, rc.top, SRCCOPY);
            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(mem_bmp);
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
        }
    }

    fn map_bmp_pt(&self, hwnd: HWND, p: Point) -> Point {
        let c = self.client_size(hwnd);
        if self.shot.width <= 0 || self.shot.height <= 0 {
            return p;
        }
        Point::new(
            p.x * c.w / self.shot.width,
            p.y * c.h / self.shot.height,
        )
    }

    fn map_bmp_rect(&self, hwnd: HWND, r: Rect) -> Rect {
        let a = self.map_bmp_pt(hwnd, Point::new(r.x, r.y));
        let b = self.map_bmp_pt(hwnd, Point::new(r.right(), r.bottom()));
        Rect::from_ltrb(a.x, a.y, b.x.max(a.x + 1), b.y.max(a.y + 1))
    }
}

fn veil_shot(shot: &Bitmap) -> Bitmap {
    let mut v = shot.clone();
    for p in v.pixels.iter_mut() {
        let b = *p & 0xFF;
        let g = (*p >> 8) & 0xFF;
        let r = (*p >> 16) & 0xFF;
        let nb = (b * 40 + 6 * 215) / 255;
        let ng = (g * 40 + 8 * 215) / 255;
        let nr = (r * 40 + 12 * 215) / 255;
        *p = nb | (ng << 8) | (nr << 16) | 0xFF000000;
    }
    v
}

fn paint_chip(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    r: Rect,
    text: &str,
    scale: f32,
) {
    if r.w < 8 || r.h < 8 || text.is_empty() {
        return;
    }
    crate::native::fill_rect_hdc(hdc, r, Color::argb(230, 10, 14, 20));
    crate::draw::draw_text_hdc(
        hdc,
        Point::new(r.x + sc(7, scale), r.y + sc(4, scale)),
        text,
        theme::TEXT,
        sc(13, scale),
    );
}

fn paint_capture_hint(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    state: &OverlayState,
    hwnd: HWND,
    scale: f32,
) {
    let text = if state.drag_confirmed {
        "松开完成  ·  按住空格平移  ·  R 滚动  ·  Esc / 右键取消"
    } else if state.scroll_after {
        "拖动框选滚动区域  ·  单击窗口  ·  Esc / 右键取消"
    } else {
        "拖动框选  ·  单击窗口  ·  按住空格平移  ·  R 滚动  ·  Esc / 右键取消"
    };
    let px = sc(14, scale);
    let (tw, th) = crate::draw::measure_text(text, px);
    let box_w = tw + sc(28, scale);
    let box_h = th + sc(10, scale);
    let screen_pt = state.bmp_pt_to_screen(state.cursor_bmp);
    let mon = state.screen_to_bmp(monitor_from_point(screen_pt));
    let mut x = mon.x + (mon.w - box_w) / 2;
    let mut y = mon.y + sc(18, scale);
    x = x.max(mon.x + 8);
    y = y.max(mon.y + 8);
    let r = state.dest_for_bmp(hwnd, Rect::new(x, y, box_w, box_h));
    paint_chip(hdc, r, text, scale);
}

fn gdi_line(hdc: windows::Win32::Graphics::Gdi::HDC, x0: i32, y0: i32, x1: i32, y1: i32, c: Color, width: i32) {
    crate::native::line_hdc(hdc, x0, y0, x1, y1, c, width);
}

fn paint_magnifier_hdc(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    shot: &Bitmap,
    cursor: Point,
    screen: Point,
    scale: f32,
    client: Rect,
) {
    let src = MAG_SRC;
    let zoom = sc(MAG_ZOOM, scale);
    let mag = src * zoom;
    let info_h = sc(44, scale);
    let pad = sc(3, scale);
    let offset = sc(28, scale);
    let total_w = mag + pad * 2;
    let total_h = mag + pad * 2 + info_h;
    let mut bx = cursor.x + offset;
    let mut by = cursor.y + offset;
    if bx + total_w > shot.width - 8 {
        bx = cursor.x - offset - total_w;
    }
    if by + total_h > shot.height - 8 {
        by = cursor.y - offset - total_h;
    }
    bx = bx.clamp(8, (shot.width - total_w - 8).max(8));
    by = by.clamp(8, (shot.height - total_h - 8).max(8));
    let mut frame = Bitmap::new(total_w, total_h);
    fill_rect(&mut frame, 0, 0, total_w, total_h, theme::MAG_BG);
    let dest = Rect::new(pad, pad, mag, mag);
    let half = src / 2;
    for y in 0..src {
        for x in 0..src {
            let sx = cursor.x - half + x;
            let sy = cursor.y - half + y;
            let c = shot.get(sx, sy);
            fill_rect(&mut frame, dest.x + x * zoom, dest.y + y * zoom, zoom, zoom, c);
        }
    }
    crate::draw::stroke_rect(
        &mut frame,
        dest.x as f32 + (src / 2 * zoom) as f32,
        dest.y as f32 + (src / 2 * zoom) as f32,
        zoom as f32,
        zoom as f32,
        theme::ACCENT,
        (1.5 * scale).max(1.0),
    );
    crate::draw::stroke_rect(
        &mut frame,
        0.5,
        0.5,
        (total_w - 1) as f32,
        (total_h - 1) as f32,
        theme::MAG_BORDER,
        (1.5 * scale).max(1.0),
    );
    let color = shot.get(cursor.x, cursor.y);
    let hex = format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    let rgb = format!("{}  {}  {}", color.r, color.g, color.b);
    let coord = format!("{}, {}", screen.x, screen.y);
    let info_y = dest.bottom() + sc(4, scale);
    crate::draw::draw_text(
        &mut frame,
        Point::new(pad + sc(2, scale), info_y),
        &hex,
        theme::TEXT,
        sc(12, scale),
    );
    crate::draw::draw_text(
        &mut frame,
        Point::new(pad + sc(74, scale), info_y),
        &rgb,
        theme::DIM,
        sc(12, scale),
    );
    crate::draw::draw_text(
        &mut frame,
        Point::new(pad + sc(2, scale), info_y + sc(16, scale)),
        &coord,
        theme::DIM,
        sc(12, scale),
    );
    let dx = if shot.width > 0 {
        bx * client.w / shot.width
    } else {
        bx
    };
    let dy = if shot.height > 0 {
        by * client.h / shot.height
    } else {
        by
    };
    frame.blit_to_hdc(hdc, Rect::new(dx, dy, total_w, total_h));
}

fn lparam_point(lp: LPARAM) -> Point {
    let v = lp.0 as u32;
    Point::new((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize_handle_delta_calculation() {
        let orig = Rect::new(100, 100, 200, 150);
        let shot_w = 1920;
        let shot_h = 1080;
        let min_size = 8;

        // Simulate dragging Right handle by +30px
        let dx = 30;
        let _dy = 0;
        let r = (orig.right() + dx).max(orig.x + min_size).clamp(orig.x + min_size, shot_w);
        assert_eq!(r, 330);
        assert_eq!(r - orig.x, 230); // 200 + 30

        // Simulate dragging TopLeft handle by (-20, -15)
        let dx = -20;
        let dy = -15;
        let l = (orig.x + dx).min(orig.right() - min_size).clamp(0, shot_w - min_size);
        let t = (orig.y + dy).min(orig.bottom() - min_size).clamp(0, shot_h - min_size);
        assert_eq!(l, 80);
        assert_eq!(t, 85);
        assert_eq!(orig.right() - l, 220); // 200 + 20
        assert_eq!(orig.bottom() - t, 165); // 150 + 15

        // Simulate dragging TopLeft PAST BottomRight (prevent inversion)
        let dx = 500;
        let dy = 500;
        let l = (orig.x + dx).min(orig.right() - min_size);
        let t = (orig.y + dy).min(orig.bottom() - min_size);
        assert_eq!(l, orig.right() - min_size);
        assert_eq!(t, orig.bottom() - min_size);
    }

    #[test]
    fn test_spacebar_pan_preserves_dimensions() {
        let mut drag_start = Point::new(100, 100);
        let mut cursor_bmp = Point::new(300, 250);

        let initial_w = cursor_bmp.x - drag_start.x;
        let initial_h = cursor_bmp.y - drag_start.y;
        assert_eq!(initial_w, 200);
        assert_eq!(initial_h, 150);

        // User moves mouse from (300, 250) to (340, 280) while holding spacebar
        let new_mouse = Point::new(340, 280);
        let dx = new_mouse.x - cursor_bmp.x;
        let dy = new_mouse.y - cursor_bmp.y;
        drag_start.x += dx;
        drag_start.y += dy;
        cursor_bmp = new_mouse;

        assert_eq!(drag_start, Point::new(140, 130));
        assert_eq!(cursor_bmp, Point::new(340, 280));
        assert_eq!(cursor_bmp.x - drag_start.x, 200);
        assert_eq!(cursor_bmp.y - drag_start.y, 150);
    }
}

