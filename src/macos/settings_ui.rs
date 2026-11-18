use crate::macos::ffi::{
    self, TSSettingsCallbacks, TSSettingsInit, TSSettingsValues,
};
use crate::macos::native::{
    MOD_ALT_BIT, MOD_CONTROL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT, VK_ESCAPE,
};
use crate::settings::{format_hotkey, PostCaptureAction, Settings, DEFAULT_MODEL};
use std::os::raw::c_void;
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HotkeyTarget {
    Capture = 0,
    Translate = 1,
}

struct SettingsUiState {
    win: *mut c_void,
    original: Settings,
    settings: Settings,
    mods: u32,
    vk: u32,
    translate_mods: u32,
    translate_vk: u32,
    recording: Option<HotkeyTarget>,
    testing_api: bool,
    api_rx: Option<Receiver<Result<(), String>>>,
    saved: bool,
    done: bool,
}

pub fn run(settings: Settings, on_suspend_hotkey: impl FnOnce()) -> Option<Settings> {
    on_suspend_hotkey();

    let state = Box::new(SettingsUiState {
        win: std::ptr::null_mut(),
        mods: settings.hotkey_modifiers,
        vk: settings.hotkey_key,
        translate_mods: settings.translate_hotkey_modifiers,
        translate_vk: settings.translate_hotkey_key,
        original: settings.clone(),
        settings,
        recording: None,
        testing_api: false,
        api_rx: None,
        saved: false,
        done: false,
    });
    let raw = Box::into_raw(state);

    unsafe {
        let save_dir_c = ffi::to_cstr(
            &(*raw)
                .settings
                .resolved_save_directory()
                .display()
                .to_string(),
        );
        let cap_c = ffi::to_cstr(&(*raw).settings.format_hotkey());
        let tr_c = ffi::to_cstr(&(*raw).settings.format_translate_hotkey());
        let key_c = ffi::to_cstr(&(*raw).settings.deep_seek_api_key);
        let model_c = ffi::to_cstr(&(*raw).settings.model_name());
        let cfg_c = ffi::to_cstr(&Settings::dir().display().to_string());
        let has_perm = ffi::ts_has_screen_permission();

        let init = TSSettingsInit {
            save_dir: save_dir_c.as_ptr(),
            quote_path: (*raw).settings.quote_path,
            post_action_idx: (*raw).settings.post_capture_action.index(),
            start_at_login: (*raw).settings.start_with_windows,
            capture_hotkey_text: cap_c.as_ptr(),
            translate_hotkey_text: tr_c.as_ptr(),
            api_key: key_c.as_ptr(),
            model: model_c.as_ptr(),
            config_dir: cfg_c.as_ptr(),
            has_screen_perm: has_perm,
        };

        let cbs = TSSettingsCallbacks {
            on_browse: Some(on_browse),
            on_open_dir: Some(on_open_dir),
            on_open_config: Some(on_open_config),
            on_record_hotkey: Some(on_record_hotkey),
            on_reset_hotkey: Some(on_reset_hotkey),
            on_hotkey_input: Some(on_hotkey_input),
            on_test_api: Some(on_test_api),
            on_open_perm: Some(on_open_perm),
            on_save: Some(on_save),
            on_cancel: Some(on_cancel),
            on_close: Some(on_close),
        };

        let win = ffi::ts_settings_create(&init, &cbs, raw as *mut c_void);
        (*raw).win = win;

        loop {
            if let Some(rx) = &(*raw).api_rx {
                if let Ok(result) = rx.try_recv() {
                    (*raw).api_rx = None;
                    (*raw).testing_api = false;
                    let (msg, kind) = match result {
                        Ok(()) => ("DeepSeek 连接验证成功，API Key 可用".to_string(), 1),
                        Err(e) => (format!("连接验证失败: {e}"), 2),
                    };
                    let mc = ffi::to_cstr(&msg);
                    ffi::ts_settings_set_api_testing((*raw).win, false, mc.as_ptr(), kind);
                }
            }
            if (*raw).done {
                break;
            }
            ffi::ts_pump_events(12);
        }

        let win_to_close = (*raw).win;
        (*raw).win = std::ptr::null_mut();
        if !win_to_close.is_null() {
            ffi::ts_settings_close(win_to_close);
        }

        let st = Box::from_raw(raw);
        if st.saved {
            Some(st.settings)
        } else {
            None
        }
    }
}

