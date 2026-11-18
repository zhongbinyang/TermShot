use crate::annotation::{AnnotKind, AnnotationSession};
use crate::bitmap::Bitmap;
use crate::draw::fill_rect;
use crate::geom::{Color, Point, Rect};
use crate::macos::ffi::{self, CachedImage, TSCanvasCallbacks};
use crate::macos::native::{
    alt_down, apply_cursor_cross, blit_bitmap_canvas, ctrl_down, cursor_arrow, cursor_cross,
    cursor_hand, cursor_pos, cursor_sizeall, cursor_sizenesw, cursor_sizens, cursor_sizenwse,
    cursor_sizewe, dpi_scale_at, fill_rect_canvas, key_down, line_canvas, monitor_from_point,
    paint_chip_canvas, sc, shift_down, stroke_rect_canvas, virtual_screen, VK_1, VK_2, VK_3, VK_4,
    VK_A, VK_ADD, VK_B, VK_C, VK_DOWN, VK_E, VK_ESCAPE, VK_H, VK_L, VK_LEFT, VK_M, VK_O,
    VK_OEM_MINUS, VK_OEM_PLUS, VK_P, VK_R, VK_RETURN, VK_RIGHT, VK_S, VK_SPACE, VK_SUBTRACT, VK_T,
    VK_TAB, VK_UP, VK_X, VK_Z,
};
use crate::overlay::{OverlayResult, ResizeHandle};
use crate::settings::PostCaptureAction;
use crate::theme;
use crate::toolbar::{ActionToolbar, ToolbarResult};
use crate::windows_enum::{hit_test, WindowInfo};
use std::os::raw::{c_char, c_void};

const DRAG_THRESHOLD: i32 = 4;
const MAG_SRC: i32 = 13;
const MAG_ZOOM: i32 = 10;

struct Outcome {
    selected: Option<Rect>,
    scroll: bool,
    action: Option<PostCaptureAction>,
}

struct OverlayState {
    win: *mut c_void,
    shot: Bitmap,
    veiled: Bitmap,
    vs: Rect,
    windows: Vec<WindowInfo>,
    ask_after: bool,
    scroll_after: bool,
    cursor_bmp: Point,
    drag_start: Point,
    dragging: bool,
    drag_confirmed: bool,
    awaiting: bool,
    locked: Option<Rect>,
    hover: Option<WindowInfo>,
    toolbar: ActionToolbar,
    ann: AnnotationSession,
    done: Option<Outcome>,
    text_at: Point,
    composed: Option<Bitmap>,
    shot_img: Option<CachedImage>,
    veiled_img: Option<CachedImage>,
    resizing: Option<ResizeHandle>,
    resize_origin: Point,
    locked_origin: Rect,
}

pub fn run(
    shot: Bitmap,
    windows: Vec<WindowInfo>,
    ask_after: bool,
    scroll_after: bool,
) -> OverlayResult {
    let vs = virtual_screen();
    let veiled = veil_shot(&shot);
    let shot_img = CachedImage::from_bitmap(&shot);
    let veiled_img = CachedImage::from_bitmap(&veiled);
    let cur = cursor_pos();
    let mut state = Box::new(OverlayState {
        win: std::ptr::null_mut(),
        shot,
        veiled,
        vs,
        windows,
        ask_after,
        scroll_after,
        cursor_bmp: Point::new(cur.x - vs.x, cur.y - vs.y),
        drag_start: Point::default(),
        dragging: false,
        drag_confirmed: false,
        awaiting: false,
        locked: None,
        hover: None,
        toolbar: {
            let mut t = ActionToolbar::default();
            t.show_scroll = true;
            t
        },
        ann: AnnotationSession::default(),
        done: None,
        text_at: Point::default(),
        composed: None,
        shot_img,
        veiled_img,
        resizing: None,
        resize_origin: Point::default(),
        locked_origin: Rect::default(),
    });
    if state.veiled_img.is_some() {
        state.veiled = Bitmap::new(1, 1);
    }
    state.hover = hit_test(&state.windows, cur).cloned();
    state.ann.attach(&state.shot);

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
            ffi::WIN_OVERLAY,
            vs.x,
            vs.y,
            (*raw).shot.width,
            (*raw).shot.height,
            0.0,
            false,
            &cbs,
            raw as *mut c_void,
        );
        (*raw).win = win;
        ffi::ts_canvas_show(win, true);
        apply_cursor_cross();

        while (*raw).done.is_none() {
            ffi::ts_pump_events(12);
        }

        let win_to_close = (*raw).win;
        (*raw).win = std::ptr::null_mut();
        if !win_to_close.is_null() {
            ffi::ts_canvas_close(win_to_close);
        }
        cursor_arrow();
        let mut state = Box::from_raw(raw);
        let outcome = state.done.take().unwrap_or(Outcome {
            selected: None,
            scroll: false,
            action: None,
        });
        OverlayResult {
            selected: outcome.selected,
            scroll: outcome.scroll,
            action: outcome.action,
            session: state.ann,
        }
    }
}

