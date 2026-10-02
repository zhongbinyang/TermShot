use crate::geom::{Color, Point, Rect};
use crate::native::{cursor_pos, dpi_scale_at, hinstance, place_topmost, sc, work_area_from_point};
use crate::theme;
use crate::util::wide;
use std::sync::atomic::{AtomicPtr, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, FW_BOLD, FW_NORMAL, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY,
    TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE, VK_TAB,
    VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, IsWindow, LoadCursorW,
    PostMessageW, RegisterClassExW, SetCursor, SetForegroundWindow, SetWindowLongPtrW,
    CS_DBLCLKS, GWLP_USERDATA, IDC_ARROW, IDC_HAND, WA_INACTIVE, WM_ACTIVATE,
    WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCCREATE,
    WM_PAINT, WM_SETCURSOR, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

pub const WM_QUICK_ACTION: u32 = WM_APP + 3;
pub const ACTION_CAPTURE: usize = 1;
pub const ACTION_TRANSLATE: usize = 2;
pub const ACTION_CLOSE_PINS: usize = 3;
pub const ACTION_OPEN_DIR: usize = 4;
pub const ACTION_SETTINGS: usize = 5;
pub const ACTION_EXIT: usize = 6;

const WM_MOUSELEAVE: u32 = 0x02A3;
const BUTTON_COUNT: usize = 6;

#[derive(Clone)]
pub struct Snapshot {
    pub capture_hotkey: String,
    pub translate_hotkey: String,
    pub api_ready: bool,
    pub ai_busy: bool,
    pub pin_count: usize,
}

struct QuickPanel {
    host: isize,
    snapshot: Snapshot,
    scale: f32,
    width: i32,
    height: i32,
    buttons: [Rect; BUTTON_COUNT],
    hover: i32,
    focus: i32,
}

static CURRENT: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn current() -> HWND {
    HWND(CURRENT.load(Ordering::SeqCst))
}

fn set_current(hwnd: HWND) {
    CURRENT.store(hwnd.0, Ordering::SeqCst);
}

pub fn close() {
    unsafe {
        let hwnd = current();
        if !hwnd.0.is_null() && IsWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
    }
}

pub fn toggle(host: HWND, snapshot: Snapshot) {
    unsafe {
        let existing = current();
        if !existing.0.is_null() && IsWindow(existing).as_bool() {
            let _ = DestroyWindow(existing);
            return;
        }

        let cursor = cursor_pos();
        let work = work_area_from_point(cursor);
        let scale = dpi_scale_at(cursor).max(1.0);
        let width = sc(356, scale);
        let height = sc(426, scale);
        let margin = sc(12, scale);
        let x = (cursor.x - width / 2).clamp(work.x + margin, work.right() - width - margin);
        let y = if cursor.y > work.y + work.h / 2 {
            (cursor.y - height - margin).max(work.y + margin)
        } else {
            (cursor.y + margin).min(work.bottom() - height - margin)
        };

        let mut state = Box::new(QuickPanel {
            host: host.0 as isize,
            snapshot,
            scale,
            width,
            height,
            buttons: [Rect::default(); BUTTON_COUNT],
            hover: -1,
            focus: 0,
        });
        state.layout();

        let class = wide("TermShotQuickPanel");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShot 快捷面板"),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            width,
            height,
            None,
            None,
            hinstance(),
            Some(Box::into_raw(state) as *mut _),
        )
        .unwrap_or_default();
        if hwnd.0.is_null() {
            return;
        }
        let radius = sc(theme::RADIUS_LG, scale);
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
        if SetWindowRgn(hwnd, region, true) == 0 {
            let _ = DeleteObject(region);
        }
        set_current(hwnd);
        place_topmost(hwnd, Rect::new(x, y, width, height), true);
        let _ = SetForegroundWindow(hwnd);
    }
}

impl QuickPanel {
    fn layout(&mut self) {
        let p = |v| sc(v, self.scale);
        self.buttons[0] = Rect::new(p(18), p(92), self.width - p(36), p(50));
        self.buttons[1] = Rect::new(p(18), p(150), self.width - p(36), p(50));
        self.buttons[2] = Rect::new(p(18), p(266), self.width - p(36), p(40));
        self.buttons[3] = Rect::new(p(18), p(352), p(100), p(42));
        self.buttons[4] = Rect::new(p(128), p(352), p(100), p(42));
        self.buttons[5] = Rect::new(p(238), p(352), p(100), p(42));
    }

    fn hit(&self, point: Point) -> i32 {
        self.buttons
            .iter()
            .position(|r| r.contains(point))
            .map(|i| i as i32)
            .unwrap_or(-1)
    }

    fn action(index: i32) -> usize {
        match index {
            0 => ACTION_CAPTURE,
            1 => ACTION_TRANSLATE,
            2 => ACTION_CLOSE_PINS,
            3 => ACTION_OPEN_DIR,
            4 => ACTION_SETTINGS,
            5 => ACTION_EXIT,
            _ => 0,
        }
    }

