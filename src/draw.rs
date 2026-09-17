use crate::bitmap::Bitmap;
use crate::geom::{Color, Point};
use crate::util::wide;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GetDC,
    GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS,
    DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_TOP, FW_BOLD, FW_NORMAL, OUT_DEFAULT_PRECIS,
    TRANSPARENT,
};

pub fn stroke_line(bmp: &mut Bitmap, x0: f32, y0: f32, x1: f32, y1: f32, c: Color, width: f32) {
    if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() || !width.is_finite() {
        return;
    }
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    if !len.is_finite() {
        return;
    }
    let steps = ((len * 2.0 + width * 2.0).ceil() as i32).clamp(1, 16384);
    let r = (width / 2.0).clamp(0.6, 64.0);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        fill_circle(bmp, x0 + dx * t, y0 + dy * t, r, c);
    }
}

pub fn fill_circle(bmp: &mut Bitmap, cx: f32, cy: f32, r: f32, c: Color) {
    let minx = (cx - r).floor() as i32;
    let maxx = (cx + r).ceil() as i32;
    let miny = (cy - r).floor() as i32;
    let maxy = (cy + r).ceil() as i32;
    let r2 = r * r;
    for y in miny..=maxy {
        for x in minx..=maxx {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                if c.a == 255 {
                    bmp.set(x, y, c);
                } else if c.a == 0 {
                    bmp.clear_pixel(x, y);
                } else {
                    bmp.blend(x, y, c);
                }
            }
        }
    }
}

pub fn fill_rect(bmp: &mut Bitmap, x: i32, y: i32, w: i32, h: i32, c: Color) {
    for py in y..y + h {
        for px in x..x + w {
            if c.a == 0 {
                bmp.clear_pixel(px, py);
            } else if c.a == 255 {
                bmp.set(px, py, c);
            } else {
                bmp.blend(px, py, c);
            }
        }
    }
}

pub fn stroke_rect(bmp: &mut Bitmap, x: f32, y: f32, w: f32, h: f32, c: Color, width: f32) {
    stroke_line(bmp, x, y, x + w, y, c, width);
    stroke_line(bmp, x + w, y, x + w, y + h, c, width);
    stroke_line(bmp, x + w, y + h, x, y + h, c, width);
    stroke_line(bmp, x, y + h, x, y, c, width);
}

pub fn stroke_ellipse(bmp: &mut Bitmap, x: f32, y: f32, w: f32, h: f32, c: Color, width: f32) {
    if w < 1.0 || h < 1.0 || !w.is_finite() || !h.is_finite() {
        return;
    }
    let steps = ((w + h) * 2.0).clamp(32.0, 2048.0) as i32;
    let mut prev = (x + w, y + h / 2.0);
    for i in 0..=steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        let px = x + w / 2.0 + (w / 2.0) * a.cos();
        let py = y + h / 2.0 + (h / 2.0) * a.sin();
        if i > 0 {
            stroke_line(bmp, prev.0, prev.1, px, py, c, width);
        }
        prev = (px, py);
    }
}

pub fn paint_arrow(bmp: &mut Bitmap, from: Point, to: Point, color: Color, width: f32) {
    let dx = to.x as f32 - from.x as f32;
    let dy = to.y as f32 - from.y as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len < 2.0 || !width.is_finite() {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = (width * 3.4).clamp(9.0, len * 0.72);
    let head_half = (width * 1.55).max(4.5);
    let back_x = to.x as f32 - ux * head_len;
    let back_y = to.y as f32 - uy * head_len;
    let halo = Color::argb(150, 0, 0, 0);
    stroke_line(
        bmp,
        from.x as f32,
        from.y as f32,
        back_x,
        back_y,
        halo,
        width + 2.2,
    );
    stroke_line(
        bmp,
        from.x as f32,
        from.y as f32,
        back_x,
        back_y,
        color,
        width,
    );
    let left = (back_x - uy * head_half, back_y + ux * head_half);
    let right = (back_x + uy * head_half, back_y - ux * head_half);
    stroke_line(bmp, to.x as f32, to.y as f32, left.0, left.1, color, width);
    stroke_line(bmp, to.x as f32, to.y as f32, right.0, right.1, color, width);
    stroke_line(bmp, left.0, left.1, right.0, right.1, color, width);
    for i in 1..5 {
        let t = i as f32 / 5.0;
        stroke_line(
            bmp,
            to.x as f32,
            to.y as f32,
            left.0 + (right.0 - left.0) * t,
            left.1 + (right.1 - left.1) * t,
            color,
            width,
        );
    }
}

pub fn draw_text(bmp: &mut Bitmap, at: Point, text: &str, color: Color, px: i32) {
    if text.trim().is_empty() {
        return;
    }
    let w = (text.chars().count() as i32 * px).max(px) + 8;
    let h = px + 12;
    unsafe {
        let hdc_screen = GetDC(windows::Win32::Foundation::HWND::default());
        let hdc = CreateCompatibleDC(hdc_screen);
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let hbmp = match CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(h) => h,
            Err(_) => {
                let _ = DeleteDC(hdc);
                ReleaseDC(windows::Win32::Foundation::HWND::default(), hdc_screen);
                return;
            }
        };
        let old_bmp = SelectObject(hdc, hbmp);
        let font = CreateFontW(
            px,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            windows::core::w!("Segoe UI"),
        );
        let old_font = SelectObject(hdc, font);
        // clear
        if !bits.is_null() {
            let n = (w * h) as usize;
            std::ptr::write_bytes(bits, 0, n * 4);
        }
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, windows::Win32::Foundation::COLORREF(color.colorref()));
        let mut wt = wide(text);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        DrawTextW(
            hdc,
            &mut wt,
            &mut rc,
            DT_LEFT | DT_TOP,
        );
        if !bits.is_null() {
            let src = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize);
            for y in 0..h {
                for x in 0..w {
                    let p = src[(y * w + x) as usize];
                    let b = (p & 0xFF) as u8;
                    let g = ((p >> 8) & 0xFF) as u8;
                    let r = ((p >> 16) & 0xFF) as u8;
                    if r | g | b != 0 {
                        bmp.blend(at.x + x, at.y + y, Color::argb(255, r, g, b));
                    }
                }
            }
        }
        SelectObject(hdc, old_font);
        SelectObject(hdc, old_bmp);
        let _ = DeleteObject(font);
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(hdc);
        ReleaseDC(windows::Win32::Foundation::HWND::default(), hdc_screen);
    }
}

