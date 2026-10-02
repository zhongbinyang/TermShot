use crate::geom::Rect;
use crate::native::{cursor_pos, dpi_scale_at, hinstance, place_topmost, sc, work_area_from_point};
use crate::theme;
use crate::util::wide;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, GetDC,
    ReleaseDC, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CALCRECT, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FW_BOLD, FW_NORMAL, HFONT,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, KillTimer, LoadCursorW,
    PostMessageW, RegisterClassExW, SetCursor, SetTimer, SetWindowLongPtrW, CS_DBLCLKS,
    GWLP_USERDATA, IDC_HAND, WM_APP, WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT,
    WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WS_VISIBLE,
};

pub const WM_SHOW_TOAST: u32 = WM_APP + 40;
const WM_MOUSELEAVE: u32 = 0x02A3;

struct Toast {
    kind: ToastKind,
    title: String,
    text: String,
    remaining: u32,
    w: i32,
    h: i32,
    scale: f32,
    hovering: bool,
}

#[derive(Clone, Copy)]
enum ToastKind {
    Info,
    Success,
    Error,
}

static HOST: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static TOASTS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

pub fn set_host(hwnd: HWND) {
    HOST.store(hwnd.0, Ordering::SeqCst);
}

pub fn show(title: &str, text: &str) {
    show_kind(ToastKind::Info, title, text);
}

fn show_kind(kind: ToastKind, title: &str, text: &str) {
    let host = HWND(HOST.load(Ordering::SeqCst));
    if host.0.is_null() {
        show_now(kind, title, text);
        return;
    }
    let payload = Box::new((kind, title.to_string(), text.to_string()));
    let lp = LPARAM(Box::into_raw(payload) as isize);
    unsafe {
        if PostMessageW(host, WM_SHOW_TOAST, WPARAM(0), lp).is_err() {
            drop(Box::from_raw(lp.0 as *mut (ToastKind, String, String)));
            show_now(kind, title, text);
        }
    }
}

pub fn show_error(title: &str, text: &str) {
    show_kind(ToastKind::Error, title, text);
}

pub fn show_success(title: &str, text: &str) {
    show_kind(ToastKind::Success, title, text);
}

pub fn show_info(title: &str, text: &str) {
    show_kind(ToastKind::Info, title, text);
}

pub fn dispatch(lparam: LPARAM) {
    let p = lparam.0 as *mut (ToastKind, String, String);
    if p.is_null() {
        return;
    }
    let b = unsafe { Box::from_raw(p) };
    show_now(b.0, &b.1, &b.2);
}

fn measure_text_height(text: &str, max_w: i32, scale: f32) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    unsafe {
        let dc = GetDC(None);
        let font = make_font(sc(11, scale), false);
        let old = SelectObject(dc, font);
        let mut wt = wide(text);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: max_w.max(1),
            bottom: 0,
        };
        DrawTextW(dc, &mut wt, &mut rc, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
        SelectObject(dc, old);
        let _ = DeleteObject(font);
        let _ = ReleaseDC(None, dc);
        rc.bottom - rc.top
    }
}

pub(crate) fn compute_toast_dims(text: &str, scale: f32) -> (i32, i32, u32) {
    let w = sc(400, scale);
    let content_pad_x = sc(32, scale); // 18 left + 14 right
    let text_w = w - content_pad_x;

    let text_trimmed = text.trim();
    let (h, duration) = if text_trimmed.is_empty() {
        (sc(48, scale), 2600)
    } else {
        let text_h = measure_text_height(text_trimmed, text_w, scale);
        let needed_h = sc(10 + 18 + 6 + 12, scale) + text_h;
        let clamped_h = needed_h.clamp(sc(82, scale), sc(220, scale));
        let ms = if text_trimmed.len() > 60 { 4500 } else { 3200 };
        (clamped_h, ms)
    };
    (w, h, duration)
}

