use crate::bitmap::Bitmap;
use crate::draw::{
    draw_text, fill_circle, fill_rect, paint_arrow, stroke_ellipse, stroke_line, stroke_rect,
};
use crate::geom::{Color, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotKind {
    None,
    Rect,
    Ellipse,
    Line,
    Arrow,
    Pencil,
    Marker,
    Mosaic,
    Text,
    Eraser,
}

impl AnnotKind {
    pub fn is_path(self) -> bool {
        matches!(
            self,
            Self::Pencil | Self::Marker | Self::Mosaic | Self::Eraser
        )
    }
}

#[derive(Clone)]
pub struct AnnotMark {
    pub kind: AnnotKind,
    pub color: Color,
    pub width: f32,
    pub from: Point,
    pub to: Point,
    pub path: Option<Vec<Point>>,
    pub text: Option<String>,
}

pub struct AnnotationSession {
    marks: Vec<AnnotMark>,
    draft: Option<AnnotMark>,
    source: Option<Bitmap>,
    layer: Option<Bitmap>,
    shape_kind: AnnotKind,
    stroke_kind: AnnotKind,
    pub tool: AnnotKind,
    pub color_index: usize,
    pub width_index: usize,
}

pub const COLORS: [Color; 6] = [
    Color::rgb(0xF2, 0x3B, 0x3B),
    Color::rgb(0xFF, 0x8C, 0x1A),
    Color::rgb(0xF5, 0xD0, 0x20),
    Color::rgb(0x2D, 0xE2, 0xA8),
    Color::rgb(0xF2, 0xF5, 0xF8),
    Color::rgb(0x1A, 0x1E, 0x24),
];

pub const WIDTHS: [f32; 4] = [2.2, 4.0, 6.5, 10.0];
pub const WIDTH_NAMES: [&str; 4] = ["细", "中", "粗", "特粗"];

impl Default for AnnotationSession {
    fn default() -> Self {
        Self {
            marks: Vec::new(),
            draft: None,
            source: None,
            layer: None,
            shape_kind: AnnotKind::Rect,
            stroke_kind: AnnotKind::Arrow,
            tool: AnnotKind::Arrow,
            color_index: 0,
            width_index: 1,
        }
    }
}

impl AnnotationSession {
    pub fn attach(&mut self, _source: &Bitmap) {
        self.source = None;
        self.layer = None;
    }

    pub fn shape_kind(&self) -> AnnotKind {
        self.shape_kind
    }
    pub fn stroke_kind(&self) -> AnnotKind {
        self.stroke_kind
    }
    pub fn has_draft(&self) -> bool {
        self.draft.is_some()
    }
    pub fn draft(&self) -> Option<&AnnotMark> {
        self.draft.as_ref()
    }
    pub fn has_marks(&self) -> bool {
        !self.marks.is_empty()
    }
    pub fn tool_active(&self) -> bool {
        self.tool != AnnotKind::None
    }
    pub fn can_draw(&self) -> bool {
        !matches!(self.tool, AnnotKind::None | AnnotKind::Text)
    }
    pub fn is_text_tool(&self) -> bool {
        self.tool == AnnotKind::Text
    }
    pub fn show_palette(&self) -> bool {
        self.tool_active()
    }
    pub fn show_color(&self) -> bool {
        !matches!(self.tool, AnnotKind::None | AnnotKind::Mosaic | AnnotKind::Eraser)
    }
    pub fn color(&self) -> Color {
        COLORS[self.color_index.min(COLORS.len() - 1)]
    }
    pub fn width(&self) -> f32 {
        WIDTHS[self.width_index.min(WIDTHS.len() - 1)]
    }

    pub fn select(&mut self, kind: AnnotKind) {
        self.cancel_draft();
        if matches!(kind, AnnotKind::Rect | AnnotKind::Ellipse) {
            self.shape_kind = kind;
        }
        if matches!(kind, AnnotKind::Line | AnnotKind::Arrow) {
            self.stroke_kind = kind;
        }
        self.tool = kind;
    }

    pub fn toggle_shape(&mut self) {
        self.cancel_draft();
        self.tool = if matches!(self.tool, AnnotKind::Rect | AnnotKind::Ellipse) {
            AnnotKind::None
        } else {
            self.shape_kind
        };
    }

    pub fn toggle_stroke(&mut self) {
        self.cancel_draft();
        self.tool = if matches!(self.tool, AnnotKind::Line | AnnotKind::Arrow) {
            AnnotKind::None
        } else {
            self.stroke_kind
        };
    }

    pub fn toggle(&mut self, kind: AnnotKind) {
        self.cancel_draft();
        if matches!(kind, AnnotKind::Rect | AnnotKind::Ellipse) {
            self.shape_kind = kind;
            self.tool = if self.tool == kind {
                AnnotKind::None
            } else {
                kind
            };
            return;
        }
        if matches!(kind, AnnotKind::Line | AnnotKind::Arrow) {
            self.stroke_kind = kind;
            self.tool = if self.tool == kind {
                AnnotKind::None
            } else {
                kind
            };
            return;
        }
        self.tool = if self.tool == kind {
            AnnotKind::None
        } else {
            kind
        };
    }

    pub fn cycle_tab(&mut self) {
        self.cancel_draft();
        if matches!(self.tool, AnnotKind::Rect | AnnotKind::Ellipse) {
            self.tool = if self.tool == AnnotKind::Rect {
                AnnotKind::Ellipse
            } else {
                AnnotKind::Rect
            };
            self.shape_kind = self.tool;
        } else if matches!(self.tool, AnnotKind::Line | AnnotKind::Arrow) {
            self.tool = if self.tool == AnnotKind::Line {
                AnnotKind::Arrow
            } else {
                AnnotKind::Line
            };
            self.stroke_kind = self.tool;
        }
    }

    pub fn begin(&mut self, from: Point) {
        if !self.can_draw() {
            return;
        }
        let mut m = AnnotMark {
            kind: self.tool,
            color: self.color(),
            width: self.width(),
            from,
            to: from,
            path: None,
            text: None,
        };
        if self.tool.is_path() {
            m.path = Some(vec![from]);
        }
        self.draft = Some(m);
    }

    pub fn move_to(&mut self, to: Point, snap: bool) {
        let Some(d) = self.draft.as_mut() else {
            return;
        };
        if let Some(path) = d.path.as_mut() {
            if let Some(last) = path.last() {
                let dx = to.x - last.x;
                let dy = to.y - last.y;
                if dx * dx + dy * dy >= 4 {
                    path.push(to);
                }
            }
            d.to = to;
            return;
        }
        d.to = constrain(d.from, to, d.kind, snap);
    }

    pub fn cancel_draft(&mut self) {
        self.draft = None;
    }

    pub fn commit_draft(&mut self) -> bool {
        let Some(d) = self.draft.take() else {
            return false;
        };
        if let Some(ref path) = d.path {
            if path.is_empty() {
                return false;
            }
            self.marks.push(d.clone());
            return true;
        }
        let dx = d.to.x - d.from.x;
        let dy = d.to.y - d.from.y;
        if dx * dx + dy * dy < 64 {
            return false;
        }
        self.marks.push(d.clone());
        true
    }

    pub fn add_text(&mut self, at: Point, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let mark = AnnotMark {
            kind: AnnotKind::Text,
            color: self.color(),
            width: self.width(),
            from: at,
            to: at,
            path: None,
            text: Some(text.to_string()),
        };
        self.marks.push(mark.clone());
    }

    pub fn undo(&mut self) -> bool {
        if self.draft.take().is_some() {
            return true;
        }
        if self.marks.is_empty() {
            return false;
        }
        self.marks.pop();
        true
    }

    pub fn set_width_index(&mut self, i: usize) {
        self.width_index = i.min(WIDTHS.len() - 1);
        let w = self.width();
        if let Some(d) = self.draft.as_mut() {
            d.width = w;
        }
    }

    pub fn stamp(&mut self, dest: &mut Bitmap, origin: Point, source: Option<&Bitmap>) {
        let _ = self.commit_draft();
        let src = crate::geom::Rect::new(origin.x, origin.y, dest.width, dest.height);
        let dest_r = crate::geom::Rect::new(0, 0, dest.width, dest.height);
        for m in &self.marks {
            paint_mark_mapped(dest, m, src, dest_r, false, source);
        }
    }

    #[allow(dead_code)]
    pub fn paint_into(&self, dest: &mut Bitmap, src: crate::geom::Rect, dest_r: crate::geom::Rect) {
        if let Some(layer) = &self.layer {
            // nearest neighbor scale
            if dest_r.w < 1 || dest_r.h < 1 || src.w < 1 || src.h < 1 {
                return;
            }
            for y in 0..dest_r.h {
                let sy = src.y + y * src.h / dest_r.h;
                for x in 0..dest_r.w {
                    let sx = src.x + x * src.w / dest_r.w;
                    let c = layer.get(sx, sy);
                    if c.a > 0 {
                        dest.blend(dest_r.x + x, dest_r.y + y, c);
                    }
                }
            }
        }
        if let Some(d) = &self.draft {
            let mut tmp = dest.clone();
            // paint draft in dest space by mapping
            paint_mark_mapped(&mut tmp, d, src, dest_r, true, self.source.as_ref());
            dest.pixels.copy_from_slice(&tmp.pixels);
        }
    }

    /// Paint committed marks onto dest. dest (0,0) maps to src origin in screenshot space.
    pub fn paint_committed(&self, dest: &mut Bitmap, src: crate::geom::Rect, source: Option<&Bitmap>) {
        let dest_r = crate::geom::Rect::new(0, 0, dest.width, dest.height);
        for m in &self.marks {
            paint_mark_mapped(dest, m, src, dest_r, false, source);
        }
    }

    pub fn paint_draft_hdc<F>(&self, hdc: windows::Win32::Graphics::Gdi::HDC, map: F, width_scale: f32)
    where
        F: Fn(Point) -> Point,
    {
        let Some(d) = &self.draft else {
            return;
        };
        let w = (d.width * width_scale.max(0.15)).max(1.0);
        let from = map(d.from);
        let to = map(d.to);
        match d.kind {
            AnnotKind::Arrow => crate::draw::paint_arrow_hdc(hdc, from, to, d.color, w),
            AnnotKind::Line => {
                crate::native::line_hdc(
                    hdc,
                    from.x,
                    from.y,
                    to.x,
                    to.y,
                    Color::argb(140, 0, 0, 0),
                    (w + 2.2).round() as i32,
                );
                crate::native::line_hdc(hdc, from.x, from.y, to.x, to.y, d.color, w.round().max(1.0) as i32);
            }
            AnnotKind::Rect => {
                let r = crate::geom::Rect::from_ltrb(
                    from.x.min(to.x),
                    from.y.min(to.y),
                    from.x.max(to.x),
                    from.y.max(to.y),
                );
                crate::native::stroke_rect_hdc(hdc, r, Color::argb(140, 0, 0, 0), (w + 2.0).round() as i32);
                crate::native::stroke_rect_hdc(hdc, r, d.color, w.round().max(1.0) as i32);
            }
            AnnotKind::Ellipse => {
                let r = crate::geom::Rect::from_ltrb(
                    from.x.min(to.x),
                    from.y.min(to.y),
                    from.x.max(to.x),
                    from.y.max(to.y),
                );
                crate::native::ellipse_stroke_hdc(hdc, r, d.color, w.round().max(1.0) as i32);
            }
            AnnotKind::Pencil | AnnotKind::Marker | AnnotKind::Eraser => {
                if let Some(path) = &d.path {
                    let pts: Vec<Point> = path.iter().copied().map(&map).collect();
                    let cw = if d.kind == AnnotKind::Marker {
                        w * 2.7
                    } else if d.kind == AnnotKind::Eraser {
                        w * 2.2
                    } else {
                        w
                    };
                    let col = if d.kind == AnnotKind::Marker {
                        d.color.with_alpha(160)
                    } else if d.kind == AnnotKind::Eraser {
                        Color::argb(180, 255, 255, 255)
                    } else {
                        d.color
                    };
                    crate::native::polyline_hdc(hdc, &pts, col, cw.round().max(1.0) as i32);
                }
            }
            AnnotKind::Mosaic => {
                if let Some(path) = &d.path {
                    let pts: Vec<Point> = path.iter().copied().map(&map).collect();
                    crate::native::polyline_hdc(
                        hdc,
                        &pts,
                        Color::argb(180, 80, 80, 80),
                        (w * 3.0).round().max(4.0) as i32,
                    );
                }
            }
            _ => {}
        }
    }

    /// Faster: paint layer + draft onto an already-copied screenshot dest covering dest_r.
    pub fn paint_over_shot(&self, dest: &mut Bitmap, src: crate::geom::Rect, dest_r: crate::geom::Rect) {
        if let Some(layer) = &self.layer {
            if dest_r.w > 0 && dest_r.h > 0 && src.w > 0 && src.h > 0 {
                for y in 0..dest_r.h {
                    let sy = src.y + y * src.h / dest_r.h;
                    for x in 0..dest_r.w {
                        let sx = src.x + x * src.w / dest_r.w;
                        let c = layer.get(sx, sy);
                        if c.a > 0 {
                            dest.blend(dest_r.x + x, dest_r.y + y, c);
                        }
                    }
                }
            }
        }
        if let Some(d) = &self.draft {
            paint_mark_mapped(dest, d, src, dest_r, true, self.source.as_ref());
        }
    }

    fn ensure_layer(&mut self) {
        if self.layer.is_some() {
            return;
        }
        if let Some(src) = &self.source {
            self.layer = Some(Bitmap::transparent(src.width, src.height));
        }
    }

    fn rebuild_layer(&mut self) {
        self.layer = None;
        self.ensure_layer();
        let marks = self.marks.clone();
        for m in &marks {
            self.paint_on_layer(m);
        }
    }

    fn paint_on_layer(&mut self, mark: &AnnotMark) {
        self.ensure_layer();
        let Some(mut layer) = self.layer.take() else {
            return;
        };
        paint_mark(&mut layer, mark, false, self.source.as_ref());
        self.layer = Some(layer);
    }
}

fn constrain(from: Point, to: Point, kind: AnnotKind, shift: bool) -> Point {
    if !shift {
        return to;
    }
    if matches!(kind, AnnotKind::Rect | AnnotKind::Ellipse) {
        snap_square(from, to)
    } else {
        snap45(from, to)
    }
}

fn snap45(from: Point, to: Point) -> Point {
    let dx = to.x as f32 - from.x as f32;
    let dy = to.y as f32 - from.y as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return to;
    }
    let snapped = (dy.atan2(dx) / (std::f32::consts::PI / 4.0)).round() * (std::f32::consts::PI / 4.0);
    Point::new(
        from.x + (snapped.cos() * len).round() as i32,
        from.y + (snapped.sin() * len).round() as i32,
    )
}

