use crate::bitmap::Bitmap;
use crate::geom::Color;
use crate::theme;
use std::io::Write;
use std::path::Path;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, ICONINFO, HICON,
};

pub fn draw_app_bitmap(size: i32) -> Bitmap {
    let mut bmp = Bitmap::transparent(size, size);
    let s = size as f32 / 32.0;
    fill_round(
        &mut bmp,
        3.0 * s,
        4.0 * s,
        26.0 * s,
        22.0 * s,
        4.0 * s,
        Color::rgb(0x12, 0x18, 0x20),
    );
    let accent = theme::ACCENT;
    // selection corners
    let frame_x = 3.0 * s;
    let frame_y = 4.0 * s;
    let frame_r = frame_x + 26.0 * s;
    let frame_b = frame_y + 22.0 * s;
    let m = 7.0 * s;
    let t = 2.4 * s;
    let thick = (2.1 * s).max(1.4);
    stroke_polyline(
        &mut bmp,
        &[
            (frame_x + m, frame_y + t),
            (frame_x + t, frame_y + t),
            (frame_x + t, frame_y + m),
        ],
        accent,
        thick,
    );
    stroke_polyline(
        &mut bmp,
        &[
            (frame_r - m, frame_y + t),
            (frame_r - t, frame_y + t),
            (frame_r - t, frame_y + m),
        ],
        accent,
        thick,
    );
    stroke_polyline(
        &mut bmp,
        &[
            (frame_x + m, frame_b - t),
            (frame_x + t, frame_b - t),
            (frame_x + t, frame_b - m),
        ],
        accent,
        thick,
    );
    stroke_polyline(
        &mut bmp,
        &[
            (frame_r - m, frame_b - t),
            (frame_r - t, frame_b - t),
            (frame_r - t, frame_b - m),
        ],
        accent,
        thick,
    );
    let cx = 16.0 * s;
    let cy = 15.0 * s;
    stroke_polyline(&mut bmp, &[(cx - 5.0 * s, cy), (cx + 5.0 * s, cy)], accent, thick);
    stroke_polyline(&mut bmp, &[(cx, cy - 5.0 * s), (cx, cy + 5.0 * s)], accent, thick);
    fill_circle(&mut bmp, cx, cy, 1.4 * s, Color::rgb(255, 255, 255));
    bmp
}

fn fill_round(bmp: &mut Bitmap, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    for py in y as i32..(y + h).ceil() as i32 {
        for px in x as i32..(x + w).ceil() as i32 {
            if round_contains(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, r) {
                bmp.set(px, py, c);
            }
        }
    }
}

fn round_contains(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> bool {
    if px < x || py < y || px >= x + w || py >= y + h {
        return false;
    }
    let cx = if px < x + r {
        x + r
    } else if px > x + w - r {
        x + w - r
    } else {
        return py >= y && py < y + h;
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

fn fill_circle(bmp: &mut Bitmap, cx: f32, cy: f32, r: f32, c: Color) {
    let minx = (cx - r).floor() as i32;
    let maxx = (cx + r).ceil() as i32;
    let miny = (cy - r).floor() as i32;
    let maxy = (cy + r).ceil() as i32;
    for y in miny..=maxy {
        for x in minx..=maxx {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r * r {
                bmp.set(x, y, c);
            }
        }
    }
}

fn stroke_polyline(bmp: &mut Bitmap, pts: &[(f32, f32)], c: Color, width: f32) {
    if pts.len() < 2 {
        return;
    }
    for w in pts.windows(2) {
        stroke_line(bmp, w[0].0, w[0].1, w[1].0, w[1].1, c, width);
    }
}

pub fn stroke_line(bmp: &mut Bitmap, x0: f32, y0: f32, x1: f32, y1: f32, c: Color, width: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let steps = (len * 2.0).ceil() as i32;
    let r = (width / 2.0).max(0.6);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        fill_circle(bmp, x0 + dx * t, y0 + dy * t, r, c);
    }
}

pub fn write_ico(path: &Path) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let sizes = [16, 32, 48, 256];
    let mut payloads = Vec::new();
    for s in sizes {
        let bmp = draw_app_bitmap(s);
        payloads.push(bmp.to_png_bytes()?);
    }
    let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    f.write_all(&0u16.to_le_bytes()).map_err(|e| e.to_string())?;
    f.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?;
    f.write_all(&(sizes.len() as u16).to_le_bytes())
        .map_err(|e| e.to_string())?;
    let mut offset = 6 + 16 * sizes.len();
    for (i, s) in sizes.iter().enumerate() {
        let w = if *s >= 256 { 0u8 } else { *s as u8 };
        f.write_all(&[w, w, 0, 0]).map_err(|e| e.to_string())?;
        f.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&32u16.to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&(payloads[i].len() as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        f.write_all(&(offset as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        offset += payloads[i].len();
    }
    for p in payloads {
        f.write_all(&p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn create_hicon() -> Option<HICON> {
    let size = crate::native::small_icon_size();
    create_hicon_size(size)
}

fn create_hicon_size(size: i32) -> Option<HICON> {
    let size = size.max(16);
    let bmp = draw_app_bitmap(size);
    unsafe {
        let stride = ((size + 31) / 32) * 4;
        let mut and_mask = vec![0xFFu8; (stride * size) as usize];
        for y in 0..size {
            for x in 0..size {
                if bmp.get(x, y).a > 16 {
                    let row = y as usize;
                    let bit = 7 - (x as usize % 8);
                    let idx = row * stride as usize + (x as usize) / 8;
                    and_mask[idx] &= !(1 << bit);
                }
            }
        }
        let mask = CreateBitmap(size, size, 1, 1, Some(and_mask.as_ptr() as *const _));
        if mask.is_invalid() {
            return None;
        }
        let color = CreateBitmap(size, size, 1, 32, Some(bmp.pixels.as_ptr() as *const _));
        if color.is_invalid() {
            let _ = DeleteObject(mask);
            return None;
        }
        let info = ICONINFO {
            fIcon: windows::Win32::Foundation::TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(mask);
        let _ = DeleteObject(color);
        icon
    }
}

pub fn destroy_icon(icon: HICON) {
    unsafe {
        let _ = DestroyIcon(icon);
    }
}