fn show_now(kind: ToastKind, title: &str, text: &str) {
    unsafe {
        let cur_pt = cursor_pos();
        let work = work_area_from_point(cur_pt);
        let scale = dpi_scale_at(cur_pt).max(1.0);

        let (w, h, duration) = compute_toast_dims(text, scale);

        let x = work.right() - w - sc(16, scale);
        let old = TOASTS.lock().ok().and_then(|list| (list.len() >= 3).then(|| list[0]));
        if let Some(old) = old {
            let _ = DestroyWindow(HWND(old as *mut core::ffi::c_void));
        }
        let stack_height = toast_stack_height(scale);
        let y = work.y + sc(16, scale) + stack_height;

        let state = Box::new(Toast {
            kind,
            title: title.to_string(),
            text: text.chars().take(600).collect(),
            remaining: if matches!(kind, ToastKind::Error) { duration.max(4500) } else { duration },
            w,
            h,
            scale,
            hovering: false,
        });

        let class = wide("TermShotToast");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(proc),
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
            x,
            y,
            w,
            h,
            None,
            None,
            hinstance(),
            Some(Box::into_raw(state) as *mut _),
        )
        .unwrap_or_default();

        let card_round = sc(8, scale);
        let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, card_round, card_round);
        if SetWindowRgn(hwnd, rgn, true) == 0 {
            let _ = DeleteObject(rgn);
        }

        if let Ok(mut list) = TOASTS.lock() {
            list.push(hwnd.0 as isize);
        }
        place_topmost(hwnd, Rect::new(x, y, w, h), false);
        let _ = SetTimer(hwnd, 1, 50, None);
    }
}

