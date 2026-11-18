use crate::geom::{Color, Rect};
use crate::macos::ffi::{self, TSCanvasCallbacks};
use crate::macos::native::{
    cursor_hand, cursor_pos, dpi_scale_at, draw_text_canvas, round_rect_canvas, sc,
    work_area_from_point,
};
use crate::theme;
use std::cell::RefCell;
use std::os::raw::c_void;

#[derive(Clone, Copy)]
enum ToastKind {
    Info,
    Success,
    Error,
}

struct ToastState {
    win: *mut c_void,
    kind: ToastKind,
    title: String,
    text: String,
    remaining: u32,
    w: i32,
    h: i32,
    scale: f32,
    hovering: bool,
    closed: bool,
}

thread_local! {
    static ACTIVE_TOASTS: RefCell<Vec<*mut ToastState>> = const { RefCell::new(Vec::new()) };
}

pub fn show(title: &str, text: &str) {
    show_kind(ToastKind::Info, title, text);
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

fn show_kind(kind: ToastKind, title: &str, text: &str) {
    let is_main = unsafe { libc::pthread_main_np() != 0 };
    let title_s = title.to_string();
    let text_s = text.to_string();
    if is_main {
        show_now(kind, &title_s, &text_s);
    } else {
        ffi::dispatch_main_fn(move || {
            show_now(kind, &title_s, &text_s);
        });
    }
}

pub fn close() {
    let list: Vec<*mut ToastState> =
        ACTIVE_TOASTS.with(|t| t.borrow_mut().drain(..).collect());
    for raw in list {
        dismiss_raw(raw, false);
    }
}

fn show_now(kind: ToastKind, title: &str, text: &str) {
    let cur_pt = cursor_pos();
    let work = work_area_from_point(cur_pt);
    let scale = dpi_scale_at(cur_pt).max(1.0);
    let (w, h, duration) = crate::toast::compute_toast_dims(text, scale);

    let oldest = ACTIVE_TOASTS.with(|t| {
        let b = t.borrow();
        (b.len() >= 3).then(|| b[0])
    });
    if let Some(old) = oldest {
        dismiss_raw(old, true);
    }

    let stack_h: i32 = ACTIVE_TOASTS.with(|t| {
        t.borrow()
            .iter()
            .map(|&p| unsafe { (*p).h + sc(8, scale) })
            .sum()
    });
    let x = work.right() - w - sc(16, scale);
    let y = work.y + sc(16, scale) + stack_h;

    let state = Box::new(ToastState {
        win: std::ptr::null_mut(),
        kind,
        title: title.to_string(),
        text: text.chars().take(600).collect(),
        remaining: if matches!(kind, ToastKind::Error) {
            duration.max(4500)
        } else {
            duration
        },
        w,
        h,
        scale,
        hovering: false,
        closed: false,
    });
    let raw = Box::into_raw(state);

    let cbs = TSCanvasCallbacks {
        on_paint: Some(on_paint),
        on_mouse_move: Some(on_mouse_move),
        on_mouse_leave: Some(on_mouse_leave),
        on_mouse_down: Some(on_mouse_down),
        on_timer: Some(on_timer),
        on_close: Some(on_close),
        ..Default::default()
    };

    unsafe {
        let win = ffi::ts_canvas_create(
            ffi::WIN_TOAST,
            x,
            y,
            w,
            h,
            sc(8, scale) as f64,
            true,
            &cbs,
            raw as *mut c_void,
        );
        (*raw).win = win;
        ACTIVE_TOASTS.with(|t| t.borrow_mut().push(raw));
        ffi::ts_canvas_show(win, false);
        ffi::ts_canvas_start_timer(win, 50);
    }
}

fn dismiss_raw(raw: *mut ToastState, reflow: bool) {
    if raw.is_null() {
        return;
    }
    ACTIVE_TOASTS.with(|t| {
        t.borrow_mut().retain(|&p| p != raw);
    });
    unsafe {
        if (*raw).closed {
            return;
        }
        (*raw).closed = true;
        let win = (*raw).win;
        (*raw).win = std::ptr::null_mut();
        if !win.is_null() {
            ffi::ts_canvas_close(win);
        }
        drop(Box::from_raw(raw));
    }
    if reflow {
        reflow_toasts();
    }
}

fn reflow_toasts() {
    let cur = cursor_pos();
    let work = work_area_from_point(cur);
    let scale = dpi_scale_at(cur).max(1.0);
    let list: Vec<*mut ToastState> = ACTIVE_TOASTS.with(|t| t.borrow().clone());
    let mut y = work.y + sc(16, scale);
    for raw in list {
        unsafe {
            if (*raw).win.is_null() {
                continue;
            }
            let x = work.right() - (*raw).w - sc(16, (*raw).scale);
            ffi::ts_canvas_set_frame((*raw).win, x, y, (*raw).w, (*raw).h);
            y += (*raw).h + sc(8, scale);
        }
    }
}

extern "C" fn on_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let t = unsafe { &*(ud as *const ToastState) };
    let scale = t.scale.max(1.0);
    let (accent, background) = match t.kind {
        ToastKind::Info => (theme::INFO, theme::PANEL),
        ToastKind::Success => (theme::SUCCESS, theme::SUCCESS_BG),
        ToastKind::Error => (theme::DANGER, theme::ERROR_BG),
    };

    round_rect_canvas(
        ctx,
        Rect::new(0, 0, t.w, t.h),
        sc(8, scale),
        background,
        theme::BORDER,
        1,
    );

    let bar_x = sc(6, scale);
    let bar_y = sc(12, scale);
    let bar_w = sc(3, scale).max(2);
    let bar_h = (t.h - sc(24, scale)).max(sc(16, scale));
    round_rect_canvas(
        ctx,
        Rect::new(bar_x, bar_y, bar_w, bar_h),
        sc(2, scale),
        accent,
        Color::argb(0, 0, 0, 0),
        0,
    );

    let empty = t.text.trim().is_empty();
    let title_rect = if empty {
        Rect::new(sc(18, scale), 0, t.w - sc(32, scale), t.h)
    } else {
        Rect::new(
            sc(18, scale),
            sc(10, scale),
            t.w - sc(32, scale),
            sc(20, scale),
        )
    };
    draw_text_canvas(
        ctx,
        title_rect,
        &t.title,
        accent,
        sc(13, scale),
        true,
        0,
        false,
    );

    if !empty {
        draw_text_canvas(
            ctx,
            Rect::new(
                sc(18, scale),
                sc(34, scale),
                t.w - sc(32, scale),
                (t.h - sc(44, scale)).max(12),
            ),
            &t.text,
            theme::TEXT,
            sc(11, scale),
            false,
            0,
            true,
        );
    }
}

extern "C" fn on_mouse_move(_x: i32, _y: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    cursor_hand();
    unsafe {
        let t = &mut *(ud as *mut ToastState);
        t.hovering = true;
        t.remaining = 2500;
    }
}

extern "C" fn on_mouse_leave(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let t = &mut *(ud as *mut ToastState);
        t.hovering = false;
        t.remaining = 2000;
    }
}

extern "C" fn on_mouse_down(_x: i32, _y: i32, _btn: i32, _clicks: i32, ud: *mut c_void) {
    dismiss_raw(ud as *mut ToastState, true);
}

extern "C" fn on_timer(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let raw = ud as *mut ToastState;
    let should_close = unsafe {
        let t = &mut *raw;
        if t.hovering {
            t.remaining = 2500;
            false
        } else {
            t.remaining = t.remaining.saturating_sub(50);
            t.remaining == 0
        }
    };
    if should_close {
        dismiss_raw(raw, true);
    }
}

extern "C" fn on_close(ud: *mut c_void) {
    dismiss_raw(ud as *mut ToastState, true);
}
