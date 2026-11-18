#![allow(dead_code)]

use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

pub const CURSOR_ARROW: i32 = 0;
pub const CURSOR_HAND: i32 = 1;
pub const CURSOR_CROSS: i32 = 2;
pub const CURSOR_SIZEALL: i32 = 3;
pub const CURSOR_SIZENS: i32 = 4;
pub const CURSOR_SIZEWE: i32 = 5;
pub const CURSOR_SIZENWSE: i32 = 6;
pub const CURSOR_SIZENESW: i32 = 7;
pub const CURSOR_IBEAM: i32 = 8;

pub const WIN_OVERLAY: i32 = 0;
pub const WIN_FLOATING: i32 = 1;
pub const WIN_POPUP: i32 = 2;
pub const WIN_TOAST: i32 = 3;
pub const WIN_CLICKTHROUGH: i32 = 4;
pub const WIN_DIALOG: i32 = 5;

#[repr(C)]
pub struct TSWindowInfo {
    pub window_id: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub title: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TSCanvasCallbacks {
    pub on_paint: Option<extern "C" fn(*mut c_void, i32, i32, *mut c_void)>,
    pub on_mouse_move: Option<extern "C" fn(i32, i32, *mut c_void)>,
    pub on_mouse_leave: Option<extern "C" fn(*mut c_void)>,
    pub on_mouse_down: Option<extern "C" fn(i32, i32, i32, i32, *mut c_void)>,
    pub on_mouse_up: Option<extern "C" fn(i32, i32, i32, *mut c_void)>,
    pub on_scroll: Option<extern "C" fn(f64, i32, i32, u32, *mut c_void)>,
    pub on_key_down: Option<extern "C" fn(u32, u32, *mut c_void)>,
    pub on_resign_key: Option<extern "C" fn(*mut c_void)>,
    pub on_timer: Option<extern "C" fn(*mut c_void)>,
    pub on_close: Option<extern "C" fn(*mut c_void)>,
    pub on_text_commit: Option<extern "C" fn(*const c_char, bool, *mut c_void)>,
}

#[repr(C)]
pub struct TSSettingsInit {
    pub save_dir: *const c_char,
    pub quote_path: bool,
    pub post_action_idx: i32,
    pub start_at_login: bool,
    pub capture_hotkey_text: *const c_char,
    pub translate_hotkey_text: *const c_char,
    pub api_key: *const c_char,
    pub model: *const c_char,
    pub config_dir: *const c_char,
    pub has_screen_perm: bool,
}

#[repr(C)]
pub struct TSSettingsValues {
    pub save_dir: *mut c_char,
    pub quote_path: bool,
    pub post_action_idx: i32,
    pub start_at_login: bool,
    pub api_key: *mut c_char,
    pub model: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TSSettingsCallbacks {
    pub on_browse: Option<extern "C" fn(*mut c_void)>,
    pub on_open_dir: Option<extern "C" fn(*mut c_void)>,
    pub on_open_config: Option<extern "C" fn(*mut c_void)>,
    pub on_record_hotkey: Option<extern "C" fn(i32, *mut c_void)>,
    pub on_reset_hotkey: Option<extern "C" fn(i32, *mut c_void)>,
    pub on_hotkey_input: Option<extern "C" fn(u32, u32, *mut c_void)>,
    pub on_test_api: Option<extern "C" fn(*mut c_void)>,
    pub on_open_perm: Option<extern "C" fn(*mut c_void)>,
    pub on_save: Option<extern "C" fn(*mut c_void)>,
    pub on_cancel: Option<extern "C" fn(*mut c_void)>,
    pub on_close: Option<extern "C" fn(*mut c_void)>,
}

extern "C" {
    pub fn ts_app_init();
    pub fn ts_app_run();
    pub fn ts_app_quit();
    pub fn ts_pump_events(timeout_ms: u32);
    pub fn ts_dispatch_main(cb: extern "C" fn(*mut c_void), user_data: *mut c_void);
    pub fn ts_sleep_ms(ms: u32);

    pub fn ts_tray_init(
        bgra: *const u32,
        w: i32,
        h: i32,
        tooltip: *const c_char,
        on_left_click: extern "C" fn(*mut c_void),
        on_menu_action: extern "C" fn(i32, *mut c_void),
        user_data: *mut c_void,
    );
    pub fn ts_tray_set_tooltip(tooltip: *const c_char);
    pub fn ts_tray_set_pin_count(pin_count: i32);
    pub fn ts_tray_anchor(out_x: *mut i32, out_y: *mut i32) -> bool;

    pub fn ts_backing_scale() -> f32;
    pub fn ts_virtual_screen(ox: *mut i32, oy: *mut i32, ow: *mut i32, oh: *mut i32);
    pub fn ts_monitor_at(
        px: i32,
        py: i32,
        work_area: bool,
        ox: *mut i32,
        oy: *mut i32,
        ow: *mut i32,
        oh: *mut i32,
    );
    pub fn ts_cursor_pos(ox: *mut i32, oy: *mut i32);
    pub fn ts_set_cursor(kind: i32);
    pub fn ts_modifiers() -> u32;
    pub fn ts_key_down_vk(vk: i32) -> bool;
    pub fn ts_frontmost_pid() -> i32;
    pub fn ts_activate_pid(pid: i32);
    pub fn ts_has_screen_permission() -> bool;
    pub fn ts_request_screen_permission() -> bool;
    pub fn ts_open_screen_permission_settings();

    pub fn ts_clipboard_get_text() -> *mut c_char;
    pub fn ts_free_string(s: *mut c_char);
    pub fn ts_clipboard_set_text(text: *const c_char) -> bool;
    pub fn ts_clipboard_set_png(png_bytes: *const u8, len: usize) -> bool;

    pub fn ts_enum_windows(out_list: *mut *mut TSWindowInfo) -> i32;
    pub fn ts_free_window_list(list: *mut TSWindowInfo, count: i32);
    pub fn ts_capture_rect(x: i32, y: i32, w: i32, h: i32, out_bgra: *mut u32) -> i32;

    pub fn ts_register_hotkey(
        hotkey_id: i32,
        mods: u32,
        vk: u32,
        cb: extern "C" fn(i32, *mut c_void),
        user_data: *mut c_void,
    ) -> bool;
    pub fn ts_unregister_hotkey(hotkey_id: i32);

    pub fn ts_measure_text(text: *const c_char, px: i32, bold: bool, out_w: *mut i32, out_h: *mut i32);
    pub fn ts_measure_wrapped_text(text: *const c_char, max_w: i32, px: i32, bold: bool) -> i32;
    pub fn ts_draw_text_bitmap(
        bgra: *mut u32,
        bw: i32,
        bh: i32,
        x: i32,
        y: i32,
        text: *const c_char,
        argb: u32,
        px: i32,
        bold: bool,
    );

    pub fn ts_canvas_create(
        kind: i32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        corner_radius: f64,
        has_shadow: bool,
        cbs: *const TSCanvasCallbacks,
        user_data: *mut c_void,
    ) -> *mut c_void;
    pub fn ts_canvas_show(win: *mut c_void, activate: bool);
    pub fn ts_canvas_set_frame(win: *mut c_void, x: i32, y: i32, w: i32, h: i32);
    pub fn ts_canvas_set_opacity(win: *mut c_void, alpha: f64);
    pub fn ts_canvas_invalidate(win: *mut c_void);
    pub fn ts_canvas_invalidate_rect(win: *mut c_void, x: i32, y: i32, w: i32, h: i32);
    pub fn ts_canvas_start_timer(win: *mut c_void, interval_ms: u32);
    pub fn ts_canvas_stop_timer(win: *mut c_void);
    pub fn ts_canvas_close(win: *mut c_void);
    pub fn ts_canvas_begin_edit(
        win: *mut c_void,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        font_px: i32,
        text_argb: u32,
        bg_argb: u32,
    );
    pub fn ts_canvas_end_edit(win: *mut c_void) -> *mut c_char;
    pub fn ts_canvas_has_edit(win: *mut c_void) -> bool;
    pub fn ts_popup_context_menu(
        win: *mut c_void,
        items: *const *const c_char,
        count: i32,
    ) -> i32;

    pub fn ts_image_create(bgra: *const u32, w: i32, h: i32) -> *mut c_void;
    pub fn ts_image_free(img: *mut c_void);
    pub fn ts_ctx_draw_image(
        ctx: *mut c_void,
        img: *mut c_void,
        dx: i32,
        dy: i32,
        dw: i32,
        dh: i32,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        full_w: i32,
        full_h: i32,
    );
    pub fn ts_ctx_draw_bgra(
        ctx: *mut c_void,
        bgra: *const u32,
        w: i32,
        h: i32,
        dx: i32,
        dy: i32,
        dw: i32,
        dh: i32,
    );
    pub fn ts_ctx_fill_rect(ctx: *mut c_void, x: i32, y: i32, w: i32, h: i32, argb: u32);
    pub fn ts_ctx_stroke_rect(
        ctx: *mut c_void,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        argb: u32,
        width: i32,
    );
    pub fn ts_ctx_round_rect(
        ctx: *mut c_void,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        fill_argb: u32,
        stroke_argb: u32,
        stroke_w: i32,
    );
    pub fn ts_ctx_fill_ellipse(ctx: *mut c_void, x: i32, y: i32, w: i32, h: i32, argb: u32);
    pub fn ts_ctx_stroke_ellipse(
        ctx: *mut c_void,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        argb: u32,
        width: i32,
    );
    pub fn ts_ctx_line(
        ctx: *mut c_void,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        argb: u32,
        width: f64,
    );
    pub fn ts_ctx_polyline(
        ctx: *mut c_void,
        xy_pairs: *const i32,
        count: i32,
        argb: u32,
        width: f64,
    );
    pub fn ts_ctx_polygon_fill(
        ctx: *mut c_void,
        xy_pairs: *const f64,
        count: i32,
        argb: u32,
    );
    pub fn ts_ctx_draw_text(
        ctx: *mut c_void,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        text: *const c_char,
        argb: u32,
        px: i32,
        bold: bool,
        align: i32,
        wrap: bool,
    );

    pub fn ts_pick_folder(title: *const c_char, initial_dir: *const c_char) -> *mut c_char;
    pub fn ts_confirm_dialog(title: *const c_char, message: *const c_char) -> bool;

    pub fn ts_trans_create(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        on_copy: extern "C" fn(*mut c_void),
        on_retry: extern "C" fn(*mut c_void),
        on_close: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> *mut c_void;
    pub fn ts_trans_update(
        win: *mut c_void,
        kind: i32,
        view_state: i32,
        source_text: *const c_char,
        img_bgra: *const u32,
        img_w: i32,
        img_h: i32,
        result_text: *const c_char,
        error_text: *const c_char,
        copied: bool,
    );
    pub fn ts_trans_show(win: *mut c_void);
    pub fn ts_trans_close(win: *mut c_void);

    pub fn ts_settings_create(
        init: *const TSSettingsInit,
        cbs: *const TSSettingsCallbacks,
        user_data: *mut c_void,
    ) -> *mut c_void;
    pub fn ts_settings_set_status(win: *mut c_void, status: *const c_char, kind: i32);
    pub fn ts_settings_set_api_testing(
        win: *mut c_void,
        testing: bool,
        status: *const c_char,
        kind: i32,
    );
    pub fn ts_settings_set_hotkey_display(
        win: *mut c_void,
        target: i32,
        text: *const c_char,
        recording: bool,
        any_recording: bool,
    );
    pub fn ts_settings_set_dir(win: *mut c_void, path: *const c_char);
    pub fn ts_settings_get_values(win: *mut c_void, out: *mut TSSettingsValues);
    pub fn ts_settings_free_values(v: *mut TSSettingsValues);
    pub fn ts_settings_close(win: *mut c_void);
}

pub fn to_cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_else(|_| CString::new("").unwrap())
}

pub unsafe fn take_cstr(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
    ts_free_string(ptr);
    Some(s)
}

pub fn capture_screen_rect(rect: Rect) -> Result<Bitmap, String> {
    if rect.w <= 0 || rect.h <= 0 {
        return Err("截取区域无效。".into());
    }
    let mut bmp = Bitmap::new(rect.w, rect.h);
    let rc = unsafe {
        ts_capture_rect(rect.x, rect.y, rect.w, rect.h, bmp.pixels.as_mut_ptr())
    };
    if rc == 0 {
        Ok(bmp)
    } else if rc == -2 {
        Err("缺少屏幕录制权限，请在「系统设置 → 隐私与安全性 → 屏幕录制」中允许 TermShot".into())
    } else {
        Err("屏幕捕获失败".into())
    }
}

pub fn measure_text(text: &str, px: i32, bold: bool) -> (i32, i32) {
    if text.is_empty() {
        return (0, px);
    }
    let c = to_cstr(text);
    let mut w = 0i32;
    let mut h = 0i32;
    unsafe {
        ts_measure_text(c.as_ptr(), px.max(1), bold, &mut w, &mut h);
    }
    (w.max(1), h.max(px))
}

pub fn measure_wrapped_text(text: &str, max_w: i32, px: i32, bold: bool) -> i32 {
    if text.trim().is_empty() {
        return 0;
    }
    let c = to_cstr(text);
    unsafe { ts_measure_wrapped_text(c.as_ptr(), max_w.max(1), px.max(1), bold) }
}

pub fn draw_text_to_bitmap(
    bmp: &mut Bitmap,
    at: Point,
    text: &str,
    color: Color,
    px: i32,
    bold: bool,
) {
    if text.trim().is_empty() || bmp.width <= 0 || bmp.height <= 0 {
        return;
    }
    let c = to_cstr(text);
    unsafe {
        ts_draw_text_bitmap(
            bmp.pixels.as_mut_ptr(),
            bmp.width,
            bmp.height,
            at.x,
            at.y,
            c.as_ptr(),
            color.bgra(),
            px.max(1),
            bold,
        );
    }
}

pub struct CachedImage {
    ptr: *mut c_void,
    pub w: i32,
    pub h: i32,
}

impl CachedImage {
    pub fn from_bitmap(bmp: &Bitmap) -> Option<Self> {
        if bmp.width <= 0 || bmp.height <= 0 || bmp.pixels.is_empty() {
            return None;
        }
        let expected = (bmp.width as usize).saturating_mul(bmp.height as usize);
        if bmp.pixels.len() < expected {
            return None;
        }
        let ptr = unsafe { ts_image_create(bmp.pixels.as_ptr(), bmp.width, bmp.height) };
        if ptr.is_null() {
            None
        } else {
            Some(Self {
                ptr,
                w: bmp.width,
                h: bmp.height,
            })
        }
    }

    pub fn blt(&self, ctx: *mut c_void, dest: Rect, src: Rect) {
        let src = src.intersect(Rect::new(0, 0, self.w, self.h));
        if src.w < 1 || src.h < 1 || dest.w < 1 || dest.h < 1 {
            return;
        }
        unsafe {
            ts_ctx_draw_image(
                ctx,
                self.ptr,
                dest.x,
                dest.y,
                dest.w,
                dest.h,
                src.x,
                src.y,
                src.w,
                src.h,
                self.w,
                self.h,
            );
        }
    }
}

impl Drop for CachedImage {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                ts_image_free(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

pub fn dispatch_main_fn<F: FnOnce() + Send + 'static>(f: F) {
    extern "C" fn trampoline<F: FnOnce() + Send + 'static>(ptr: *mut c_void) {
        let boxed = unsafe { Box::from_raw(ptr as *mut F) };
        boxed();
    }
    let raw = Box::into_raw(Box::new(f)) as *mut c_void;
    unsafe {
        ts_dispatch_main(trampoline::<F>, raw);
    }
}