fn apply_resize_cursor(h: ResizeHandle) {
    match h {
        ResizeHandle::TopLeft | ResizeHandle::BottomRight => cursor_sizenwse(),
        ResizeHandle::TopRight | ResizeHandle::BottomLeft => cursor_sizenesw(),
        ResizeHandle::Top | ResizeHandle::Bottom => cursor_sizens(),
        ResizeHandle::Left | ResizeHandle::Right => cursor_sizewe(),
        ResizeHandle::Inside => cursor_sizeall(),
        ResizeHandle::None => {}
    }
}

impl OverlayState {
    fn invalidate(&self) {
        if !self.win.is_null() {
            unsafe {
                ffi::ts_canvas_invalidate(self.win);
            }
        }
    }

    fn clamp_pt(&self, p: Point) -> Point {
        Point::new(
            p.x.clamp(0, (self.shot.width - 1).max(0)),
            p.y.clamp(0, (self.shot.height - 1).max(0)),
        )
    }

    fn highlight_bmp(&self) -> Option<Rect> {
        if let Some(l) = self.locked {
            return Some(l);
        }
        if self.drag_confirmed {
            return Some(self.normalized_drag());
        }
        self.hover.as_ref().map(|w| self.screen_to_bmp(w.bounds))
    }

    fn screen_to_bmp(&self, r: Rect) -> Rect {
        let x = r.x - self.vs.x;
        let y = r.y - self.vs.y;
        Rect::new(x, y, r.w, r.h).intersect(Rect::new(0, 0, self.shot.width, self.shot.height))
    }

    fn bmp_to_screen(&self, r: Rect) -> Rect {
        Rect::new(r.x + self.vs.x, r.y + self.vs.y, r.w, r.h)
    }

    fn bmp_pt_to_screen(&self, p: Point) -> Point {
        Point::new(p.x + self.vs.x, p.y + self.vs.y)
    }

    fn normalized_drag(&self) -> Rect {
        let x1 = self.drag_start.x.min(self.cursor_bmp.x).clamp(0, self.shot.width - 1);
        let y1 = self.drag_start.y.min(self.cursor_bmp.y).clamp(0, self.shot.height - 1);
        let x2 = self.drag_start.x.max(self.cursor_bmp.x).clamp(0, self.shot.width);
        let y2 = self.drag_start.y.max(self.cursor_bmp.y).clamp(0, self.shot.height);
        Rect::from_ltrb(x1, y1, x2.max(x1 + 1), y2.max(y1 + 1))
    }

    fn clamp_sel(&self, p: Point) -> Point {
        if let Some(r) = self.locked {
            Point::new(
                p.x.clamp(r.x, (r.right() - 1).max(r.x)),
                p.y.clamp(r.y, (r.bottom() - 1).max(r.y)),
            )
        } else {
            p
        }
    }

    fn ui_scale(&self, hr: Rect) -> f32 {
        let at = self.bmp_pt_to_screen(hr.center());
        dpi_scale_at(at).max(1.0)
    }

    fn hit_resize_handle(&self, pt: Point) -> ResizeHandle {
        let Some(dest) = self.locked else {
            return ResizeHandle::None;
        };
        let scale = self.ui_scale(dest);
        let handle_radius = sc(8, scale).max(6);
        let r2 = handle_radius * handle_radius;
        let dist_sq = |px: i32, py: i32| (pt.x - px) * (pt.x - px) + (pt.y - py) * (pt.y - py);

        if dist_sq(dest.x, dest.y) <= r2 {
            return ResizeHandle::TopLeft;
        }
        if dist_sq(dest.right(), dest.y) <= r2 {
            return ResizeHandle::TopRight;
        }
        if dist_sq(dest.right(), dest.bottom()) <= r2 {
            return ResizeHandle::BottomRight;
        }
        if dist_sq(dest.x, dest.bottom()) <= r2 {
            return ResizeHandle::BottomLeft;
        }

        if dest.w >= sc(40, scale) {
            if dist_sq(dest.x + dest.w / 2, dest.y) <= r2 {
                return ResizeHandle::Top;
            }
            if dist_sq(dest.x + dest.w / 2, dest.bottom()) <= r2 {
                return ResizeHandle::Bottom;
            }
        }
        if dest.h >= sc(40, scale) {
            if dist_sq(dest.x, dest.y + dest.h / 2) <= r2 {
                return ResizeHandle::Left;
            }
            if dist_sq(dest.right(), dest.y + dest.h / 2) <= r2 {
                return ResizeHandle::Right;
            }
        }

        let edge_tol = sc(5, scale).max(4);
        let in_x = pt.x >= dest.x - edge_tol && pt.x <= dest.right() + edge_tol;
        let in_y = pt.y >= dest.y - edge_tol && pt.y <= dest.bottom() + edge_tol;
        if in_x && (pt.y - dest.y).abs() <= edge_tol {
            return ResizeHandle::Top;
        }
        if in_x && (pt.y - dest.bottom()).abs() <= edge_tol {
            return ResizeHandle::Bottom;
        }
        if in_y && (pt.x - dest.x).abs() <= edge_tol {
            return ResizeHandle::Left;
        }
        if in_y && (pt.x - dest.right()).abs() <= edge_tol {
            return ResizeHandle::Right;
        }

        if dest.contains(pt) && !self.ann.tool_active() {
            return ResizeHandle::Inside;
        }
        ResizeHandle::None
    }

