use crate::annotation::{AnnotKind, AnnotationSession, COLORS, WIDTHS, WIDTH_NAMES};
use crate::draw::{fill_circle, fill_rect, paint_arrow, stroke_ellipse, stroke_line, stroke_rect};
use crate::geom::{Color, Point, Rect};
use crate::theme;

pub const ANNOT_COUNT: usize = 8;
pub const MAX_COUNT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarResult {
    Miss,
    Chrome,
    Shape,
    Stroke,
    Pencil,
    Marker,
    Mosaic,
    AnnotText,
    Eraser,
    Undo,
    Scroll,
    Pin,
    Save,
    CopyImage,
    CopyPath,
    CopyText,
    Translate,
    Close,
    Color,
    Width,
}

impl ToolbarResult {
    pub fn to_action(self) -> Option<crate::settings::PostCaptureAction> {
        use crate::settings::PostCaptureAction::*;
        Some(match self {
            Self::Pin => Pin,
            Self::Save => SaveImage,
            Self::CopyImage => CopyImage,
            Self::CopyPath => CopyPath,
            Self::CopyText => CopyText,
            Self::Translate => Translate,
            _ => return None,
        })
    }
}

pub struct ActionToolbar {
    buttons: [Rect; MAX_COUNT],
    swatches: [Rect; 6],
    widths: [Rect; 4],
    pub bounds: Rect,
    pub hover_index: i32,
    pub pressed_index: i32,
    pub hover_color: i32,
    pub hover_width: i32,
    pub show_palette: bool,
    pub show_color: bool,
    pub undo_enabled: bool,
    pub show_scroll: bool,
    pub color_index: usize,
    pub width_index: usize,
    pub tool: AnnotKind,
    pub shape_kind: AnnotKind,
    pub stroke_kind: AnnotKind,
}

impl Default for ActionToolbar {
    fn default() -> Self {
        Self {
            buttons: [Rect::default(); MAX_COUNT],
            swatches: [Rect::default(); 6],
            widths: [Rect::default(); 4],
            bounds: Rect::default(),
            hover_index: -1,
            pressed_index: -1,
            hover_color: -1,
            hover_width: -1,
            show_palette: false,
            show_color: true,
            undo_enabled: false,
            show_scroll: false,
            color_index: 0,
            width_index: 1,
            tool: AnnotKind::Arrow,
            shape_kind: AnnotKind::Rect,
            stroke_kind: AnnotKind::Arrow,
        }
    }
}

impl ActionToolbar {
    pub fn visible_count(&self) -> usize {
        ANNOT_COUNT + if self.show_scroll { 8 } else { 7 }
    }

    fn sc(v: i32, scale: f32) -> i32 {
        ((v as f32 * scale).round() as i32).max(1)
    }

    pub fn sync(&mut self, ann: &AnnotationSession) {
        self.tool = ann.tool;
        self.shape_kind = ann.shape_kind();
        self.stroke_kind = ann.stroke_kind();
        self.show_palette = ann.show_palette();
        self.show_color = ann.show_color();
        self.undo_enabled = ann.has_marks() || ann.has_draft();
        self.color_index = ann.color_index;
        self.width_index = ann.width_index;
    }