    fn activate(&self, index: i32) {
        let action = Self::action(index);
        if action == 0 || (action == ACTION_CLOSE_PINS && self.snapshot.pin_count == 0) {
            return;
        }
        let host = HWND(self.host as *mut core::ffi::c_void);
        unsafe {
            let _ = PostMessageW(host, WM_QUICK_ACTION, WPARAM(action), LPARAM(0));
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut QuickPanel;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let panel = &mut *ptr;
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd, panel);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let point = lp(lparam);
            let next = panel.hit(point);
            if next != panel.hover {
                panel.hover = next;
                let _ = InvalidateRect(hwnd, None, false);
            }
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut track);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            panel.hover = -1;
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let index = panel.hit(lp(lparam));
            if index >= 0 {
                panel.activate(index);
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = wparam.0 as u32;
            if key == VK_ESCAPE.0 as u32 {
                let _ = DestroyWindow(hwnd);
            } else if key == VK_TAB.0 as u32 || key == VK_DOWN.0 as u32 {
                panel.focus = (panel.focus + 1).rem_euclid(BUTTON_COUNT as i32);
                let _ = InvalidateRect(hwnd, None, false);
            } else if key == VK_UP.0 as u32 {
                panel.focus = (panel.focus - 1).rem_euclid(BUTTON_COUNT as i32);
                let _ = InvalidateRect(hwnd, None, false);
            } else if key == VK_RETURN.0 as u32 || key == VK_SPACE.0 as u32 {
                panel.activate(panel.focus);
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 as u32 & 0xFFFF) == WA_INACTIVE {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = if panel.hover >= 0 {
                LoadCursorW(None, IDC_HAND).unwrap_or_default()
            } else {
                LoadCursorW(None, IDC_ARROW).unwrap_or_default()
            };
            SetCursor(cursor);
            LRESULT(1)
        }
        WM_DESTROY => {
            drop(Box::from_raw(ptr));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if current() == hwnd {
                set_current(HWND::default());
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint(hwnd: HWND, panel: &QuickPanel) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mem = CreateCompatibleDC(hdc);
    let bitmap = CreateCompatibleBitmap(hdc, panel.width, panel.height);
    let old_bitmap = SelectObject(mem, bitmap);
    let p = |v| sc(v, panel.scale);

    let bg = CreateSolidBrush(COLORREF(theme::WIN_BG.colorref()));
    let border = CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref()));
    let old_brush = SelectObject(mem, bg);
    let old_pen = SelectObject(mem, border);
    let _ = RoundRect(mem, 0, 0, panel.width, panel.height, p(12), p(12));

    text(mem, "TermShot", Rect::new(p(20), p(14), p(190), p(28)), p(18), true, theme::TEXT, DT_LEFT);
    text(
        mem,
        "截图、识别与翻译",
        Rect::new(p(20), p(43), p(220), p(20)),
        p(11),
        false,
        theme::DIM,
        DT_LEFT,
    );

    let status_color = if panel.snapshot.ai_busy {
        theme::WARNING
    } else if panel.snapshot.api_ready {
        theme::SUCCESS
    } else {
        theme::DIM
    };
    let status = if panel.snapshot.ai_busy {
        "AI 处理中"
    } else if panel.snapshot.api_ready {
        "AI 已就绪"
    } else {
        "AI 未配置"
    };
    let dot = CreateSolidBrush(COLORREF(status_color.colorref()));
    let prev = SelectObject(mem, dot);
    let _ = windows::Win32::Graphics::Gdi::Ellipse(mem, p(250), p(25), p(258), p(33));
    SelectObject(mem, prev);
    let _ = DeleteObject(dot);
    text(mem, status, Rect::new(p(265), p(17), p(74), p(24)), p(10), false, status_color, DT_LEFT);

    button(mem, panel, 0, "区域截图", &panel.snapshot.capture_hotkey, true, true);
    button(mem, panel, 1, "翻译剪贴板", &panel.snapshot.translate_hotkey, false, true);

    text(mem, "贴图", Rect::new(p(20), p(218), p(100), p(22)), p(12), true, theme::TEXT, DT_LEFT);
    let pin_status = if panel.snapshot.pin_count == 0 {
        "当前没有桌面贴图".to_string()
    } else {
        format!("当前有 {} 张桌面贴图", panel.snapshot.pin_count)
    };
    text(mem, &pin_status, Rect::new(p(20), p(240), p(260), p(20)), p(10), false, theme::DIM, DT_LEFT);
    button(
        mem,
        panel,
        2,
        "关闭全部贴图",
        if panel.snapshot.pin_count == 0 { "无贴图" } else { "" },
        false,
        panel.snapshot.pin_count > 0,
    );

    text(mem, "快捷入口", Rect::new(p(20), p(320), p(120), p(22)), p(11), true, theme::DIM, DT_LEFT);
    small_button(mem, panel, 3, "保存目录");
    small_button(mem, panel, 4, "设置");
    small_button(mem, panel, 5, "退出");

    let _ = BitBlt(hdc, 0, 0, panel.width, panel.height, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old_brush);
    SelectObject(mem, old_pen);
    SelectObject(mem, old_bitmap);
    let _ = DeleteObject(bg);
    let _ = DeleteObject(border);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem);
    let _ = EndPaint(hwnd, &ps);
}

