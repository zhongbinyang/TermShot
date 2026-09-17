#![allow(clippy::too_many_arguments)]
use crate::geom::Color;
use crate::native::{
    cursor_pos, dpi_scale_at, enable_dark_title, hinstance, sc, set_font, ui_font,
    work_area_from_point,
};
use crate::settings::{format_hotkey, PostCaptureAction, Settings};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, RoundRect,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DRAW_TEXT_FORMAT, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, DT_WORDBREAK, HBRUSH, HDC, HFONT, HPEN, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID,
    SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_CONTROL, VK_ESCAPE, VK_MENU,
    VK_RETURN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, GetWindowTextW, LoadCursorW, RegisterClassExW, SendMessageW,
    SetCursor, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
    TranslateMessage, BN_CLICKED, BS_AUTOCHECKBOX, BS_OWNERDRAW, CBS_DROPDOWNLIST, CB_ADDSTRING,
    CB_GETCURSEL, CB_SETCURSEL, ES_AUTOHSCROLL, ES_PASSWORD, ES_READONLY, GWLP_USERDATA,
    IDC_HAND, MSG, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WM_COMMAND,
    WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT,
    WM_SETCURSOR, WM_SYSKEYDOWN, WNDCLASSEXW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_OVERLAPPED,
    WS_SYSMENU, WS_VISIBLE,
};

const EM_SETPASSWORDCHAR: u32 = 0x00CC;
const WM_MOUSELEAVE: u32 = 0x02A3;


const TAB_COUNT: usize = 4;
const TAB_ITEMS: [(&str, &str); TAB_COUNT] = [
    ("⚙", "通用偏好"),
    ("⌨", "快捷按键"),
    ("🤖", "AI 智能"),
    ("ℹ", "关于软件"),
];

struct Ui {
    settings: Settings,
    hwnd: HWND,
    active_tab: usize,
    hover_tab: Option<usize>,
    tab_controls: [Vec<HWND>; TAB_COUNT],

    // Tab 0 controls
    dir: HWND,
    browse_btn: HWND,
    open_btn: HWND,
    quote: HWND,
    action: HWND,
    startup: HWND,

    // Tab 1 controls
    hotkey: HWND,
    record_btn: HWND,
    reset_btn: HWND,

    // Tab 2 controls
    key: HWND,
    toggle_key_btn: HWND,
    model: HWND,

    // Tab 3 controls
    open_config_btn: HWND,

    // Bottom persistent controls
    save_btn: HWND,
    cancel_btn: HWND,

    // State
    recording: bool,
    show_key: bool,
    status_text: String,
    mods: u32,
    vk: u32,
    saved: bool,
    scale: f32,

    // GDI resources
    font: HFONT,
    font_bold: HFONT,
    font_title: HFONT,
    font_sm: HFONT,
    font_code: HFONT,
    bg_brush: HBRUSH,
    sidebar_brush: HBRUSH,
    panel_brush: HBRUSH,
    hover_brush: HBRUSH,
    input_brush: HBRUSH,
    bottom_brush: HBRUSH,
    accent_brush: HBRUSH,
    border_pen: HPEN,
    border_brush: HBRUSH,
}

