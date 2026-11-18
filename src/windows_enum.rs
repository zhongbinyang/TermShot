use crate::geom::Point;

#[cfg(target_os = "windows")]
mod imp {
    use crate::geom::{Point, Rect};
    use crate::native::{dwm_bounds, is_cloaked, is_iconic, is_visible, monitor_from_point};
    use crate::util::from_wide;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowTextW};

    #[derive(Clone)]
    pub struct WindowInfo {
        #[allow(dead_code)]
        pub hwnd: HWND,
        pub bounds: Rect,
        #[allow(dead_code)]
        pub title: String,
    }

    const SKIP: &[&str] = &[
        "Progman",
        "WorkerW",
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "NotifyIconOverflowWindow",
        "Xaml_WindowedPopupClass",
        "ForegroundStaging",
    ];

    struct Collect {
        exclude: HWND,
        list: Vec<WindowInfo>,
    }

    pub fn snapshot(exclude: HWND) -> Vec<WindowInfo> {
        let mut state = Collect {
            exclude,
            list: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut Collect as isize));
        }
        state.list
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut Collect);
        if hwnd == state.exclude {
            return BOOL(1);
        }
        if !is_visible(hwnd) || is_iconic(hwnd) || is_cloaked(hwnd) {
            return BOOL(1);
        }
        let cls = class_name(hwnd);
        if SKIP.iter().any(|s| *s == cls) {
            return BOOL(1);
        }
        if let Some(bounds) = dwm_bounds(hwnd) {
            if bounds.w >= 8 && bounds.h >= 8 {
                state.list.push(WindowInfo {
                    hwnd,
                    bounds,
                    title: window_title(hwnd),
                });
            }
        }
        BOOL(1)
    }

    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        unsafe {
            GetClassNameW(hwnd, &mut buf);
        }
        from_wide(&buf)
    }

    fn window_title(hwnd: HWND) -> String {
        let mut buf = [0u16; 512];
        unsafe {
            GetWindowTextW(hwnd, &mut buf);
        }
        from_wide(&buf)
    }

    #[allow(dead_code)]
    pub fn monitor_at(p: Point) -> Rect {
        monitor_from_point(p)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::geom::{Point, Rect};
    use crate::macos::ffi;
    use std::ffi::CStr;

    #[derive(Clone)]
    pub struct WindowInfo {
        #[allow(dead_code)]
        pub window_id: u32,
        pub bounds: Rect,
        #[allow(dead_code)]
        pub title: String,
    }

    pub fn snapshot() -> Vec<WindowInfo> {
        let mut list_ptr: *mut ffi::TSWindowInfo = std::ptr::null_mut();
        let count = unsafe { ffi::ts_enum_windows(&mut list_ptr) };
        if count <= 0 || list_ptr.is_null() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(count as usize);
        unsafe {
            let slice = std::slice::from_raw_parts(list_ptr, count as usize);
            for item in slice {
                let title = if item.title.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(item.title).to_string_lossy().into_owned()
                };
                out.push(WindowInfo {
                    window_id: item.window_id,
                    bounds: Rect::new(item.x, item.y, item.w, item.h),
                    title,
                });
            }
            ffi::ts_free_window_list(list_ptr, count);
        }
        out
    }

    #[allow(dead_code)]
    pub fn monitor_at(p: Point) -> Rect {
        crate::native::monitor_from_point(p)
    }
}

pub use imp::*;

pub fn hit_test(windows: &[WindowInfo], p: Point) -> Option<&WindowInfo> {
    windows.iter().find(|w| w.bounds.contains(p))
}
