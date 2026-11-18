#![allow(dead_code)]

use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use crate::macos::ffi;
use std::os::raw::c_void;

pub const HOTKEY_CAPTURE_ID: i32 = 0x7E05;
pub const HOTKEY_TRANSLATE_ID: i32 = 0x7E06;

pub const MOD_ALT_BIT: u32 = 1;
pub const MOD_CONTROL_BIT: u32 = 2;
pub const MOD_SHIFT_BIT: u32 = 4;
pub const MOD_WIN_BIT: u32 = 8;

pub const VK_TAB: u32 = 0x09;
pub const VK_RETURN: u32 = 0x0D;
pub const VK_SHIFT: u32 = 0x10;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_MENU: u32 = 0x12;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_SPACE: u32 = 0x20;
pub const VK_LEFT: u32 = 0x25;
pub const VK_UP: u32 = 0x26;
pub const VK_RIGHT: u32 = 0x27;
pub const VK_DOWN: u32 = 0x28;
pub const VK_1: u32 = 0x31;
pub const VK_2: u32 = 0x32;
pub const VK_3: u32 = 0x33;
pub const VK_4: u32 = 0x34;
pub const VK_A: u32 = 0x41;
pub const VK_B: u32 = 0x42;
pub const VK_C: u32 = 0x43;
pub const VK_E: u32 = 0x45;
pub const VK_H: u32 = 0x48;
pub const VK_L: u32 = 0x4C;
pub const VK_M: u32 = 0x4D;
pub const VK_O: u32 = 0x4F;
pub const VK_P: u32 = 0x50;
pub const VK_R: u32 = 0x52;
pub const VK_S: u32 = 0x53;
pub const VK_T: u32 = 0x54;
pub const VK_X: u32 = 0x58;
pub const VK_Z: u32 = 0x5A;
pub const VK_ADD: u32 = 0x6B;
pub const VK_SUBTRACT: u32 = 0x6D;
pub const VK_OEM_PLUS: u32 = 0xBB;
pub const VK_OEM_MINUS: u32 = 0xBD;

pub fn enable_dpi() {
    unsafe {
        ffi::ts_app_init();
    }
}

pub fn dpi_scale_system() -> f32 {
    unsafe { ffi::ts_backing_scale().max(1.0) }
}

pub fn dpi_scale_at(_p: Point) -> f32 {
    dpi_scale_system()
}

pub fn sc(v: i32, scale: f32) -> i32 {
    ((v as f32 * scale).round() as i32).max(1)
}

pub fn small_icon_size() -> i32 {
    64
}

pub fn virtual_screen() -> Rect {
    let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
    unsafe {
        ffi::ts_virtual_screen(&mut x, &mut y, &mut w, &mut h);
    }
    Rect::new(x, y, w.max(1), h.max(1))
}

pub fn work_area_from_point(p: Point) -> Rect {
    let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
    unsafe {
        ffi::ts_monitor_at(p.x, p.y, true, &mut x, &mut y, &mut w, &mut h);
    }
    Rect::new(x, y, w.max(1), h.max(1))
}

pub fn monitor_from_point(p: Point) -> Rect {
    let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
    unsafe {
        ffi::ts_monitor_at(p.x, p.y, false, &mut x, &mut y, &mut w, &mut h);
    }
    Rect::new(x, y, w.max(1), h.max(1))
}

pub fn cursor_pos() -> Point {
    let (mut x, mut y) = (0, 0);
    unsafe {
        ffi::ts_cursor_pos(&mut x, &mut y);
    }
    Point::new(x, y)
}

pub fn apply_cursor_cross() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_CROSS);
    }
}

pub fn cursor_arrow() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_ARROW);
    }
}

pub fn cursor_hand() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_HAND);
    }
}

pub fn cursor_cross() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_CROSS);
    }
}

pub fn cursor_sizeall() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_SIZEALL);
    }
}

pub fn cursor_sizens() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_SIZENS);
    }
}

pub fn cursor_sizewe() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_SIZEWE);
    }
}

pub fn cursor_sizenwse() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_SIZENWSE);
    }
}

pub fn cursor_sizenesw() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_SIZENESW);
    }
}

pub fn cursor_ibeam() {
    unsafe {
        ffi::ts_set_cursor(ffi::CURSOR_IBEAM);
    }
}

pub fn key_down(vk: i32) -> bool {
    unsafe { ffi::ts_key_down_vk(vk) }
}

pub fn shift_down() -> bool {
    unsafe { (ffi::ts_modifiers() & MOD_SHIFT_BIT) != 0 }
}

pub fn ctrl_down() -> bool {
    unsafe { (ffi::ts_modifiers() & MOD_CONTROL_BIT) != 0 }
}

pub fn alt_down() -> bool {
    unsafe { (ffi::ts_modifiers() & MOD_ALT_BIT) != 0 }
}

pub fn cmd_down() -> bool {
    unsafe { (ffi::ts_modifiers() & MOD_WIN_BIT) != 0 }
}

pub fn sleep_ms(ms: u32) {
    unsafe {
        ffi::ts_sleep_ms(ms);
    }
}

pub fn fill_rect_canvas(ctx: *mut c_void, r: Rect, c: Color) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    unsafe {
        ffi::ts_ctx_fill_rect(ctx, r.x, r.y, r.w, r.h, c.bgra());
    }
}