pub fn run(settings: Settings, on_suspend_hotkey: impl FnOnce()) -> Option<Settings> {
    on_suspend_hotkey();
    let scale = dpi_scale_at(cursor_pos()).max(1.0);
    let p = |v: i32| sc(v, scale);

    let font = ui_font(p(13));
    let font_bold = make_font(p(14), true, "Segoe UI");
    let font_title = make_font(p(17), true, "Segoe UI");
    let font_sm = make_font(p(12), false, "Segoe UI");
    let font_code = make_font(p(12), true, "Consolas");

    let bg_brush = unsafe { CreateSolidBrush(COLORREF(theme::WIN_BG.colorref())) };
    let sidebar_brush = unsafe { CreateSolidBrush(COLORREF(theme::SIDEBAR_BG.colorref())) };
    let panel_brush = unsafe { CreateSolidBrush(COLORREF(theme::PANEL.colorref())) };
    let hover_brush = unsafe { CreateSolidBrush(COLORREF(theme::HOVER_BG.colorref())) };
    let input_brush = unsafe { CreateSolidBrush(COLORREF(theme::INPUT_BG.colorref())) };
    let bottom_brush = unsafe { CreateSolidBrush(COLORREF(theme::BOTTOM_BAR_BG.colorref())) };
    let accent_brush = unsafe { CreateSolidBrush(COLORREF(theme::ACCENT.colorref())) };
    let border_pen = unsafe { CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref())) };
    let border_brush = unsafe { CreateSolidBrush(COLORREF(theme::BORDER.colorref())) };

    let mut ui = Box::new(Ui {
        mods: settings.hotkey_modifiers,
        vk: settings.hotkey_key,
        settings,
        hwnd: HWND::default(),
        active_tab: 0,
        hover_tab: None,
        tab_controls: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],

        dir: HWND::default(),
        browse_btn: HWND::default(),
        open_btn: HWND::default(),
        quote: HWND::default(),
        action: HWND::default(),
        startup: HWND::default(),

        hotkey: HWND::default(),
        record_btn: HWND::default(),
        reset_btn: HWND::default(),

        key: HWND::default(),
        toggle_key_btn: HWND::default(),
        model: HWND::default(),

        open_config_btn: HWND::default(),

        save_btn: HWND::default(),
        cancel_btn: HWND::default(),

        recording: false,
        show_key: false,
        status_text: "提示: Esc 键关闭窗口，Enter 键保存配置".to_string(),
        saved: false,
        scale,

        font,
        font_bold,
        font_title,
        font_sm,
        font_code,
        bg_brush,
        sidebar_brush,
        panel_brush,
        hover_brush,
        input_brush,
        bottom_brush,
        accent_brush,
        border_pen,
        border_brush,
    });

    unsafe {
        let class = wide("TermShotSettingsModern");
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
        let client_w = p(680);
        let client_h = p(540);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE;

        let mut win_rc = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        let _ = AdjustWindowRectEx(&mut win_rc, style, false, WINDOW_EX_STYLE(0));
        let w = win_rc.right - win_rc.left;
        let h = win_rc.bottom - win_rc.top;
        let x = work.x + ((work.w - w) / 2).max(0);
        let y = work.y + ((work.h - h) / 2).max(0);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShot — 设置"),
            style,
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

            // Intercept keyboard events for hotkey recording and dialog controls
            if msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN {
                let vk = msg.wParam.0 as u32;
                if ui.recording {
                    if vk == VK_ESCAPE.0 as u32 {
                        ui.recording = false;
                        let text = wide(&format_hotkey(ui.mods, ui.vk));
                        let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(text.as_ptr()));
                        let btn_text = wide("录制");
                        let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_text.as_ptr()));
                        set_status(ui.as_mut(), "已取消录制快捷键");
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.record_btn, None, true);
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
                        set_status(ui.as_mut(), "请配合 Ctrl / Alt / Shift 组合键使用");
                        continue;
                    }

                    ui.mods = mods;
                    ui.vk = vk;
                    ui.recording = false;
                    let text = wide(&format_hotkey(ui.mods, ui.vk));
                    let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(text.as_ptr()));
                    let btn_text = wide("录制");
                    let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_text.as_ptr()));
                    set_status(ui.as_mut(), "新快捷键录制完成，点击“保存设置”生效");
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.record_btn, None, true);
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

fn make_font(px: i32, bold: bool, name: &str) -> HFONT {
    let wide_name = wide(name);
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
            windows::core::PCWSTR(wide_name.as_ptr()),
        )
    }
}

fn set_status(ui: &mut Ui, text: &str) {
    ui.status_text = text.to_string();
    let p = |v: i32| sc(v, ui.scale);
    let rc = RECT {
        left: p(16),
        top: p(484),
        right: p(420),
        bottom: p(540),
    };
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.hwnd, Some(&rc), false);
    }
}

fn hit_tab(ui: &Ui, x: i32, y: i32) -> Option<usize> {
    let p = |v: i32| sc(v, ui.scale);
    let tab_x = p(10);
    let tab_w = p(160);
    let tab_h = p(40);
    for i in 0..TAB_COUNT {
        let ty = p(76) + i as i32 * p(46);
        if x >= tab_x && x < tab_x + tab_w && y >= ty && y < ty + tab_h {
            return Some(i);
        }
    }
    None
}

fn switch_tab(ui: &mut Ui, new_tab: usize) {
    if ui.active_tab == new_tab || new_tab >= TAB_COUNT {
        return;
    }
    unsafe {
        for (idx, list) in ui.tab_controls.iter().enumerate() {
            let cmd = if idx == new_tab { SW_SHOW } else { SW_HIDE };
            for &ctrl in list {
                let _ = ShowWindow(ctrl, cmd);
            }
        }
    }
    ui.active_tab = new_tab;
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.hwnd, None, false);
    }
}