    fn handle_resize_drag(&mut self, pt: Point, h: ResizeHandle) {
        let cur = self.clamp_pt(pt);
        let dx = cur.x - self.resize_origin.x;
        let dy = cur.y - self.resize_origin.y;
        let orig = self.locked_origin;
        let min_size = 8;

        let mut l = orig.x;
        let mut t = orig.y;
        let mut r = orig.right();
        let mut b = orig.bottom();

        match h {
            ResizeHandle::TopLeft => {
                l = (orig.x + dx).min(orig.right() - min_size);
                t = (orig.y + dy).min(orig.bottom() - min_size);
            }
            ResizeHandle::Top => {
                t = (orig.y + dy).min(orig.bottom() - min_size);
            }
            ResizeHandle::TopRight => {
                r = (orig.right() + dx).max(orig.x + min_size);
                t = (orig.y + dy).min(orig.bottom() - min_size);
            }
            ResizeHandle::Right => {
                r = (orig.right() + dx).max(orig.x + min_size);
            }
            ResizeHandle::BottomRight => {
                r = (orig.right() + dx).max(orig.x + min_size);
                b = (orig.bottom() + dy).max(orig.y + min_size);
            }
            ResizeHandle::Bottom => {
                b = (orig.bottom() + dy).max(orig.y + min_size);
            }
            ResizeHandle::BottomLeft => {
                l = (orig.x + dx).min(orig.right() - min_size);
                b = (orig.bottom() + dy).max(orig.y + min_size);
            }
            ResizeHandle::Left => {
                l = (orig.x + dx).min(orig.right() - min_size);
            }
            ResizeHandle::Inside => {
                let max_dx = (self.shot.width - orig.right()).max(0);
                let min_dx = -orig.x;
                let clamped_dx = dx.clamp(min_dx, max_dx);
                let max_dy = (self.shot.height - orig.bottom()).max(0);
                let min_dy = -orig.y;
                let clamped_dy = dy.clamp(min_dy, max_dy);
                l = orig.x + clamped_dx;
                t = orig.y + clamped_dy;
                r = l + orig.w;
                b = t + orig.h;
            }
            ResizeHandle::None => return,
        }

        l = l.clamp(0, (self.shot.width - min_size).max(0));
        t = t.clamp(0, (self.shot.height - min_size).max(0));
        r = r.clamp(l + min_size, self.shot.width);
        b = b.clamp(t + min_size, self.shot.height);

        self.locked = Some(Rect::from_ltrb(l, t, r, b));
        apply_resize_cursor(h);
        self.rebuild_composed();
        self.invalidate();
    }

    fn paint_handles(&self, ctx: *mut c_void, dest: Rect, scale: f32) {
        let size = sc(7, scale).max(5);
        let half = size / 2;
        let mut pts = Vec::with_capacity(8);
        pts.push(Point::new(dest.x, dest.y));
        pts.push(Point::new(dest.right(), dest.y));
        pts.push(Point::new(dest.right(), dest.bottom()));
        pts.push(Point::new(dest.x, dest.bottom()));
        if dest.w >= sc(40, scale) {
            pts.push(Point::new(dest.x + dest.w / 2, dest.y));
            pts.push(Point::new(dest.x + dest.w / 2, dest.bottom()));
        }
        if dest.h >= sc(40, scale) {
            pts.push(Point::new(dest.x, dest.y + dest.h / 2));
            pts.push(Point::new(dest.right(), dest.y + dest.h / 2));
        }
        for p in pts {
            let r = Rect::new(p.x - half, p.y - half, size, size);
            fill_rect_canvas(ctx, r, Color::rgb(255, 255, 255));
            stroke_rect_canvas(ctx, r, Color::argb(220, 10, 14, 20), 1);
        }
    }