    pub fn relayout(&mut self, selection: Rect, confine: Rect, scale: f32) {
        let n = self.visible_count() as i32;
        let pad = Self::sc(6, scale);
        let btn = Self::sc(32, scale);
        let gap = Self::sc(2, scale);
        let sep = Self::sc(8, scale);
        let pal = if self.show_palette {
            Self::sc(30, scale)
        } else {
            0
        };
        let w = pad * 2 + btn * n + gap * (n - 3) + sep * 2;
        let h = pad * 2 + btn + pal;
        let margin = Self::sc(8, scale);

        let mut x = selection.x + (selection.w - w) / 2;
        let mut y = selection.bottom() + margin;
        if y + h > confine.bottom() - 2 {
            y = selection.y - h - margin;
        }
        if y < confine.y + 2 {
            y = (selection.bottom() - h - margin)
                .clamp(confine.y + 2, (confine.bottom() - h - 2).max(confine.y + 2));
        }
        let min_x = confine.x + 2;
        let max_x = (confine.right() - w - 2).max(min_x);
        x = x.clamp(min_x, max_x);
        let min_y = confine.y + 2;
        let max_y = (confine.bottom() - h - 2).max(min_y);
        y = y.clamp(min_y, max_y);
        self.bounds = Rect::new(x, y, w, h);

        let mut bx = x + pad;
        let by = y + pad;
        for i in 0..n as usize {
            self.buttons[i] = Rect::new(bx, by, btn, btn);
            bx += btn
                + if i == ANNOT_COUNT - 1 || i == n as usize - 2 {
                    sep
                } else {
                    gap
                };
        }
        for i in n as usize..MAX_COUNT {
            self.buttons[i] = Rect::default();
        }

        if self.show_palette {
            let dot = Self::sc(12, scale);
            let dg = Self::sc(7, scale);
            let cn = if self.show_color { COLORS.len() as i32 } else { 0 };
            let chip_w = Self::sc(22, scale);
            let chip_h = Self::sc(16, scale);
            let wg = Self::sc(5, scale);
            let wn = WIDTHS.len() as i32;
            let pal_sep = if cn > 0 { Self::sc(12, scale) } else { 0 };
            let row_w = cn * dot + (cn - 1).max(0) * dg + pal_sep + wn * chip_w + (wn - 1) * wg;
            let mut sx = x + (w - row_w) / 2;
            let sy = by + btn + Self::sc(8, scale);
            for i in 0..COLORS.len() {
                self.swatches[i] = if cn > 0 {
                    Rect::new(sx, sy + (chip_h - dot) / 2, dot, dot)
                } else {
                    Rect::default()
                };
                if cn > 0 {
                    sx += dot + dg;
                }
            }
            if cn > 0 {
                sx += pal_sep - dg;
            }
            for i in 0..WIDTHS.len() {
                self.widths[i] = Rect::new(sx, sy, chip_w, chip_h);
                sx += chip_w + wg;
            }
        } else {
            self.swatches = [Rect::default(); 6];
            self.widths = [Rect::default(); 4];
        }
    }

    pub fn hit_test(&self, p: Point) -> i32 {
        for i in 0..self.visible_count() {
            if self.buttons[i].contains(p) {
                return i as i32;
            }
        }
        if self.bounds.contains(p) {
            -2
        } else {
            -1
        }
    }

    pub fn hit_color(&self, p: Point) -> i32 {
        if !self.show_palette || !self.show_color {
            return -1;
        }
        for (i, r) in self.swatches.iter().enumerate() {
            if r.contains(p) {
                return i as i32;
            }
        }
        -1
    }

    pub fn hit_width(&self, p: Point) -> i32 {
        if !self.show_palette {
            return -1;
        }
        for (i, r) in self.widths.iter().enumerate() {
            if r.contains(p) {
                return i as i32;
            }
        }
        -1
    }

    pub fn hit(&self, p: Point) -> ToolbarResult {
        let b = self.hit_test(p);
        if b >= 0 {
            return self.result_at(b as usize);
        }
        if self.hit_color(p) >= 0 {
            return ToolbarResult::Color;
        }
        if self.hit_width(p) >= 0 {
            return ToolbarResult::Width;
        }
        if self.bounds.contains(p) {
            ToolbarResult::Chrome
        } else {
            ToolbarResult::Miss
        }
    }

    pub fn result_at(&self, index: usize) -> ToolbarResult {
        if index < ANNOT_COUNT {
            return match index {
                0 => ToolbarResult::Shape,
                1 => ToolbarResult::Stroke,
                2 => ToolbarResult::Pencil,
                3 => ToolbarResult::Marker,
                4 => ToolbarResult::Mosaic,
                5 => ToolbarResult::AnnotText,
                6 => ToolbarResult::Eraser,
                7 => ToolbarResult::Undo,
                _ => ToolbarResult::Miss,
            };
        }
        let a = index - ANNOT_COUNT;
        if self.show_scroll {
            match a {
                0 => ToolbarResult::Scroll,
                1 => ToolbarResult::Pin,
                2 => ToolbarResult::Save,
                3 => ToolbarResult::CopyImage,
                4 => ToolbarResult::CopyPath,
                5 => ToolbarResult::CopyText,
                6 => ToolbarResult::Translate,
                7 => ToolbarResult::Close,
                _ => ToolbarResult::Miss,
            }
        } else {
            match a {
                0 => ToolbarResult::Pin,
                1 => ToolbarResult::Save,
                2 => ToolbarResult::CopyImage,
                3 => ToolbarResult::CopyPath,
                4 => ToolbarResult::CopyText,
                5 => ToolbarResult::Translate,
                6 => ToolbarResult::Close,
                _ => ToolbarResult::Miss,
            }
        }
    }

