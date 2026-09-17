use crate::bitmap::{capture_virtual_screen, try_save_png};
use crate::geom::Point;
use crate::icon;
use crate::native::{
    enable_dpi, foreground, hinstance, is_window, register_hotkey, set_foreground, sleep_ms,
    unregister_hotkey, HOTKEY_ID, WM_HOTKEY,
};
use crate::settings::{PostCaptureAction, Settings};
use crate::util::wide;
use crate::windows_enum::snapshot;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    InsertMenuW, PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetWindowLongPtrW,
    TrackPopupMenu, TranslateMessage, CS_DBLCLKS, GWLP_USERDATA, MF_BYPOSITION, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MSG, TPM_LEFTALIGN, TPM_RETURNCMD, WM_APP, WM_DESTROY, WM_LBUTTONUP,
    WM_RBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

const WM_TRAY: u32 = WM_APP + 1;

const ID_CAPTURE: usize = 1;
const ID_CLOSE_PINS: usize = 2;
const ID_OPEN_DIR: usize = 3;
const ID_SETTINGS: usize = 4;
const ID_EXIT: usize = 5;

struct App {
    hwnd: HWND,
    settings: Settings,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    nid: NOTIFYICONDATAW,
    busy: bool,
}

pub fn run_tray() {
    enable_dpi();
    let settings = Settings::load();
    let first = !Settings::file_path().exists();
    if settings.start_with_windows {
        let _ = crate::install::apply_startup(true, None);
    }
    if first {
        settings.save();
    }

    let icon = icon::create_hicon().unwrap_or_default();
    let mut app = Box::new(App {
        hwnd: HWND::default(),
        settings,
        icon,
        nid: NOTIFYICONDATAW::default(),
        busy: false,
    });

    unsafe {
        let class = wide("TermShotApp");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShotHotkeySink"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance(),
            Some(app.as_mut() as *mut App as *mut _),
        )
        .unwrap_or_default();
        app.hwnd = hwnd;
        crate::toast::set_host(hwnd);

        app.nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        app.nid.hWnd = hwnd;
        app.nid.uID = 1;
        app.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        app.nid.uCallbackMessage = WM_TRAY;
        app.nid.hIcon = icon;
        let tip = wide(&format!("终端截图  {}", app.settings.format_hotkey()));
        let n = tip.len().min(127);
        app.nid.szTip[..n].copy_from_slice(&tip[..n]);
        let _ = Shell_NotifyIconW(NIM_ADD, &app.nid);

        apply_hotkey(app.as_mut());
        if first {
            crate::toast::show(
                "终端截图已驻留托盘",
                &format!(
                    "左键或 {} 框选，截完后选择保存、复制或贴图",
                    app.settings.format_hotkey()
                ),
            );
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn apply_hotkey(app: &mut App) {
    unregister_hotkey(app.hwnd);
    if !register_hotkey(app.hwnd, app.settings.hotkey_modifiers, app.settings.hotkey_key) {
        crate::toast::show(
            "快捷键注册失败",
            &(app.settings.format_hotkey() + " 可能已被占用"),
        );
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let app = &mut *ptr;
    match msg {
        m if m == WM_HOTKEY && wparam.0 as i32 == HOTKEY_ID => {
            start_capture(app, false);
            LRESULT(0)
        }
        m if m == WM_TRAY => {
            match lparam.0 as u32 {
                WM_LBUTTONUP => start_capture(app, false),
                WM_RBUTTONUP => tray_menu(app),
                _ => {}
            }
            LRESULT(0)
        }
        m if m == crate::toast::WM_SHOW_TOAST => {
            crate::toast::dispatch(lparam);
            LRESULT(0)
        }
        WM_DESTROY => {
            crate::toast::set_host(HWND::default());
            let _ = Shell_NotifyIconW(NIM_DELETE, &app.nid);
            unregister_hotkey(hwnd);
            icon::destroy_icon(app.icon);
            crate::pin::close_all();
            crate::toast::close();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn tray_menu(app: &mut App) {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, ID_CAPTURE, windows::core::w!("截图"));
        let mut flags = MF_BYPOSITION | MF_STRING;
        if crate::pin::count() == 0 {
            flags |= MF_GRAYED;
        }
        let _ = InsertMenuW(menu, 1, flags, ID_CLOSE_PINS, windows::core::w!("关闭全部贴图"));
        let _ = InsertMenuW(menu, 2, MF_BYPOSITION | MF_SEPARATOR, 0, windows::core::PCWSTR::null());
        let _ = InsertMenuW(menu, 3, MF_BYPOSITION | MF_STRING, ID_OPEN_DIR, windows::core::w!("打开保存目录"));
        let _ = InsertMenuW(menu, 4, MF_BYPOSITION | MF_STRING, ID_SETTINGS, windows::core::w!("设置"));
        let _ = InsertMenuW(menu, 5, MF_BYPOSITION | MF_SEPARATOR, 0, windows::core::PCWSTR::null());
        let _ = InsertMenuW(menu, 6, MF_BYPOSITION | MF_STRING, ID_EXIT, windows::core::w!("退出"));
        let _ = SetForegroundWindow(app.hwnd);
        let cur = crate::native::cursor_pos();
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN, cur.x, cur.y, 0, app.hwnd, None).0 as usize;
        match cmd {
            ID_CAPTURE => {
                sleep_ms(150);
                start_capture(app, false);
            }
            ID_CLOSE_PINS => crate::pin::close_all(),
            ID_OPEN_DIR => open_dir(app),
            ID_SETTINGS => open_settings(app),
            ID_EXIT => {
                let _ = DestroyWindow(app.hwnd);
            }
            _ => {}
        }
    }
}

fn start_capture(app: &mut App, scroll_after: bool) {
    if app.busy {
        return;
    }
    app.busy = true;
    crate::native::apply_cursor_cross();
    let prev = foreground();
    let shot = match capture_virtual_screen() {
        Ok(s) => s,
        Err(e) => {
            crate::toast::show("截图失败", &e);
            app.busy = false;
            return;
        }
    };
    let windows = snapshot(HWND::default());
    let ask = !scroll_after && app.settings.post_capture_action == PostCaptureAction::Ask;
    let result = crate::overlay::run(shot.clone(), windows, prev, ask, scroll_after);
    if is_window(prev) {
        set_foreground(prev);
    }
    let Some(rect) = result.selected else {
        app.busy = false;
        return;
    };
    if result.scroll {
        if is_window(prev) {
            set_foreground(prev);
        }
        if let Some(bmp) = crate::scroll::run_scroll(rect) {
            finish(app, bmp, rect, None);
        }
        app.busy = false;
        return;
    }
    let vs = crate::native::virtual_screen();
    let bmp_rect = crate::geom::Rect::new(rect.x - vs.x, rect.y - vs.y, rect.w, rect.h);
    let mut crop = shot.crop(bmp_rect);
    let mut session = result.session;
    session.stamp(&mut crop, crate::geom::Point::new(bmp_rect.x, bmp_rect.y), Some(&shot));
    finish(app, crop, rect, result.action);
    app.busy = false;
}

fn finish(app: &mut App, mut bmp: crate::bitmap::Bitmap, screen: crate::geom::Rect, chosen: Option<PostCaptureAction>) {
    let mut action = chosen.unwrap_or(app.settings.post_capture_action);
    if action == PostCaptureAction::Ask {
        match crate::capture_pin::run(bmp, screen) {
            Some((picked, stamped, _)) => {
                action = picked;
                bmp = stamped;
            }
            None => return,
        }
    }
    match action {
        PostCaptureAction::SaveImage => save_image(&bmp, &app.settings),
        PostCaptureAction::CopyImage => copy_image(&bmp),
        PostCaptureAction::Pin => {
            crate::pin::open(bmp, Point::new(screen.x, screen.y), &app.settings);
        }
        PostCaptureAction::CopyText => copy_text(bmp, app.settings.clone()),
        PostCaptureAction::Translate => translate(bmp, app.settings.clone()),
        PostCaptureAction::Ask | PostCaptureAction::CopyPath => save_and_path(&bmp, &app.settings),
    }
}

fn save_image(bmp: &crate::bitmap::Bitmap, settings: &Settings) {
    match try_save_png(bmp, settings) {
        Ok(p) => crate::toast::show("已保存", &p.display().to_string()),
        Err(e) => crate::toast::show("保存失败", &e),
    }
}

fn copy_image(bmp: &crate::bitmap::Bitmap) {
    if crate::clipboard::set_image(bmp) {
        crate::toast::show("已复制图片", "可直接粘贴到聊天或文档");
    } else {
        crate::toast::show("复制图片失败", "剪贴板正被占用，请再截一次");
    }
}

fn save_and_path(bmp: &crate::bitmap::Bitmap, settings: &Settings) {
    match try_save_png(bmp, settings) {
        Ok(p) => {
            let s = p.display().to_string();
            if crate::clipboard::set_path(&s, settings.quote_path) {
                crate::toast::show(
                    "已复制路径",
                    &if settings.quote_path {
                        format!("\"{s}\"")
                    } else {
                        s
                    },
                );
            } else {
                crate::toast::show("已保存，但写入剪贴板失败", &s);
            }
        }
        Err(e) => crate::toast::show("保存失败，剪贴板未改动", &e),
    }
}

fn copy_text(bmp: crate::bitmap::Bitmap, settings: Settings) {
    if !settings.has_vision() {
        crate::toast::show("无法识别文字", "请先在设置中填写 DeepSeek API Key");
        return;
    }
    crate::toast::show("正在识别文字", &crate::vision::status_line(&settings));
    std::thread::spawn(move || match crate::vision::ocr(&bmp, &settings) {
        Ok(t) => {
            if crate::clipboard::set_text(&t) {
                crate::toast::show("已复制文字", &t);
            } else {
                crate::toast::show("复制文字失败", "剪贴板正被占用，请再试一次");
            }
        }
        Err(e) => crate::toast::show("识别失败", &e),
    });
}

fn translate(bmp: crate::bitmap::Bitmap, settings: Settings) {
    if !settings.has_vision() {
        crate::toast::show("无法翻译", "请先在设置中填写 DeepSeek API Key");
        return;
    }
    crate::toast::show(
        "正在翻译",
        &(crate::vision::status_line(&settings) + " · 目标语言由模型决定"),
    );
    std::thread::spawn(move || match crate::vision::translate(&bmp, &settings) {
        Ok(t) => {
            if crate::clipboard::set_text(&t) {
                crate::toast::show("已复制译文", &t);
            } else {
                crate::toast::show("复制译文失败", "剪贴板正被占用，请再试一次");
            }
        }
        Err(e) => crate::toast::show("翻译失败", &e),
    });
}

fn open_dir(app: &App) {
    let dir = app.settings.resolved_save_directory();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
}

fn open_settings(app: &mut App) {
    unregister_hotkey(app.hwnd);
    if let Some(s) = crate::settings_ui::run(app.settings.clone(), || {}) {
        app.settings = s;
        let _ = crate::install::apply_startup(app.settings.start_with_windows, None);
    }
    apply_hotkey(app);
}
