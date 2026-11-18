use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use crate::macos::ffi::{self, CachedImage, TSCanvasCallbacks};
use crate::macos::native::{
    blit_bitmap_canvas, cursor_hand, cursor_pos, cursor_sizeall, dpi_scale_at, draw_text_canvas,
    round_rect_canvas, sc, stroke_rect_canvas, work_area_from_point, MOD_SHIFT_BIT, VK_ESCAPE,
};
use crate::settings::Settings;
use crate::theme;
use std::cell::RefCell;
use std::os::raw::c_void;

const MIN_ZOOM: f32 = 0.08;
const MAX_ZOOM: f32 = 8.0;

struct PinState {
    win: *mut c_void,
    bmp: Bitmap,
    img: Option<CachedImage>,
    win_rect: Rect,
    scale: f32,
    zoom: f32,
    opacity: u8,
    drag: bool,
    drag_off: Point,
    hovering: bool,
    toolbar_hover: i32,
    hud_text: String,
    hud_until: Option<std::time::Instant>,
    closed: bool,
}

thread_local! {
    static PINS: RefCell<Vec<*mut PinState>> = const { RefCell::new(Vec::new()) };
}

pub fn open(bmp: Bitmap, origin: Point, _settings: &Settings) {
    let work = work_area_from_point(origin);
    let scale = dpi_scale_at(origin).max(1.0);
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
    let img = CachedImage::from_bitmap(&bmp);

    let state = Box::new(PinState {
        win: std::ptr::null_mut(),
        bmp,
        img,
        win_rect: Rect::new(origin.x, origin.y, w, h),
        scale,
        zoom,
        opacity: 255,
        drag: false,
        drag_off: Point::default(),
        hovering: false,
        toolbar_hover: -1,
        hud_text: String::new(),
        hud_until: None,
        closed: false,
    });
    let raw = Box::into_raw(state);

    let cbs = TSCanvasCallbacks {
        on_paint: Some(on_paint),
        on_mouse_move: Some(on_mouse_move),
        on_mouse_leave: Some(on_mouse_leave),
        on_mouse_down: Some(on_mouse_down),
        on_mouse_up: Some(on_mouse_up),
        on_scroll: Some(on_scroll),
        on_key_down: Some(on_key_down),
        on_timer: Some(on_timer),
        on_close: Some(on_close),
        ..Default::default()
    };

    unsafe {
        let win = ffi::ts_canvas_create(
            ffi::WIN_FLOATING,
            origin.x,
            origin.y,
            w,
            h,
            0.0,
            true,
            &cbs,
            raw as *mut c_void,
        );
        (*raw).win = win;
        PINS.with(|p| p.borrow_mut().push(raw));
        ffi::ts_tray_set_pin_count(count() as i32);
        ffi::ts_canvas_show(win, true);
    }
}

pub fn close_all() {
    let list: Vec<*mut PinState> = PINS.with(|p| p.borrow_mut().drain(..).collect());
    for raw in list {
        close_pin_raw(raw);
    }
    unsafe {
        ffi::ts_tray_set_pin_count(0);
    }
}

pub fn count() -> usize {
    PINS.with(|p| p.borrow().len())
}

fn close_pin_raw(raw: *mut PinState) {
    if raw.is_null() {
        return;
    }
    PINS.with(|p| p.borrow_mut().retain(|&x| x != raw));
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
        ffi::ts_tray_set_pin_count(count() as i32);
    }
}

impl PinState {
    fn invalidate(&self) {
        if !self.win.is_null() {
            unsafe {
                ffi::ts_canvas_invalidate(self.win);
            }
        }
    }

    fn toolbar_rects(&self) -> [Rect; 4] {
        let scale = self.scale;
        let gap = sc(3, scale);
        let margin = sc(6, scale);
        let available = (self.win_rect.w - margin * 2 - gap * 3).max(4);
        let width = sc(48, scale).min((available / 4).max(sc(24, scale)));
        let height = sc(28, scale).min((self.win_rect.h - margin * 2).max(sc(20, scale)));
        let total = width * 4 + gap * 3;
        let start = (self.win_rect.w - margin - total).max(margin);
        [
            Rect::new(start, margin, width, height),
            Rect::new(start + width + gap, margin, width, height),
            Rect::new(start + (width + gap) * 2, margin, width, height),
            Rect::new(start + (width + gap) * 3, margin, width, height),
        ]
    }