fn build_controls(ui: &mut Ui) {
    unsafe {
        let h = ui.hwnd;
        let s = ui.scale;
        let p = |v: i32| sc(v, s);

        // ================= Tab 0: 通用偏好 =================
        let dir = edit(
            h,
            p(284),
            p(106),
            p(220),
            p(26),
            &ui.settings.resolved_save_directory().display().to_string(),
            false,
            false,
            ui.font,
        );
        let browse_btn = owner_button(h, p(512), p(106), p(66), p(26), "浏览...", 101, ui.font);
        let open_btn = owner_button(h, p(584), p(106), p(60), p(26), "打开", 102, ui.font);

        let quote = check(
            h,
            p(216),
            p(144),
            p(428),
            p(24),
            "复制保存路径时包含双引号 (方便终端直接 cd / cat 路径)",
            ui.settings.quote_path,
            201,
            ui.font,
        );

        let action = combo(h, p(300), p(275), p(344), p(200), ui.font);
        for (_, t) in PostCaptureAction::all() {
            let w = wide(t);
            SendMessageW(action, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
        }
        SendMessageW(
            action,
            CB_SETCURSEL,
            WPARAM(ui.settings.post_capture_action.index() as usize),
            LPARAM(0),
        );

        let startup = check(
            h,
            p(216),
            p(318),
            p(428),
            p(24),
            "开机时自动启动 TermShot (随 Windows 登录并在托盘常驻)",
            ui.settings.start_with_windows,
            202,
            ui.font,
        );

        ui.dir = dir;
        ui.browse_btn = browse_btn;
        ui.open_btn = open_btn;
        ui.quote = quote;
        ui.action = action;
        ui.startup = startup;
        ui.tab_controls[0] = vec![dir, browse_btn, open_btn, quote, action, startup];

        // ================= Tab 1: 快捷按键 =================
        let hotkey = edit(
            h,
            p(300),
            p(106),
            p(174),
            p(26),
            &ui.settings.format_hotkey(),
            false,
            true,
            ui.font,
        );
        let record_btn = owner_button(h, p(482), p(106), p(78), p(26), "录制", 103, ui.font);
        let reset_btn = owner_button(h, p(566), p(106), p(78), p(26), "恢复默认", 104, ui.font);

        ui.hotkey = hotkey;
        ui.record_btn = record_btn;
        ui.reset_btn = reset_btn;
        ui.tab_controls[1] = vec![hotkey, record_btn, reset_btn];

        // ================= Tab 2: AI 智能 =================
        let key = edit(
            h,
            p(284),
            p(106),
            p(284),
            p(26),
            &ui.settings.deep_seek_api_key,
            true,
            false,
            ui.font,
        );
        let toggle_key_btn = owner_button(h, p(574), p(106), p(70), p(26), "显示", 105, ui.font);
        let model = edit(
            h,
            p(284),
            p(144),
            p(360),
            p(26),
            &ui.settings.model_name(),
            false,
            false,
            ui.font,
        );

        ui.key = key;
        ui.toggle_key_btn = toggle_key_btn;
        ui.model = model;
        ui.tab_controls[2] = vec![key, toggle_key_btn, model];

        // ================= Tab 3: 关于软件 =================
        let open_config_btn = owner_button(
            h,
            p(504),
            p(320),
            p(140),
            p(28),
            "打开配置目录",
            106,
            ui.font,
        );

        ui.open_config_btn = open_config_btn;
        ui.tab_controls[3] = vec![open_config_btn];

        // Hide tabs 1, 2, 3 controls initially
        for &c in &ui.tab_controls[1] {
            let _ = ShowWindow(c, SW_HIDE);
        }
        for &c in &ui.tab_controls[2] {
            let _ = ShowWindow(c, SW_HIDE);
        }
        for &c in &ui.tab_controls[3] {
            let _ = ShowWindow(c, SW_HIDE);
        }

        // ================= 底部持久化栏 =================
        ui.save_btn = owner_button(h, p(424), p(496), p(126), p(32), "保存设置 (Enter)", 1, ui.font_bold);
        ui.cancel_btn = owner_button(h, p(558), p(496), p(102), p(32), "取消 (Esc)", 2, ui.font);
    }
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

fn owner_button(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    id: i32,
    font: HFONT,
) -> HWND {
    let hwnd = child(
        parent,
        "BUTTON",
        text,
        (WS_CHILD.0 | WS_VISIBLE.0 | BS_OWNERDRAW as u32) as i32,
        x,
        y,
        w,
        h,
        id,
        font,
    );
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(button_subclass_proc), id as usize, 0);
    }
    hwnd
}