    pub fn set_hover(&mut self, p: Point) -> bool {
        let mut h = self.hit_test(p);
        if h < 0 {
            h = -1;
        }
        let c = self.hit_color(p);
        let w = self.hit_width(p);
        if h == self.hover_index && c == self.hover_color && w == self.hover_width {
            return false;
        }
        self.hover_index = h;
        self.hover_color = c;
        self.hover_width = w;
        true
    }

    #[allow(dead_code)]
    pub fn paint(&self, dest: &mut crate::bitmap::Bitmap, scale: f32) {
        self.paint_at(dest, scale, Point::new(0, 0));
    }

    pub fn paint_at(&self, dest: &mut crate::bitmap::Bitmap, scale: f32, origin: Point) {
        if self.bounds.w < 8 {
            return;
        }
        let loc = |r: Rect| Rect::new(r.x - origin.x, r.y - origin.y, r.w, r.h);
        fill_round(
            dest,
            loc(self.bounds),
            Self::sc(8, scale),
            Color::argb(242, 18, 22, 30),
        );
        let n = self.visible_count();
        for i in 0..n {
            self.paint_button(dest, i, scale, origin);
        }
        if self.show_palette {
            self.paint_palette(dest, scale, origin);
        }
    }

    fn is_checked(&self, kind: ToolbarResult) -> bool {
        match kind {
            ToolbarResult::Shape => matches!(self.tool, AnnotKind::Rect | AnnotKind::Ellipse),
            ToolbarResult::Stroke => matches!(self.tool, AnnotKind::Line | AnnotKind::Arrow),
            ToolbarResult::Pencil => self.tool == AnnotKind::Pencil,
            ToolbarResult::Marker => self.tool == AnnotKind::Marker,
            ToolbarResult::Mosaic => self.tool == AnnotKind::Mosaic,
            ToolbarResult::AnnotText => self.tool == AnnotKind::Text,
            ToolbarResult::Eraser => self.tool == AnnotKind::Eraser,
            _ => false,
        }
    }

    fn paint_button(&self, dest: &mut crate::bitmap::Bitmap, i: usize, scale: f32, origin: Point) {
        let r = Rect::new(
            self.buttons[i].x - origin.x,
            self.buttons[i].y - origin.y,
            self.buttons[i].w,
            self.buttons[i].h,
        );
        if r.w < 2 {
            return;
        }
        let kind = self.result_at(i);
        let hover = self.hover_index == i as i32;
        let pressed = self.pressed_index == i as i32;
        let checked = self.is_checked(kind);
        if hover || pressed || checked {
            fill_round(dest, r, Self::sc(6, scale), theme::ACCENT_DARK);
        }
        let color = if kind == ToolbarResult::Close && hover {
            theme::DANGER
        } else if kind == ToolbarResult::Undo && !self.undo_enabled {
            theme::DIM
        } else if hover || checked {
            theme::ACCENT_HI
        } else {
            theme::TEXT
        };
        let icon = icon_box(r);
        let sw = (1.5 * scale).max(1.2);
        match kind {
            ToolbarResult::Shape => draw_icon_shape(dest, icon, self.shape_kind, color, sw),
            ToolbarResult::Stroke => draw_icon_stroke(dest, icon, self.stroke_kind, color, sw),
            ToolbarResult::Pencil => draw_icon_pencil(dest, icon, color, sw),
            ToolbarResult::Marker => draw_icon_marker(dest, icon, color, scale),
            ToolbarResult::Mosaic => draw_icon_mosaic(dest, icon, color),
            ToolbarResult::AnnotText => draw_icon_text(dest, icon, color, sw),
            ToolbarResult::Eraser => draw_icon_eraser(dest, icon, color, sw),
            ToolbarResult::Undo => draw_icon_undo(dest, icon, color, sw),
            ToolbarResult::Scroll => draw_icon_scroll(dest, icon, color, sw),
            ToolbarResult::Pin => draw_icon_pin(dest, icon, color, sw),
            ToolbarResult::Save => draw_icon_save(dest, icon, color, sw),
            ToolbarResult::CopyImage => draw_icon_copy_image(dest, icon, color, sw),
            ToolbarResult::CopyPath => draw_icon_copy_path(dest, icon, color, sw),
            ToolbarResult::CopyText => draw_icon_copy_text(dest, icon, color, sw),
            ToolbarResult::Translate => draw_icon_translate(dest, icon, color, scale),
            _ => draw_icon_close(dest, icon, color, sw),
        }
    }