unsafe fn button(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    panel: &QuickPanel,
    index: usize,
    label: &str,
    shortcut: &str,
    primary: bool,
    enabled: bool,
) {
    let rect = panel.buttons[index];
    let active = panel.hover == index as i32 || panel.focus == index as i32;
    let bg = if !enabled {
        theme::INPUT_BG
    } else if primary {
        if active { theme::ACCENT_HI } else { theme::ACCENT }
    } else if active {
        theme::HOVER_BG
    } else {
        theme::PANEL
    };
    let fg = if !enabled {
        theme::DIM
    } else if primary {
        Color::rgb(0x05, 0x22, 0x1B)
    } else {
        theme::TEXT
    };
    card(hdc, rect, bg, if active { theme::ACCENT } else { theme::BORDER }, sc(8, panel.scale));
    text(hdc, label, Rect::new(rect.x + sc(16, panel.scale), rect.y, rect.w / 2, rect.h), sc(13, panel.scale), true, fg, DT_LEFT);
    text(hdc, shortcut, Rect::new(rect.x + rect.w / 2, rect.y, rect.w / 2 - sc(16, panel.scale), rect.h), sc(10, panel.scale), false, if primary { fg } else { theme::DIM }, windows::Win32::Graphics::Gdi::DT_RIGHT);
}

unsafe fn small_button(hdc: windows::Win32::Graphics::Gdi::HDC, panel: &QuickPanel, index: usize, label: &str) {
    let rect = panel.buttons[index];
    let active = panel.hover == index as i32 || panel.focus == index as i32;
    card(hdc, rect, if active { theme::HOVER_BG } else { theme::PANEL }, if active { theme::ACCENT } else { theme::BORDER }, sc(7, panel.scale));
    text(hdc, label, rect, sc(11, panel.scale), false, if active { theme::ACCENT_HI } else { theme::TEXT }, DT_CENTER);
}

unsafe fn card(hdc: windows::Win32::Graphics::Gdi::HDC, rect: Rect, bg: Color, stroke: Color, radius: i32) {
    let brush = CreateSolidBrush(COLORREF(bg.colorref()));
    let pen = CreatePen(PS_SOLID, 1, COLORREF(stroke.colorref()));
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);
    let _ = RoundRect(hdc, rect.x, rect.y, rect.right(), rect.bottom(), radius, radius);
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
}

unsafe fn text(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    value: &str,
    rect: Rect,
    size: i32,
    bold: bool,
    color: Color,
    align: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
) {
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        0,
        windows::core::w!("Microsoft YaHei UI"),
    );
    let old = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(color.colorref()));
    let mut wide_value = wide(value);
    let mut rc = RECT {
        left: rect.x,
        top: rect.y,
        right: rect.right(),
        bottom: rect.bottom(),
    };
    DrawTextW(hdc, &mut wide_value, &mut rc, align | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    SelectObject(hdc, old);
    let _ = DeleteObject(font);
}

fn lp(lparam: LPARAM) -> Point {
    let raw = lparam.0 as u32;
    Point::new((raw & 0xFFFF) as i16 as i32, ((raw >> 16) & 0xFFFF) as i16 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(scale: f32) -> QuickPanel {
        let mut panel = QuickPanel {
            host: 0,
            snapshot: Snapshot {
                capture_hotkey: "Ctrl+Shift+S".into(),
                translate_hotkey: "Ctrl+Shift+T".into(),
                api_ready: true,
                ai_busy: false,
                pin_count: 2,
            },
            scale,
            width: sc(356, scale),
            height: sc(426, scale),
            buttons: [Rect::default(); BUTTON_COUNT],
            hover: -1,
            focus: 0,
        };
        panel.layout();
        panel
    }

    #[test]
    fn layout_keeps_all_actions_inside_panel() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let panel = panel(scale);
            let bounds = Rect::new(0, 0, panel.width, panel.height);
            for button in panel.buttons {
                assert!(bounds.contains(Point::new(button.x, button.y)));
                assert!(button.right() <= bounds.right());
                assert!(button.bottom() <= bounds.bottom());
            }
        }
    }

    #[test]
    fn actions_have_stable_message_ids() {
        assert_eq!(QuickPanel::action(0), ACTION_CAPTURE);
        assert_eq!(QuickPanel::action(1), ACTION_TRANSLATE);
        assert_eq!(QuickPanel::action(5), ACTION_EXIT);
        assert_eq!(QuickPanel::action(-1), 0);
    }
}
