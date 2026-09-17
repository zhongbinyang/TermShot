use windows::core::PCWSTR;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[allow(dead_code)]
pub fn pcw(v: &[u16]) -> PCWSTR {
    PCWSTR(v.as_ptr())
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn clamp_i32(v: i32, lo: i32, hi: i32) -> i32 {
    v.max(lo).min(hi)
}

pub fn exe_path() -> std::io::Result<std::path::PathBuf> {
    std::env::current_exe()
}

pub fn local_app_data() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("LOCALAPPDATA") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(".")
}

pub fn pictures_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("USERPROFILE") {
        return std::path::PathBuf::from(p).join("Pictures");
    }
    local_app_data()
}

pub fn start_menu_programs() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("APPDATA") {
        return std::path::PathBuf::from(p)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");
    }
    local_app_data()
}
