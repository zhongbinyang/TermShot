use crate::annotation::{AnnotKind, AnnotationSession};
use crate::bitmap::Bitmap;
use crate::geom::{Point, Rect};
use crate::macos::ffi::{self, TSCanvasCallbacks};
use crate::macos::native::{
    blit_bitmap_canvas, cursor_arrow, cursor_cross, cursor_hand, dpi_scale_at, fill_rect_canvas,
    paint_chip_canvas, sc, shift_down, work_area_from_point, VK_1, VK_2, VK_3, VK_4, VK_A, VK_B,
    VK_C, VK_E, VK_ESCAPE, VK_H, VK_L, VK_M, VK_O, VK_P, VK_S, VK_T, VK_TAB, VK_X, VK_Z,
};
use crate::settings::PostCaptureAction;
use crate::theme;
use crate::toolbar::{ActionToolbar, ToolbarResult};
use std::os::raw::{c_char, c_void};

pub fn run(bmp: Bitmap, screen_rect: Rect) -> Option<(PostCaptureAction, Bitmap, AnnotationSession)> {
    let scale = dpi_scale_at(Point::new(screen_rect.x, screen_rect.y)).max(1.0);
    let work = work_area_from_point(Point::new(screen_rect.x, screen_rect.y));
    let (form_rect, img_client) = layout(&bmp, screen_rect, work, scale);

    let mut ann = AnnotationSession::default();
    ann.attach(&bmp);
    ann.select(AnnotKind::Arrow);
    ann.width_index = 1;

    let state = Box::new(PinAsk {
        win: std::ptr::null_mut(),
        bmp,
        form_size: (form_rect.w, form_rect.h),
        toolbar: ActionToolbar::default(),
        ann,
        scale,
        image_client: img_client,
        done: None,
        text_at: Point::default(),
        lit: None,
    });
    let raw = Box::into_raw(state);

    let cbs = TSCanvasCallbacks {
        on_paint: Some(on_paint),
        on_mouse_move: Some(on_mouse_move),
        on_mouse_down: Some(on_mouse_down),
        on_mouse_up: Some(on_mouse_up),
        on_key_down: Some(on_key_down),
        on_text_commit: Some(on_text_commit),
        on_close: Some(on_close),
        ..Default::default()
    };

    unsafe {
        let win = ffi::ts_canvas_create(
            ffi::WIN_FLOATING,
            form_rect.x,
            form_rect.y,
            form_rect.w,
            form_rect.h,
            sc(6, scale) as f64,
            true,
            &cbs,
            raw as *mut c_void,
        );
        (*raw).win = win;
        ffi::ts_canvas_show(win, true);

        while (*raw).done.is_none() {
            ffi::ts_pump_events(12);
        }

        let win_to_close = (*raw).win;
        (*raw).win = std::ptr::null_mut();
        if !win_to_close.is_null() {
            ffi::ts_canvas_close(win_to_close);
        }
        cursor_arrow();

        let mut st = Box::from_raw(raw);
        match st.done {
            Some(Some(action)) => {
                let mut out_bmp = st.bmp;
                let source = out_bmp.clone();
                st.ann.stamp(&mut out_bmp, Point::new(0, 0), Some(&source));
                Some((action, out_bmp, st.ann))
            }
            _ => None,
        }
    }
}

struct PinAsk {
    win: *mut c_void,
    bmp: Bitmap,
    form_size: (i32, i32),
    toolbar: ActionToolbar,
    ann: AnnotationSession,
    scale: f32,
    image_client: Rect,
    done: Option<Option<PostCaptureAction>>,
    text_at: Point,
    lit: Option<Bitmap>,
}

fn layout(bmp: &Bitmap, screen: Rect, work: Rect, scale: f32) -> (Rect, Rect) {
    let tool_h = (96.0 * scale).round() as i32;
    let gap = (8.0 * scale).round() as i32;
    let margin = 8;
    let max_w = (work.w - margin * 2).max(120);
    let max_h = (work.h - margin * 2 - tool_h - gap).max(80);
    let fit = (1.0f32)
        .min(max_w as f32 / bmp.width as f32)
        .min(max_h as f32 / bmp.height as f32);
    let img_w = ((bmp.width as f32 * fit).round() as i32).max(1);
    let img_h = ((bmp.height as f32 * fit).round() as i32).max(1);
    let tool_w = (560.0 * scale).round() as i32;
    let mut form_w = img_w.max(tool_w);
    let outside = screen.y + img_h + gap + tool_h <= work.bottom() - margin;
    let mut form_h = if outside { img_h + gap + tool_h } else { img_h };
    let mut x = screen.x;
    let mut y = screen.y;
    if x + form_w > work.right() - margin {
        x = work.right() - form_w - margin;
    }
    if y + form_h > work.bottom() - margin {
        y = work.bottom() - form_h - margin;
    }
    x = x.max(work.x + margin);
    y = y.max(work.y + margin);
    let inset = 1;
    form_w += inset * 2;
    form_h += inset;
    let img = Rect::new((form_w - img_w) / 2, inset, img_w, img_h);
    (Rect::new(x, y, form_w, form_h), img)
}

