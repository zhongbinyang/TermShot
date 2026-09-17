#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_ltrb(l: i32, t: i32, r: i32, b: i32) -> Self {
        Self {
            x: l,
            y: t,
            w: (r - l).max(0),
            h: (b - t).max(0),
        }
    }

    pub fn right(self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.right() && p.y < self.bottom()
    }

    pub fn inflate(self, dx: i32, dy: i32) -> Self {
        Self::from_ltrb(self.x - dx, self.y - dy, self.right() + dx, self.bottom() + dy)
    }

    pub fn intersect(self, o: Self) -> Self {
        let l = self.x.max(o.x);
        let t = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        Self::from_ltrb(l, t, r, b)
    }

    pub fn union(self, o: Self) -> Self {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let l = self.x.min(o.x);
        let t = self.y.min(o.y);
        let r = self.right().max(o.right());
        let b = self.bottom().max(o.bottom());
        Self::from_ltrb(l, t, r, b)
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.w / 2, self.y + self.h / 2)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { a: 255, r, g, b }
    }

    pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self { a, r, g, b }
    }

    pub fn colorref(self) -> u32 {
        (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16)
    }

    pub fn bgra(self) -> u32 {
        (self.b as u32) | ((self.g as u32) << 8) | ((self.r as u32) << 16) | ((self.a as u32) << 24)
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_operations() {
        let r1 = Rect::new(10, 20, 100, 80);
        assert_eq!(r1.right(), 110);
        assert_eq!(r1.bottom(), 100);
        assert!(r1.contains(Point::new(50, 50)));
        assert!(!r1.contains(Point::new(5, 5)));
        assert_eq!(r1.center(), Point::new(60, 60));

        let r2 = Rect::new(50, 60, 100, 100);
        let intersection = r1.intersect(r2);
        assert_eq!(intersection, Rect::new(50, 60, 60, 40));

        let inflated = r1.inflate(5, 10);
        assert_eq!(inflated, Rect::new(5, 10, 110, 100));

        let union_rect = r1.union(r2);
        assert_eq!(union_rect, Rect::new(10, 20, 140, 140));
    }

    #[test]
    fn test_color_encoding() {
        let c = Color::argb(255, 0x12, 0x34, 0x56);
        assert_eq!(c.colorref(), 0x00563412); // 0x00BBGGRR
        assert_eq!(c.bgra(), 0xFF123456);     // 0xAARRGGBB in byte order B, G, R, A
        let transparent = c.with_alpha(128);
        assert_eq!(transparent.a, 128);
    }
}