    fn paint_palette(&self, dest: &mut crate::bitmap::Bitmap, scale: f32, origin: Point) {
        if self.show_color {
            for (i, &col) in COLORS.iter().enumerate() {
                let r = Rect::new(
                    self.swatches[i].x - origin.x,
                    self.swatches[i].y - origin.y,
                    self.swatches[i].w,
                    self.swatches[i].h,
                );
                if r.w < 2 {
                    continue;
                }
                fill_circle(
                    dest,
                    r.x as f32 + r.w as f32 / 2.0,
                    r.y as f32 + r.h as f32 / 2.0,
                    r.w as f32 / 2.0,
                    col,
                );
            }
        }
        for i in 0..WIDTHS.len() {
            let r = Rect::new(
                self.widths[i].x - origin.x,
                self.widths[i].y - origin.y,
                self.widths[i].w,
                self.widths[i].h,
            );
            if r.w < 2 {
                continue;
            }
            let on = i == self.width_index || self.hover_width == i as i32;
            if on {
            fill_round(dest, r, Self::sc(4, scale), theme::ACCENT_DARK);
            }
            let lw = 1.4 + i as f32 * 1.7;
            stroke_line(
                dest,
                r.x as f32 + 3.0,
                r.y as f32 + r.h as f32 / 2.0,
                r.right() as f32 - 3.0,
                r.y as f32 + r.h as f32 / 2.0,
                if i == self.width_index {
                    theme::ACCENT_HI
                } else {
                    theme::TEXT
                },
                lw,
            );
        }
        let _ = WIDTH_NAMES;
    }

    pub fn tip_at(&self, i: usize) -> &'static str {
        match self.result_at(i) {
            ToolbarResult::Shape if self.shape_kind == AnnotKind::Ellipse => {
                "椭圆  ·  Tab 切方框  ·  Shift 正圆"
            }
            ToolbarResult::Shape => "方框  ·  Tab 切椭圆  ·  Shift 正方形",
            ToolbarResult::Stroke if self.stroke_kind == AnnotKind::Line => {
                "直线  ·  Tab 切箭头  ·  Shift 45°"
            }
            ToolbarResult::Stroke => "箭头  ·  A  ·  Tab 切直线  ·  Shift 45°",
            ToolbarResult::Pencil => "铅笔  ·  B",
            ToolbarResult::Marker => "荧光笔  ·  H",
            ToolbarResult::Mosaic => "马赛克  ·  M",
            ToolbarResult::AnnotText => "文字  ·  X",
            ToolbarResult::Eraser => "橡皮  ·  E",
            ToolbarResult::Undo => "撤销  ·  Z",
            ToolbarResult::Scroll => "滚动截图  ·  R",
            ToolbarResult::Pin => "贴到桌面  ·  T",
            ToolbarResult::Save => "保存图片  ·  S",
            ToolbarResult::CopyImage => "复制图片  ·  C",
            ToolbarResult::CopyPath => "复制图片地址  ·  P",
            ToolbarResult::CopyText => "复制文字  ·  O",
            ToolbarResult::Translate => "翻译  ·  L",
            ToolbarResult::Close => "取消  ·  Esc",
            _ => "",
        }
    }

    pub fn hover_tip(&self, scale: f32) -> Option<(Rect, String)> {
        let text = if self.hover_width >= 0 {
            WIDTH_NAMES
                .get(self.hover_width as usize)
                .unwrap_or(&"")
                .to_string()
        } else if self.hover_index >= 0 {
            self.tip_at(self.hover_index as usize).to_string()
        } else {
            return None;
        };
        if text.is_empty() {
            return None;
        }
        let px = Self::sc(13, scale);
        let (tw0, th0) = crate::draw::measure_text(&text, px);
        let tw = tw0 + Self::sc(14, scale);
        let th = th0 + Self::sc(8, scale);
        let anchor = if self.hover_width >= 0 {
            self.widths.get(self.hover_width as usize).copied()?
        } else {
            self.buttons.get(self.hover_index as usize).copied()?
        };
        if anchor.w < 2 {
            return None;
        }
        let mut x = anchor.x + (anchor.w - tw) / 2;
        let mut y = self.bounds.y - th - Self::sc(6, scale);
        if y < 2 {
            y = self.bounds.bottom() + Self::sc(6, scale);
        }
        x = x.max(2);
        Some((Rect::new(x, y, tw, th), text))
    }
}

