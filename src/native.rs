#![allow(dead_code)]
use crate::geom::{Color, Point, Rect};
use crate::util::wide;
use windows::core::Result;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateSolidBrush, DeleteObject, FillRect, GetDC, GetMonitorInfoW, MonitorFromPoint,
    ReleaseDC, SetBkMode, SetTextColor, TextOutW, HDC, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, RegisterHotKey, UnregisterHotKey,
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowRect, IsIconic, IsWindow,
    IsWindowVisible, LoadCursorW, SetCursor, SetForegroundWindow, SetWindowPos, HWND_TOPMOST,
    IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_SIZEALL, SM_CXSMICON, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW,
};

pub const WM_HOTKEY: u32 = 0x0312;
pub const WM_SETCURSOR: u32 = 0x0020;
pub const HTCLIENT: isize = 1;
pub const HOTKEY_CAPTURE_ID: i32 = 0x7E05;
pub const HOTKEY_TRANSLATE_ID: i32 = 0x7E06;

pub fn enable_dpi() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn dpi_scale_system() -> f32 {
    unsafe {
        let dpi = GetDpiForSystem();
        (if dpi == 0 { 96 } else { dpi }) as f32 / 96.0
    }
}

pub fn dpi_scale_hwnd(hwnd: HWND) -> f32 {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi > 0 {
            return dpi as f32 / 96.0;
        }
    }
    dpi_scale_system()
}

pub fn dpi_scale_at(p: Point) -> f32 {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: p.x, y: p.y }, MONITOR_DEFAULTTONEAREST);
        let mut x = 0u32;
        let mut y = 0u32;
        if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() && x > 0 {
            return x as f32 / 96.0;
        }
    }
    dpi_scale_system()
}

pub fn sc(v: i32, scale: f32) -> i32 {
    ((v as f32 * scale).round() as i32).max(1)
}

pub fn small_icon_size() -> i32 {
    unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64)
}

pub fn stroke_rect_hdc(hdc: HDC, r: Rect, c: Color, width: i32) {
    let w = width.max(1);
    fill_rect_hdc(hdc, Rect::new(r.x, r.y, r.w, w), c);
    fill_rect_hdc(hdc, Rect::new(r.x, r.bottom() - w, r.w, w), c);
    fill_rect_hdc(hdc, Rect::new(r.x, r.y, w, r.h), c);
    fill_rect_hdc(hdc, Rect::new(r.right() - w, r.y, w, r.h), c);
}

pub fn ui_font(px: i32) -> windows::Win32::Graphics::Gdi::HFONT {
    unsafe {
        windows::Win32::Graphics::Gdi::CreateFontW(
            -px,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::FW_NORMAL.0 as i32,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
            windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
            0,
            windows::core::w!("Segoe UI"),
        )
    }
}

pub fn set_font(hwnd: HWND, font: windows::Win32::Graphics::Gdi::HFONT) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_SETFONT,
            windows::Win32::Foundation::WPARAM(font.0 as usize),
            windows::Win32::Foundation::LPARAM(1),
        );
    }
}

pub fn hinstance() -> windows::Win32::Foundation::HMODULE {
    unsafe { GetModuleHandleW(None).unwrap_or_default() }
}

pub fn virtual_screen() -> Rect {
    unsafe {
        Rect::new(
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

pub fn work_area_from_point(p: Point) -> Rect {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: p.x, y: p.y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            return rect_from_win(info.rcWork);
        }
        virtual_screen()
    }
}

pub fn monitor_from_point(p: Point) -> Rect {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: p.x, y: p.y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            return rect_from_win(info.rcMonitor);
        }
        virtual_screen()
    }
}

pub fn rect_from_win(r: RECT) -> Rect {
    Rect::from_ltrb(r.left, r.top, r.right, r.bottom)
}

pub fn win_from_rect(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

pub fn cursor_pos() -> Point {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    Point::new(p.x, p.y)
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn set_foreground(hwnd: HWND) {
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
}

pub fn is_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd).as_bool() }
}

pub fn place_topmost(hwnd: HWND, r: Rect, activate: bool) {
    unsafe {
        let flags = if activate {
            SWP_SHOWWINDOW
        } else {
            SWP_SHOWWINDOW | SWP_NOACTIVATE
        };
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            r.x,
            r.y,
            r.w,
            r.h,
            flags,
        );
    }
}

pub fn apply_cursor_cross() {
    unsafe {
        if let Ok(c) = LoadCursorW(None, IDC_CROSS) {
            SetCursor(c);
        }
    }
}

pub fn apply_cursor_id(id: windows::core::PCWSTR) {
    unsafe {
        if let Ok(c) = LoadCursorW(None, id) {
            SetCursor(c);
        }
    }
}

pub fn cursor_arrow() {
    apply_cursor_id(IDC_ARROW);
}
pub fn cursor_hand() {
    apply_cursor_id(IDC_HAND);
}
pub fn cursor_cross() {
    apply_cursor_id(IDC_CROSS);
}
pub fn cursor_sizeall() {
    apply_cursor_id(IDC_SIZEALL);
}
pub fn cursor_ibeam() {
    apply_cursor_id(IDC_IBEAM);
}

pub fn key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

pub fn shift_down() -> bool {
    key_down(VK_SHIFT.0 as i32)
}

pub fn capture(hwnd: HWND) {
    unsafe {
        let _ = SetCapture(hwnd);
    }
}

pub fn release_capture() {
    unsafe {
        let _ = ReleaseCapture();
    }
}

pub fn has_capture(hwnd: HWND) -> bool {
    unsafe { GetCapture() == hwnd }
}