impl PinAsk {
    fn inv(&self) {
        if !self.win.is_null() {
            unsafe {
                ffi::ts_canvas_invalidate(self.win);
            }
        }
    }

    fn has_edit(&self) -> bool {
        !self.win.is_null() && unsafe { ffi::ts_canvas_has_edit(self.win) }
    }

    fn client_to_bmp(&self, p: Point) -> Point {
        let ic = self.image_client;
        let x = (p.x - ic.x) as f32 * self.bmp.width as f32 / ic.w.max(1) as f32;
        let y = (p.y - ic.y) as f32 * self.bmp.height as f32 / ic.h.max(1) as f32;
        Point::new(
            (x as i32).clamp(0, (self.bmp.width - 1).max(0)),
            (y as i32).clamp(0, (self.bmp.height - 1).max(0)),
        )
    }

    fn on_move(&mut self, p: Point) {
        if self.ann.has_draft() {
            self.ann.move_to(self.client_to_bmp(p), shift_down());
            self.inv();
            return;
        }
        if self.toolbar.set_hover(p) {
            self.inv();
        }
        let hit = self.toolbar.hit(p);
        if hit == ToolbarResult::Color || hit == ToolbarResult::Width || self.toolbar.hover_index >= 0 {
            cursor_hand();
        } else if self.ann.tool_active() && self.image_client.contains(p) {
            cursor_cross();
        } else {
            cursor_arrow();
        }
    }

    fn on_down(&mut self, p: Point, btn: i32) {
        if btn == 1 {
            if self.ann.has_draft() {
                self.ann.cancel_draft();
                self.inv();
            } else {
                self.done = Some(None);
            }
            return;
        }
        if btn != 0 {
            return;
        }
        let hit = self.toolbar.hit(p);
        if hit == ToolbarResult::Color {
            let c = self.toolbar.hit_color(p);
            if c >= 0 {
                self.ann.color_index = c as usize;
            }
            self.inv();
            return;
        }
        if hit == ToolbarResult::Width {
            let w = self.toolbar.hit_width(p);
            if w >= 0 {
                self.ann.set_width_index(w as usize);
            }
            self.inv();
            return;
        }
        if hit != ToolbarResult::Miss {
            self.end_text(true);
            self.toolbar.pressed_index = self.toolbar.hit_test(p);
            self.inv();
            return;
        }
        self.end_text(true);
        if !self.image_client.contains(p) {
            return;
        }
        let at = self.client_to_bmp(p);
        if self.ann.is_text_tool() {
            self.begin_text(at, p);
            return;
        }
        if self.ann.can_draw() {
            self.ann.begin(at);
            self.inv();
        }
    }

    fn on_up(&mut self, p: Point, btn: i32) {
        if btn != 0 {
            return;
        }
        if self.ann.has_draft() {
            self.ann.move_to(self.client_to_bmp(p), shift_down());
            self.ann.commit_draft();
            self.rebuild_lit();
            self.inv();
            return;
        }
        let hit = self.toolbar.hit_test(p);
        let pressed = self.toolbar.pressed_index;
        self.toolbar.pressed_index = -1;
        if pressed >= 0 && hit == pressed {
            self.apply(self.toolbar.result_at(pressed as usize));
        } else {
            self.inv();
        }
    }

    fn on_key(&mut self, vk: u32) {
        if vk == VK_ESCAPE {
            if self.has_edit() {
                self.end_text(false);
                self.inv();
                return;
            }
            if self.ann.has_draft() {
                self.ann.cancel_draft();
                self.inv();
                return;
            }
            self.done = Some(None);
            return;
        }
        if self.has_edit() {
            return;
        }
        match vk {
            VK_TAB => self.ann.cycle_tab(),
            VK_A => self.ann.toggle(AnnotKind::Arrow),
            VK_B => self.ann.toggle(AnnotKind::Pencil),
            VK_H => self.ann.toggle(AnnotKind::Marker),
            VK_M => self.ann.toggle(AnnotKind::Mosaic),
            VK_X => self.ann.toggle(AnnotKind::Text),
            VK_E => self.ann.toggle(AnnotKind::Eraser),
            VK_Z => {
                self.ann.undo();
                self.rebuild_lit();
            }
            VK_1 => self.ann.set_width_index(0),
            VK_2 => self.ann.set_width_index(1),
            VK_3 => self.ann.set_width_index(2),
            VK_4 => self.ann.set_width_index(3),
            VK_T => self.choose(PostCaptureAction::Pin),
            VK_S => self.choose(PostCaptureAction::SaveImage),
            VK_C => self.choose(PostCaptureAction::CopyImage),
            VK_P => self.choose(PostCaptureAction::CopyPath),
            VK_O => self.choose(PostCaptureAction::CopyText),
            VK_L => self.choose(PostCaptureAction::Translate),
            _ => {}
        }
        self.inv();
    }

