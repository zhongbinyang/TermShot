use crate::bitmap::Bitmap;
use crate::geom::{Point, Rect};
use crate::native::{cursor_sizeall, hinstance, work_area_from_point};
use crate::settings::Settings;
use crate::theme;
use crate::util::wide;
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FrameRect, InvalidateRect, PAINTSTRUCT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    GetWindowRect, InsertMenuW, IsWindow, PostMessageW, RegisterClassExW, SetLayeredWindowAttributes,
    SetWindowLongPtrW, SetWindowPos, TrackPopupMenu, CS_DBLCLKS, GWLP_USERDATA, HWND_TOPMOST,
    LWA_ALPHA, MF_BYPOSITION, MF_GRAYED, MF_STRING, SWP_NOZORDER, TPM_LEFTALIGN, TPM_RETURNCMD,
    WM_APP, WM_CONTEXTMENU, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WS_VISIBLE,
};

const MIN_ZOOM: f32 = 0.08;
const MAX_ZOOM: f32 = 8.0;
const WM_OCR_DONE: u32 = WM_APP + 50;

pub struct Pin {
    hwnd: HWND,
    bmp: Bitmap,
    zoom: f32,
    opacity: u8,
    drag: bool,
    drag_off: Point,
    full_text: Option<String>,
    ocr_busy: bool,
}

static PINS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

pub fn open(bmp: Bitmap, origin: Point, settings: &Settings) -> HWND {
    let work = work_area_from_point(origin);
    let mut zoom = 1.0f32;
    let max_w = (work.w as f32 * 0.7).max(80.0);
    let max_h = (work.h as f32 * 0.7).max(80.0);
    if bmp.width as f32 * zoom > max_w {
        zoom = max_w / bmp.width as f32;
    }
    if bmp.height as f32 * zoom > max_h {
        zoom = max_h / bmp.height as f32;
    }
    let w = ((bmp.width as f32 * zoom).round() as i32).max(24);
    let h = ((bmp.height as f32 * zoom).round() as i32).max(24);
    let start_ocr = settings.has_vision();
    let state = Box::new(Pin {
        hwnd: HWND::default(),
        bmp,
        zoom,
        opacity: 255,
        drag: false,
        drag_off: Point::default(),
        full_text: None,
        ocr_busy: start_ocr,
    });
    unsafe {
        let class = wide("TermShotPastePin");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!(""),
            WS_POPUP | WS_VISIBLE,
            origin.x,
            origin.y,
            w,
            h,
            None,
            None,
            hinstance(),
            Some(Box::into_raw(state) as *mut _),
        )
        .unwrap_or_default();
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, origin.x, origin.y, w, h, SWP_NOZORDER);
        let _ = SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            255,
            LWA_ALPHA,
        );
        if let Ok(mut g) = PINS.lock() {
            g.push(hwnd.0 as isize);
        }
        if start_ocr {
            spawn_ocr(hwnd, settings.clone());
        }
        hwnd
    }
}

fn spawn_ocr(hwnd: HWND, settings: Settings) {
    let raw = hwnd.0 as isize;
    let bmp = unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pin;
        if ptr.is_null() {
            return;
        }
        (*ptr).bmp.clone()
    };
    std::thread::spawn(move || {
        let result = crate::vision::ocr(&bmp, &settings);
        let hwnd = HWND(raw as *mut core::ffi::c_void);
        unsafe {
            if !IsWindow(hwnd).as_bool() {
                return;
            }
            let payload = Box::new(result);
            let _ = PostMessageW(
                hwnd,
                WM_OCR_DONE,
                WPARAM(0),
                LPARAM(Box::into_raw(payload) as isize),
            );
        }
    });
}