fn snap_square(from: Point, to: Point) -> Point {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let sx = if dx < 0 { -1 } else { 1 };
    let sy = if dy < 0 { -1 } else { 1 };
    let s = dx.abs().max(dy.abs());
    Point::new(from.x + sx * s, from.y + sy * s)
}

fn bounds(a: Point, b: Point) -> (f32, f32, f32, f32) {
    let x = a.x.min(b.x) as f32;
    let y = a.y.min(b.y) as f32;
    let w = (a.x - b.x).abs().max(1) as f32;
    let h = (a.y - b.y).abs().max(1) as f32;
    (x, y, w, h)
}

fn paint_mark(bmp: &mut Bitmap, m: &AnnotMark, preview: bool, source: Option<&Bitmap>) {
    let w = m.width.max(1.0);
    match m.kind {
        AnnotKind::Arrow => paint_arrow(bmp, m.from, m.to, m.color, w),
        AnnotKind::Line => {
            let halo = Color::argb(140, 0, 0, 0);
            stroke_line(
                bmp,
                m.from.x as f32,
                m.from.y as f32,
                m.to.x as f32,
                m.to.y as f32,
                halo,
                w + 2.2,
            );
            stroke_line(
                bmp,
                m.from.x as f32,
                m.from.y as f32,
                m.to.x as f32,
                m.to.y as f32,
                m.color,
                w,
            );
        }
        AnnotKind::Rect => {
            let (x, y, bw, bh) = bounds(m.from, m.to);
            stroke_rect(bmp, x, y, bw, bh, Color::argb(140, 0, 0, 0), w + 2.0);
            stroke_rect(bmp, x, y, bw, bh, m.color, w);
        }
        AnnotKind::Ellipse => {
            let (x, y, bw, bh) = bounds(m.from, m.to);
            stroke_ellipse(bmp, x, y, bw, bh, Color::argb(140, 0, 0, 0), w + 2.0);
            stroke_ellipse(bmp, x, y, bw, bh, m.color, w);
        }
        AnnotKind::Pencil => {
            paint_path(bmp, m.path.as_deref(), m.color, w);
        }
        AnnotKind::Marker => {
            let c = m.color.with_alpha(92);
            paint_path(bmp, m.path.as_deref(), c, w * 2.7);
        }
        AnnotKind::Mosaic => paint_mosaic(bmp, m.path.as_deref(), source, m.width),
        AnnotKind::Eraser => {
            if preview {
                paint_path(bmp, m.path.as_deref(), Color::argb(70, 255, 255, 255), w * 2.2);
            } else {
                paint_eraser(bmp, m.path.as_deref(), w * 2.2);
            }
        }
        AnnotKind::Text => {
            if let Some(t) = &m.text {
                let px = (w * 4.2).clamp(12.0, 64.0) as i32;
                draw_text(bmp, m.from, t, m.color, px);
            }
        }
        AnnotKind::None => {}
    }
}