    fn nudge_or_expand(&mut self, vk: u32) -> bool {
        let Some(orig) = self.locked else {
            return false;
        };
        let ctrl = ctrl_down();
        let alt = alt_down();
        let shift = shift_down();
        let step = if shift { 10 } else { 1 };
        let min_size = 8;

        let mut l = orig.x;
        let mut t = orig.y;
        let mut r = orig.right();
        let mut b = orig.bottom();

        if vk == VK_LEFT {
            if ctrl {
                l -= step;
            } else if alt {
                l = (l + step).min(r - min_size);
            } else {
                l -= step;
                r -= step;
            }
        } else if vk == VK_RIGHT {
            if ctrl {
                r += step;
            } else if alt {
                r = (r - step).max(l + min_size);
            } else {
                l += step;
                r += step;
            }
        } else if vk == VK_UP {
            if ctrl {
                t -= step;
            } else if alt {
                t = (t + step).min(b - min_size);
            } else {
                t -= step;
                b -= step;
            }
        } else if vk == VK_DOWN {
            if ctrl {
                b += step;
            } else if alt {
                b = (b - step).max(t + min_size);
            } else {
                t += step;
                b += step;
            }
        } else if vk == VK_OEM_PLUS || vk == VK_ADD {
            let pad = if shift { 16 } else { 8 };
            l -= pad;
            t -= pad;
            r += pad;
            b += pad;
        } else if vk == VK_OEM_MINUS || vk == VK_SUBTRACT {
            let pad = if shift { 16 } else { 8 };
            l = (l + pad).min(orig.center().x - min_size / 2);
            t = (t + pad).min(orig.center().y - min_size / 2);
            r = (r - pad).max(l + min_size);
            b = (b - pad).max(t + min_size);
        } else {
            return false;
        }

        l = l.clamp(0, (self.shot.width - min_size).max(0));
        t = t.clamp(0, (self.shot.height - min_size).max(0));
        r = r.clamp(l + min_size, self.shot.width);
        b = b.clamp(t + min_size, self.shot.height);

        self.locked = Some(Rect::from_ltrb(l, t, r, b));
        self.rebuild_composed();
        self.invalidate();
        true
    }

    fn on_move(&mut self, raw_pt: Point) {
        let pt = self.clamp_pt(raw_pt);
        if self.awaiting {
            self.cursor_bmp = pt;
            if self.ann.has_draft() {
                self.ann.move_to(self.clamp_sel(pt), shift_down());
                self.invalidate();
                return;
            }
            if let Some(h) = self.resizing {
                self.handle_resize_drag(pt, h);
                return;
            }
            let dirty = self.toolbar.set_hover(pt);
            let tb_hit = self.toolbar.hit(pt);
            if tb_hit == ToolbarResult::Color
                || tb_hit == ToolbarResult::Width
                || self.toolbar.hover_index >= 0
            {
                cursor_hand();
            } else if tb_hit == ToolbarResult::Chrome {
                cursor_arrow();
            } else {
                let handle = self.hit_resize_handle(pt);
                if handle != ResizeHandle::None {
                    apply_resize_cursor(handle);
                } else if self.ann.tool_active() {
                    cursor_cross();
                } else {
                    cursor_arrow();
                }
            }
            if dirty {
                self.invalidate();
            }
            return;
        }
        if self.dragging {
            if key_down(VK_SPACE as i32) {
                cursor_sizeall();
                let dx = pt.x - self.cursor_bmp.x;
                let dy = pt.y - self.cursor_bmp.y;
                self.drag_start.x = (self.drag_start.x + dx).clamp(0, self.shot.width);
                self.drag_start.y = (self.drag_start.y + dy).clamp(0, self.shot.height);
                self.cursor_bmp = pt;
            } else {
                self.cursor_bmp = pt;
                let dx = self.cursor_bmp.x - self.drag_start.x;
                let dy = self.cursor_bmp.y - self.drag_start.y;
                if dx.abs() >= DRAG_THRESHOLD || dy.abs() >= DRAG_THRESHOLD {
                    self.drag_confirmed = true;
                }
            }
        } else {
            self.cursor_bmp = pt;
            self.hover = hit_test(&self.windows, self.bmp_pt_to_screen(pt)).cloned();
        }
        self.invalidate();
    }

