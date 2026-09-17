use crate::geom::{Color, Point, Rect};
use crate::native::virtual_screen;
use crate::settings::Settings;
use image::RgbaImage;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, SetStretchBltMode, StretchBlt, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, COLORONCOLOR, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};

/// Top-down 32bpp BGRA bitmap.
#[derive(Clone)]
pub struct Bitmap {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

impl Bitmap {
    pub fn new(width: i32, height: i32) -> Self {
        let w = width.max(1) as usize;
        let h = height.max(1) as usize;
        let n = w.saturating_mul(h);
        let mut pixels = Vec::new();
        if n == 0 || n > 256_000_000 || pixels.try_reserve_exact(n).is_err() {
            return Self {
                width: 1,
                height: 1,
                pixels: vec![0],
            };
        }
        pixels.resize(n, 0);
        Self {
            width: w as i32,
            height: h as i32,
            pixels,
        }
    }

    pub fn transparent(width: i32, height: i32) -> Self {
        Self::new(width, height)
    }

    pub fn index(&self, x: i32, y: i32) -> usize {
        (y as usize)
            .saturating_mul(self.width.max(0) as usize)
            .saturating_add(x as usize)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    pub fn get(&self, x: i32, y: i32) -> Color {
        if self.pixels.is_empty() || self.width <= 0 || self.height <= 0 {
            return Color::rgb(0, 0, 0);
        }
        let x = x.clamp(0, self.width - 1);
        let y = y.clamp(0, self.height - 1);
        let i = self.index(x, y);
        let p = self.pixels.get(i).copied().unwrap_or(0);
        Color::argb(
            ((p >> 24) & 0xFF) as u8,
            ((p >> 16) & 0xFF) as u8,
            ((p >> 8) & 0xFF) as u8,
            (p & 0xFF) as u8,
        )
    }

    pub fn set(&mut self, x: i32, y: i32, c: Color) {
        if self.in_bounds(x, y) {
            let i = (y * self.width + x) as usize;
            self.pixels[i] = c.bgra();
        }
    }

    pub fn clear_pixel(&mut self, x: i32, y: i32) {
        if self.in_bounds(x, y) {
            let i = (y * self.width + x) as usize;
            self.pixels[i] = 0;
        }
    }

    pub fn blend(&mut self, x: i32, y: i32, c: Color) {
        if !self.in_bounds(x, y) {
            return;
        }
        if c.a == 0 {
            return;
        }
        if c.a == 255 {
            self.set(x, y, c);
            return;
        }
        let d = self.get(x, y);
        let a = c.a as u32;
        let ia = 255 - a;
        let out = Color::argb(
            255,
            ((c.r as u32 * a + d.r as u32 * ia) / 255) as u8,
            ((c.g as u32 * a + d.g as u32 * ia) / 255) as u8,
            ((c.b as u32 * a + d.b as u32 * ia) / 255) as u8,
        );
        self.set(x, y, out);
    }

    pub fn crop(&self, r: Rect) -> Bitmap {
        let r = r.intersect(Rect::new(0, 0, self.width, self.height));
        let mut out = Bitmap::new(r.w.max(1), r.h.max(1));
        if r.w <= 0 || r.h <= 0 {
            return out;
        }
        let n = r.w as usize;
        for y in 0..r.h {
            let src = ((r.y + y) as usize).saturating_mul(self.width as usize).saturating_add(r.x as usize);
            let dst = (y as usize).saturating_mul(out.width as usize);
            if src + n <= self.pixels.len() && dst + n <= out.pixels.len() {
                out.pixels[dst..dst + n].copy_from_slice(&self.pixels[src..src + n]);
            }
        }
        out
    }

    pub fn blit_from(&mut self, src: &Bitmap, dest: Point, src_rect: Rect) {
        let sr = src_rect.intersect(Rect::new(0, 0, src.width, src.height));
        for y in 0..sr.h {
            for x in 0..sr.w {
                let c = src.get(sr.x + x, sr.y + y);
                if c.a == 0 {
                    continue;
                }
                if c.a == 255 {
                    self.set(dest.x + x, dest.y + y, c);
                } else {
                    self.blend(dest.x + x, dest.y + y, c);
                }
            }
        }
    }

    pub fn overlay_copy(&mut self, src: &Bitmap, dest: Point, src_rect: Rect) {
        let sr = src_rect.intersect(Rect::new(0, 0, src.width, src.height));
        for y in 0..sr.h {
            for x in 0..sr.w {
                self.set(dest.x + x, dest.y + y, src.get(sr.x + x, sr.y + y));
            }
        }
    }

    pub fn to_png_bytes(&self) -> Result<Vec<u8>, String> {
        let mut img = RgbaImage::new(self.width as u32, self.height as u32);
        for y in 0..self.height {
            for x in 0..self.width {
                let c = self.get(x, y);
                img.put_pixel(x as u32, y as u32, image::Rgba([c.r, c.g, c.b, c.a.max(255)]));
            }
        }
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(buf)
    }

    pub fn save_png(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = self.to_png_bytes()?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    pub fn scaled_max_edge(&self, max_edge: i32) -> Bitmap {
        let edge = self.width.max(self.height);
        if edge <= max_edge {
            return self.clone();
        }
        let scale = max_edge as f32 / edge as f32;
        let nw = ((self.width as f32 * scale).round() as i32).max(1);
        let nh = ((self.height as f32 * scale).round() as i32).max(1);
        let mut out = Bitmap::new(nw, nh);
        for y in 0..nh {
            for x in 0..nw {
                let sx = x * self.width / nw;
                let sy = y * self.height / nh;
                out.set(x, y, self.get(sx, sy));
            }
        }
        out
    }

    pub fn blit_to_hdc(&self, hdc: HDC, dest: Rect) {
        self.blit_region_to_hdc(hdc, dest, Rect::new(0, 0, self.width, self.height));
    }

    pub fn blit_region_to_hdc(&self, hdc: HDC, dest: Rect, src: Rect) {
        let src = src.intersect(Rect::new(0, 0, self.width, self.height));
        if src.w < 1 || src.h < 1 || dest.w < 1 || dest.h < 1 || self.pixels.is_empty() {
            return;
        }
        unsafe {
            let info = dib_info(self.width, self.height);
            let _ = StretchDIBits(
                hdc,
                dest.x,
                dest.y,
                dest.w,
                dest.h,
                src.x,
                src.y,
                src.w,
                src.h,
                Some(self.pixels.as_ptr() as *const _),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}

/// Screen-backed DIB selected into a memory DC. BitBlt uses top-left origin,
/// unlike StretchDIBits on a top-down packed DIB (YSrc is unreliable).
pub struct GdiBitmap {
    hdc: HDC,
    hbmp: HBITMAP,
    old: HGDIOBJ,
    pub w: i32,
    pub h: i32,
}

impl GdiBitmap {
    pub fn from_bitmap(bmp: &Bitmap) -> Option<Self> {
        if bmp.width <= 0 || bmp.height <= 0 || bmp.pixels.is_empty() {
            return None;
        }
        let expected = (bmp.width as usize).saturating_mul(bmp.height as usize);
        if bmp.pixels.len() < expected {
            return None;
        }
        unsafe {
            let hdc_screen = GetDC(HWND::default());
            if hdc_screen.is_invalid() {
                return None;
            }
            let hdc = CreateCompatibleDC(hdc_screen);
            ReleaseDC(HWND::default(), hdc_screen);
            if hdc.is_invalid() {
                return None;
            }
            let info = dib_info(bmp.width, bmp.height);
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let hbmp = match CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(h) if !h.is_invalid() && !bits.is_null() => h,
                _ => {
                    let _ = DeleteDC(hdc);
                    return None;
                }
            };
            std::ptr::copy_nonoverlapping(bmp.pixels.as_ptr(), bits as *mut u32, expected);
            let old = SelectObject(hdc, hbmp);
            Some(Self {
                hdc,
                hbmp,
                old,
                w: bmp.width,
                h: bmp.height,
            })
        }
    }

    pub fn blt(&self, hdc: HDC, dest: Rect, src: Rect) {
        let src = src.intersect(Rect::new(0, 0, self.w, self.h));
        if src.w < 1 || src.h < 1 || dest.w < 1 || dest.h < 1 {
            return;
        }
        unsafe {
            if dest.w == src.w && dest.h == src.h {
                let _ = BitBlt(
                    hdc,
                    dest.x,
                    dest.y,
                    dest.w,
                    dest.h,
                    self.hdc,
                    src.x,
                    src.y,
                    SRCCOPY,
                );
            } else {
                let _ = SetStretchBltMode(hdc, COLORONCOLOR);
                let _ = StretchBlt(
                    hdc,
                    dest.x,
                    dest.y,
                    dest.w,
                    dest.h,
                    self.hdc,
                    src.x,
                    src.y,
                    src.w,
                    src.h,
                    SRCCOPY,
                );
            }
        }
    }
}

impl Drop for GdiBitmap {
    fn drop(&mut self) {
        unsafe {
            if !self.hdc.is_invalid() {
                let _ = SelectObject(self.hdc, self.old);
                let _ = DeleteDC(self.hdc);
            }
            if !self.hbmp.is_invalid() {
                let _ = DeleteObject(self.hbmp);
            }
        }
    }
}

fn dib_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            biSizeImage: (w * h * 4) as u32,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn capture_virtual_screen() -> Result<Bitmap, String> {
    capture_rect(virtual_screen())
}

pub fn capture_rect(screen: Rect) -> Result<Bitmap, String> {
    if screen.w <= 0 || screen.h <= 0 {
        return Err("截取区域无效。".into());
    }
    unsafe {
        let hdc_screen = GetDC(HWND::default());
        if hdc_screen.is_invalid() {
            return Err("无法获取屏幕 DC".into());
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let info = dib_info(screen.w, screen.h);
        let hbmp = CreateDIBSection(hdc_mem, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| e.to_string())?;
        let old = SelectObject(hdc_mem, hbmp);
        let ok = BitBlt(
            hdc_mem,
            0,
            0,
            screen.w,
            screen.h,
            hdc_screen,
            screen.x,
            screen.y,
            SRCCOPY,
        );
        SelectObject(hdc_mem, old);
        let mut bmp = Bitmap::new(screen.w, screen.h);
        if !bits.is_null() {
            let count = (screen.w * screen.h) as usize;
            let src = std::slice::from_raw_parts(bits as *const u32, count);
            bmp.pixels.copy_from_slice(src);
        }
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(HWND::default(), hdc_screen);
        ok.map_err(|e| e.to_string())?;
        Ok(bmp)
    }
}

pub fn next_png_path(dir: &Path) -> PathBuf {
    let stamp = chrono_like_stamp();
    let path = dir.join(format!("{stamp}.png"));
    if !path.exists() {
        return path;
    }
    for i in 1..1000 {
        let p = dir.join(format!("{stamp}-{i}.png"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{stamp}-{}.png", std::process::id()))
}

fn chrono_like_stamp() -> String {
    unsafe {
        let st = windows::Win32::System::SystemInformation::GetLocalTime();
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}

pub fn try_save_png(bmp: &Bitmap, settings: &Settings) -> Result<PathBuf, String> {
    let dir = settings.resolved_save_directory();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = next_png_path(&dir);
    bmp.save_png(&path)?;
    Ok(path)
}
