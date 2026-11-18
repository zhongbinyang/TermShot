use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use crate::macos::ffi::{self, TSCanvasCallbacks};
use crate::macos::native::{
    cursor_arrow, cursor_hand, dpi_scale_at, draw_text_canvas, key_down, round_rect_canvas, sc,
    sleep_ms, stroke_rect_canvas, virtual_screen, VK_ESCAPE, VK_R, VK_RETURN,
};
use crate::scroll::{HudDecision, Stitcher, MAX_HEIGHT};
use crate::theme;
use std::os::raw::c_void;

struct ScrollHud {
    border_win: *mut c_void,
    bar_win: *mut c_void,
    border_rect: Rect,
    hole_local: Rect,
    bar_w: i32,
    bar_h: i32,
    done_btn: Rect,
    cancel_btn: Rect,
    scale: f32,
    height: i32,
    hover: i32, // -1 none, 0 done, 1 cancel
    decision: Option<HudDecision>,
}

pub fn run_scroll(region: Rect) -> Option<Bitmap> {
    let vs = virtual_screen();
    let region = region.intersect(vs);
    if region.w < 8 || region.h < 8 {
        return None;
    }

    let center = Point::new(region.x + region.w / 2, region.y + region.h / 2);
    let scale = dpi_scale_at(center).max(1.0);
    let border_w = sc(2, scale).max(2);
    let pad_out = border_w + sc(2, scale);
    let border_rect = Rect::new(
        region.x - pad_out,
        region.y - pad_out,
        region.w + pad_out * 2,
        region.h + pad_out * 2,
    );
    let hole_local = Rect::new(pad_out - border_w, pad_out - border_w, region.w + border_w * 2, region.h + border_w * 2);

    let bar_h = sc(42, scale);
    let pad = sc(10, scale);
    let btn_w = sc(84, scale);
    let bar_w = sc(420, scale);
    let mut bx = region.x + (region.w - bar_w) / 2;
    let mut by = region.bottom() + pad;
    if by + bar_h > vs.bottom() - 4 {
        by = region.y - bar_h - pad;
    }
    bx = bx.clamp(vs.x + 4, (vs.right() - bar_w - 4).max(vs.x + 4));
    by = by.clamp(vs.y + 4, (vs.bottom() - bar_h - 4).max(vs.y + 4));

    let btn_y = sc(6, scale);
    let btn_h = bar_h - sc(12, scale);
    let cancel_btn = Rect::new(bar_w - pad - btn_w, btn_y, btn_w, btn_h);
    let done_btn = Rect::new(cancel_btn.x - sc(8, scale) - btn_w, btn_y, btn_w, btn_h);

    let hud = Box::new(ScrollHud {
        border_win: std::ptr::null_mut(),
        bar_win: std::ptr::null_mut(),
        border_rect,
        hole_local,
        bar_w,
        bar_h,
        done_btn,
        cancel_btn,
        scale,
        height: region.h,
        hover: -1,
        decision: None,
    });
    let raw = Box::into_raw(hud);

    let border_cbs = TSCanvasCallbacks {
        on_paint: Some(on_border_paint),
        ..Default::default()
    };
    let bar_cbs = TSCanvasCallbacks {
        on_paint: Some(on_bar_paint),
        on_mouse_move: Some(on_bar_move),
        on_mouse_leave: Some(on_bar_leave),
        on_mouse_down: Some(on_bar_down),
        ..Default::default()
    };

    unsafe {
        let border_win = ffi::ts_canvas_create(
            ffi::WIN_CLICKTHROUGH,
            border_rect.x,
            border_rect.y,
            border_rect.w,
            border_rect.h,
            0.0,
            false,
            &border_cbs,
            raw as *mut c_void,
        );
        let bar_win = ffi::ts_canvas_create(
            ffi::WIN_TOAST,
            bx,
            by,
            bar_w,
            bar_h,
            sc(8, scale) as f64,
            true,
            &bar_cbs,
            raw as *mut c_void,
        );
        (*raw).border_win = border_win;
        (*raw).bar_win = bar_win;
        ffi::ts_canvas_show(border_win, false);
        ffi::ts_canvas_show(bar_win, false);
        ffi::ts_pump_events(20);
        sleep_ms(120);

        let first = match crate::bitmap::capture_rect(region) {
            Ok(b) => b,
            Err(e) => {
                crate::toast::show_error("滚动截图失败", &e);
                cleanup_hud(raw);
                return None;
            }
        };
        let mut session = Stitcher::new();
        session.begin(&first);
        (*raw).height = session.height();
        ffi::ts_canvas_invalidate(bar_win);

        while key_down(VK_RETURN as i32)
            || key_down(VK_ESCAPE as i32)
            || key_down(VK_R as i32)
        {
            sleep_ms(20);
            ffi::ts_pump_events(0);
        }

        let mut last_capture = std::time::Instant::now();
        loop {
            ffi::ts_pump_events(15);

            match (*raw).decision.take() {
                Some(HudDecision::Cancel) => {
                    cleanup_hud(raw);
                    return None;
                }
                Some(HudDecision::Finish) => break,
                None => {}
            }

            if key_down(VK_ESCAPE as i32) {
                cleanup_hud(raw);
                return None;
            }
            if key_down(VK_RETURN as i32) {
                break;
            }
            if session.height() >= MAX_HEIGHT {
                break;
            }

            if last_capture.elapsed() >= std::time::Duration::from_millis(50) {
                last_capture = std::time::Instant::now();
                if let Ok(frame) = crate::bitmap::capture_rect(region) {
                    if session.append(&frame) {
                        (*raw).height = session.height();
                        ffi::ts_canvas_invalidate(bar_win);
                    }
                }
            }
        }

        cleanup_hud(raw);
        Some(session.take())
    }
}

