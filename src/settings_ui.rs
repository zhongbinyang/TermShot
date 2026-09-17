#![allow(clippy::too_many_arguments)]
use crate::native::{cursor_pos, dpi_scale_at, enable_dark_title, hinstance, sc, set_font, ui_font, work_area_from_point};
use crate::settings::{format_hotkey, PostCaptureAction, Settings};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowTextW,
    RegisterClassExW, SendMessageW, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
    TranslateMessage, BN_CLICKED, BS_AUTOCHECKBOX, BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING,
    CB_GETCURSEL, CB_SETCURSEL, ES_AUTOHSCROLL, ES_PASSWORD, GWLP_USERDATA, MSG, SW_SHOW,
    WM_COMMAND, WM_DESTROY, WM_KEYDOWN, WNDCLASSEXW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_OVERLAPPED,
    WS_SYSMENU, WS_VISIBLE,
};

struct Ui {
    settings: Settings,
    hwnd: HWND,
    dir: HWND,
    action: HWND,
    hotkey: HWND,
    quote: HWND,
    startup: HWND,
    key: HWND,
    model: HWND,
    recording: bool,
    mods: u32,
    vk: u32,
    saved: bool,
    scale: f32,
    font: HFONT,
}

pub fn run(settings: Settings, on_suspend_hotkey: impl FnOnce()) -> Option<Settings> {
    on_suspend_hotkey();
    let scale = dpi_scale_at(cursor_pos()).max(1.0);
    let font = ui_font(sc(15, scale));
    let mut ui = Box::new(Ui {
        mods: settings.hotkey_modifiers,
        vk: settings.hotkey_key,
        settings,
        hwnd: HWND::default(),
        dir: HWND::default(),
        action: HWND::default(),
        hotkey: HWND::default(),
        quote: HWND::default(),
        startup: HWND::default(),
        key: HWND::default(),
        model: HWND::default(),
        recording: false,
        saved: false,
        scale,
        font,
    });
    unsafe {
        let class = wide("TermShotSettings");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            hbrBackground: CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::WIN_BG.colorref())),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let work = work_area_from_point(cursor_pos());
        let w = sc(520, scale);
        let h = sc(420, scale);
        let x = work.x + ((work.w - w) / 2).max(0);
        let y = work.y + ((work.h - h) / 2).max(0);
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("终端截图 — 设置"),
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
            // if window destroyed
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !hwnd.is_invalid() && windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        let _ = DeleteObject(ui.font);
    }
    if ui.saved {
        Some(ui.settings.clone())
    } else {
        None
    }
}

fn build_controls(ui: &mut Ui) {
    unsafe {
        let h = ui.hwnd;
        let s = ui.scale;
        let p = |v: i32| sc(v, s);
        let inst = hinstance();
        label(h, p(20), p(20), p(80), p(24), "保存目录", ui.font);
        ui.dir = edit(
            h,
            p(110),
            p(18),
            p(280),
            p(26),
            &ui.settings.resolved_save_directory().display().to_string(),
            false,
            ui.font,
        );
        button(h, p(400), p(18), p(80), p(26), "浏览", 101, ui.font);
        label(h, p(20), p(56), p(80), p(24), "截图后", ui.font);
        ui.action = combo(h, p(110), p(54), p(370), p(200), ui.font);
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
        label(h, p(20), p(92), p(80), p(24), "快捷键", ui.font);
        ui.hotkey = edit(h, p(110), p(90), p(280), p(26), &ui.settings.format_hotkey(), false, ui.font);
        button(h, p(400), p(90), p(80), p(26), "录制", 102, ui.font);
        ui.quote = check(h, p(110), p(130), p(160), p(24), "路径加双引号", ui.settings.quote_path, 201, ui.font);
        ui.startup = check(h, p(280), p(130), p(160), p(24), "开机自启动", ui.settings.start_with_windows, 202, ui.font);
        label(h, p(20), p(170), p(80), p(24), "API Key", ui.font);
        ui.key = edit(h, p(110), p(168), p(370), p(26), &ui.settings.deep_seek_api_key, true, ui.font);
        label(h, p(20), p(206), p(80), p(24), "模型名称", ui.font);
        ui.model = edit(h, p(110), p(204), p(370), p(26), &ui.settings.model_name(), false, ui.font);
        button(h, p(300), p(320), p(80), p(32), "保存", 1, ui.font);
        button(h, p(390), p(320), p(80), p(32), "取消", 2, ui.font);
        let _ = inst;
    }
}

