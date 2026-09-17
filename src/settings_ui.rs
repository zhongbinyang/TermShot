#![allow(clippy::too_many_arguments)]
use crate::native::{
    cursor_pos, dpi_scale_at, enable_dark_title, hinstance, sc, set_font, ui_font,
    work_area_from_point,
};
use crate::settings::{format_hotkey, PostCaptureAction, Settings};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, InvalidateRect, SetBkColor, SetBkMode,
    SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, HBRUSH, HFONT,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RETURN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowTextW,
    RegisterClassExW, SendMessageW, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
    TranslateMessage, BN_CLICKED, BS_AUTOCHECKBOX, BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING,
    CB_GETCURSEL, CB_SETCURSEL, ES_AUTOHSCROLL, ES_PASSWORD, ES_READONLY,
    GWLP_USERDATA, MSG, SW_SHOW, WM_COMMAND, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_KEYDOWN, WM_SYSKEYDOWN, WNDCLASSEXW, WS_BORDER, WS_CAPTION,
    WS_CHILD, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
};

const EM_SETPASSWORDCHAR: u32 = 0x00CC;

struct Ui {
    settings: Settings,
    hwnd: HWND,
    dir: HWND,
    action: HWND,
    hotkey: HWND,
    record_btn: HWND,
    status: HWND,
    quote: HWND,
    startup: HWND,
    key: HWND,
    toggle_key_btn: HWND,
    model: HWND,
    recording: bool,
    show_key: bool,
    mods: u32,
    vk: u32,
    saved: bool,
    scale: f32,
    font: HFONT,
    font_bold: HFONT,
    font_sm: HFONT,
    bg_brush: HBRUSH,
    input_brush: HBRUSH,
}