    fn on_down(&mut self, raw_pt: Point, btn: i32) {
        if btn == 1 {
            if self.awaiting && self.ann.has_draft() {
                self.ann.cancel_draft();
                self.invalidate();
            } else {
                self.cancel();
            }
            return;
        }
        if btn != 0 {
            return;
        }
        let pt = self.clamp_pt(raw_pt);
        if self.awaiting {
            let hit = self.toolbar.hit(pt);
            if hit == ToolbarResult::Color {
                let c = self.toolbar.hit_color(pt);
                if c >= 0 {
                    self.ann.color_index = c as usize;
                }
                self.invalidate();
                return;
            }
            if hit == ToolbarResult::Width {
                let w = self.toolbar.hit_width(pt);
                if w >= 0 {
                    self.ann.set_width_index(w as usize);
                }
                self.invalidate();
                return;
            }
            if hit != ToolbarResult::Miss {
                self.end_text(true);
                self.toolbar.pressed_index = self.toolbar.hit_test(pt);
                self.invalidate();
                return;
            }
            self.end_text(true);

            let handle = self.hit_resize_handle(pt);
            if handle != ResizeHandle::None {
                self.resizing = Some(handle);
                self.resize_origin = pt;
                self.locked_origin = self.locked.unwrap_or_default();
                apply_resize_cursor(handle);
                return;
            }

            self.cursor_bmp = pt;
            let at = self.clamp_sel(pt);
            if self.ann.is_text_tool() {
                self.begin_text(at);
                return;
            }
            if self.ann.can_draw() {
                self.ann.begin(at);
                cursor_cross();
                self.invalidate();
            }
            return;
        }
        self.dragging = true;
        self.drag_confirmed = false;
        self.drag_start = pt;
        self.cursor_bmp = pt;
        self.invalidate();
    }

    fn on_up(&mut self, raw_pt: Point, btn: i32) {
        if btn != 0 {
            return;
        }
        let pt = self.clamp_pt(raw_pt);
        if self.awaiting {
            if self.resizing.is_some() {
                self.resizing = None;
                self.rebuild_composed();
                self.invalidate();
                return;
            }
            if self.ann.has_draft() {
                self.ann.move_to(self.clamp_sel(pt), shift_down());
                self.ann.commit_draft();
                self.rebuild_composed();
                self.invalidate();
                return;
            }
            let hit = self.toolbar.hit_test(pt);
            let pressed = self.toolbar.pressed_index;
            self.toolbar.pressed_index = -1;
            if pressed >= 0 && hit == pressed {
                self.apply_toolbar(self.toolbar.result_at(pressed as usize));
            } else {
                self.invalidate();
            }
            return;
        }
        if self.dragging {
            self.cursor_bmp = pt;
            if self.drag_confirmed {
                let r = self.normalized_drag();
                if r.w >= 1 && r.h >= 1 {
                    self.complete(self.bmp_to_screen(r));
                } else {
                    self.dragging = false;
                }
            } else if let Some(w) = self.hover.clone() {
                self.complete(w.bounds);
            } else {
                self.dragging = false;
                self.drag_confirmed = false;
            }
            self.invalidate();
        }
    }

    fn has_edit(&self) -> bool {
        !self.win.is_null() && unsafe { ffi::ts_canvas_has_edit(self.win) }
    }

