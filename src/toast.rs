use crate::geom::Rect;
use crate::native::{cursor_pos, dpi_scale_at, hinstance, place_topmost, sc, work_area_from_point};
use crate::theme;
use crate::util::wide;
use std::sync::atomic::{AtomicPtr, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FW_BOLD, FW_NORMAL, HFONT, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, KillTimer, LoadCursorW,
    PostMessageW, RegisterClassExW, SetCursor, SetTimer, SetWindowLongPtrW, CS_DBLCLKS,
    GWLP_USERDATA, IDC_HAND, WM_APP, WM_DESTROY, WM_LBUTTONDOWN, WM_PAINT, WM_SETCURSOR,
    WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};

pub const WM_SHOW_TOAST: u32 = WM_APP + 40;

struct Toast {
    title: String,
    text: String,
    remaining: u32,
    w: i32,
    h: i32,
    scale: f32,
}

static HOST: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static CURRENT: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn set_host(hwnd: HWND) {
    HOST.store(hwnd.0, Ordering::SeqCst);
}

pub fn show(title: &str, text: &str) {
    let host = HWND(HOST.load(Ordering::SeqCst));
    if host.0.is_null() {
        show_now(title, text);
        return;
    }
    let payload = Box::new((title.to_string(), text.to_string()));
    let lp = LPARAM(Box::into_raw(payload) as isize);
    unsafe {
        if PostMessageW(host, WM_SHOW_TOAST, WPARAM(0), lp).is_err() {
            drop(Box::from_raw(lp.0 as *mut (String, String)));
            show_now(title, text);
        }
    }
}

pub fn dispatch(lparam: LPARAM) {
    let p = lparam.0 as *mut (String, String);
    if p.is_null() {
        return;
    }
    let b = unsafe { Box::from_raw(p) };
    show_now(&b.0, &b.1);
}

fn current() -> HWND {
    HWND(CURRENT.load(Ordering::SeqCst))
}

fn set_current(hwnd: HWND) {
    CURRENT.store(hwnd.0, Ordering::SeqCst);
}

fn show_now(title: &str, text: &str) {
    unsafe {
        let cur_pt = cursor_pos();
        let work = work_area_from_point(cur_pt);
        let scale = dpi_scale_at(cur_pt).max(1.0);

        let w = sc(340, scale);
        let h = sc(86, scale);
        let x = work.right() - w - sc(16, scale);
        let y = work.bottom() - h - sc(16, scale);

        let cur = current();
        if !cur.0.is_null() && !cur.is_invalid() {
            let ptr = GetWindowLongPtrW(cur, GWLP_USERDATA) as *mut Toast;
            if !ptr.is_null() {
                let t = &mut *ptr;
                t.title = title.to_string();
                t.text = text.chars().take(400).collect();
                t.remaining = 3200;
                t.w = w;
                t.h = h;
                t.scale = scale;

                let card_round = sc(8, scale);
                let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, card_round, card_round);
                let _ = SetWindowRgn(cur, rgn, true);
                let _ = DeleteObject(rgn);

                place_topmost(cur, Rect::new(x, y, w, h), false);
                let _ = InvalidateRect(cur, None, false);
                return;
            }
        }

        let state = Box::new(Toast {
            title: title.to_string(),
            text: text.chars().take(400).collect(),
            remaining: 3200,
            w,
            h,
            scale,
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
        let _ = SetWindowRgn(hwnd, rgn, true);
        let _ = DeleteObject(rgn);

        set_current(hwnd);
        place_topmost(hwnd, Rect::new(x, y, w, h), false);
        let _ = SetTimer(hwnd, 1, 50, None);
    }
}

pub fn close() {
    unsafe {
        let cur = current();
        if !cur.0.is_null() {
            let _ = DestroyWindow(cur);
            set_current(HWND::default());
        }
    }
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
                let bg_brush = CreateSolidBrush(COLORREF(theme::PANEL.colorref()));
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
                let bar_h = t.h - sc(24, scale);
                let accent_brush = CreateSolidBrush(COLORREF(theme::ACCENT.colorref()));
                let old_ab = SelectObject(mem_dc, accent_brush);
                let _ = RoundRect(mem_dc, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, sc(2, scale), sc(2, scale));
                SelectObject(mem_dc, old_ab);
                let _ = DeleteObject(accent_brush);

                // 3. Title Text
                SetBkMode(mem_dc, TRANSPARENT);
                SetTextColor(mem_dc, COLORREF(theme::ACCENT_HI.colorref()));
                let font_title = make_font(sc(13, scale), true);
                let old_font = SelectObject(mem_dc, font_title);

                let mut wt_title = wide(&t.title);
                let mut rc_title = RECT {
                    left: sc(18, scale),
                    top: sc(10, scale),
                    right: t.w - sc(14, scale),
                    bottom: sc(28, scale),
                };
                DrawTextW(
                    mem_dc,
                    &mut wt_title,
                    &mut rc_title,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // 4. Content Text (Multi-line word break)
                SetTextColor(mem_dc, COLORREF(theme::TEXT.colorref()));
                let font_text = make_font(sc(11, scale), false);
                SelectObject(mem_dc, font_text);

                let mut wt_text = wide(&t.text);
                let mut rc_text = RECT {
                    left: sc(18, scale),
                    top: sc(32, scale),
                    right: t.w - sc(14, scale),
                    bottom: t.h - sc(8, scale),
                };
                DrawTextW(
                    mem_dc,
                    &mut wt_text,
                    &mut rc_text,
                    DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
                );

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
        WM_TIMER => {
            if !ptr.is_null() {
                (*ptr).remaining = (*ptr).remaining.saturating_sub(50);
                if (*ptr).remaining == 0 {
                    let _ = DestroyWindow(hwnd);
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
            if current() == hwnd {
                set_current(HWND::default());
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