pub fn run(settings: Settings, on_suspend_hotkey: impl FnOnce()) -> Option<Settings> {
    on_suspend_hotkey();
    let scale = dpi_scale_at(cursor_pos()).max(1.0);
    let p = |v: i32| sc(v, scale);
    let font = ui_font(p(14));
    let font_bold = make_font(p(15), true);
    let font_sm = make_font(p(12), false);
    let bg_brush = unsafe { CreateSolidBrush(COLORREF(theme::WIN_BG.colorref())) };
    let input_brush = unsafe { CreateSolidBrush(COLORREF(theme::INPUT_BG.colorref())) };

    let mut ui = Box::new(Ui {
        mods: settings.hotkey_modifiers,
        vk: settings.hotkey_key,
        settings,
        hwnd: HWND::default(),
        dir: HWND::default(),
        action: HWND::default(),
        hotkey: HWND::default(),
        record_btn: HWND::default(),
        status: HWND::default(),
        quote: HWND::default(),
        startup: HWND::default(),
        key: HWND::default(),
        toggle_key_btn: HWND::default(),
        model: HWND::default(),
        recording: false,
        show_key: false,
        saved: false,
        scale,
        font,
        font_bold,
        font_sm,
        bg_brush,
        input_brush,
    });

    unsafe {
        let class = wide("TermShotSettings");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            hbrBackground: ui.bg_brush,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let work = work_area_from_point(cursor_pos());
        let w = p(550);
        let h = p(480);
        let x = work.x + ((work.w - w) / 2).max(0);
        let y = work.y + ((work.h - h) / 2).max(0);
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShot — 设置"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            hinstance(),
            Some(ui.as_mut() as *mut Ui as *mut _),
        )
        .unwrap_or_default();
        ui.hwnd = hwnd;
        enable_dark_title(hwnd);
        build_controls(ui.as_mut());
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if ui.saved || hwnd.is_invalid() {
                break;
            }
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
                break;
            }

            // Keyboard interception for hotkey recording and dialog shortcuts
            if msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN {
                let vk = msg.wParam.0 as u32;
                if ui.recording {
                    if vk == VK_ESCAPE.0 as u32 {
                        // Cancel recording
                        ui.recording = false;
                        let text = wide(&format_hotkey(ui.mods, ui.vk));
                        let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(text.as_ptr()));
                        let btn_text = wide("录制");
                        let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_text.as_ptr()));
                        let stat = wide("已取消录制快捷键");
                        let _ = SetWindowTextW(ui.status, windows::core::PCWSTR(stat.as_ptr()));
                        continue;
                    }

                    let is_modifier = vk == VK_CONTROL.0 as u32
                        || vk == VK_SHIFT.0 as u32
                        || vk == VK_MENU.0 as u32
                        || vk == 0x5B // VK_LWIN
                        || vk == 0x5C; // VK_RWIN

                    let mut mods = 0u32;
                    if (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 {
                        mods |= crate::native::MOD_CONTROL_BIT;
                    }
                    if (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 {
                        mods |= crate::native::MOD_SHIFT_BIT;
                    }
                    if (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 {
                        mods |= crate::native::MOD_ALT_BIT;
                    }

                    if is_modifier {
                        let mut hint = String::new();
                        if mods & crate::native::MOD_CONTROL_BIT != 0 {
                            hint.push_str("Ctrl + ");
                        }
                        if mods & crate::native::MOD_SHIFT_BIT != 0 {
                            hint.push_str("Shift + ");
                        }
                        if mods & crate::native::MOD_ALT_BIT != 0 {
                            hint.push_str("Alt + ");
                        }
                        hint.push_str("...");
                        let w = wide(&hint);
                        let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                        continue;
                    }

                    let is_fkey = (0x70..=0x7B).contains(&vk);
                    if mods == 0 && !is_fkey {
                        let stat = wide("请配合 Ctrl / Alt / Shift 组合键使用");
                        let _ = SetWindowTextW(ui.status, windows::core::PCWSTR(stat.as_ptr()));
                        continue;
                    }

                    ui.mods = mods;
                    ui.vk = vk;
                    ui.recording = false;
                    let text = wide(&format_hotkey(ui.mods, ui.vk));
                    let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(text.as_ptr()));
                    let btn_text = wide("录制");
                    let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_text.as_ptr()));
                    let stat = wide("新快捷键录制完成，点击“保存”生效");
                    let _ = SetWindowTextW(ui.status, windows::core::PCWSTR(stat.as_ptr()));
                    continue;
                } else if vk == VK_ESCAPE.0 as u32 {
                    let _ = DestroyWindow(hwnd);
                    continue;
                } else if vk == VK_RETURN.0 as u32 {
                    save_from_ui(ui.as_mut());
                    ui.saved = true;
                    let _ = DestroyWindow(hwnd);
                    continue;
                }
            }

            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if !hwnd.is_invalid() && windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
    }

    if ui.saved {
        Some(ui.settings.clone())
    } else {
        None
    }
}

fn make_font(px: i32, bold: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            if bold { 700 } else { 400 },
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            windows::core::w!("Segoe UI"),
        )
    }
}