fn icon_box(btn: Rect) -> Rect {
    let s = (btn.w.min(btn.h) as f32 * 0.52).round() as i32;
    Rect::new(btn.x + (btn.w - s) / 2, btn.y + (btn.h - s) / 2, s, s)
}

fn draw_icon_shape(
    dest: &mut crate::bitmap::Bitmap,
    icon: Rect,
    shape_kind: AnnotKind,
    color: Color,
    sw: f32,
) {
    let m = 1.0;
    let x = icon.x as f32 + m;
    let y = icon.y as f32 + m;
    let w = (icon.w as f32 - m * 2.0).max(2.0);
    let h = (icon.h as f32 - m * 2.0).max(2.0);
    if shape_kind == AnnotKind::Ellipse {
        stroke_ellipse(dest, x, y, w, h, color, sw);
    } else {
        stroke_rect(dest, x, y, w, h, color, sw);
    }
}

fn draw_icon_stroke(
    dest: &mut crate::bitmap::Bitmap,
    icon: Rect,
    stroke_kind: AnnotKind,
    color: Color,
    sw: f32,
) {
    let m = 2.0;
    let p1 = Point::new(icon.x + m as i32, icon.bottom() - m as i32);
    let p2 = Point::new(icon.right() - m as i32, icon.y + m as i32);
    if stroke_kind == AnnotKind::Line {
        stroke_line(dest, p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32, color, sw);
    } else {
        paint_arrow(dest, p1, p2, color, sw);
    }
}

fn draw_icon_pencil(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let tip = Point::new(icon.x + 2, icon.bottom() - 2);
    let cap = Point::new(icon.right() - 2, icon.y + 2);
    let dx = (cap.x - tip.x) as f32;
    let dy = (cap.y - tip.y) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 4.0 {
        stroke_line(dest, tip.x as f32, tip.y as f32, cap.x as f32, cap.y as f32, color, sw);
        return;
    }
    let nx = -dy / len * 2.2;
    let ny = dx / len * 2.2;
    let b1_x = tip.x as f32 + dx * 0.28 + nx;
    let b1_y = tip.y as f32 + dy * 0.28 + ny;
    let b2_x = tip.x as f32 + dx * 0.28 - nx;
    let b2_y = tip.y as f32 + dy * 0.28 - ny;
    let c1_x = cap.x as f32 + nx;
    let c1_y = cap.y as f32 + ny;
    let c2_x = cap.x as f32 - nx;
    let c2_y = cap.y as f32 - ny;
    stroke_line(dest, tip.x as f32, tip.y as f32, b1_x, b1_y, color, sw.min(1.4));
    stroke_line(dest, tip.x as f32, tip.y as f32, b2_x, b2_y, color, sw.min(1.4));
    fill_circle(dest, tip.x as f32, tip.y as f32, 1.2, color);
    stroke_line(dest, b1_x, b1_y, c1_x, c1_y, color, sw.min(1.4));
    stroke_line(dest, b2_x, b2_y, c2_x, c2_y, color, sw.min(1.4));
    stroke_line(dest, c1_x, c1_y, c2_x, c2_y, color, sw.min(1.4));
}

fn draw_icon_marker(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, scale: f32) {
    let sw = (3.2 * scale).max(2.4);
    let p1 = Point::new(icon.x + 3, icon.bottom() - 5);
    let p2 = Point::new(icon.right() - 4, icon.y + 3);
    stroke_line(dest, p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32, color, sw);
    let mark_y = icon.bottom() as f32 - 2.5;
    stroke_line(
        dest,
        icon.x as f32 + 1.0,
        mark_y,
        icon.right() as f32 - 1.0,
        mark_y,
        color.with_alpha(150),
        (2.8 * scale).max(2.0),
    );
}