pub fn close() {
    unsafe {
        let list = TOASTS
            .lock()
            .map(|mut list| list.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        for raw in list {
            let _ = DestroyWindow(HWND(raw as *mut core::ffi::c_void));
        }
    }
}

unsafe fn toast_stack_height(scale: f32) -> i32 {
    let list = TOASTS.lock().map(|list| list.clone()).unwrap_or_default();
    list.into_iter()
        .filter_map(|raw| {
            let ptr = GetWindowLongPtrW(HWND(raw as *mut core::ffi::c_void), GWLP_USERDATA) as *mut Toast;
            (!ptr.is_null()).then(|| (*ptr).h + sc(8, scale))
        })
        .sum()
}

fn make_font(px: i32, bold: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
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
        )
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Toast;
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !ptr.is_null() {
                let t = &*ptr;
                let scale = t.scale.max(1.0);

                let mem_dc = CreateCompatibleDC(hdc);
                let mem_bmp = CreateCompatibleBitmap(hdc, t.w, t.h);
                let old_bmp = SelectObject(mem_dc, mem_bmp);

                let card_round = sc(8, scale);

                // 1. Background Card
                let (accent, background) = match t.kind {
                    ToastKind::Info => (theme::INFO, theme::PANEL),
                    ToastKind::Success => (theme::SUCCESS, theme::SUCCESS_BG),
                    ToastKind::Error => (theme::DANGER, theme::ERROR_BG),
                };
                let bg_brush = CreateSolidBrush(COLORREF(background.colorref()));
                let border_pen = CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref()));
                let old_b = SelectObject(mem_dc, bg_brush);
                let old_p = SelectObject(mem_dc, border_pen);
                let _ = RoundRect(mem_dc, 0, 0, t.w, t.h, card_round, card_round);
                SelectObject(mem_dc, old_b);
                SelectObject(mem_dc, old_p);
                let _ = DeleteObject(bg_brush);
                let _ = DeleteObject(border_pen);

                // 2. Left Accent Indicator Stripe
                let bar_x = sc(6, scale);
                let bar_y = sc(12, scale);
                let bar_w = sc(3, scale).max(2);
                let bar_h = (t.h - sc(24, scale)).max(sc(16, scale));
                let accent_brush = CreateSolidBrush(COLORREF(accent.colorref()));
                let old_ab = SelectObject(mem_dc, accent_brush);
                let _ = RoundRect(mem_dc, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, sc(2, scale), sc(2, scale));
                SelectObject(mem_dc, old_ab);
                let _ = DeleteObject(accent_brush);

                // 3. Title Text
                SetBkMode(mem_dc, TRANSPARENT);
                SetTextColor(mem_dc, COLORREF(accent.colorref()));
                let font_title = make_font(sc(13, scale), true);
                let old_font = SelectObject(mem_dc, font_title);

                let is_empty_text = t.text.trim().is_empty();
                let mut wt_title = wide(&t.title);
                let mut rc_title = if is_empty_text {
                    RECT {
                        left: sc(18, scale),
                        top: 0,
                        right: t.w - sc(14, scale),
                        bottom: t.h,
                    }
                } else {
                    RECT {
                        left: sc(18, scale),
                        top: sc(10, scale),
                        right: t.w - sc(14, scale),
                        bottom: sc(28, scale),
                    }
                };
                DrawTextW(
                    mem_dc,
                    &mut wt_title,
                    &mut rc_title,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // 4. Content Text (Multi-line word break with ellipsis)
                let font_text = make_font(sc(11, scale), false);
                if !is_empty_text {
                    SetTextColor(mem_dc, COLORREF(theme::TEXT.colorref()));
                    SelectObject(mem_dc, font_text);

                    let mut wt_text = wide(&t.text);
                    let mut rc_text = RECT {
                        left: sc(18, scale),
                        top: sc(34, scale),
                        right: t.w - sc(14, scale),
                        bottom: t.h - sc(10, scale),
                    };
                    DrawTextW(
                        mem_dc,
                        &mut wt_text,
                        &mut rc_text,
                        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX | DT_END_ELLIPSIS,
                    );
                }

                // Blit to screen
                let _ = BitBlt(hdc, 0, 0, t.w, t.h, mem_dc, 0, 0, SRCCOPY);

                SelectObject(mem_dc, old_font);
                let _ = DeleteObject(font_title);
                let _ = DeleteObject(font_text);

                SelectObject(mem_dc, old_bmp);
                let _ = DeleteObject(mem_bmp);
                let _ = DeleteDC(mem_dc);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !ptr.is_null() {
                (*ptr).hovering = true;
                (*ptr).remaining = 2500;
            }
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if !ptr.is_null() {
                (*ptr).hovering = false;
                (*ptr).remaining = 2000;
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if !ptr.is_null() {
                let cur = cursor_pos();
                let mut pt = windows::Win32::Foundation::POINT { x: cur.x, y: cur.y };
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                let is_inside = pt.x >= 0 && pt.x < (*ptr).w && pt.y >= 0 && pt.y < (*ptr).h;

                if is_inside || (*ptr).hovering {
                    (*ptr).remaining = 2500;
                } else {
                    (*ptr).remaining = (*ptr).remaining.saturating_sub(50);
                    if (*ptr).remaining == 0 {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
            SetCursor(cursor);
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, 1);
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            if let Ok(mut list) = TOASTS.lock() {
                list.retain(|raw| *raw != hwnd.0 as isize);
            }
            reflow_toasts();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn reflow_toasts() {
    let cursor = cursor_pos();
    let work = work_area_from_point(cursor);
    let scale = dpi_scale_at(cursor).max(1.0);
    let list = TOASTS.lock().map(|list| list.clone()).unwrap_or_default();
    let mut y = work.y + sc(16, scale);
    for raw in list {
        let hwnd = HWND(raw as *mut core::ffi::c_void);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Toast;
        if ptr.is_null() {
            continue;
        }
        let toast = &*ptr;
        let x = work.right() - toast.w - sc(16, toast.scale);
        place_topmost(hwnd, Rect::new(x, y, toast.w, toast.h), false);
        y += toast.h + sc(8, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_dimensions_empty_and_content() {
        let (w, h_empty, dur_empty) = compute_toast_dims("", 1.0);
        assert_eq!(w, 400);
        assert_eq!(h_empty, 48);
        assert_eq!(dur_empty, 2600);

        let (_, h_short, dur_short) = compute_toast_dims("已复制图片", 1.0);
        assert!(h_short >= 82);
        assert_eq!(dur_short, 3200);

        let long_text = "这是一段较长的文字，用于测试在多行情况下高度是否能够正确计算并伸展，避免文字被截断。\
            包含大量描述信息以及文件路径：C:\\Users\\zhong\\Pictures\\Screenshots\\2026-09-17 22-18-04.png。";
        let (_, h_long, dur_long) = compute_toast_dims(long_text, 1.0);
        assert!(h_long > h_short);
        assert!(h_long <= 220);
        assert_eq!(dur_long, 4500);
    }
}