fn label(parent: HWND, x: i32, y: i32, w: i32, h: i32, text: &str, font: HFONT) -> HWND {
    child(parent, "STATIC", text, 0, x, y, w, h, 0, font)
}
fn edit(parent: HWND, x: i32, y: i32, w: i32, h: i32, text: &str, password: bool, font: HFONT) -> HWND {
    let style = WS_CHILD.0
        | WS_VISIBLE.0
        | WS_BORDER.0
        | ES_AUTOHSCROLL as u32
        | if password { ES_PASSWORD as u32 } else { 0 };
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
        SendMessageW(hwnd, windows::Win32::UI::WindowsAndMessaging::BM_SETCHECK, WPARAM(if on { 1 } else { 0 }), LPARAM(0));
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

fn child(parent: HWND, cls: &str, text: &str, style: i32, x: i32, y: i32, w: i32, h: i32, id: i32, font: HFONT) -> HWND {
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
    unsafe { SendMessageW(hwnd, windows::Win32::UI::WindowsAndMessaging::BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
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
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xFFFF;
            let code = ((wparam.0 as u32) >> 16) & 0xFFFF;
            if code == BN_CLICKED && id == 101 {
                if let Some(p) = browse_folder(hwnd) {
                    let w = wide(&p);
                    let _ = SetWindowTextW(ui.dir, windows::core::PCWSTR(w.as_ptr()));
                }
            } else if id == 102 {
                ui.recording = true;
                let w = wide("按下新快捷键…");
                let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
            } else if id == 1 {
                save_from_ui(ui);
                ui.saved = true;
                let _ = DestroyWindow(hwnd);
            } else if id == 2 {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if ui.recording {
                let vk = wparam.0 as u32;
                if vk == 0x1B {
                    ui.recording = false;
                    let w = wide(&format_hotkey(ui.mods, ui.vk));
                    let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                    return LRESULT(0);
                }
                let mut mods = 0u32;
                if GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000 != 0 {
                    mods |= crate::native::MOD_CONTROL_BIT;
                }
                if GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000 != 0 {
                    mods |= crate::native::MOD_SHIFT_BIT;
                }
                if GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000 != 0 {
                    mods |= crate::native::MOD_ALT_BIT;
                }
                if mods == 0 {
                    return LRESULT(0);
                }
                ui.mods = mods;
                ui.vk = vk;
                ui.recording = false;
                let w = wide(&format_hotkey(ui.mods, ui.vk));
                let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            if !ui.font.is_invalid() {
                let _ = DeleteObject(ui.font);
                ui.font = HFONT::default();
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

fn browse_folder(owner: HWND) -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FILEOPENDIALOGOPTIONS, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
        SIGDN_FILESYSPATH,
    };
    unsafe {
        let dlg: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let opts = dlg.GetOptions().unwrap_or(FILEOPENDIALOGOPTIONS(0));
        dlg.SetOptions(FILEOPENDIALOGOPTIONS(
            opts.0 | FOS_PICKFOLDERS.0 | FOS_FORCEFILESYSTEM.0,
        ))
        .ok()?;
        let title = wide("选择保存目录");
        let _ = dlg.SetTitle(windows::core::PCWSTR(title.as_ptr()));
        dlg.Show(owner).ok()?;
        let item = dlg.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        s
    }
}