fn draw_icon_mosaic(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color) {
    let size = (icon.w.min(icon.h) / 3).max(2);
    let start_x = icon.x + (icon.w - size * 3) / 2;
    let start_y = icon.y + (icon.h - size * 3) / 2;
    for row in 0..3 {
        for col in 0..3 {
            let cx = start_x + col * size;
            let cy = start_y + row * size;
            let pad = 1;
            let c = if (row + col) % 2 == 0 {
                color
            } else {
                color.with_alpha(75)
            };
            fill_rect(dest, cx, cy, (size - pad).max(1), (size - pad).max(1), c);
        }
    }
}

fn draw_icon_text(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let x = icon.x as f32 + 2.0;
    let y = icon.y as f32 + 2.0;
    let w = (icon.w as f32 - 4.0).max(4.0);
    let h = (icon.h as f32 - 4.0).max(4.0);
    let cx = x + w * 0.5;
    let top_w = (sw + 0.4).max(1.5);
    stroke_line(dest, x, y, x + w, y, color, top_w);
    stroke_line(dest, x, y, x, y + 2.0, color, top_w);
    stroke_line(dest, x + w, y, x + w, y + 2.0, color, top_w);
    stroke_line(dest, cx, y, cx, y + h, color, top_w);
    stroke_line(dest, cx - 2.5, y + h, cx + 2.5, y + h, color, top_w);
}

fn draw_icon_eraser(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let x = icon.x as f32 + 2.0;
    let y = icon.y as f32 + 4.0;
    let w = (icon.w as f32 - 4.0).max(6.0);
    let h = (icon.h as f32 - 8.0).max(4.0);
    let skew = 3.0f32;
    let p1 = (x + skew, y);
    let p2 = (x + w, y);
    let p3 = (x + w - skew, y + h);
    let p4 = (x, y + h);
    stroke_line(dest, p1.0, p1.1, p2.0, p2.1, color, sw);
    stroke_line(dest, p2.0, p2.1, p3.0, p3.1, color, sw);
    stroke_line(dest, p3.0, p3.1, p4.0, p4.1, color, sw);
    stroke_line(dest, p4.0, p4.1, p1.0, p1.1, color, sw);
    let band_x1 = x + w * 0.45;
    let band_y1 = y;
    let band_x2 = x + w * 0.45 - skew;
    let band_y2 = y + h;
    stroke_line(dest, band_x1, band_y1, band_x2, band_y2, color.with_alpha(180), sw.min(1.4));
}

fn draw_icon_undo(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let cx = icon.x as f32 + icon.w as f32 * 0.52;
    let cy = icon.y as f32 + icon.h as f32 * 0.55;
    let rx = (icon.w as f32 * 0.38).max(3.0);
    let ry = (icon.h as f32 * 0.38).max(3.0);
    let steps = 14;
    let start_angle = std::f32::consts::PI * 0.25;
    let end_angle = std::f32::consts::PI * 1.25;
    let mut prev_x = cx + rx * start_angle.cos();
    let mut prev_y = cy - ry * start_angle.sin();
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let angle = start_angle + (end_angle - start_angle) * t;
        let px = cx + rx * angle.cos();
        let py = cy - ry * angle.sin();
        stroke_line(dest, prev_x, prev_y, px, py, color, sw);
        prev_x = px;
        prev_y = py;
    }
    let tip_x = prev_x;
    let tip_y = prev_y;
    stroke_line(dest, tip_x, tip_y, tip_x + 4.5, tip_y - 1.0, color, sw);
    stroke_line(dest, tip_x, tip_y, tip_x + 2.0, tip_y + 4.5, color, sw);
}

fn draw_icon_scroll(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let x = icon.x as f32 + 3.0;
    let w = (icon.w as f32 - 6.0).max(6.0);
    let y = icon.y as f32 + 1.5;
    let h = (icon.h as f32 - 3.0).max(8.0);
    stroke_line(dest, x, y, x, y + h, color, sw);
    stroke_line(dest, x + w, y, x + w, y + h, color, sw);
    stroke_line(dest, x, y, x + w * 0.3, y, color, sw);
    stroke_line(dest, x + w * 0.7, y, x + w, y, color, sw);
    stroke_line(dest, x, y + h, x + w * 0.3, y + h, color, sw);
    stroke_line(dest, x + w * 0.7, y + h, x + w, y + h, color, sw);
    let cx = x + w * 0.5;
    let c1_y = y + h * 0.35;
    stroke_line(dest, cx - 2.5, c1_y - 2.0, cx, c1_y + 1.0, color, sw);
    stroke_line(dest, cx + 2.5, c1_y - 2.0, cx, c1_y + 1.0, color, sw);
    let c2_y = y + h * 0.68;
    stroke_line(dest, cx - 2.5, c2_y - 2.0, cx, c2_y + 1.0, color, sw);
    stroke_line(dest, cx + 2.5, c2_y - 2.0, cx, c2_y + 1.0, color, sw);
}