fn build_controls(ui: &mut Ui) {
    unsafe {
        let h = ui.hwnd;
        let s = ui.scale;
        let p = |v: i32| sc(v, s);

        // Section 1: 基础偏好
        label(h, p(24), p(16), p(200), p(22), "基础设置", ui.font_bold);

        label(h, p(24), p(46), p(76), p(26), "保存目录", ui.font);
        ui.dir = edit(
            h,
            p(106),
            p(44),
            p(264),
            p(26),
            &ui.settings.resolved_save_directory().display().to_string(),
            false,
            false,
            ui.font,
        );
        button(h, p(378), p(44), p(64), p(26), "浏览...", 101, ui.font);
        button(h, p(448), p(44), p(64), p(26), "打开", 102, ui.font);

        label(h, p(24), p(84), p(76), p(26), "截图后动作", ui.font);
        ui.action = combo(h, p(106), p(82), p(406), p(200), ui.font);
        for (_, t) in PostCaptureAction::all() {
            let w = wide(t);
            SendMessageW(ui.action, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
        }
        SendMessageW(
            ui.action,
            CB_SETCURSEL,
            WPARAM(ui.settings.post_capture_action.index() as usize),
            LPARAM(0),
        );

        label(h, p(24), p(122), p(76), p(26), "截图快捷键", ui.font);
        ui.hotkey = edit(
            h,
            p(106),
            p(120),
            p(230),
            p(26),
            &ui.settings.format_hotkey(),
            false,
            true,
            ui.font,
        );
        ui.record_btn = button(h, p(344), p(120), p(80), p(26), "录制", 103, ui.font);
        button(h, p(432), p(120), p(80), p(26), "恢复默认", 104, ui.font);

        ui.quote = check(
            h,
            p(106),
            p(158),
            p(190),
            p(24),
            "复制路径包含双引号",
            ui.settings.quote_path,
            201,
            ui.font,
        );
        ui.startup = check(
            h,
            p(310),
            p(158),
            p(180),
            p(24),
            "开机自启动",
            ui.settings.start_with_windows,
            202,
            ui.font,
        );

        // Section 2: AI 智能识别
        label(h, p(24), p(202), p(260), p(22), "AI 智能识别 (DeepSeek)", ui.font_bold);
        label(
            h,
            p(24),
            p(228),
            p(488),
            p(18),
            "用于截图工具栏中的文字提取 (OCR) 与多语言划词翻译",
            ui.font_sm,
        );

        label(h, p(24), p(256), p(76), p(26), "API Key", ui.font);
        ui.key = edit(
            h,
            p(106),
            p(254),
            p(334),
            p(26),
            &ui.settings.deep_seek_api_key,
            true,
            false,
            ui.font,
        );
        ui.toggle_key_btn = button(h, p(448), p(254), p(64), p(26), "显示", 105, ui.font);

        label(h, p(24), p(294), p(76), p(26), "识别模型", ui.font);
        ui.model = edit(
            h,
            p(106),
            p(292),
            p(406),
            p(26),
            &ui.settings.model_name(),
            false,
            false,
            ui.font,
        );
        label(
            h,
            p(106),
            p(322),
            p(406),
            p(18),
            "推荐: deepseek-chat 或 deepseek-flash",
            ui.font_sm,
        );

        // Section 3: 底部提示与操作
        ui.status = label(
            h,
            p(24),
            p(382),
            p(280),
            p(32),
            "提示: Esc 键关闭，Enter 键保存",
            ui.font_sm,
        );
        button(h, p(314), p(380), p(96), p(32), "保存 (Enter)", 1, ui.font);
        button(h, p(418), p(380), p(94), p(32), "取消 (Esc)", 2, ui.font);
    }
}

fn label(parent: HWND, x: i32, y: i32, w: i32, h: i32, text: &str, font: HFONT) -> HWND {
    child(parent, "STATIC", text, 0, x, y, w, h, 0, font)
}

fn edit(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    password: bool,
    readonly: bool,
    font: HFONT,
) -> HWND {
    let mut style = WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32;
    if password {
        style |= ES_PASSWORD as u32;
    }
    if readonly {
        style |= ES_READONLY as u32;
    }
    child(parent, "EDIT", text, style as i32, x, y, w, h, 0, font)
}

fn button(parent: HWND, x: i32, y: i32, w: i32, h: i32, text: &str, id: i32, font: HFONT) -> HWND {
    child(
        parent,
        "BUTTON",
        text,
        (WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32) as i32,
        x,
        y,
        w,
        h,
        id,
        font,
    )
}

fn check(parent: HWND, x: i32, y: i32, w: i32, h: i32, text: &str, on: bool, id: i32, font: HFONT) -> HWND {
    let hwnd = child(
        parent,
        "BUTTON",
        text,
        (WS_CHILD.0 | WS_VISIBLE.0 | BS_AUTOCHECKBOX as u32) as i32,
        x,
        y,
        w,
        h,
        id,
        font,
    );
    unsafe {
        SendMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::BM_SETCHECK,
            WPARAM(if on { 1 } else { 0 }),
            LPARAM(0),
        );
    }
    hwnd
}