    fn toolbar_hit(&self, point: Point) -> i32 {
        self.toolbar_rects()
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| index as i32)
            .unwrap_or(-1)
    }

    fn paint(&self, ctx: *mut c_void) {
        let client = Rect::new(0, 0, self.win_rect.w, self.win_rect.h);
        if let Some(img) = &self.img {
            img.blt(ctx, client, Rect::new(0, 0, self.bmp.width, self.bmp.height));
        } else {
            blit_bitmap_canvas(ctx, &self.bmp, client);
        }
        stroke_rect_canvas(ctx, client, theme::ACCENT, 1);

        if self.hovering {
            self.paint_toolbar(ctx, client);
        }
        if !self.hud_text.is_empty() {
            self.paint_hud(ctx, client);
        }
    }

    fn paint_toolbar(&self, ctx: *mut c_void, client: Rect) {
        let scale = self.scale;
        let rects = self.toolbar_rects();
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
        round_rect_canvas(
            ctx,
            container,
            sc(5, scale),
            Color::argb(235, 12, 17, 24),
            theme::BORDER,
            1,
        );
        for (index, rect) in rects.iter().enumerate() {
            if self.toolbar_hover == index as i32 {
                round_rect_canvas(
                    ctx,
                    *rect,
                    sc(4, scale),
                    if index == 3 {
                        theme::ERROR_BG
                    } else {
                        theme::ACCENT_DARK
                    },
                    Color::argb(0, 0, 0, 0),
                    0,
                );
            }
            let color = if index == 3 && self.toolbar_hover == index as i32 {
                theme::DANGER
            } else if self.toolbar_hover == index as i32 {
                theme::ACCENT_HI
            } else {
                theme::TEXT
            };
            draw_text_canvas(
                ctx,
                *rect,
                labels[index],
                color,
                sc(10, scale),
                false,
                1,
                false,
            );
        }
    }

    fn paint_hud(&self, ctx: *mut c_void, client: Rect) {
        let scale = self.scale;
        let px = sc(12, scale);
        let (tw, th) = crate::draw::measure_text(&self.hud_text, px);
        let width = tw + sc(24, scale);
        let height = th + sc(12, scale);
        let rect = Rect::new(
            client.x + (client.w - width) / 2,
            client.y + (client.h - height) / 2,
            width,
            height,
        );
        round_rect_canvas(
            ctx,
            rect,
            sc(6, scale),
            Color::argb(230, 12, 17, 24),
            theme::ACCENT,
            1,
        );
        draw_text_canvas(ctx, rect, &self.hud_text, theme::TEXT, px, false, 1, false);
    }
}

fn activate_toolbar(raw: *mut PinState, index: i32) {
    if raw.is_null() {
        return;
    }
    match index {
        0 => unsafe {
            if crate::clipboard::set_image(&(*raw).bmp) {
                crate::toast::show_success("已复制图片", "可直接粘贴到其他应用");
            } else {
                crate::toast::show_error("复制失败", "剪贴板正被占用，请稍后重试");
            }
        },
        1 => unsafe {
            let bmp = (*raw).bmp.clone();
            crate::macos::app::request_image_ai(crate::macos::app::ImageAiKind::Ocr, bmp);
        },
        2 => unsafe {
            match crate::bitmap::try_save_png(&(*raw).bmp, &Settings::load()) {
                Ok(path) => crate::toast::show_success("已保存", &path.display().to_string()),
                Err(error) => crate::toast::show_error("保存失败", &error),
            }
        },
        3 => close_pin_raw(raw),
        _ => {}
    }
}

fn show_context_menu(raw: *mut PinState) {
    if raw.is_null() {
        return;
    }
    let labels = ["识别文字…", "翻译截图…", "复制图片", "保存图片", "关闭贴图"];
    let cstrs: Vec<std::ffi::CString> = labels.iter().map(|s| ffi::to_cstr(s)).collect();
    let ptrs: Vec<*const std::os::raw::c_char> = cstrs.iter().map(|c| c.as_ptr()).collect();
    let picked = unsafe { ffi::ts_popup_context_menu((*raw).win, ptrs.as_ptr(), ptrs.len() as i32) };
    match picked {
        1 => unsafe {
            crate::macos::app::request_image_ai(
                crate::macos::app::ImageAiKind::Ocr,
                (*raw).bmp.clone(),
            );
        },
        2 => unsafe {
            crate::macos::app::request_image_ai(
                crate::macos::app::ImageAiKind::Translate,
                (*raw).bmp.clone(),
            );
        },
        3 => unsafe {
            if crate::clipboard::set_image(&(*raw).bmp) {
                crate::toast::show_success("已复制图片", "可直接粘贴到其他应用");
            }
        },
        4 => unsafe {
            if let Ok(p) = crate::bitmap::try_save_png(&(*raw).bmp, &Settings::load()) {
                crate::toast::show_success("已保存", &p.display().to_string());
            }
        },
        5 => close_pin_raw(raw),
        _ => {}
    }
}

extern "C" fn on_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *const PinState)).paint(ctx);
        }
    }
}

