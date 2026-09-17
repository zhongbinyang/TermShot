use crate::native::{MOD_ALT_BIT, MOD_CONTROL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT};
use crate::util::{local_app_data, pictures_dir};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_MODEL: &str = "deepseek-flash";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum PostCaptureAction {
    Ask = 0,
    SaveImage = 1,
    CopyImage = 2,
    CopyPath = 3,
    Pin = 4,
    CopyText = 5,
    Translate = 6,
}

impl Default for PostCaptureAction {
    fn default() -> Self {
        Self::Ask
    }
}

impl PostCaptureAction {
    pub fn all() -> &'static [(Self, &'static str)] {
        &[
            (Self::Ask, "每次询问（截完后选择）"),
            (Self::SaveImage, "保存图片"),
            (Self::CopyImage, "复制图片"),
            (Self::CopyPath, "复制保存图片的地址"),
            (Self::Pin, "贴到桌面"),
            (Self::CopyText, "复制文字（模型识别）"),
            (Self::Translate, "翻译并复制译文"),
        ]
    }

    #[allow(dead_code)]
    pub fn uses_path(self) -> bool {
        matches!(self, Self::Ask | Self::CopyPath)
    }

    pub fn from_index(i: i32) -> Self {
        Self::all()
            .get(i as usize)
            .map(|x| x.0)
            .unwrap_or(Self::Ask)
    }

    pub fn index(self) -> i32 {
        Self::all()
            .iter()
            .position(|(a, _)| *a == self)
            .unwrap_or(0) as i32
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub save_directory: String,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u32,
    pub post_capture_action: PostCaptureAction,
    pub quote_path: bool,
    pub start_with_windows: bool,
    pub deep_seek_api_key: String,
    pub deep_seek_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_directory: String::new(),
            hotkey_modifiers: MOD_CONTROL_BIT | MOD_SHIFT_BIT,
            hotkey_key: 0x53, // S
            post_capture_action: PostCaptureAction::Ask,
            quote_path: true,
            start_with_windows: true,
            deep_seek_api_key: String::new(),
            deep_seek_model: DEFAULT_MODEL.into(),
        }
    }
}

impl Settings {
    pub fn dir() -> PathBuf {
        local_app_data().join("TermShot")
    }

    pub fn file_path() -> PathBuf {
        Self::dir().join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::file_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&text) {
                return s;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let dir = Self::dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = Self::file_path();
        let tmp = dir.join("settings.json.tmp");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::copy(&tmp, &path);
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    pub fn has_vision(&self) -> bool {
        !self.deep_seek_api_key.trim().is_empty()
    }

    pub fn model_name(&self) -> String {
        let m = self.deep_seek_model.trim();
        if m.is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            m.to_string()
        }
    }

    pub fn resolved_save_directory(&self) -> PathBuf {
        if self.save_directory.trim().is_empty() {
            pictures_dir().join("Screenshots")
        } else {
            PathBuf::from(&self.save_directory)
        }
    }

    pub fn format_hotkey(&self) -> String {
        format_hotkey(self.hotkey_modifiers, self.hotkey_key)
    }
}

pub fn format_hotkey(modifiers: u32, key: u32) -> String {
    let mut parts = Vec::new();
    if modifiers & MOD_CONTROL_BIT != 0 {
        parts.push("Ctrl");
    }
    if modifiers & MOD_SHIFT_BIT != 0 {
        parts.push("Shift");
    }
    if modifiers & MOD_ALT_BIT != 0 {
        parts.push("Alt");
    }
    if modifiers & MOD_WIN_BIT != 0 {
        parts.push("Win");
    }
    parts.push(format_key(key));
    // leak-free: own the key string
    let key_s = format_key(key).to_string();
    let mut out = Vec::new();
    if modifiers & MOD_CONTROL_BIT != 0 {
        out.push("Ctrl".into());
    }
    if modifiers & MOD_SHIFT_BIT != 0 {
        out.push("Shift".into());
    }
    if modifiers & MOD_ALT_BIT != 0 {
        out.push("Alt".into());
    }
    if modifiers & MOD_WIN_BIT != 0 {
        out.push("Win".into());
    }
    out.push(key_s);
    let _ = parts;
    out.join("+")
}

fn format_key(key: u32) -> &'static str {
    match key {
        0x30 => "0",
        0x31 => "1",
        0x32 => "2",
        0x33 => "3",
        0x34 => "4",
        0x35 => "5",
        0x36 => "6",
        0x37 => "7",
        0x38 => "8",
        0x39 => "9",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x2C => "PrintScreen",
        0x70 => "F1",
        0x71 => "F2",
        0x72 => "F3",
        0x73 => "F4",
        0x74 => "F5",
        0x75 => "F6",
        0x76 => "F7",
        0x77 => "F8",
        0x78 => "F9",
        0x79 => "F10",
        0x7A => "F11",
        0x7B => "F12",
        k if (0x41..=0x5A).contains(&k) => {
            // can't return from stack - use match for letters
            LETTERS[(k - 0x41) as usize]
        }
        _ => "Key",
    }
}

const LETTERS: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];