unsafe fn set_status(st: &SettingsUiState, text: &str) {
    if st.win.is_null() {
        return;
    }
    let kind = if st.recording.is_some() {
        3
    } else if text.contains("失败")
        || text.contains("不能")
        || text.contains("不可用")
        || text.contains("请先")
        || text.contains("请配合")
    {
        2
    } else if text.contains("成功") || text.starts_with("已") || text.contains("完成") {
        1
    } else {
        0
    };
    let c = ffi::to_cstr(text);
    ffi::ts_settings_set_status(st.win, c.as_ptr(), kind);
}

unsafe fn settings_from_ui(st: &SettingsUiState) -> Settings {
    let mut s = st.settings.clone();
    if st.win.is_null() {
        return s;
    }
    let mut vals = TSSettingsValues {
        save_dir: std::ptr::null_mut(),
        quote_path: s.quote_path,
        post_action_idx: s.post_capture_action.index(),
        start_at_login: s.start_with_windows,
        api_key: std::ptr::null_mut(),
        model: std::ptr::null_mut(),
    };
    ffi::ts_settings_get_values(st.win, &mut vals);
    let dir_str = if !vals.save_dir.is_null() {
        std::ffi::CStr::from_ptr(vals.save_dir)
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    };
    let def = crate::util::pictures_dir().join("Screenshots");
    if dir_str.eq_ignore_ascii_case(&def.display().to_string()) {
        s.save_directory.clear();
    } else {
        s.save_directory = dir_str;
    }
    s.post_capture_action = PostCaptureAction::from_index(vals.post_action_idx);
    s.quote_path = vals.quote_path;
    s.start_with_windows = vals.start_at_login;
    if !vals.api_key.is_null() {
        s.deep_seek_api_key = std::ffi::CStr::from_ptr(vals.api_key)
            .to_string_lossy()
            .trim()
            .to_string();
    }
    if !vals.model.is_null() {
        let m = std::ffi::CStr::from_ptr(vals.model)
            .to_string_lossy()
            .trim()
            .to_string();
        s.deep_seek_model = if m.is_empty() {
            DEFAULT_MODEL.into()
        } else {
            m
        };
    }
    ffi::ts_settings_free_values(&mut vals);
    s.hotkey_modifiers = st.mods;
    s.hotkey_key = st.vk;
    s.translate_hotkey_modifiers = st.translate_mods;
    s.translate_hotkey_key = st.translate_vk;
    s
}

fn hotkeys_conflict(st: &SettingsUiState) -> bool {
    st.mods == st.translate_mods && st.vk == st.translate_vk
}

unsafe fn restore_hotkey_controls(st: &SettingsUiState, target: HotkeyTarget) {
    let (mods, vk) = match target {
        HotkeyTarget::Capture => (st.mods, st.vk),
        HotkeyTarget::Translate => (st.translate_mods, st.translate_vk),
    };
    let text = ffi::to_cstr(&format_hotkey(mods, vk));
    ffi::ts_settings_set_hotkey_display(
        st.win,
        target as i32,
        text.as_ptr(),
        false,
        st.recording.is_some(),
    );
}

extern "C" fn on_browse(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        let cur = settings_from_ui(st).resolved_save_directory();
        let title_c = ffi::to_cstr("选择保存目录");
        let init_c = ffi::to_cstr(&cur.display().to_string());
        if let Some(picked) = ffi::take_cstr(ffi::ts_pick_folder(title_c.as_ptr(), init_c.as_ptr()))
        {
            let pc = ffi::to_cstr(&picked);
            ffi::ts_settings_set_dir(st.win, pc.as_ptr());
            set_status(st, "已更新保存目录路径");
        }
    }
}

extern "C" fn on_open_dir(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        let dir = settings_from_ui(st).resolved_save_directory();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::process::Command::new("open").arg(&dir).spawn();
        set_status(st, "已在访达 (Finder) 中打开保存目录");
    }
}

extern "C" fn on_open_config(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        let dir = Settings::dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::process::Command::new("open").arg(&dir).spawn();
        set_status(st, "已在访达 (Finder) 中打开配置目录");
    }
}

extern "C" fn on_open_perm(_ud: *mut c_void) {
    unsafe {
        ffi::ts_open_screen_permission_settings();
    }
}

extern "C" fn on_record_hotkey(target_i: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let target = if target_i == 0 {
        HotkeyTarget::Capture
    } else {
        HotkeyTarget::Translate
    };
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        if let Some(prev) = st.recording.take() {
            restore_hotkey_controls(st, prev);
            if prev == target {
                set_status(st, "已取消录制快捷键");
                return;
            }
        }
        st.recording = Some(target);
        let wait_c = ffi::to_cstr("等待按下快捷键...");
        ffi::ts_settings_set_hotkey_display(st.win, target as i32, wait_c.as_ptr(), true, true);
        let purpose = match target {
            HotkeyTarget::Capture => "截图",
            HotkeyTarget::Translate => "剪贴板翻译",
        };
        set_status(
            st,
            &format!("正在录制{purpose}快捷键，请按组合键；按 Esc 取消"),
        );
    }
}