fn check(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    on: bool,
    id: i32,
    font: HFONT,
) -> HWND {
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
            WINDOW_EX_STYLE(0),
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

unsafe extern "system" fn button_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _ref_data: usize,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
        }
        WM_MOUSELEAVE => {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
        }
        WM_SETCURSOR => {
            let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
            SetCursor(cursor);
            return LRESULT(1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
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

fn draw_owner_button(ui: &Ui, dis: &DRAWITEMSTRUCT) {
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let is_pressed = (dis.itemState.0 & ODS_SELECTED.0) != 0;
    let is_disabled = (dis.itemState.0 & ODS_DISABLED.0) != 0;
    let is_accent = dis.CtlID == 1;
    let is_recording_btn = dis.CtlID == 103 && ui.recording;

    let cur = cursor_pos();
    let mut pt = windows::Win32::Foundation::POINT { x: cur.x, y: cur.y };
    let _ = unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(dis.hwndItem, &mut pt) };
    let is_hover = pt.x >= 0
        && pt.x < (rc.right - rc.left)
        && pt.y >= 0
        && pt.y < (rc.bottom - rc.top);

    let (bg_color, border_color, text_color, font) = if is_disabled {
        (
            theme::PANEL,
            theme::BORDER,
            theme::DIM,
            ui.font,
        )
    } else if is_accent {
        if is_pressed {
            (
                Color::rgb(0x22, 0xB0, 0x82),
                Color::rgb(0x22, 0xB0, 0x82),
                Color::rgb(0x06, 0x22, 0x1B),
                ui.font_bold,
            )
        } else if is_hover {
            (
                theme::ACCENT_HI,
                theme::ACCENT_HI,
                Color::rgb(0x06, 0x22, 0x1B),
                ui.font_bold,
            )
        } else {
            (
                theme::ACCENT,
                theme::ACCENT,
                Color::rgb(0x0B, 0x3D, 0x33),
                ui.font_bold,
            )
        }
    } else if is_recording_btn {
        if is_pressed || is_hover {
            (
                Color::rgb(0x52, 0x1A, 0x1A),
                theme::DANGER,
                Color::rgb(0xFF, 0x99, 0x99),
                ui.font_bold,
            )
        } else {
            (
                Color::rgb(0x3B, 0x14, 0x14),
                theme::DANGER,
                theme::DANGER,
                ui.font_bold,
            )
        }
    } else if is_pressed {
        (
            Color::rgb(0x12, 0x18, 0x20),
            theme::ACCENT,
            theme::TEXT,
            ui.font,
        )
    } else if is_hover {
        (
            theme::HOVER_BG,
            Color::rgb(0x3D, 0x4B, 0x5E),
            theme::TEXT,
            ui.font,
        )
    } else {
        (
            theme::PANEL,
            theme::BORDER,
            theme::TEXT,
            ui.font,
        )
    };

    let p = |v: i32| sc(v, ui.scale);
    let round = p(6);

    unsafe {
        let brush = CreateSolidBrush(COLORREF(bg_color.colorref()));
        let pen = CreatePen(PS_SOLID, 1, COLORREF(border_color.colorref()));
        let old_brush = SelectObject(hdc, brush);
        let old_pen = SelectObject(hdc, pen);

        let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, round, round);

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush);
        let _ = DeleteObject(pen);

        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(text_color.colorref()));
        let old_font = SelectObject(hdc, font);

        let mut buf = [0u16; 128];
        let len = GetWindowTextW(dis.hwndItem, &mut buf) as usize;
        if len > 0 {
            let mut draw_rc = rc;
            if is_pressed {
                draw_rc.top += p(1);
            }
            DrawTextW(
                hdc,
                &mut buf[..len],
                &mut draw_rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
        SelectObject(hdc, old_font);
    }
}

fn draw_gdi_text(
    hdc: HDC,
    text: &str,
    mut rc: RECT,
    font: HFONT,
    color: Color,
    flags: DRAW_TEXT_FORMAT,
) {
    unsafe {
        let old_font = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(color.colorref()));
        let mut wt = wide(text);
        let len = wt.len().saturating_sub(1);
        if len > 0 {
            DrawTextW(hdc, &mut wt[..len], &mut rc, flags);
        }
        SelectObject(hdc, old_font);
    }
}

fn draw_card_box(
    hdc: HDC,
    left: i32,
    top: i32,
    w: i32,
    h: i32,
    round: i32,
    fill_brush: HBRUSH,
    border_pen: HPEN,
) {
    unsafe {
        let old_brush = SelectObject(hdc, fill_brush);
        let old_pen = SelectObject(hdc, border_pen);
        let _ = RoundRect(hdc, left, top, left + w, top + h, round, round);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
    }
}

