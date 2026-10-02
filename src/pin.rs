use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use crate::native::{cursor_hand, cursor_sizeall, dpi_scale_hwnd, fill_rect_hdc, hinstance, sc, stroke_rect_hdc, work_area_from_point};
use crate::settings::Settings;
use crate::theme;
use crate::util::wide;
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    GetWindowRect, InsertMenuW, RegisterClassExW, SetLayeredWindowAttributes,
    SetTimer, KillTimer, SetWindowLongPtrW, SetWindowPos, TrackPopupMenu, CS_DBLCLKS, GWLP_USERDATA, HWND_TOPMOST,
    LWA_ALPHA, MF_BYPOSITION, MF_STRING, SWP_NOZORDER, TPM_LEFTALIGN, TPM_RETURNCMD,
    WM_CONTEXTMENU, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WS_VISIBLE,
};

const MIN_ZOOM: f32 = 0.08;
const MAX_ZOOM: f32 = 8.0;
const WM_MOUSELEAVE: u32 = 0x02A3;
const HUD_TIMER: usize = 1;

pub struct Pin {
    hwnd: HWND,
    bmp: Bitmap,
    zoom: f32,
    opacity: u8,
    drag: bool,
    drag_off: Point,
    hovering: bool,
    toolbar_hover: i32,
    hud_text: String,
    hud_until: Option<std::time::Instant>,
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
    let _ = settings;
    let state = Box::new(Pin {
        hwnd: HWND::default(),
        bmp,
        zoom,
        opacity: 255,
        drag: false,
        drag_off: Point::default(),
        hovering: false,
        toolbar_hover: -1,
        hud_text: String::new(),
        hud_until: None,
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
        hwnd
    }
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
            stroke_rect_hdc(hdc, client, theme::ACCENT, 1);
            if pin.hovering {
                paint_toolbar(hdc, pin, client, dpi_scale_hwnd(hwnd).max(1.0));
            }
            if !pin.hud_text.is_empty() {
                paint_hud(hdc, pin, client, dpi_scale_hwnd(hwnd).max(1.0));
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let p = lp(lparam);
            let hit = toolbar_hit(hwnd, p);
            if pin.hovering && hit >= 0 {
                activate_toolbar(hwnd, pin, hit);
                return LRESULT(0);
            }
            pin.drag = true;
            pin.drag_off = p;
            crate::native::capture(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            pin.hovering = true;
            pin.toolbar_hover = toolbar_hit(hwnd, lp(lparam));
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut track);
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
            let _ = InvalidateRect(hwnd, None, false);
            cursor_sizeall();
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if !pin.drag {
                pin.hovering = false;
                pin.toolbar_hover = -1;
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = crate::native::cursor_pos();
            let mut point = windows::Win32::Foundation::POINT {
                x: cursor.x,
                y: cursor.y,
            };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point);
            if pin.hovering && toolbar_hit(hwnd, Point::new(point.x, point.y)) >= 0 {
                cursor_hand();
            } else {
                cursor_sizeall();
            }
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            pin.drag = false;
            crate::native::release_capture();
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            if pin.hovering && toolbar_hit(hwnd, lp(lparam)) >= 0 {
                return LRESULT(0);
            }
            crate::clipboard::set_image(&pin.bmp);
            crate::toast::show_success("已复制图片", "可直接粘贴");
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
                pin.hud_text = format!("透明度 {}%", (pin.opacity as u32 * 100) / 255);
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
                pin.hud_text = format!("{}%", (pin.zoom * 100.0).round() as i32);
            }
            pin.hud_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(900));
            let _ = SetTimer(hwnd, HUD_TIMER, 50, None);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_CONTEXTMENU => {
            let cmd = popup(hwnd, pin);
            match cmd {
                1 => {
                    let _ = crate::app::request_image_ai(crate::app::ImageAiKind::Ocr, pin.bmp.clone());
                }
                2 => {
                    let _ = crate::app::request_image_ai(crate::app::ImageAiKind::Translate, pin.bmp.clone());
                }
                3 => {
                    crate::clipboard::set_image(&pin.bmp);
                    crate::toast::show_success("已复制图片", "可直接粘贴到其他应用");
                }
                4 => {
                    if let Ok(p) = crate::bitmap::try_save_png(&pin.bmp, &Settings::load()) {
                        crate::toast::show_success("已保存", &p.display().to_string());
                    }
                }
                5 => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == HUD_TIMER
                && pin
                    .hud_until
                    .is_some_and(|until| std::time::Instant::now() >= until)
            {
                pin.hud_text.clear();
                pin.hud_until = None;
                let _ = KillTimer(hwnd, HUD_TIMER);
                let _ = InvalidateRect(hwnd, None, false);
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
            let _ = KillTimer(hwnd, HUD_TIMER);
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

fn popup(hwnd: HWND, pin: &Pin) -> i32 {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let _ = pin;
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, 1, windows::core::w!("识别文字…"));
        let _ = InsertMenuW(menu, 1, MF_BYPOSITION | MF_STRING, 2, windows::core::w!("翻译截图…"));
        let _ = InsertMenuW(menu, 2, MF_BYPOSITION | MF_STRING, 3, windows::core::w!("复制图片"));
        let _ = InsertMenuW(menu, 3, MF_BYPOSITION | MF_STRING, 4, windows::core::w!("保存图片"));
        let _ = InsertMenuW(menu, 4, MF_BYPOSITION | MF_STRING, 5, windows::core::w!("关闭贴图"));
        let cur = crate::native::cursor_pos();
        TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN, cur.x, cur.y, 0, hwnd, None).0 as i32
    }
}

unsafe fn activate_toolbar(hwnd: HWND, pin: &mut Pin, index: i32) {
    match index {
        0 => {
            if crate::clipboard::set_image(&pin.bmp) {
                crate::toast::show_success("已复制图片", "可直接粘贴到其他应用");
            } else {
                crate::toast::show_error("复制失败", "剪贴板正被占用，请稍后重试");
            }
        }
        1 => {
            let _ = crate::app::request_image_ai(crate::app::ImageAiKind::Ocr, pin.bmp.clone());
        }
        2 => match crate::bitmap::try_save_png(&pin.bmp, &Settings::load()) {
            Ok(path) => crate::toast::show_success("已保存", &path.display().to_string()),
            Err(error) => crate::toast::show_error("保存失败", &error),
        },
        3 => {
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe fn toolbar_rects(hwnd: HWND) -> [Rect; 4] {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let scale = dpi_scale_hwnd(hwnd).max(1.0);
    let gap = sc(3, scale);
    let margin = sc(6, scale);
    let available = (rc.right - rc.left - margin * 2 - gap * 3).max(4);
    let width = sc(48, scale).min((available / 4).max(sc(24, scale)));
    let height = sc(28, scale).min((rc.bottom - rc.top - margin * 2).max(sc(20, scale)));
    let total = width * 4 + gap * 3;
    let start = (rc.right - margin - total).max(margin);
    [
        Rect::new(start, margin, width, height),
        Rect::new(start + width + gap, margin, width, height),
        Rect::new(start + (width + gap) * 2, margin, width, height),
        Rect::new(start + (width + gap) * 3, margin, width, height),
    ]
}

unsafe fn toolbar_hit(hwnd: HWND, point: Point) -> i32 {
    toolbar_rects(hwnd)
        .iter()
        .position(|rect| rect.contains(point))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

unsafe fn paint_toolbar(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    pin: &Pin,
    client: Rect,
    scale: f32,
) {
    let rects = toolbar_rects(pin.hwnd);
    if rects[0].w <= 0 || client.h < sc(32, scale) {
        return;
    }
    let labels = if rects[0].w >= sc(40, scale) {
        ["复制", "OCR", "保存", "关闭"]
    } else {
        ["C", "O", "S", "×"]
    };
    let container = Rect::from_ltrb(
        rects[0].x - sc(4, scale),
        rects[0].y - sc(4, scale),
        rects[3].right() + sc(4, scale),
        rects[3].bottom() + sc(4, scale),
    );
    fill_rect_hdc(hdc, container, Color::argb(235, 12, 17, 24));
    stroke_rect_hdc(hdc, container, theme::BORDER, 1);
    for (index, rect) in rects.iter().enumerate() {
        if pin.toolbar_hover == index as i32 {
            fill_rect_hdc(
                hdc,
                *rect,
                if index == 3 { theme::ERROR_BG } else { theme::ACCENT_DARK },
            );
        }
        let color = if index == 3 && pin.toolbar_hover == index as i32 {
            theme::DANGER
        } else if pin.toolbar_hover == index as i32 {
            theme::ACCENT_HI
        } else {
            theme::TEXT
        };
        let (tw, th) = crate::draw::measure_text(labels[index], sc(10, scale));
        crate::draw::draw_text_hdc(
            hdc,
            Point::new(rect.x + (rect.w - tw) / 2, rect.y + (rect.h - th) / 2),
            labels[index],
            color,
            sc(10, scale),
        );
    }
}

unsafe fn paint_hud(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    pin: &Pin,
    client: Rect,
    scale: f32,
) {
    let (tw, th) = crate::draw::measure_text(&pin.hud_text, sc(12, scale));
    let width = tw + sc(24, scale);
    let height = th + sc(12, scale);
    let rect = Rect::new(
        client.x + (client.w - width) / 2,
        client.y + (client.h - height) / 2,
        width,
        height,
    );
    fill_rect_hdc(hdc, rect, Color::argb(230, 12, 17, 24));
    stroke_rect_hdc(hdc, rect, theme::ACCENT, 1);
    crate::draw::draw_text_hdc(
        hdc,
        Point::new(rect.x + sc(12, scale), rect.y + sc(6, scale)),
        &pin.hud_text,
        theme::TEXT,
        sc(12, scale),
    );
}

fn lp(lparam: LPARAM) -> Point {
    let v = lparam.0 as u32;
    Point::new((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}
