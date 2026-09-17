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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wide_roundtrip() {
        let original = "TermShot 终端截图 测试 123";
        let w = wide(original);
        assert_eq!(*w.last().unwrap(), 0);
        let back = from_wide(&w);
        assert_eq!(back, original);
    }

    #[test]
    fn test_clamp_i32() {
        assert_eq!(clamp_i32(5, 10, 20), 10);
        assert_eq!(clamp_i32(15, 10, 20), 15);
        assert_eq!(clamp_i32(25, 10, 20), 20);
    }
}