pub fn paint_arrow_hdc(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    from: Point,
    to: Point,
    color: Color,
    width: f32,
) {
    let dx = to.x as f32 - from.x as f32;
    let dy = to.y as f32 - from.y as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len < 2.0 || !width.is_finite() {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = (width * 3.4).clamp(9.0, len * 0.72);
    let head_half = (width * 1.55).max(4.5);
    let back_x = to.x as f32 - ux * head_len;
    let back_y = to.y as f32 - uy * head_len;
    let halo = Color::argb(150, 0, 0, 0);
    crate::native::line_hdc(
        hdc,
        from.x,
        from.y,
        back_x.round() as i32,
        back_y.round() as i32,
        halo,
        (width + 2.2).round() as i32,
    );
    crate::native::line_hdc(
        hdc,
        from.x,
        from.y,
        back_x.round() as i32,
        back_y.round() as i32,
        color,
        width.round().max(1.0) as i32,
    );
    let left = Point::new(
        (back_x - uy * head_half).round() as i32,
        (back_y + ux * head_half).round() as i32,
    );
    let right = Point::new(
        (back_x + uy * head_half).round() as i32,
        (back_y - ux * head_half).round() as i32,
    );
    let pw = width.round().clamp(1.0, 24.0) as i32;
    crate::native::line_hdc(hdc, to.x, to.y, left.x, left.y, color, pw);
    crate::native::line_hdc(hdc, to.x, to.y, right.x, right.y, color, pw);
    crate::native::line_hdc(hdc, left.x, left.y, right.x, right.y, color, pw);
    let steps = (head_len.ceil() as i32).clamp(4, 36);
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        crate::native::line_hdc(
            hdc,
            (to.x as f32 + (left.x - to.x) as f32 * t).round() as i32,
            (to.y as f32 + (left.y - to.y) as f32 * t).round() as i32,
            (to.x as f32 + (right.x - to.x) as f32 * t).round() as i32,
            (to.y as f32 + (right.y - to.y) as f32 * t).round() as i32,
            color,
            pw.max(2),
        );
    }
}

pub fn measure_text(text: &str, px: i32) -> (i32, i32) {
    if text.is_empty() {
        return (0, px);
    }
    unsafe {
        let hdc = GetDC(windows::Win32::Foundation::HWND::default());
        let font = CreateFontW(
            -px,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            windows::core::w!("Segoe UI"),
        );
        let old = SelectObject(hdc, font);
        let wt = wide(text);
        let mut sz = windows::Win32::Foundation::SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wt[..wt.len().saturating_sub(1)], &mut sz);
        SelectObject(hdc, old);
        let _ = DeleteObject(font);
        ReleaseDC(windows::Win32::Foundation::HWND::default(), hdc);
        (sz.cx.max(1), sz.cy.max(px))
    }
}

pub fn paint_chip_hdc(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    r: crate::geom::Rect,
    text: &str,
    scale: f32,
) {
    if r.w < 8 || r.h < 8 || text.is_empty() {
        return;
    }
    let pad = ((7.0 * scale).round() as i32).max(4);
    crate::native::fill_rect_hdc(hdc, r, Color::argb(230, 10, 14, 20));
    draw_text_hdc(
        hdc,
        Point::new(r.x + pad, r.y + ((4.0 * scale).round() as i32).max(2)),
        text,
        crate::theme::TEXT,
        ((13.0 * scale).round() as i32).max(11),
    );
}

pub fn draw_text_hdc(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    at: Point,
    text: &str,
    color: Color,
    px: i32,
) {
    if text.trim().is_empty() {
        return;
    }
    unsafe {
        let font = CreateFontW(
            -px,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            windows::core::w!("Segoe UI"),
        );
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, windows::Win32::Foundation::COLORREF(color.colorref()));
        let mut wt = wide(text);
        let (tw, th) = measure_text(text, px);
        let mut rc = RECT {
            left: at.x,
            top: at.y,
            right: at.x + tw + 8,
            bottom: at.y + th + 4,
        };
        DrawTextW(hdc, &mut wt, &mut rc, DT_LEFT | DT_TOP | DT_NOPREFIX | DT_SINGLELINE);
        SelectObject(hdc, old);
        let _ = DeleteObject(font);
    }
}