extern "C" fn on_reset_hotkey(target_i: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let target = if target_i == 0 {
        HotkeyTarget::Capture
    } else {
        HotkeyTarget::Translate
    };
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        if let Some(prev) = st.recording.take() {
            restore_hotkey_controls(st, prev);
        }
        let mods = MOD_CONTROL_BIT | MOD_SHIFT_BIT;
        let vk = match target {
            HotkeyTarget::Capture => 0x53,
            HotkeyTarget::Translate => 0x54,
        };
        match target {
            HotkeyTarget::Capture => {
                st.mods = mods;
                st.vk = vk;
            }
            HotkeyTarget::Translate => {
                st.translate_mods = mods;
                st.translate_vk = vk;
            }
        }
        restore_hotkey_controls(st, target);
        let val = format_hotkey(mods, vk);
        set_status(st, &format!("已恢复默认快捷键: {val}（点击保存后生效）"));
    }
}

extern "C" fn on_hotkey_input(vk: u32, mods: u32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        let Some(target) = st.recording else {
            return;
        };
        if vk == VK_ESCAPE {
            st.recording = None;
            restore_hotkey_controls(st, target);
            set_status(st, "已取消录制快捷键");
            return;
        }
        if vk == 0 {
            let mut hint = String::new();
            if mods & MOD_CONTROL_BIT != 0 {
                hint.push_str("Ctrl + ");
            }
            if mods & MOD_SHIFT_BIT != 0 {
                hint.push_str("Shift + ");
            }
            if mods & MOD_ALT_BIT != 0 {
                hint.push_str("Alt + ");
            }
            if mods & MOD_WIN_BIT != 0 {
                hint.push_str("Cmd + ");
            }
            hint.push_str("...");
            let c = ffi::to_cstr(&hint);
            ffi::ts_settings_set_hotkey_display(st.win, target as i32, c.as_ptr(), true, true);
            return;
        }
        let is_fkey = (0x70..=0x7B).contains(&vk);
        if mods == 0 && !is_fkey {
            set_status(st, "请配合 Ctrl / Alt / Shift / Cmd 组合键使用");
            return;
        }
        match target {
            HotkeyTarget::Capture => {
                st.mods = mods;
                st.vk = vk;
            }
            HotkeyTarget::Translate => {
                st.translate_mods = mods;
                st.translate_vk = vk;
            }
        }
        st.recording = None;
        restore_hotkey_controls(st, target);
        if hotkeys_conflict(st) {
            set_status(st, "截图与翻译快捷键不能相同，请重新设置其中一组");
        } else {
            set_status(st, "新快捷键录制完成，点击“保存设置”生效");
        }
    }
}

extern "C" fn on_test_api(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        if st.testing_api {
            return;
        }
        let settings = settings_from_ui(st);
        if !settings.has_vision() {
            set_status(st, "请先填写 DeepSeek API Key");
            return;
        }
        st.testing_api = true;
        let msg_c = ffi::to_cstr("正在验证 DeepSeek 连接，请稍候…");
        ffi::ts_settings_set_api_testing(st.win, true, msg_c.as_ptr(), 0);

        let (tx, rx) = std::sync::mpsc::channel();
        st.api_rx = Some(rx);
        std::thread::spawn(move || {
            let result = crate::vision::test_connection(&settings);
            let _ = tx.send(result);
            ffi::dispatch_main_fn(|| {});
        });
    }
}

extern "C" fn on_save(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        if hotkeys_conflict(st) {
            set_status(st, "截图与翻译快捷键不能相同，请修改后再保存");
            return;
        }
        let settings = settings_from_ui(st);
        let dir = settings.resolved_save_directory();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            set_status(st, &format!("保存目录不可用: {err}"));
            return;
        }
        if !dir.is_dir() {
            set_status(st, "保存目录不可用：指定路径不是文件夹");
            return;
        }
        st.settings = settings;
        st.settings.save();
        st.saved = true;
        st.done = true;
    }
}

extern "C" fn on_cancel(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SettingsUiState);
        if !st.saved && settings_from_ui(st) != st.original {
            let title_c = ffi::to_cstr("TermShot — 放弃更改");
            let msg_c = ffi::to_cstr("当前设置尚未保存，确定放弃这些更改吗？");
            if !ffi::ts_confirm_dialog(title_c.as_ptr(), msg_c.as_ptr()) {
                return;
            }
        }
        st.done = true;
    }
}

extern "C" fn on_close(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        (*(ud as *mut SettingsUiState)).done = true;
    }
}