fn draw_icon_pin(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let cx = icon.x as f32 + icon.w as f32 * 0.58;
    let cy = icon.y as f32 + icon.h as f32 * 0.42;
    stroke_line(dest, cx - 1.0, cy - 5.0, cx + 5.0, cy + 1.0, color, sw + 0.6);
    stroke_line(dest, cx + 2.0, cy - 2.0, cx - 2.0, cy + 2.0, color, sw + 1.2);
    stroke_line(dest, cx - 4.0, cy - 1.0, cx + 1.0, cy + 4.0, color, sw);
    let needle_tip_x = icon.x as f32 + 2.0;
    let needle_tip_y = icon.bottom() as f32 - 2.0;
    stroke_line(dest, cx - 1.5, cy + 1.5, needle_tip_x, needle_tip_y, color, sw.min(1.4));
}

fn draw_icon_save(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let x = icon.x as f32 + 1.5;
    let r = icon.right() as f32 - 1.5;
    let b = icon.bottom() as f32 - 1.5;
    let tray_top = b - icon.h as f32 * 0.32;
    stroke_line(dest, x, tray_top, x, b, color, sw);
    stroke_line(dest, x, b, r, b, color, sw);
    stroke_line(dest, r, b, r, tray_top, color, sw);
    let cx = icon.x as f32 + icon.w as f32 * 0.5;
    let arrow_tip_y = b - 3.5;
    let arrow_top_y = icon.y as f32 + 1.5;
    stroke_line(dest, cx, arrow_top_y, cx, arrow_tip_y, color, sw);
    let wing_span = (icon.w as f32 * 0.26).max(3.0);
    let wing_y = arrow_tip_y - (icon.h as f32 * 0.24).max(3.0);
    stroke_line(dest, cx - wing_span, wing_y, cx, arrow_tip_y, color, sw);
    stroke_line(dest, cx + wing_span, wing_y, cx, arrow_tip_y, color, sw);
}

fn draw_icon_copy_image(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let w = (icon.w as f32 * 0.68).round();
    let h = (icon.h as f32 * 0.68).round();
    stroke_rect(
        dest,
        icon.x as f32 + icon.w as f32 - w,
        icon.y as f32,
        w,
        h,
        color.with_alpha(160),
        sw,
    );
    let fx = icon.x as f32;
    let fy = icon.bottom() as f32 - h;
    fill_rect(
        dest,
        fx as i32,
        fy as i32,
        w as i32,
        h as i32,
        Color::argb(220, 18, 22, 30),
    );
    stroke_rect(dest, fx, fy, w, h, color, sw);
    stroke_line(
        dest,
        fx + 2.0,
        fy + h - 2.0,
        fx + w * 0.42,
        fy + h * 0.40,
        color,
        sw.min(1.2),
    );
    stroke_line(
        dest,
        fx + w * 0.42,
        fy + h * 0.40,
        fx + w - 2.0,
        fy + h - 2.0,
        color,
        sw.min(1.2),
    );
    fill_circle(dest, fx + w * 0.72, fy + h * 0.35, 1.2, color);
}

fn draw_icon_copy_path(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let cx = icon.x as f32 + icon.w as f32 * 0.5;
    let cy = icon.y as f32 + icon.h as f32 * 0.5;
    let d = (icon.w as f32 * 0.26).max(3.0);
    stroke_ellipse(dest, cx - d - 2.5, cy + d - 2.5, 5.0, 5.0, color, sw);
    stroke_ellipse(dest, cx + d - 2.5, cy - d - 2.5, 5.0, 5.0, color, sw);
    stroke_line(
        dest,
        cx - d * 0.5,
        cy + d * 0.5,
        cx + d * 0.5,
        cy - d * 0.5,
        color,
        sw + 0.4,
    );
}

