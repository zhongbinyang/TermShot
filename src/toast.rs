use crate::draw::{draw_text, fill_rect};
use crate::geom::{Point, Rect};
use crate::native::{
    cursor_pos, dpi_scale_at, hinstance, monitor_from_point, place_topmost, sc, virtual_screen,
};
use crate::theme;
use crate::util::wide;
use std::sync::atomic::{AtomicPtr, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, KillTimer, PostMessageW,
    RegisterClassExW, SetTimer, SetWindowLongPtrW, CS_DBLCLKS, GWLP_USERDATA, WM_APP, WM_DESTROY,
    WM_LBUTTONDOWN, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
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
        let cur = current();
        if !cur.0.is_null() && !cur.is_invalid() {
            let ptr = GetWindowLongPtrW(cur, GWLP_USERDATA) as *mut Toast;
            if !ptr.is_null() {
                (*ptr).title = title.to_string();
                (*ptr).text = text.chars().take(400).collect();
                (*ptr).remaining = 3200;
                let _ = InvalidateRect(cur, None, false);
                return;
            }
        }
        let scale = dpi_scale_at(cursor_pos()).max(1.0);
        let w = sc(360, scale);
        let h = sc(110, scale);
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
        let vs = virtual_screen();
        let mon = monitor_from_point(Point::new(vs.right() - 40, vs.bottom() - 40));
        let x = mon.right() - w - sc(24, scale);
        let y = mon.bottom() - h - sc(48, scale);
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
                let mut frame = crate::bitmap::Bitmap::new(t.w, t.h);
                fill_rect(&mut frame, 0, 0, t.w, t.h, theme::WIN_BG);
                draw_text(
                    &mut frame,
                    Point::new(sc(16, t.scale), sc(12, t.scale)),
                    &t.title,
                    theme::ACCENT_HI,
                    sc(16, t.scale),
                );
                draw_text(
                    &mut frame,
                    Point::new(sc(16, t.scale), sc(40, t.scale)),
                    &t.text,
                    theme::TEXT,
                    sc(13, t.scale),
                );
                frame.blit_to_hdc(hdc, Rect::new(0, 0, t.w, t.h));
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