unsafe fn cleanup_hud(raw: *mut ScrollHud) {
    if raw.is_null() {
        return;
    }
    let bwin = (*raw).border_win;
    let bar = (*raw).bar_win;
    (*raw).border_win = std::ptr::null_mut();
    (*raw).bar_win = std::ptr::null_mut();
    if !bwin.is_null() {
        ffi::ts_canvas_close(bwin);
    }
    if !bar.is_null() {
        ffi::ts_canvas_close(bar);
    }
    cursor_arrow();
    drop(Box::from_raw(raw));
}

extern "C" fn on_border_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let hud = unsafe { &*(ud as *const ScrollHud) };
    let bw = sc(2, hud.scale).max(2);
    let _ = hud.border_rect;
    stroke_rect_canvas(
        ctx,
        hud.hole_local.inflate(1, 1),
        Color::argb(220, 8, 10, 14),
        bw + 1,
    );
    stroke_rect_canvas(ctx, hud.hole_local, theme::ACCENT, bw);
}

extern "C" fn on_bar_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let hud = unsafe { &*(ud as *const ScrollHud) };
    let s = hud.scale;
    round_rect_canvas(
        ctx,
        Rect::new(0, 0, hud.bar_w, hud.bar_h),
        sc(8, s),
        Color::rgb(20, 26, 36),
        Color::rgb(46, 58, 76),
        1,
    );

    let msg = if hud.height >= MAX_HEIGHT {
        format!("已达到高度上限  ·  {} px  ·  请完成", hud.height)
    } else {
        format!("已捕获 {} px  ·  滚轮继续  ·  Enter 完成", hud.height)
    };
    let text_w = (hud.done_btn.x - sc(24, s)).max(sc(100, s));
    draw_text_canvas(
        ctx,
        Rect::new(sc(14, s), 0, text_w, hud.bar_h),
        &msg,
        theme::TEXT,
        sc(13, s),
        false,
        0,
        false,
    );

    let (done_bg, done_fg) = if hud.hover == 0 {
        (theme::ACCENT_HI, Color::rgb(0x06, 0x22, 0x1B))
    } else {
        (theme::ACCENT, Color::rgb(0x06, 0x22, 0x1B))
    };
    round_rect_canvas(ctx, hud.done_btn, sc(6, s), done_bg, done_bg, 1);
    draw_text_canvas(
        ctx,
        hud.done_btn,
        "✓ 完成",
        done_fg,
        sc(12, s),
        true,
        1,
        false,
    );

    let (cancel_bg, cancel_border) = if hud.hover == 1 {
        (theme::HOVER_BG, Color::rgb(0x42, 0x54, 0x6E))
    } else {
        (theme::PANEL, Color::rgb(0x35, 0x43, 0x58))
    };
    round_rect_canvas(ctx, hud.cancel_btn, sc(6, s), cancel_bg, cancel_border, 1);
    draw_text_canvas(
        ctx,
        hud.cancel_btn,
        "✕ 取消",
        theme::TEXT,
        sc(12, s),
        true,
        1,
        false,
    );
}

extern "C" fn on_bar_move(x: i32, y: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let hud = &mut *(ud as *mut ScrollHud);
        let p = Point::new(x, y);
        let h = if hud.done_btn.contains(p) {
            0
        } else if hud.cancel_btn.contains(p) {
            1
        } else {
            -1
        };
        if h >= 0 {
            cursor_hand();
        } else {
            cursor_arrow();
        }
        if h != hud.hover {
            hud.hover = h;
            ffi::ts_canvas_invalidate(hud.bar_win);
        }
    }
}

extern "C" fn on_bar_leave(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let hud = &mut *(ud as *mut ScrollHud);
        if hud.hover != -1 {
            hud.hover = -1;
            ffi::ts_canvas_invalidate(hud.bar_win);
        }
    }
}

extern "C" fn on_bar_down(x: i32, y: i32, btn: i32, _clicks: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let hud = &mut *(ud as *mut ScrollHud);
        if btn == 1 {
            hud.decision = Some(HudDecision::Cancel);
            return;
        }
        if btn != 0 {
            return;
        }
        let p = Point::new(x, y);
        if hud.done_btn.contains(p) {
            hud.decision = Some(HudDecision::Finish);
        } else if hud.cancel_btn.contains(p) {
            hud.decision = Some(HudDecision::Cancel);
        }
    }
}