fn draw_icon_copy_text(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let x = icon.x as f32 + 1.5;
    let y = icon.y as f32 + 1.5;
    let r = icon.right() as f32 - 1.5;
    let b = icon.bottom() as f32 - 1.5;
    let bracket = (icon.w as f32 * 0.25).max(3.0);
    stroke_line(dest, x, y, x + bracket, y, color, sw);
    stroke_line(dest, x, y, x, y + bracket, color, sw);
    stroke_line(dest, r - bracket, y, r, y, color, sw);
    stroke_line(dest, r, y, r, y + bracket, color, sw);
    stroke_line(dest, x, b, x + bracket, b, color, sw);
    stroke_line(dest, x, b, x, b - bracket, color, sw);
    stroke_line(dest, r - bracket, b, r, b, color, sw);
    stroke_line(dest, r, b, r, b - bracket, color, sw);
    let cx = (x + r) * 0.5;
    let cy = (y + b) * 0.5;
    let tw = bracket * 1.3;
    let th = bracket * 1.4;
    stroke_line(dest, cx - tw * 0.5, cy - th * 0.5, cx + tw * 0.5, cy - th * 0.5, color, sw);
    stroke_line(dest, cx, cy - th * 0.5, cx, cy + th * 0.5, color, sw);
}

fn draw_icon_translate(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, scale: f32) {
    let ax = icon.x as f32 + 0.5;
    let ay = icon.y as f32 + 0.5;
    let aw = (icon.w as f32 * 0.46).round();
    let ah = (icon.h as f32 * 0.52).round();
    let sw_a = (1.4 * scale).max(1.1);
    stroke_line(dest, ax + aw * 0.5, ay, ax, ay + ah, color, sw_a);
    stroke_line(dest, ax + aw * 0.5, ay, ax + aw, ay + ah, color, sw_a);
    stroke_line(
        dest,
        ax + aw * 0.22,
        ay + ah * 0.62,
        ax + aw * 0.78,
        ay + ah * 0.62,
        color,
        sw_a,
    );

    let cw = (icon.w as f32 * 0.54).round();
    let ch = (icon.h as f32 * 0.58).round();
    let cx = icon.right() as f32 - cw;
    let cy = icon.bottom() as f32 - ch;
    let sw_c = (1.3 * scale).max(1.1);
    stroke_line(dest, cx + cw * 0.5, cy, cx + cw * 0.5, cy + ch * 0.20, color, sw_c);
    stroke_line(dest, cx, cy + ch * 0.22, cx + cw, cy + ch * 0.22, color, sw_c);
    stroke_line(
        dest,
        cx + cw * 0.5,
        cy + ch * 0.22,
        cx + cw * 0.32,
        cy + ch * 0.60,
        color,
        sw_c,
    );
    stroke_line(
        dest,
        cx + cw * 0.32,
        cy + ch * 0.60,
        cx + cw * 0.05,
        cy + ch,
        color,
        sw_c,
    );
    stroke_line(
        dest,
        cx + cw * 0.40,
        cy + ch * 0.35,
        cx + cw * 0.95,
        cy + ch,
        color,
        sw_c,
    );
}

fn draw_icon_close(dest: &mut crate::bitmap::Bitmap, icon: Rect, color: Color, sw: f32) {
    let m = 2.0;
    let x1 = icon.x as f32 + m;
    let y1 = icon.y as f32 + m;
    let x2 = icon.right() as f32 - m;
    let y2 = icon.bottom() as f32 - m;
    stroke_line(dest, x1, y1, x2, y2, color, sw + 0.2);
    stroke_line(dest, x2, y1, x1, y2, color, sw + 0.2);
}

fn fill_round(bmp: &mut crate::bitmap::Bitmap, r: Rect, radius: i32, c: Color) {
    let rad = radius.min(r.w / 2).min(r.h / 2).max(1) as f32;
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if round_hit(px, py, r.x as f32, r.y as f32, r.w as f32, r.h as f32, rad) {
                bmp.blend(x, y, c);
            }
        }
    }
}

fn round_hit(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> bool {
    if px < x || py < y || px >= x + w || py >= y + h {
        return false;
    }
    let cx = if px < x + r {
        x + r
    } else if px > x + w - r {
        x + w - r
    } else {
        return true;
    };
    let cy = if py < y + r {
        y + r
    } else if py > y + h - r {
        y + h - r
    } else {
        return true;
    };
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= r * r
}