fn combo(parent: HWND, x: i32, y: i32, w: i32, h: i32, font: HFONT) -> HWND {
    child(
        parent,
        "COMBOBOX",
        "",
        (WS_CHILD.0
            | WS_VISIBLE.0
            | CBS_DROPDOWNLIST as u32
            | windows::Win32::UI::WindowsAndMessaging::WS_VSCROLL.0) as i32,
        x,
        y,
        w,
        h,
        0,
        font,
    )
}

fn child(
    parent: HWND,
    cls: &str,
    text: &str,
    style: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    font: HFONT,
) -> HWND {
    let c = wide(cls);
    let t = wide(text);
    unsafe {
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(c.as_ptr()),
            windows::core::PCWSTR(t.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style as u32),
            x,
            y,
            w,
            h,
            parent,
            windows::Win32::UI::WindowsAndMessaging::HMENU(id as isize as *mut _),
            hinstance(),
            None,
        )
        .unwrap_or_default();
        set_font(hwnd, font);
        hwnd
    }
}

fn get_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 1024];
    unsafe {
        GetWindowTextW(hwnd, &mut buf);
    }
    crate::util::from_wide(&buf)
}

fn checked(hwnd: HWND) -> bool {
    unsafe {
        SendMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::BM_GETCHECK,
            WPARAM(0),
            LPARAM(0),
        )
        .0 == 1
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ui = &mut *ptr;
    match msg {
        WM_CTLCOLORSTATIC => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
            let ctrl = HWND(lparam.0 as *mut _);
            if ctrl == ui.hotkey {
                SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
                SetBkColor(hdc, COLORREF(theme::INPUT_BG.colorref()));
                return LRESULT(ui.input_brush.0 as isize);
            }
            SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
            SetBkColor(hdc, COLORREF(theme::WIN_BG.colorref()));
            SetBkMode(hdc, TRANSPARENT);
            LRESULT(ui.bg_brush.0 as isize)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
            SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
            SetBkColor(hdc, COLORREF(theme::INPUT_BG.colorref()));
            LRESULT(ui.input_brush.0 as isize)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xFFFF;
            let code = ((wparam.0 as u32) >> 16) & 0xFFFF;
            if code == BN_CLICKED {
                match id {
                    101 => {
                        let current = get_text(ui.dir);
                        if let Some(p) = browse_folder(hwnd, &current) {
                            let w = wide(&p);
                            let _ = SetWindowTextW(ui.dir, windows::core::PCWSTR(w.as_ptr()));
                        }
                    }
                    102 => {
                        let current = get_text(ui.dir);
                        let p = if current.trim().is_empty() {
                            ui.settings.resolved_save_directory()
                        } else {
                            std::path::PathBuf::from(&current)
                        };
                        let _ = std::fs::create_dir_all(&p);
                        let _ = std::process::Command::new("explorer").arg(&p).spawn();
                    }
                    103 => {
                        ui.recording = !ui.recording;
                        if ui.recording {
                            let btn_w = wide("停止录制");
                            let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_w.as_ptr()));
                            let tip = wide("按下新快捷键组合 (如 Ctrl+Alt+A)... 按 Esc 取消");
                            let _ = SetWindowTextW(ui.status, windows::core::PCWSTR(tip.as_ptr()));
                            let place = wide("等待按下快捷键...");
                            let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(place.as_ptr()));
                        } else {
                            let btn_w = wide("录制");
                            let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_w.as_ptr()));
                            let w = wide(&format_hotkey(ui.mods, ui.vk));
                            let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                            let tip = wide("已取消录制");
                            let _ = SetWindowTextW(ui.status, windows::core::PCWSTR(tip.as_ptr()));
                        }
                    }
                    104 => {
                        ui.mods = crate::native::MOD_CONTROL_BIT | crate::native::MOD_SHIFT_BIT;
                        ui.vk = 0x53; // 'S'
                        ui.recording = false;
                        let w = wide(&format_hotkey(ui.mods, ui.vk));
                        let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                        let btn_w = wide("录制");
                        let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_w.as_ptr()));
                        let stat = wide("已恢复默认快捷键: Ctrl+Shift+S");
                        let _ = SetWindowTextW(ui.status, windows::core::PCWSTR(stat.as_ptr()));
                    }
                    105 => {
                        ui.show_key = !ui.show_key;
                        let ch: usize = if ui.show_key { 0 } else { 0x25CF };
                        SendMessageW(ui.key, EM_SETPASSWORDCHAR, WPARAM(ch), LPARAM(0));
                        let btn_text = wide(if ui.show_key { "隐藏" } else { "显示" });
                        let _ = SetWindowTextW(ui.toggle_key_btn, windows::core::PCWSTR(btn_text.as_ptr()));
                        let _ = InvalidateRect(ui.key, None, true);
                    }
                    1 => {
                        save_from_ui(ui);
                        ui.saved = true;
                        let _ = DestroyWindow(hwnd);
                    }
                    2 => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if !ui.font.is_invalid() {
                let _ = DeleteObject(ui.font);
                ui.font = HFONT::default();
            }
            if !ui.font_bold.is_invalid() {
                let _ = DeleteObject(ui.font_bold);
                ui.font_bold = HFONT::default();
            }
            if !ui.font_sm.is_invalid() {
                let _ = DeleteObject(ui.font_sm);
                ui.font_sm = HFONT::default();
            }
            if !ui.bg_brush.is_invalid() {
                let _ = DeleteObject(ui.bg_brush);
                ui.bg_brush = HBRUSH::default();
            }
            if !ui.input_brush.is_invalid() {
                let _ = DeleteObject(ui.input_brush);
                ui.input_brush = HBRUSH::default();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn save_from_ui(ui: &mut Ui) {
    ui.settings.save_directory = get_text(ui.dir);
    let def = crate::util::pictures_dir().join("Screenshots");
    if ui.settings.save_directory.eq_ignore_ascii_case(&def.display().to_string()) {
        ui.settings.save_directory.clear();
    }
    let idx = unsafe { SendMessageW(ui.action, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32 };
    ui.settings.post_capture_action = PostCaptureAction::from_index(idx);
    ui.settings.quote_path = checked(ui.quote);
    ui.settings.start_with_windows = checked(ui.startup);
    ui.settings.deep_seek_api_key = get_text(ui.key).trim().to_string();
    let m = get_text(ui.model).trim().to_string();
    ui.settings.deep_seek_model = if m.is_empty() {
        crate::settings::DEFAULT_MODEL.into()
    } else {
        m
    };
    ui.settings.hotkey_modifiers = ui.mods;
    ui.settings.hotkey_key = ui.vk;
    ui.settings.save();
}

fn browse_folder(owner: HWND, initial: &str) -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FILEOPENDIALOGOPTIONS, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
        SIGDN_FILESYSPATH,
    };
    unsafe {
        let dlg: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let opts = dlg.GetOptions().unwrap_or(FILEOPENDIALOGOPTIONS(0));
        let _ = dlg.SetOptions(FILEOPENDIALOGOPTIONS(
            opts.0 | FOS_PICKFOLDERS.0 | FOS_FORCEFILESYSTEM.0,
        ));
        let title = wide("选择保存目录");
        let _ = dlg.SetTitle(windows::core::PCWSTR(title.as_ptr()));
        if !initial.trim().is_empty() {
            let p = std::path::Path::new(initial);
            if p.exists() {
                let path_w = wide(initial);
                if let Ok(item) = windows::Win32::UI::Shell::SHCreateItemFromParsingName::<_, _, windows::Win32::UI::Shell::IShellItem>(
                    windows::core::PCWSTR(path_w.as_ptr()),
                    None,
                ) {
                    let _ = dlg.SetFolder(&item);
                }
            }
        }
        dlg.Show(owner).ok()?;
        let item = dlg.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        s
    }
}