impl AnnotMark {
    pub fn bounds(&self) -> crate::geom::Rect {
        let pad = ((self.width * 5.0).ceil() as i32).max(16);
        if let Some(path) = &self.path {
            if path.is_empty() {
                return crate::geom::Rect::new(self.from.x, self.from.y, 1, 1).inflate(pad, pad);
            }
            let mut minx = i32::MAX;
            let mut miny = i32::MAX;
            let mut maxx = i32::MIN;
            let mut maxy = i32::MIN;
            for p in path {
                minx = minx.min(p.x);
                miny = miny.min(p.y);
                maxx = maxx.max(p.x);
                maxy = maxy.max(p.y);
            }
            return crate::geom::Rect::from_ltrb(minx, miny, maxx + 1, maxy + 1).inflate(pad, pad);
        }
        if self.kind == AnnotKind::Text {
            let w = self
                .text
                .as_ref()
                .map(|t| (t.chars().count() as i32 * 12).max(24))
                .unwrap_or(24);
            return crate::geom::Rect::new(self.from.x, self.from.y, w, 28).inflate(pad, pad);
        }
        crate::geom::Rect::from_ltrb(
            self.from.x.min(self.to.x),
            self.from.y.min(self.to.y),
            self.from.x.max(self.to.x) + 1,
            self.from.y.max(self.to.y) + 1,
        )
        .inflate(pad, pad)
    }
}