pub fn register_hotkey(hwnd: HWND, id: i32, modifiers: u32, vk: u32) -> bool {
    unsafe {
        let mods = HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT.0);
        RegisterHotKey(hwnd, id, mods, vk).is_ok()
    }
}

pub fn unregister_hotkey(hwnd: HWND, id: i32) {
    unsafe {
        let _ = UnregisterHotKey(hwnd, id);
    }
}

pub fn enable_dark_title(hwnd: HWND) {
    unsafe {
        let mut dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const _,
            4,
        );
        // older attr 19
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(19),
            &mut dark as *mut i32 as *const _,
            4,
        );
    }
}

pub fn dwm_bounds(hwnd: HWND) -> Option<Rect> {
    unsafe {
        let mut r = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok()
            && r.right > r.left
            && r.bottom > r.top
        {
            return Some(rect_from_win(r));
        }
        let mut wr = RECT::default();
        if GetWindowRect(hwnd, &mut wr).is_ok() && wr.right > wr.left && wr.bottom > wr.top {
            return Some(rect_from_win(wr));
        }
        None
    }
}

pub fn is_cloaked(hwnd: HWND) -> bool {
    unsafe {
        let mut cloaked: i32 = 0;
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut i32 as *mut _,
            4,
        )
        .is_ok()
            && cloaked != 0
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub fn is_iconic(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd).as_bool() }
}

pub fn bitblt_screen(hdc_dest: HDC, dest: Rect, src: Point) -> Result<()> {
    unsafe {
        let hdc_src = GetDC(None);
        let ok = BitBlt(
            hdc_dest,
            dest.x,
            dest.y,
            dest.w,
            dest.h,
            hdc_src,
            src.x,
            src.y,
            SRCCOPY,
        );
        ReleaseDC(None, hdc_src);
        ok?;
        Ok(())
    }
}

pub fn fill_rect_hdc(hdc: HDC, r: Rect, c: Color) {
    unsafe {
        let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF(c.colorref()));
        let rc = win_from_rect(r);
        let _ = FillRect(hdc, &rc, br);
        let _ = DeleteObject(br);
    }
}

pub fn line_hdc(hdc: HDC, x0: i32, y0: i32, x1: i32, y1: i32, c: Color, width: i32) {
    // Overlay WM_PAINT HDCs have aborted inside CreatePen/LineTo/Polygon.
    // Rectangles work because they only FillRect — match that here.
    let w = width.clamp(1, 24);
    let dx = x1 as i64 - x0 as i64;
    let dy = y1 as i64 - y0 as i64;
    let mut steps = dx.abs().max(dy.abs());
    if steps < 1 {
        steps = 1;
    }
    steps = steps.min(4096);
    unsafe {
        let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF(c.colorref()));
        if br.is_invalid() {
            return;
        }
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = (x0 as f64 + dx as f64 * t).round() as i32 - w / 2;
            let y = (y0 as f64 + dy as f64 * t).round() as i32 - w / 2;
            let rc = RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + w,
            };
            let _ = FillRect(hdc, &rc, br);
        }
        let _ = DeleteObject(br);
    }
}

pub fn polygon_fill_hdc(hdc: HDC, pts: &[Point], c: Color) {
    if pts.len() < 3 {
        return;
    }
    // Avoid GDI Polygon() — it can abort on some overlay HDC/driver combos.
    let w = 2;
    for win in pts.windows(2) {
        line_hdc(hdc, win[0].x, win[0].y, win[1].x, win[1].y, c, w);
    }
    let a = pts[0];
    let b = pts[pts.len() - 1];
    line_hdc(hdc, a.x, a.y, b.x, b.y, c, w);
    let origin = pts[0];
    for pt in pts.iter().skip(1) {
        line_hdc(hdc, origin.x, origin.y, pt.x, pt.y, c, w.max(3));
    }
}

pub fn ellipse_stroke_hdc(hdc: HDC, r: Rect, c: Color, width: i32) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    let w = width.clamp(1, 24);
    let cx = r.x as f32 + r.w as f32 * 0.5;
    let cy = r.y as f32 + r.h as f32 * 0.5;
    let rx = (r.w as f32 * 0.5).max(1.0);
    let ry = (r.h as f32 * 0.5).max(1.0);
    let steps = ((r.w + r.h) / 6).clamp(16, 72);
    let mut prev_x = (cx + rx).round() as i32;
    let mut prev_y = cy.round() as i32;
    for i in 1..=steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        let x = (cx + rx * a.cos()).round() as i32;
        let y = (cy + ry * a.sin()).round() as i32;
        line_hdc(hdc, prev_x, prev_y, x, y, c, w);
        prev_x = x;
        prev_y = y;
    }
}

pub fn polyline_hdc(hdc: HDC, pts: &[Point], c: Color, width: i32) {
    if pts.len() < 2 {
        return;
    }
    for win in pts.windows(2) {
        line_hdc(hdc, win[0].x, win[0].y, win[1].x, win[1].y, c, width);
    }
}

pub fn text_out(hdc: HDC, x: i32, y: i32, text: &str, c: Color) {
    unsafe {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, windows::Win32::Foundation::COLORREF(c.colorref()));
        let w = wide(text);
        let _ = TextOutW(hdc, x, y, &w[..w.len().saturating_sub(1)]);
    }
}

pub fn sleep_ms(ms: u32) {
    unsafe {
        windows::Win32::System::Threading::Sleep(ms);
    }
}

pub const MOD_CONTROL_BIT: u32 = MOD_CONTROL.0;
pub const MOD_SHIFT_BIT: u32 = MOD_SHIFT.0;
pub const MOD_ALT_BIT: u32 = MOD_ALT.0;
pub const MOD_WIN_BIT: u32 = MOD_WIN.0;