extern "C" fn on_mouse_move(x: i32, y: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let pin = &mut *(ud as *mut PinState);
        pin.hovering = true;
        pin.toolbar_hover = pin.toolbar_hit(Point::new(x, y));
        if pin.toolbar_hover >= 0 {
            cursor_hand();
        } else {
            cursor_sizeall();
        }
        if pin.drag {
            let cur = cursor_pos();
            let nx = cur.x - pin.drag_off.x;
            let ny = cur.y - pin.drag_off.y;
            pin.win_rect.x = nx;
            pin.win_rect.y = ny;
            ffi::ts_canvas_set_frame(pin.win, nx, ny, pin.win_rect.w, pin.win_rect.h);
        }
        pin.invalidate();
    }
}

extern "C" fn on_mouse_leave(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let pin = &mut *(ud as *mut PinState);
        if !pin.drag {
            pin.hovering = false;
            pin.toolbar_hover = -1;
            pin.invalidate();
        }
    }
}

extern "C" fn on_mouse_down(x: i32, y: i32, btn: i32, clicks: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let raw = ud as *mut PinState;
    if btn == 1 {
        show_context_menu(raw);
        return;
    }
    if btn != 0 {
        return;
    }
    unsafe {
        let p = Point::new(x, y);
        let hit = (*raw).toolbar_hit(p);
        if (*raw).hovering && hit >= 0 {
            activate_toolbar(raw, hit);
            return;
        }
        if clicks >= 2 {
            crate::clipboard::set_image(&(*raw).bmp);
            crate::toast::show_success("已复制图片", "可直接粘贴");
            return;
        }
        (*raw).drag = true;
        (*raw).drag_off = p;
    }
}

extern "C" fn on_mouse_up(_x: i32, _y: i32, btn: i32, ud: *mut c_void) {
    if btn == 0 && !ud.is_null() {
        unsafe {
            (*(ud as *mut PinState)).drag = false;
        }
    }
}

extern "C" fn on_scroll(delta_y: f64, _x: i32, _y: i32, mods: u32, ud: *mut c_void) {
    if ud.is_null() || delta_y.abs() < 0.01 {
        return;
    }
    unsafe {
        let pin = &mut *(ud as *mut PinState);
        let shift = (mods & MOD_SHIFT_BIT) != 0;
        if shift {
            let step = (delta_y.abs() * 4.0).clamp(6.0, 22.0) as u8;
            let next = if delta_y > 0.0 {
                pin.opacity.saturating_add(step)
            } else {
                pin.opacity.saturating_sub(step)
            };
            pin.opacity = next.clamp(51, 255);
            ffi::ts_canvas_set_opacity(pin.win, pin.opacity as f64 / 255.0);
            pin.hud_text = format!("透明度 {}%", (pin.opacity as u32 * 100) / 255);
        } else {
            let step = (delta_y.abs() as f32 * 0.04).clamp(0.03, 0.14);
            let factor = if delta_y > 0.0 {
                1.0 + step
            } else {
                1.0 / (1.0 + step)
            };
            let old = pin.zoom;
            pin.zoom = (pin.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            if (pin.zoom - old).abs() > f32::EPSILON {
                let w = ((pin.bmp.width as f32 * pin.zoom).round() as i32).max(24);
                let h = ((pin.bmp.height as f32 * pin.zoom).round() as i32).max(24);
                let old_w = pin.win_rect.w.max(1);
                let old_h = pin.win_rect.h.max(1);
                let cur = cursor_pos();
                let fx = (cur.x - pin.win_rect.x) as f32 / old_w as f32;
                let fy = (cur.y - pin.win_rect.y) as f32 / old_h as f32;
                let nx = cur.x - (fx * w as f32).round() as i32;
                let ny = cur.y - (fy * h as f32).round() as i32;
                pin.win_rect = Rect::new(nx, ny, w, h);
                ffi::ts_canvas_set_frame(pin.win, nx, ny, w, h);
            }
            pin.hud_text = format!("{}%", (pin.zoom * 100.0).round() as i32);
        }
        pin.hud_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(900));
        ffi::ts_canvas_start_timer(pin.win, 50);
        pin.invalidate();
    }
}

extern "C" fn on_key_down(vk: u32, _mods: u32, ud: *mut c_void) {
    if vk == VK_ESCAPE {
        close_pin_raw(ud as *mut PinState);
    }
}

extern "C" fn on_timer(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let pin = &mut *(ud as *mut PinState);
        if pin
            .hud_until
            .is_some_and(|until| std::time::Instant::now() >= until)
        {
            pin.hud_text.clear();
            pin.hud_until = None;
            ffi::ts_canvas_stop_timer(pin.win);
            pin.invalidate();
        }
    }
}

extern "C" fn on_close(ud: *mut c_void) {
    close_pin_raw(ud as *mut PinState);
}