fn paint_mark_mapped(
    dest: &mut Bitmap,
    m: &AnnotMark,
    src: crate::geom::Rect,
    dest_r: crate::geom::Rect,
    preview: bool,
    source: Option<&Bitmap>,
) {
    let sx = dest_r.w as f32 / src.w.max(1) as f32;
    let sy = dest_r.h as f32 / src.h.max(1) as f32;
    let map = |p: Point| -> Point {
        Point::new(
            dest_r.x + ((p.x - src.x) as f32 * sx).round() as i32,
            dest_r.y + ((p.y - src.y) as f32 * sy).round() as i32,
        )
    };
    let mut mapped = m.clone();
    mapped.from = map(m.from);
    mapped.to = map(m.to);
    mapped.width = m.width * (sx + sy) * 0.5;
    if let Some(path) = &m.path {
        mapped.path = Some(path.iter().copied().map(map).collect());
    }
    paint_mark(dest, &mapped, preview, source);
}

fn paint_path(bmp: &mut Bitmap, path: Option<&[Point]>, c: Color, w: f32) {
    let Some(path) = path else { return };
    if path.is_empty() {
        return;
    }
    if path.len() == 1 {
        fill_circle(bmp, path[0].x as f32, path[0].y as f32, w / 2.0, c);
        return;
    }
    for win in path.windows(2) {
        stroke_line(
            bmp,
            win[0].x as f32,
            win[0].y as f32,
            win[1].x as f32,
            win[1].y as f32,
            c,
            w,
        );
    }
}

fn paint_eraser(bmp: &mut Bitmap, path: Option<&[Point]>, w: f32) {
    let Some(path) = path else { return };
    for p in path {
        fill_circle(bmp, p.x as f32, p.y as f32, w / 2.0, Color::argb(0, 0, 0, 0));
    }
}

fn paint_mosaic(bmp: &mut Bitmap, path: Option<&[Point]>, source: Option<&Bitmap>, width: f32) {
    let Some(path) = path else { return };
    let Some(src) = source else { return };
    let cell = ((width * 3.2).round() as i32).max(6);
    let mut seen = std::collections::HashSet::new();
    for p in path {
        let gx = (p.x / cell) * cell;
        let gy = (p.y / cell) * cell;
        if !seen.insert((gx, gy)) {
            continue;
        }
        let c = src.get(gx + cell / 2, gy + cell / 2);
        fill_rect(bmp, gx, gy, cell, cell, c);
    }
}