    fn on_key(&mut self, vk: u32) {
        if vk == VK_ESCAPE {
            if self.has_edit() {
                self.end_text(false);
                self.invalidate();
                return;
            }
            if self.awaiting && self.ann.has_draft() {
                self.ann.cancel_draft();
                self.invalidate();
                return;
            }
            self.cancel();
            return;
        }
        if self.has_edit() {
            return;
        }
        if vk == VK_R {
            self.try_scroll();
            return;
        }
        if !self.awaiting {
            if vk == VK_RETURN {
                if self.drag_confirmed {
                    self.complete(self.bmp_to_screen(self.normalized_drag()));
                } else if let Some(w) = self.hover.clone() {
                    self.complete(w.bounds);
                }
            }
            return;
        }
        if self.nudge_or_expand(vk) {
            return;
        }
        match vk {
            VK_TAB => {
                self.ann.cycle_tab();
                self.invalidate();
            }
            VK_A => {
                self.ann.toggle(AnnotKind::Arrow);
                self.invalidate();
            }
            VK_B => {
                self.ann.toggle(AnnotKind::Pencil);
                self.invalidate();
            }
            VK_H => {
                self.ann.toggle(AnnotKind::Marker);
                self.invalidate();
            }
            VK_M => {
                self.ann.toggle(AnnotKind::Mosaic);
                self.invalidate();
            }
            VK_X => {
                self.ann.toggle(AnnotKind::Text);
                self.invalidate();
            }
            VK_E => {
                self.ann.toggle(AnnotKind::Eraser);
                self.invalidate();
            }
            VK_Z if self.ann.undo() => {
                self.rebuild_composed();
                self.invalidate();
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
        if (VK_1..=VK_4).contains(&vk) {
            self.invalidate();
        }
    }

    fn apply_toolbar(&mut self, r: ToolbarResult) {
        match r {
            ToolbarResult::Shape => {
                self.end_text(true);
                self.ann.toggle_shape();
            }
            ToolbarResult::Stroke => {
                self.end_text(true);
                self.ann.toggle_stroke();
            }
            ToolbarResult::Pencil => {
                self.end_text(true);
                self.ann.toggle(AnnotKind::Pencil);
            }
            ToolbarResult::Marker => {
                self.end_text(true);
                self.ann.toggle(AnnotKind::Marker);
            }
            ToolbarResult::Mosaic => {
                self.end_text(true);
                self.ann.toggle(AnnotKind::Mosaic);
            }
            ToolbarResult::AnnotText => {
                self.end_text(true);
                self.ann.toggle(AnnotKind::Text);
            }
            ToolbarResult::Eraser => {
                self.end_text(true);
                self.ann.toggle(AnnotKind::Eraser);
            }
            ToolbarResult::Undo => {
                self.end_text(false);
                self.ann.undo();
                self.rebuild_composed();
            }
            ToolbarResult::Close => self.cancel(),
            ToolbarResult::Scroll => self.choose_scroll(None),
            other => {
                if let Some(a) = other.to_action() {
                    self.choose(a);
                }
            }
        }
        self.invalidate();
    }

    fn complete(&mut self, mut screen: Rect) {
        screen = screen.intersect(self.vs);
        if screen.w < 1 || screen.h < 1 {
            return;
        }
        if self.scroll_after {
            self.done = Some(Outcome {
                selected: Some(screen),
                scroll: true,
                action: None,
            });
            return;
        }
        if self.ask_after {
            self.enter_ask(screen);
            return;
        }
        self.done = Some(Outcome {
            selected: Some(screen),
            scroll: false,
            action: None,
        });
    }

    fn enter_ask(&mut self, screen: Rect) {
        self.awaiting = true;
        self.dragging = false;
        self.drag_confirmed = false;
        self.locked = Some(self.screen_to_bmp(screen));
        self.ann.width_index = 1;
        self.ann.select(AnnotKind::Arrow);
        self.rebuild_composed();
        self.invalidate();
    }

    fn choose(&mut self, action: PostCaptureAction) {
        self.end_text(true);
        let _ = self.ann.commit_draft();
        let sel = self.locked.map(|r| self.bmp_to_screen(r));
        self.done = Some(Outcome {
            selected: sel,
            scroll: false,
            action: Some(action),
        });
    }

    fn try_scroll(&mut self) {
        if self.awaiting {
            self.choose_scroll(None);
            return;
        }
        if self.drag_confirmed {
            self.choose_scroll(Some(self.bmp_to_screen(self.normalized_drag())));
            return;
        }
        if let Some(w) = self.hover.clone() {
            self.choose_scroll(Some(w.bounds));
        }
    }

    fn choose_scroll(&mut self, screen: Option<Rect>) {
        self.end_text(false);
        let mut r = screen.or_else(|| self.locked.map(|x| self.bmp_to_screen(x)));
        if let Some(ref mut rr) = r {
            *rr = rr.intersect(self.vs);
            if rr.w < 1 || rr.h < 1 {
                return;
            }
        } else {
            return;
        }
        self.done = Some(Outcome {
            selected: r,
            scroll: true,
            action: None,
        });
    }

    fn cancel(&mut self) {
        self.end_text(false);
        self.done = Some(Outcome {
            selected: None,
            scroll: false,
            action: None,
        });
    }

    fn begin_text(&mut self, bmp_pt: Point) {
        self.end_text(true);
        self.text_at = bmp_pt;
        if self.win.is_null() {
            return;
        }
        let scale = dpi_scale_at(self.bmp_pt_to_screen(bmp_pt)).max(1.0);
        unsafe {
            ffi::ts_canvas_begin_edit(
                self.win,
                bmp_pt.x.clamp(8, (self.shot.width - 240).max(8)),
                bmp_pt.y.clamp(8, (self.shot.height - 40).max(8)),
                sc(220, scale),
                sc(28, scale),
                sc(14, scale),
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
                    self.rebuild_composed();
                }
            }
        }
    }

    fn rebuild_composed(&mut self) {
        let Some(hr) = self.locked else {
            self.composed = None;
            return;
        };
        let mut lit = self.shot.crop(hr);
        self.ann.paint_committed(&mut lit, hr, Some(&self.shot));
        self.composed = Some(lit);
    }

    fn paint(&mut self, ctx: *mut c_void) {
        let full = Rect::new(0, 0, self.shot.width, self.shot.height);
        if let Some(g) = &self.veiled_img {
            g.blt(ctx, full, full);
        } else {
            blit_bitmap_canvas(ctx, &self.veiled, full);
        }

        if let Some(hr) = self.highlight_bmp() {
            let scale = self.ui_scale(hr);
            if self.awaiting {
                if let Some(lit) = &self.composed {
                    blit_bitmap_canvas(ctx, lit, hr);
                } else if let Some(g) = &self.shot_img {
                    g.blt(ctx, hr, hr);
                } else {
                    blit_bitmap_canvas(ctx, &self.shot, full);
                }
                self.ann.paint_draft_canvas(ctx, |p| p, 1.0);
            } else if let Some(g) = &self.shot_img {
                g.blt(ctx, hr, hr);
            } else {
                blit_bitmap_canvas(ctx, &self.shot.crop(hr), hr);
            }

            stroke_rect_canvas(ctx, hr, Color::argb(220, 8, 10, 14), sc(3, scale));
            stroke_rect_canvas(ctx, hr, theme::ACCENT, 1);

            if self.awaiting {
                self.paint_handles(ctx, hr, scale);
            }

            let size_text = format!("{} × {} px", hr.w.max(1), hr.h.max(1));
            let size_px = sc(12, scale);
            let (size_w, size_h) = crate::draw::measure_text(&size_text, size_px);
            let badge_w = size_w + sc(16, scale);
            let badge_h = size_h + sc(8, scale);
            let badge_y = if hr.y - badge_h - sc(6, scale) >= 0 {
                hr.y - badge_h - sc(6, scale)
            } else {
                hr.y + sc(6, scale)
            };
            paint_chip_canvas(
                ctx,
                Rect::new(hr.x, badge_y, badge_w, badge_h),
                &size_text,
                scale,
            );

            if self.awaiting {
                self.toolbar.sync(&self.ann);
                let confine = {
                    let c = self.bmp_pt_to_screen(Point::new(hr.x + hr.w / 2, hr.y + hr.h / 2));
                    let mon = monitor_from_point(c);
                    self.screen_to_bmp(mon)
                };
                let confine = if confine.w < 8 {
                    full
                } else {
                    confine
                };
                self.toolbar.relayout(hr, confine, scale);
                self.toolbar.undo_enabled =
                    self.ann.has_marks() || self.ann.has_draft() || self.has_edit();
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

        if !self.awaiting {
            let scale = dpi_scale_at(self.bmp_pt_to_screen(self.cursor_bmp)).max(1.0);
            line_canvas(
                ctx,
                0,
                self.cursor_bmp.y,
                self.shot.width,
                self.cursor_bmp.y,
                theme::ACCENT.with_alpha(200),
                scale.max(1.0) as i32,
            );
            line_canvas(
                ctx,
                self.cursor_bmp.x,
                0,
                self.cursor_bmp.x,
                self.shot.height,
                theme::ACCENT.with_alpha(200),
                scale.max(1.0) as i32,
            );
            paint_magnifier_canvas(
                ctx,
                &self.shot,
                self.cursor_bmp,
                self.bmp_pt_to_screen(self.cursor_bmp),
                scale,
            );
            paint_capture_hint(ctx, self, scale);
        }
    }
}

fn veil_shot(shot: &Bitmap) -> Bitmap {
    let mut v = shot.clone();
    for p in v.pixels.iter_mut() {
        let b = *p & 0xFF;
        let g = (*p >> 8) & 0xFF;
        let r = (*p >> 16) & 0xFF;
        let nb = (b * 40 + 6 * 215) / 255;
        let ng = (g * 40 + 8 * 215) / 255;
        let nr = (r * 40 + 12 * 215) / 255;
        *p = nb | (ng << 8) | (nr << 16) | 0xFF000000;
    }
    v
}

fn paint_capture_hint(ctx: *mut c_void, state: &OverlayState, scale: f32) {
    let text = if state.drag_confirmed {
        "松开完成  ·  按住空格平移  ·  R 滚动  ·  Esc / 右键取消"
    } else if state.scroll_after {
        "拖动框选滚动区域  ·  单击窗口  ·  Esc / 右键取消"
    } else {
        "拖动框选  ·  单击窗口  ·  按住空格平移  ·  R 滚动  ·  Esc / 右键取消"
    };
    let px = sc(14, scale);
    let (tw, th) = crate::draw::measure_text(text, px);
    let box_w = tw + sc(28, scale);
    let box_h = th + sc(10, scale);
    let screen_pt = state.bmp_pt_to_screen(state.cursor_bmp);
    let mon = state.screen_to_bmp(monitor_from_point(screen_pt));
    let mut x = mon.x + (mon.w - box_w) / 2;
    let mut y = mon.y + sc(18, scale);
    x = x.max(mon.x + 8);
    y = y.max(mon.y + 8);
    paint_chip_canvas(ctx, Rect::new(x, y, box_w, box_h), text, scale);
}

fn paint_magnifier_canvas(
    ctx: *mut c_void,
    shot: &Bitmap,
    cursor: Point,
    screen: Point,
    scale: f32,
) {
    let src = MAG_SRC;
    let zoom = sc(MAG_ZOOM, scale);
    let mag = src * zoom;
    let info_h = sc(44, scale);
    let pad = sc(3, scale);
    let offset = sc(28, scale);
    let total_w = mag + pad * 2;
    let total_h = mag + pad * 2 + info_h;
    let mut bx = cursor.x + offset;
    let mut by = cursor.y + offset;
    if bx + total_w > shot.width - 8 {
        bx = cursor.x - offset - total_w;
    }
    if by + total_h > shot.height - 8 {
        by = cursor.y - offset - total_h;
    }
    bx = bx.clamp(8, (shot.width - total_w - 8).max(8));
    by = by.clamp(8, (shot.height - total_h - 8).max(8));

    let mut frame = Bitmap::new(total_w, total_h);
    fill_rect(&mut frame, 0, 0, total_w, total_h, theme::MAG_BG);
    let dest = Rect::new(pad, pad, mag, mag);
    let half = src / 2;
    for y in 0..src {
        for x in 0..src {
            let sx = cursor.x - half + x;
            let sy = cursor.y - half + y;
            let c = shot.get(sx, sy);
            fill_rect(&mut frame, dest.x + x * zoom, dest.y + y * zoom, zoom, zoom, c);
        }
    }
    crate::draw::stroke_rect(
        &mut frame,
        dest.x as f32 + (src / 2 * zoom) as f32,
        dest.y as f32 + (src / 2 * zoom) as f32,
        zoom as f32,
        zoom as f32,
        theme::ACCENT,
        (1.5 * scale).max(1.0),
    );
    crate::draw::stroke_rect(
        &mut frame,
        0.5,
        0.5,
        (total_w - 1) as f32,
        (total_h - 1) as f32,
        theme::MAG_BORDER,
        (1.5 * scale).max(1.0),
    );
    let color = shot.get(cursor.x, cursor.y);
    let hex = format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    let rgb = format!("{}  {}  {}", color.r, color.g, color.b);
    let coord = format!("{}, {}", screen.x, screen.y);
    let info_y = dest.bottom() + sc(4, scale);
    crate::draw::draw_text(
        &mut frame,
        Point::new(pad + sc(2, scale), info_y),
        &hex,
        theme::TEXT,
        sc(12, scale),
    );
    crate::draw::draw_text(
        &mut frame,
        Point::new(pad + sc(74, scale), info_y),
        &rgb,
        theme::DIM,
        sc(12, scale),
    );
    crate::draw::draw_text(
        &mut frame,
        Point::new(pad + sc(2, scale), info_y + sc(16, scale)),
        &coord,
        theme::DIM,
        sc(12, scale),
    );
    blit_bitmap_canvas(ctx, &frame, Rect::new(bx, by, total_w, total_h));
}

extern "C" fn on_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut OverlayState)).paint(ctx);
        }
    }
}

extern "C" fn on_mouse_move(x: i32, y: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut OverlayState)).on_move(Point::new(x, y));
        }
    }
}

extern "C" fn on_mouse_down(x: i32, y: i32, btn: i32, _clicks: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut OverlayState)).on_down(Point::new(x, y), btn);
        }
    }
}

extern "C" fn on_mouse_up(x: i32, y: i32, btn: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut OverlayState)).on_up(Point::new(x, y), btn);
        }
    }
}

extern "C" fn on_key_down(vk: u32, _mods: u32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut OverlayState)).on_key(vk);
        }
    }
}

extern "C" fn on_text_commit(text: *const c_char, commit: bool, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut OverlayState);
        if commit && !text.is_null() {
            let s = std::ffi::CStr::from_ptr(text).to_string_lossy();
            if !s.trim().is_empty() {
                st.ann.add_text(st.text_at, &s);
                st.rebuild_composed();
            }
        }
        st.invalidate();
    }
}

extern "C" fn on_close(ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut OverlayState)).cancel();
        }
    }
}
