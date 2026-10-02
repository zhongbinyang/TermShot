use crate::bitmap::Bitmap;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};

const CF_UNICODETEXT_ID: u32 = 13;

pub fn get_text() -> Result<String, String> {
    for _ in 0..8 {
        unsafe {
            if OpenClipboard(HWND::default()).is_err() {
                crate::native::sleep_ms(40);
                continue;
            }

            if IsClipboardFormatAvailable(CF_UNICODETEXT_ID).is_err() {
                let _ = CloseClipboard();
                return Err("剪贴板中没有可翻译的文本".into());
            }

            let handle = match GetClipboardData(CF_UNICODETEXT_ID) {
                Ok(handle) => handle,
                Err(_) => {
                    let _ = CloseClipboard();
                    crate::native::sleep_ms(40);
                    continue;
                }
            };
            let global = HGLOBAL(handle.0);
            let ptr = GlobalLock(global) as *const u16;
            if ptr.is_null() {
                let _ = CloseClipboard();
                crate::native::sleep_ms(40);
                continue;
            }

            let capacity = GlobalSize(global) / std::mem::size_of::<u16>();
            let slice = std::slice::from_raw_parts(ptr, capacity);
            let len = slice.iter().position(|&c| c == 0).unwrap_or(capacity);
            let text = String::from_utf16_lossy(&slice[..len]);
            let _ = GlobalUnlock(global);
            let _ = CloseClipboard();

            if text.trim().is_empty() {
                return Err("剪贴板中的文本为空".into());
            }
            return Ok(text);
        }
    }
    Err("剪贴板正被其他程序占用，请稍后重试".into())
}

pub fn set_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    retry(|| unsafe {
        OpenClipboard(HWND::default())?;
        EmptyClipboard()?;
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
        let ptr = GlobalLock(h);
        if ptr.is_null() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return Err(windows::core::Error::from_win32());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes);
        let _ = GlobalUnlock(h);
        if SetClipboardData(CF_UNICODETEXT_ID, HANDLE(h.0)).is_err() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return Err(windows::core::Error::from_win32());
        }
        let _ = CloseClipboard();
        Ok(())
    })
}

pub fn set_path(path: &str, quote: bool) -> bool {
    let text = if quote {
        format!("\"{path}\"")
    } else {
        path.to_string()
    };
    set_text(&text)
}

pub fn set_image(bmp: &Bitmap) -> bool {
    // CF_DIB: BITMAPINFOHEADER + packed BGRX (bottom-up)
    let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
    let pix_size = (bmp.width * bmp.height * 4) as usize;
    let total = header_size + pix_size;
    retry(|| unsafe {
        OpenClipboard(HWND::default())?;
        EmptyClipboard()?;
        let h = GlobalAlloc(GMEM_MOVEABLE, total)?;
        let ptr = GlobalLock(h) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return Err(windows::core::Error::from_win32());
        }
        let hdr = BITMAPINFOHEADER {
            biSize: header_size as u32,
            biWidth: bmp.width,
            biHeight: bmp.height, // bottom-up
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: pix_size as u32,
            ..Default::default()
        };
        std::ptr::copy_nonoverlapping(
            &hdr as *const _ as *const u8,
            ptr,
            header_size,
        );
        let dest = ptr.add(header_size) as *mut u32;
        for y in 0..bmp.height {
            let src_y = bmp.height - 1 - y;
            for x in 0..bmp.width {
                *dest.add((y * bmp.width + x) as usize) = bmp.pixels[bmp.index(x, src_y)];
            }
        }
        let _ = GlobalUnlock(h);
        if SetClipboardData(8u32, HANDLE(h.0)).is_err() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return Err(windows::core::Error::from_win32());
        }
        let _ = CloseClipboard();
        Ok(())
    })
}

fn retry(mut f: impl FnMut() -> windows::core::Result<()>) -> bool {
    for _ in 0..8 {
        if f().is_ok() {
            return true;
        }
        crate::native::sleep_ms(40);
    }
    false
}