fn paint_dialog(ui: &Ui, hdc: HDC, w: i32, h: i32) {
    let s = ui.scale;
    let p = |v: i32| sc(v, s);

    let sidebar_w = p(180);
    let bottom_y = p(484);

    // 1. Fill main background
    let whole_rc = RECT {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    unsafe {
        FillRect(hdc, &whole_rc, ui.bg_brush);
    }

    // 2. Sidebar background & divider
    let sidebar_rc = RECT {
        left: 0,
        top: 0,
        right: sidebar_w,
        bottom: bottom_y,
    };
    unsafe {
        FillRect(hdc, &sidebar_rc, ui.sidebar_brush);
        let div_rc = RECT {
            left: sidebar_w - 1,
            top: 0,
            right: sidebar_w,
            bottom: bottom_y,
        };
        FillRect(hdc, &div_rc, ui.border_brush);
    }

    // Sidebar Brand Header
    draw_gdi_text(
        hdc,
        "TermShot",
        RECT {
            left: p(16),
            top: p(18),
            right: sidebar_w - p(16),
            bottom: p(42),
        },
        ui.font_title,
        theme::TEXT,
        DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_gdi_text(
        hdc,
        "v1.0.0 · 智能极速截图",
        RECT {
            left: p(16),
            top: p(42),
            right: sidebar_w - p(16),
            bottom: p(60),
        },
        ui.font_sm,
        theme::DIM,
        DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
    );
    let brand_div = RECT {
        left: p(14),
        top: p(64),
        right: sidebar_w - p(14),
        bottom: p(65),
    };
    unsafe {
        FillRect(hdc, &brand_div, ui.border_brush);
    }

    // Sidebar Tabs
    let tab_x = p(10);
    let tab_w = sidebar_w - p(20);
    let tab_h = p(40);
    let round_sm = p(6);

    for (i, &(icon, label)) in TAB_ITEMS.iter().enumerate() {
        let ty = p(76) + i as i32 * p(46);
        let is_active = i == ui.active_tab;
        let is_hover = Some(i) == ui.hover_tab;

        if is_active {
            draw_card_box(hdc, tab_x, ty, tab_w, tab_h, round_sm, ui.panel_brush, ui.border_pen);
            // Accent left bar
            let bar_rc = RECT {
                left: tab_x + p(2),
                top: ty + p(8),
                right: tab_x + p(5),
                bottom: ty + tab_h - p(8),
            };
            unsafe {
                FillRect(hdc, &bar_rc, ui.accent_brush);
            }
        } else if is_hover {
            let brush = ui.hover_brush;
            let pen = ui.border_pen;
            draw_card_box(hdc, tab_x, ty, tab_w, tab_h, round_sm, brush, pen);
        }

        let text_color = if is_active {
            theme::ACCENT_HI
        } else if is_hover {
            theme::TEXT
        } else {
            theme::DIM
        };

        // Tab icon + text
        let display = format!(" {}   {}", icon, label);
        draw_gdi_text(
            hdc,
            &display,
            RECT {
                left: tab_x + p(10),
                top: ty,
                right: tab_x + tab_w - p(8),
                bottom: ty + tab_h,
            },
            if is_active { ui.font_bold } else { ui.font },
            text_color,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }

    // 3. Right Content Area
    let content_x = p(200);
    let card_w = p(460);
    let round_card = p(8);

    // Active tab header text
    let (tab_title, tab_sub) = match ui.active_tab {
        0 => ("通用偏好设置", "配置截图保存路径、终端输出格式与开机行为"),
        1 => ("快捷按键设置", "管理全局唤醒快捷键及内置高效截图操作速查"),
        2 => ("DeepSeek AI 智能识别", "配置大语言模型 API，赋能截图 OCR 文字提取与划词翻译"),
        _ => ("关于 TermShot", "轻量、极速、无广告的现代化原生截图工具"),
    };

    draw_gdi_text(
        hdc,
        tab_title,
        RECT {
            left: content_x,
            top: p(16),
            right: content_x + card_w,
            bottom: p(40),
        },
        ui.font_title,
        theme::TEXT,
        DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_gdi_text(
        hdc,
        tab_sub,
        RECT {
            left: content_x,
            top: p(42),
            right: content_x + card_w,
            bottom: p(60),
        },
        ui.font_sm,
        theme::DIM,
        DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
    );

    // Render Cards according to active tab
    match ui.active_tab {
        0 => {
            // Card 1: 存储路径与终端交互
            let c1_y = p(66);
            let c1_h = p(154);
            draw_card_box(hdc, content_x, c1_y, card_w, c1_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "📁   存储路径与终端交互",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "保存目录:",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(44),
                    right: content_x + p(80),
                    bottom: c1_y + p(70),
                },
                ui.font,
                theme::TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "说明: 留空将默认使用 Windows 系统 Pictures/Screenshots 目录",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(118),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(140),
                },
                ui.font_sm,
                theme::DIM,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            // Card 2: 截图后行为与启动项
            let c2_y = p(232);
            let c2_h = p(140);
            draw_card_box(hdc, content_x, c2_y, card_w, c2_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "⚡   截图后行为与启动项",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c2_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "完成时动作:",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(46),
                    right: content_x + p(96),
                    bottom: c2_y + p(72),
                },
                ui.font,
                theme::TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
        1 => {
            // Card 1: 全局唤醒快捷键
            let c1_y = p(66);
            let c1_h = p(126);
            draw_card_box(hdc, content_x, c1_y, card_w, c1_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "🎯   全局唤醒截图",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "唤醒快捷键:",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(44),
                    right: content_x + p(96),
                    bottom: c1_y + p(70),
                },
                ui.font,
                theme::TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "推荐使用 Ctrl / Alt / Shift 组合键或 F1-F12 单键；录制时按 Esc 可取消。",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(88),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(112),
                },
                ui.font_sm,
                theme::DIM,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            // Card 2: 截图模式内置快捷键速查表
            let c2_y = p(204);
            let c2_h = p(266);
            draw_card_box(hdc, content_x, c2_y, card_w, c2_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "📖   截图模式内置快捷键速查",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(12),
                    right: content_x + card_w - p(16),
                    bottom: c2_y + p(34),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            let cheatsheet = [
                ("Enter / 双击选区", "完成截图并根据设定写入剪贴板或保存文件"),
                ("Ctrl + S", "另存为 PNG 文件到自定义目录"),
                ("Ctrl + C", "仅复制当前截取图像到剪贴板，不写路径"),
                ("Ctrl + Z", "撤销上一步涂鸦、画笔、马赛克或标注"),
                ("Esc", "取消并立即放弃退出当前截图"),
                ("Tab", "切换当前选区编辑控制锚点 / 缩放手柄"),
                ("方向键 ↑ ↓ ← →", "以 1 像素高精度微调当前选区位置"),
                ("Shift + 方向键", "以 10 像素步长快速调整当前选区尺寸"),
            ];

            let badge_bg = Color::rgb(0x11, 0x16, 0x20);
            let badge_border = theme::BORDER;
            let round_badge = p(4);

            for (i, &(key_cap, desc)) in cheatsheet.iter().enumerate() {
                let row_y = c2_y + p(38) + i as i32 * p(28);

                // Key badge
                let badge_w = p(126);
                let badge_h = p(22);
                let bx = content_x + p(16);

                let brush = unsafe { CreateSolidBrush(COLORREF(badge_bg.colorref())) };
                let pen = unsafe { CreatePen(PS_SOLID, 1, COLORREF(badge_border.colorref())) };
                draw_card_box(hdc, bx, row_y, badge_w, badge_h, round_badge, brush, pen);
                unsafe {
                    let _ = DeleteObject(brush);
                    let _ = DeleteObject(pen);
                }

                draw_gdi_text(
                    hdc,
                    key_cap,
                    RECT {
                        left: bx,
                        top: row_y,
                        right: bx + badge_w,
                        bottom: row_y + badge_h,
                    },
                    ui.font_code,
                    theme::ACCENT_HI,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Description
                draw_gdi_text(
                    hdc,
                    desc,
                    RECT {
                        left: bx + badge_w + p(12),
                        top: row_y,
                        right: content_x + card_w - p(16),
                        bottom: row_y + badge_h,
                    },
                    ui.font_sm,
                    theme::TEXT,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
            }
        }
        2 => {
            // Card 1: DeepSeek API 服务配置
            let c1_y = p(66);
            let c1_h = p(168);
            draw_card_box(hdc, content_x, c1_y, card_w, c1_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "🔑   DeepSeek API 密钥与模型",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(14),
                    right: content_x + p(260),
                    bottom: c1_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            // API Status badge
            let has_key = !ui.settings.deep_seek_api_key.trim().is_empty();
            let (status_icon_text, status_color) = if has_key {
                ("●  API Key 已就绪", theme::ACCENT)
            } else {
                ("○  未配置 (功能受限)", Color::rgb(0xE5, 0xC0, 0x7B))
            };
            draw_gdi_text(
                hdc,
                status_icon_text,
                RECT {
                    left: content_x + card_w - p(160),
                    top: c1_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(36),
                },
                ui.font_sm,
                status_color,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "API Key:",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(44),
                    right: content_x + p(80),
                    bottom: c1_y + p(70),
                },
                ui.font,
                theme::TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "识别模型:",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(82),
                    right: content_x + p(80),
                    bottom: c1_y + p(108),
                },
                ui.font,
                theme::TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "推荐模型: deepseek-chat 或 deepseek-flash",
                RECT {
                    left: content_x + p(84),
                    top: c1_y + p(116),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(138),
                },
                ui.font_sm,
                theme::DIM,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            // Card 2: AI 特性说明
            let c2_y = p(246);
            let c2_h = p(224);
            draw_card_box(hdc, content_x, c2_y, card_w, c2_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "✨   AI 智能视觉工作流",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c2_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            let ai_features = [
                ("🔍  智能 OCR 文字提取", "截图完成后点击工具栏提取文字，自动保留多行排版、代码缩进及表格数据。"),
                ("🌐  多语言双语即时翻译", "划选屏幕外文内容，一键中英互译并直接呈现译文悬浮层，大幅提升阅读效率。"),
                ("🔒  本地按需与隐私安全", "仅在主动触发“提取文字”或“翻译”时调用官方 API，绝不在后台自动上传屏幕。"),
            ];

            for (i, &(title, desc)) in ai_features.iter().enumerate() {
                let fy = c2_y + p(44) + i as i32 * p(56);
                draw_gdi_text(
                    hdc,
                    title,
                    RECT {
                        left: content_x + p(16),
                        top: fy,
                        right: content_x + card_w - p(16),
                        bottom: fy + p(22),
                    },
                    ui.font_bold,
                    theme::ACCENT_HI,
                    DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
                );
                draw_gdi_text(
                    hdc,
                    desc,
                    RECT {
                        left: content_x + p(16),
                        top: fy + p(22),
                        right: content_x + card_w - p(16),
                        bottom: fy + p(48),
                    },
                    ui.font_sm,
                    theme::DIM,
                    DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
                );
            }
        }
        _ => {
            // Card 1: 软件规格与技术特性
            let c1_y = p(66);
            let c1_h = p(204);
            draw_card_box(hdc, content_x, c1_y, card_w, c1_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "💻   TermShot for Windows",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "版本 1.0.0 · 专为开发者与极客打造的超轻量高性能截图利器",
                RECT {
                    left: content_x + p(16),
                    top: c1_y + p(38),
                    right: content_x + card_w - p(16),
                    bottom: c1_y + p(58),
                },
                ui.font_sm,
                theme::ACCENT_HI,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            let about_items = [
                "⚡ 极速冷启动与超低内存：纯 Rust 打造，常驻内存 < 15MB，无任何 Electron 沉重负担",
                "🎨 原生 GDI 双缓冲渲染：纯 Win32 原生平滑自绘，极致响应，杜绝卡顿与界面撕裂",
                "🛡 永久开源免费：基于宽松友好的 MIT License 开源协议，支持个人与团队自由商用",
                "📋 开发者终端友好：支持复制文件路径附带双引号，方便在 PowerShell / bash 中一键 cd 或引用",
            ];

            for (i, &item) in about_items.iter().enumerate() {
                let iy = c1_y + p(68) + i as i32 * p(30);
                draw_gdi_text(
                    hdc,
                    item,
                    RECT {
                        left: content_x + p(16),
                        top: iy,
                        right: content_x + card_w - p(16),
                        bottom: iy + p(26),
                    },
                    ui.font_sm,
                    theme::TEXT,
                    DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
                );
            }

            // Card 2: 本地配置与数据管理
            let c2_y = p(282);
            let c2_h = p(140);
            draw_card_box(hdc, content_x, c2_y, card_w, c2_h, round_card, ui.panel_brush, ui.border_pen);

            draw_gdi_text(
                hdc,
                "📂   本地配置与数据管理",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(14),
                    right: content_x + card_w - p(16),
                    bottom: c2_y + p(36),
                },
                ui.font_bold,
                theme::TEXT,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "配置存储路径:  %LOCALAPPDATA%\\TermShot\\settings.json",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(44),
                    right: content_x + p(320),
                    bottom: c2_y + p(70),
                },
                ui.font,
                theme::TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );

            draw_gdi_text(
                hdc,
                "提示: 所有偏好设置以格式化 JSON 保存，便于版本管理、备份与跨机器迁移。",
                RECT {
                    left: content_x + p(16),
                    top: c2_y + p(96),
                    right: content_x + card_w - p(16),
                    bottom: c2_y + p(122),
                },
                ui.font_sm,
                theme::DIM,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
    }

    // 4. Bottom Persistent Action Bar
    let bottom_rc = RECT {
        left: 0,
        top: bottom_y,
        right: w,
        bottom: h,
    };
    unsafe {
        FillRect(hdc, &bottom_rc, ui.bottom_brush);
        let top_div = RECT {
            left: 0,
            top: bottom_y,
            right: w,
            bottom: bottom_y + 1,
        };
        FillRect(hdc, &top_div, ui.border_brush);
    }

    // Status / Tip text on bottom left
    let status_color = if ui.recording {
        theme::ACCENT_HI
    } else {
        theme::DIM
    };
    draw_gdi_text(
        hdc,
        &ui.status_text,
        RECT {
            left: p(20),
            top: bottom_y + p(14),
            right: p(410),
            bottom: h - p(10),
        },
        ui.font_sm,
        status_color,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
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
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            paint_dialog(ui, mem_dc, w, h);

            let _ = BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(mem_bmp);
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let hit = hit_tab(ui, x, y);
            if hit != ui.hover_tab {
                ui.hover_tab = hit;
                let p = |v: i32| sc(v, ui.scale);
                let sidebar_rc = RECT {
                    left: 0,
                    top: 0,
                    right: p(180),
                    bottom: p(484),
                };
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, Some(&sidebar_rc), false);
            }
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if ui.hover_tab.is_some() {
                ui.hover_tab = None;
                let p = |v: i32| sc(v, ui.scale);
                let sidebar_rc = RECT {
                    left: 0,
                    top: 0,
                    right: p(180),
                    bottom: p(484),
                };
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, Some(&sidebar_rc), false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            if let Some(tab_idx) = hit_tab(ui, x, y) {
                switch_tab(ui, tab_idx);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cur = cursor_pos();
            let mut pt = windows::Win32::Foundation::POINT { x: cur.x, y: cur.y };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
            if hit_tab(ui, pt.x, pt.y).is_some() {
                let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                SetCursor(cursor);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            draw_owner_button(ui, dis);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut _);
            let ctrl = HWND(lparam.0 as *mut _);
            if ctrl == ui.hotkey {
                SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
                SetBkColor(hdc, COLORREF(theme::INPUT_BG.colorref()));
                return LRESULT(ui.input_brush.0 as isize);
            }
            SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
            SetBkColor(hdc, COLORREF(theme::PANEL.colorref()));
            SetBkMode(hdc, TRANSPARENT);
            LRESULT(ui.panel_brush.0 as isize)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let hdc = HDC(wparam.0 as *mut _);
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
                            set_status(ui, "已更新保存目录路径");
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
                        set_status(ui, "已在资源管理器中打开保存目录");
                    }
                    103 => {
                        ui.recording = !ui.recording;
                        if ui.recording {
                            let btn_w = wide("停止录制");
                            let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_w.as_ptr()));
                            set_status(ui, "请直接按下新的快捷键组合（如 Ctrl+Alt+A），按 Esc 取消");
                            let place = wide("等待按下快捷键...");
                            let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(place.as_ptr()));
                        } else {
                            let btn_w = wide("录制");
                            let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_w.as_ptr()));
                            let w = wide(&format_hotkey(ui.mods, ui.vk));
                            let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                            set_status(ui, "已取消录制快捷键");
                        }
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.record_btn, None, true);
                    }
                    104 => {
                        ui.mods = crate::native::MOD_CONTROL_BIT | crate::native::MOD_SHIFT_BIT;
                        ui.vk = 0x53; // 'S'
                        ui.recording = false;
                        let w = wide(&format_hotkey(ui.mods, ui.vk));
                        let _ = SetWindowTextW(ui.hotkey, windows::core::PCWSTR(w.as_ptr()));
                        let btn_w = wide("录制");
                        let _ = SetWindowTextW(ui.record_btn, windows::core::PCWSTR(btn_w.as_ptr()));
                        set_status(ui, "已恢复默认快捷键: Ctrl+Shift+S (点击保存后生效)");
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.record_btn, None, true);
                    }
                    105 => {
                        ui.show_key = !ui.show_key;
                        let ch: usize = if ui.show_key { 0 } else { 0x25CF };
                        SendMessageW(ui.key, EM_SETPASSWORDCHAR, WPARAM(ch), LPARAM(0));
                        let btn_text = wide(if ui.show_key { "隐藏" } else { "显示" });
                        let _ = SetWindowTextW(ui.toggle_key_btn, windows::core::PCWSTR(btn_text.as_ptr()));
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.key, None, true);
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(ui.toggle_key_btn, None, true);
                    }
                    106 => {
                        let p = Settings::dir();
                        let _ = std::fs::create_dir_all(&p);
                        let _ = std::process::Command::new("explorer").arg(&p).spawn();
                        set_status(ui, "已在文件资源管理器中打开配置目录");
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
            if !ui.font_title.is_invalid() {
                let _ = DeleteObject(ui.font_title);
                ui.font_title = HFONT::default();
            }
            if !ui.font_sm.is_invalid() {
                let _ = DeleteObject(ui.font_sm);
                ui.font_sm = HFONT::default();
            }
            if !ui.font_code.is_invalid() {
                let _ = DeleteObject(ui.font_code);
                ui.font_code = HFONT::default();
            }
            if !ui.bg_brush.is_invalid() {
                let _ = DeleteObject(ui.bg_brush);
                ui.bg_brush = HBRUSH::default();
            }
            if !ui.sidebar_brush.is_invalid() {
                let _ = DeleteObject(ui.sidebar_brush);
                ui.sidebar_brush = HBRUSH::default();
            }
            if !ui.panel_brush.is_invalid() {
                let _ = DeleteObject(ui.panel_brush);
                ui.panel_brush = HBRUSH::default();
            }
            if !ui.hover_brush.is_invalid() {
                let _ = DeleteObject(ui.hover_brush);
                ui.hover_brush = HBRUSH::default();
            }
            if !ui.input_brush.is_invalid() {
                let _ = DeleteObject(ui.input_brush);
                ui.input_brush = HBRUSH::default();
            }
            if !ui.bottom_brush.is_invalid() {
                let _ = DeleteObject(ui.bottom_brush);
                ui.bottom_brush = HBRUSH::default();
            }
            if !ui.accent_brush.is_invalid() {
                let _ = DeleteObject(ui.accent_brush);
                ui.accent_brush = HBRUSH::default();
            }
            if !ui.border_pen.is_invalid() {
                let _ = DeleteObject(ui.border_pen);
                ui.border_pen = HPEN::default();
            }
            if !ui.border_brush.is_invalid() {
                let _ = DeleteObject(ui.border_brush);
                ui.border_brush = HBRUSH::default();
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