pub fn close_all() {
    let list = PINS.lock().map(|mut g| g.drain(..).collect::<Vec<_>>()).unwrap_or_default();
    unsafe {
        for h in list {
            let hwnd = HWND(h as *mut core::ffi::c_void);
            if !hwnd.is_invalid() {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

pub fn count() -> usize {
    PINS.lock().map(|g| g.len()).unwrap_or(0)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    if msg == WM_OCR_DONE {
        let p = lparam.0 as *mut Result<String, String>;
        if !p.is_null() {
            let r = Box::from_raw(p);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pin;
            if !ptr.is_null() {
                (*ptr).ocr_busy = false;
                match *r {
                    Ok(t) => (*ptr).full_text = Some(t),
                    Err(_) => {}
                }
            }
        }
        return LRESULT(0);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pin;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let pin = &mut *ptr;
    pin.hwnd = hwnd;
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let client = Rect::from_ltrb(rc.left, rc.top, rc.right, rc.bottom);
            pin.bmp.blit_to_hdc(hdc, client);
            let br = CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::ACCENT.colorref()));
            let _ = FrameRect(hdc, &rc, br);
            let _ = DeleteObject(br);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let p = lp(lparam);
            pin.drag = true;
            pin.drag_off = p;
            crate::native::capture(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if pin.drag {
                let cur = crate::native::cursor_pos();
                let x = cur.x - pin.drag_off.x;
                let y = cur.y - pin.drag_off.y;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    SWP_NOZORDER,
                );
            }
            cursor_sizeall();
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            pin.drag = false;
            crate::native::release_capture();
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            crate::clipboard::set_image(&pin.bmp);
            crate::toast::show("已复制图片", "可直接粘贴");
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 as u32) >> 16) as i16;
            let shift = crate::native::shift_down();
            if shift {
                let next = if delta > 0 {
                    pin.opacity.saturating_add(20)
                } else {
                    pin.opacity.saturating_sub(20)
                };
                pin.opacity = next.clamp(51, 255);
                let _ = SetLayeredWindowAttributes(
                    hwnd,
                    windows::Win32::Foundation::COLORREF(0),
                    pin.opacity,
                    LWA_ALPHA,
                );
            } else {
                let factor = if delta > 0 { 1.12f32 } else { 1.0 / 1.12 };
                let old = pin.zoom;
                pin.zoom = (pin.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                if (pin.zoom - old).abs() > f32::EPSILON {
                    let w = ((pin.bmp.width as f32 * pin.zoom).round() as i32).max(24);
                    let h = ((pin.bmp.height as f32 * pin.zoom).round() as i32).max(24);
                    let mut wr = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut wr);
                    let old_w = (wr.right - wr.left).max(1);
                    let old_h = (wr.bottom - wr.top).max(1);
                    let cur = crate::native::cursor_pos();
                    let fx = (cur.x - wr.left) as f32 / old_w as f32;
                    let fy = (cur.y - wr.top) as f32 / old_h as f32;
                    let nx = cur.x - (fx * w as f32).round() as i32;
                    let ny = cur.y - (fy * h as f32).round() as i32;
                    let _ = SetWindowPos(hwnd, HWND_TOPMOST, nx, ny, w, h, SWP_NOZORDER);
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }
        WM_CONTEXTMENU => {
            let cmd = popup(hwnd, pin);
            match cmd {
                1 => copy_all_text(pin),
                2 => {
                    crate::clipboard::set_image(&pin.bmp);
                    crate::toast::show("已复制图片", "");
                }
                3 => {
                    if let Ok(p) = crate::bitmap::try_save_png(&pin.bmp, &Settings::load()) {
                        crate::toast::show("已保存", &p.display().to_string());
                    }
                }
                4 => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wparam.0 as u32;
            if vk == 0x1B {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Ok(mut g) = PINS.lock() {
                g.retain(|h| *h != hwnd.0 as isize);
            }
            drop(Box::from_raw(ptr));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn copy_all_text(pin: &Pin) {
    if let Some(t) = &pin.full_text {
        if crate::clipboard::set_text(t) {
            crate::toast::show("已复制文字", t);
        } else {
            crate::toast::show("复制文字失败", "剪贴板正被占用，请再试一次");
        }
        return;
    }
    if pin.ocr_busy {
        crate::toast::show("正在识别文字", "稍后再右键复制");
        return;
    }
    crate::toast::show("无法识别文字", "请先在设置中填写 DeepSeek API Key");
}

fn popup(hwnd: HWND, pin: &Pin) -> i32 {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let mut flags = MF_BYPOSITION | MF_STRING;
        if pin.full_text.is_none() {
            flags |= MF_GRAYED;
        }
        let _ = InsertMenuW(menu, 0, flags, 1, windows::core::w!("复制全部文字"));
        let _ = InsertMenuW(menu, 1, MF_BYPOSITION | MF_STRING, 2, windows::core::w!("复制图片"));
        let _ = InsertMenuW(menu, 2, MF_BYPOSITION | MF_STRING, 3, windows::core::w!("保存图片"));
        let _ = InsertMenuW(menu, 3, MF_BYPOSITION | MF_STRING, 4, windows::core::w!("关闭"));
        let cur = crate::native::cursor_pos();
        TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN, cur.x, cur.y, 0, hwnd, None).0 as i32
    }
}

fn lp(lparam: LPARAM) -> Point {
    let v = lparam.0 as u32;
    Point::new((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}