pub fn stroke_rect_canvas(ctx: *mut c_void, r: Rect, c: Color, width: i32) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    unsafe {
        ffi::ts_ctx_stroke_rect(ctx, r.x, r.y, r.w, r.h, c.bgra(), width.max(1));
    }
}

pub fn round_rect_canvas(
    ctx: *mut c_void,
    r: Rect,
    radius: i32,
    fill: Color,
    stroke: Color,
    stroke_w: i32,
) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    unsafe {
        ffi::ts_ctx_round_rect(
            ctx,
            r.x,
            r.y,
            r.w,
            r.h,
            radius,
            fill.bgra(),
            stroke.bgra(),
            stroke_w,
        );
    }
}

pub fn fill_ellipse_canvas(ctx: *mut c_void, r: Rect, c: Color) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    unsafe {
        ffi::ts_ctx_fill_ellipse(ctx, r.x, r.y, r.w, r.h, c.bgra());
    }
}

pub fn ellipse_stroke_canvas(ctx: *mut c_void, r: Rect, c: Color, width: i32) {
    if r.w < 1 || r.h < 1 {
        return;
    }
    unsafe {
        ffi::ts_ctx_stroke_ellipse(ctx, r.x, r.y, r.w, r.h, c.bgra(), width.max(1));
    }
}

pub fn line_canvas(ctx: *mut c_void, x0: i32, y0: i32, x1: i32, y1: i32, c: Color, width: i32) {
    unsafe {
        ffi::ts_ctx_line(
            ctx,
            x0 as f64,
            y0 as f64,
            x1 as f64,
            y1 as f64,
            c.bgra(),
            width.max(1) as f64,
        );
    }
}

pub fn polyline_canvas(ctx: *mut c_void, pts: &[Point], c: Color, width: i32) {
    if pts.len() < 2 {
        return;
    }
    let mut flat = Vec::with_capacity(pts.len() * 2);
    for p in pts {
        flat.push(p.x);
        flat.push(p.y);
    }
    unsafe {
        ffi::ts_ctx_polyline(
            ctx,
            flat.as_ptr(),
            pts.len() as i32,
            c.bgra(),
            width.max(1) as f64,
        );
    }
}

pub fn paint_arrow_canvas(ctx: *mut c_void, from: Point, to: Point, color: Color, width: f32) {
    let dx = to.x as f32 - from.x as f32;
    let dy = to.y as f32 - from.y as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len < 2.0 || !width.is_finite() {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let max_head = (len * 0.72).max(1.0);
    let min_head = (width * 1.5).min(max_head);
    let head_len = (width * 3.4).clamp(min_head, max_head);
    let head_half = (width * 1.55).min(head_len * 0.8).max(2.0);
    let back_x = to.x as f32 - ux * head_len;
    let back_y = to.y as f32 - uy * head_len;
    let halo = Color::argb(150, 0, 0, 0);
    unsafe {
        ffi::ts_ctx_line(
            ctx,
            from.x as f64,
            from.y as f64,
            back_x as f64,
            back_y as f64,
            halo.bgra(),
            (width + 2.2) as f64,
        );
        ffi::ts_ctx_line(
            ctx,
            from.x as f64,
            from.y as f64,
            back_x as f64,
            back_y as f64,
            color.bgra(),
            width.max(1.0) as f64,
        );
        let tri: [f64; 6] = [
            to.x as f64,
            to.y as f64,
            (back_x - uy * head_half) as f64,
            (back_y + ux * head_half) as f64,
            (back_x + uy * head_half) as f64,
            (back_y - ux * head_half) as f64,
        ];
        ffi::ts_ctx_polygon_fill(ctx, tri.as_ptr(), 3, color.bgra());
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_text_canvas(
    ctx: *mut c_void,
    r: Rect,
    text: &str,
    color: Color,
    px: i32,
    bold: bool,
    align: i32,
    wrap: bool,
) {
    if text.trim().is_empty() || r.w < 1 || r.h < 1 {
        return;
    }
    let c = ffi::to_cstr(text);
    unsafe {
        ffi::ts_ctx_draw_text(
            ctx,
            r.x,
            r.y,
            r.w,
            r.h,
            c.as_ptr(),
            color.bgra(),
            px.max(1),
            bold,
            align,
            wrap,
        );
    }
}

pub fn paint_chip_canvas(ctx: *mut c_void, r: Rect, text: &str, scale: f32) {
    if r.w < 8 || r.h < 8 || text.is_empty() {
        return;
    }
    let pad = sc(7, scale).max(4);
    round_rect_canvas(
        ctx,
        r,
        sc(5, scale),
        Color::argb(230, 10, 14, 20),
        Color::argb(120, 42, 51, 64),
        1,
    );
    draw_text_canvas(
        ctx,
        Rect::new(r.x + pad, r.y, (r.w - pad * 2).max(1), r.h),
        text,
        crate::theme::TEXT,
        sc(13, scale).max(11),
        false,
        0,
        false,
    );
}

pub fn blit_bitmap_canvas(ctx: *mut c_void, bmp: &Bitmap, dest: Rect) {
    if dest.w < 1 || dest.h < 1 || bmp.width <= 0 || bmp.height <= 0 || bmp.pixels.is_empty() {
        return;
    }
    unsafe {
        ffi::ts_ctx_draw_bgra(
            ctx,
            bmp.pixels.as_ptr(),
            bmp.width,
            bmp.height,
            dest.x,
            dest.y,
            dest.w,
            dest.h,
        );
    }
}