    fn apply(&mut self, r: ToolbarResult) {
        match r {
            ToolbarResult::Shape => self.ann.toggle_shape(),
            ToolbarResult::Stroke => self.ann.toggle_stroke(),
            ToolbarResult::Pencil => self.ann.toggle(AnnotKind::Pencil),
            ToolbarResult::Marker => self.ann.toggle(AnnotKind::Marker),
            ToolbarResult::Mosaic => self.ann.toggle(AnnotKind::Mosaic),
            ToolbarResult::AnnotText => self.ann.toggle(AnnotKind::Text),
            ToolbarResult::Eraser => self.ann.toggle(AnnotKind::Eraser),
            ToolbarResult::Undo => {
                self.ann.undo();
                self.rebuild_lit();
            }
            ToolbarResult::Close => self.done = Some(None),
            other => {
                if let Some(a) = other.to_action() {
                    self.choose(a);
                }
            }
        }
        self.inv();
    }

    fn choose(&mut self, a: PostCaptureAction) {
        self.end_text(true);
        let _ = self.ann.commit_draft();
        self.done = Some(Some(a));
    }

    fn begin_text(&mut self, at: Point, client: Point) {
        self.end_text(true);
        self.text_at = at;
        if self.win.is_null() {
            return;
        }
        unsafe {
            ffi::ts_canvas_begin_edit(
                self.win,
                client.x,
                client.y,
                sc(220, self.scale),
                sc(28, self.scale),
                sc(14, self.scale),
                theme::TEXT.bgra(),
                theme::INPUT_BG.bgra(),
            );
        }
    }

    fn end_text(&mut self, commit: bool) {
        if self.win.is_null() {
            return;
        }
        unsafe {
            if !ffi::ts_canvas_has_edit(self.win) {
                return;
            }
            if let Some(text) = ffi::take_cstr(ffi::ts_canvas_end_edit(self.win)) {
                if commit && !text.trim().is_empty() {
                    self.ann.add_text(self.text_at, &text);
                    self.rebuild_lit();
                }
            }
        }
    }

    fn rebuild_lit(&mut self) {
        if !self.ann.has_marks() {
            self.lit = None;
            return;
        }
        let mut lit = self.bmp.clone();
        let src = Rect::new(0, 0, lit.width, lit.height);
        self.ann.paint_committed(&mut lit, src, Some(&self.bmp));
        self.lit = Some(lit);
    }

    fn paint(&mut self, ctx: *mut c_void) {
        let client = Rect::new(0, 0, self.form_size.0, self.form_size.1);
        fill_rect_canvas(ctx, client, theme::WIN_BG);
        let src = self.lit.as_ref().unwrap_or(&self.bmp);
        blit_bitmap_canvas(ctx, src, self.image_client);

        let ic = self.image_client;
        let bw = self.bmp.width.max(1);
        let bh = self.bmp.height.max(1);
        let ws = ic.w as f32 / bw as f32;
        self.ann.paint_draft_canvas(
            ctx,
            |p| Point::new(ic.x + p.x * ic.w / bw, ic.y + p.y * ic.h / bh),
            ws,
        );

        let scale = self.scale;
        self.toolbar.sync(&self.ann);
        self.toolbar.undo_enabled = self.ann.has_marks() || self.ann.has_draft() || self.has_edit();
        self.toolbar.relayout(self.image_client, client, scale);
        let tb = self.toolbar.bounds;
        if tb.w > 8 && tb.h > 8 {
            let mut chrome = Bitmap::new(tb.w, tb.h);
            self.toolbar
                .paint_at(&mut chrome, scale, Point::new(tb.x, tb.y));
            blit_bitmap_canvas(ctx, &chrome, tb);
        }
        if let Some((tip_r, text)) = self.toolbar.hover_tip(scale) {
            paint_chip_canvas(ctx, tip_r, &text, scale);
        }
    }
}

extern "C" fn on_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut PinAsk)).paint(ctx);
        }
    }
}

extern "C" fn on_mouse_move(x: i32, y: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut PinAsk)).on_move(Point::new(x, y));
        }
    }
}

extern "C" fn on_mouse_down(x: i32, y: i32, btn: i32, _clicks: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut PinAsk)).on_down(Point::new(x, y), btn);
        }
    }
}

extern "C" fn on_mouse_up(x: i32, y: i32, btn: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut PinAsk)).on_up(Point::new(x, y), btn);
        }
    }
}

extern "C" fn on_key_down(vk: u32, _mods: u32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut PinAsk)).on_key(vk);
        }
    }
}

extern "C" fn on_text_commit(text: *const c_char, commit: bool, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut PinAsk);
        if commit && !text.is_null() {
            let s = std::ffi::CStr::from_ptr(text).to_string_lossy();
            if !s.trim().is_empty() {
                st.ann.add_text(st.text_at, &s);
                st.rebuild_lit();
            }
        }
        st.inv();
    }
}

extern "C" fn on_close(ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            let st = &mut *(ud as *mut PinAsk);
            if st.done.is_none() {
                st.done = Some(None);
            }
        }
    }
}
