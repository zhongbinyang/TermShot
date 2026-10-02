use crate::bitmap::{capture_virtual_screen, try_save_png};
use crate::geom::Point;
use crate::icon;
use crate::native::{
    enable_dpi, foreground, hinstance, is_window, register_hotkey, set_foreground, sleep_ms,
    unregister_hotkey, HOTKEY_CAPTURE_ID, HOTKEY_TRANSLATE_ID, WM_HOTKEY,
};
use crate::settings::{PostCaptureAction, Settings};
use crate::util::wide;
use crate::windows_enum::snapshot;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    InsertMenuW, PostMessageW, PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetWindowLongPtrW,
    TrackPopupMenu, TranslateMessage, CS_DBLCLKS, GWLP_USERDATA, MF_BYPOSITION, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MSG, TPM_LEFTALIGN, TPM_RETURNCMD, WM_APP, WM_DESTROY, WM_LBUTTONUP,
    WM_RBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_AI_DONE: u32 = WM_APP + 2;
const WM_AI_START: u32 = WM_APP + 5;

const ID_CAPTURE: usize = 1;
const ID_TRANSLATE_CLIPBOARD: usize = 2;
const ID_CLOSE_PINS: usize = 3;
const ID_OPEN_DIR: usize = 4;
const ID_SETTINGS: usize = 5;
const ID_EXIT: usize = 6;

struct App {
    hwnd: HWND,
    settings: Settings,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    nid: NOTIFYICONDATAW,
    busy: bool,
    ai_busy: bool,
    next_ai_id: u64,
    current_ai: Option<AiRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageAiKind {
    Ocr,
    Translate,
}

#[derive(Clone, Debug)]
enum AiTaskKind {
    Ocr,
    ScreenshotTranslation,
    ClipboardTranslation,
}

#[derive(Clone)]
enum AiSource {
    Image(Arc<crate::bitmap::Bitmap>),
    Text(String),
}

#[derive(Clone)]
struct AiRequest {
    id: u64,
    kind: AiTaskKind,
    source: AiSource,
}

struct AiCompletion {
    id: u64,
    result: Result<String, String>,
}

struct ImageAiStart {
    kind: ImageAiKind,
    bitmap: crate::bitmap::Bitmap,
}

static HOST: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn request_image_ai(kind: ImageAiKind, bitmap: crate::bitmap::Bitmap) -> bool {
    let host = HWND(HOST.load(Ordering::SeqCst));
    if host.0.is_null() {
        return false;
    }
    let payload = Box::new(ImageAiStart { kind, bitmap });
    let raw = Box::into_raw(payload);
    let posted = unsafe { PostMessageW(host, WM_AI_START, WPARAM(0), LPARAM(raw as isize)) };
    if posted.is_err() {
        unsafe { drop(Box::from_raw(raw)); }
        return false;
    }
    true
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
        ai_busy: false,
        next_ai_id: 0,
        current_ai: None,
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
        HOST.store(hwnd.0, Ordering::SeqCst);
        crate::toast::set_host(hwnd);

        app.nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        app.nid.hWnd = hwnd;
        app.nid.uID = 1;
        app.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        app.nid.uCallbackMessage = WM_TRAY;
        app.nid.hIcon = icon;
        let tip = wide(&format!(
            "TermShot · 截图 {} · 翻译 {}",
            app.settings.format_hotkey(),
            app.settings.format_translate_hotkey()
        ));
        let n = tip.len().min(127);
        app.nid.szTip[..n].copy_from_slice(&tip[..n]);
        let _ = Shell_NotifyIconW(NIM_ADD, &app.nid);

        apply_hotkeys(app.as_mut());
        if first {
            crate::toast::show(
                "终端截图已驻留托盘",
                &format!(
                    "{} 框选截图；复制文字后按 {} 翻译",
                    app.settings.format_hotkey(),
                    app.settings.format_translate_hotkey()
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

fn apply_hotkeys(app: &mut App) {
    unregister_hotkey(app.hwnd, HOTKEY_CAPTURE_ID);
    unregister_hotkey(app.hwnd, HOTKEY_TRANSLATE_ID);
    if !register_hotkey(
        app.hwnd,
        HOTKEY_CAPTURE_ID,
        app.settings.hotkey_modifiers,
        app.settings.hotkey_key,
    ) {
        crate::toast::show_error(
            "截图快捷键注册失败",
            &(app.settings.format_hotkey() + " 可能已被占用"),
        );
    }
    if !register_hotkey(
        app.hwnd,
        HOTKEY_TRANSLATE_ID,
        app.settings.translate_hotkey_modifiers,
        app.settings.translate_hotkey_key,
    ) {
        crate::toast::show_error(
            "翻译快捷键注册失败",
            &(app.settings.format_translate_hotkey() + " 可能已被占用或与截图快捷键冲突"),
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
        m if m == WM_HOTKEY && wparam.0 as i32 == HOTKEY_CAPTURE_ID => {
            start_capture(app, false);
            LRESULT(0)
        }
        m if m == WM_HOTKEY && wparam.0 as i32 == HOTKEY_TRANSLATE_ID => {
            translate_clipboard(app);
            LRESULT(0)
        }
        m if m == WM_TRAY => {
            match lparam.0 as u32 {
                WM_LBUTTONUP => toggle_quick_panel(app),
                WM_RBUTTONUP => tray_menu(app),
                _ => {}
            }
            LRESULT(0)
        }
        m if m == crate::quick_panel::WM_QUICK_ACTION => {
            handle_quick_action(app, wparam.0);
            LRESULT(0)
        }
        m if m == crate::toast::WM_SHOW_TOAST => {
            crate::toast::dispatch(lparam);
            LRESULT(0)
        }
        m if m == WM_AI_DONE => {
            dispatch_ai_result(app, lparam);
            LRESULT(0)
        }
        m if m == WM_AI_START => {
            dispatch_ai_start(app, lparam);
            LRESULT(0)
        }
        m if m == crate::translation_window::WM_AI_RETRY => {
            retry_ai(app, wparam.0 as u64);
            LRESULT(0)
        }
        WM_DESTROY => {
            HOST.store(std::ptr::null_mut(), Ordering::SeqCst);
            crate::toast::set_host(HWND::default());
            let _ = Shell_NotifyIconW(NIM_DELETE, &app.nid);
            unregister_hotkey(hwnd, HOTKEY_CAPTURE_ID);
            unregister_hotkey(hwnd, HOTKEY_TRANSLATE_ID);
            icon::destroy_icon(app.icon);
            crate::pin::close_all();
            crate::quick_panel::close();
            crate::translation_window::close();
            crate::toast::close();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn toggle_quick_panel(app: &App) {
    crate::quick_panel::toggle(
        app.hwnd,
        crate::quick_panel::Snapshot {
            capture_hotkey: app.settings.format_hotkey(),
            translate_hotkey: app.settings.format_translate_hotkey(),
            api_ready: app.settings.has_vision(),
            ai_busy: app.ai_busy,
            pin_count: crate::pin::count(),
        },
    );
}

fn handle_quick_action(app: &mut App, action: usize) {
    match action {
        crate::quick_panel::ACTION_CAPTURE => {
            sleep_ms(120);
            start_capture(app, false);
        }
        crate::quick_panel::ACTION_TRANSLATE => translate_clipboard(app),
        crate::quick_panel::ACTION_CLOSE_PINS => {
            crate::pin::close_all();
            crate::toast::show_success("已关闭全部贴图", "");
        }
        crate::quick_panel::ACTION_OPEN_DIR => open_dir(app),
        crate::quick_panel::ACTION_SETTINGS => open_settings(app),
        crate::quick_panel::ACTION_EXIT => unsafe {
            let _ = DestroyWindow(app.hwnd);
        },
        _ => {}
    }
}

fn tray_menu(app: &mut App) {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, ID_CAPTURE, windows::core::w!("截图"));
        let _ = InsertMenuW(
            menu,
            1,
            MF_BYPOSITION | MF_STRING,
            ID_TRANSLATE_CLIPBOARD,
            windows::core::w!("翻译剪贴板"),
        );
        let mut flags = MF_BYPOSITION | MF_STRING;
        if crate::pin::count() == 0 {
            flags |= MF_GRAYED;
        }
        let _ = InsertMenuW(menu, 2, flags, ID_CLOSE_PINS, windows::core::w!("关闭全部贴图"));
        let _ = InsertMenuW(menu, 3, MF_BYPOSITION | MF_SEPARATOR, 0, windows::core::PCWSTR::null());
        let _ = InsertMenuW(menu, 4, MF_BYPOSITION | MF_STRING, ID_OPEN_DIR, windows::core::w!("打开保存目录"));
        let _ = InsertMenuW(menu, 5, MF_BYPOSITION | MF_STRING, ID_SETTINGS, windows::core::w!("设置"));
        let _ = InsertMenuW(menu, 6, MF_BYPOSITION | MF_SEPARATOR, 0, windows::core::PCWSTR::null());
        let _ = InsertMenuW(menu, 7, MF_BYPOSITION | MF_STRING, ID_EXIT, windows::core::w!("退出"));
        let _ = SetForegroundWindow(app.hwnd);
        let cur = crate::native::cursor_pos();
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN, cur.x, cur.y, 0, app.hwnd, None).0 as usize;
        match cmd {
            ID_CAPTURE => {
                sleep_ms(150);
                start_capture(app, false);
            }
            ID_TRANSLATE_CLIPBOARD => translate_clipboard(app),
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
            crate::toast::show_error("截图失败", &e);
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
        PostCaptureAction::CopyText => start_ai(
            app,
            AiTaskKind::Ocr,
            AiSource::Image(Arc::new(bmp)),
        ),
        PostCaptureAction::Translate => start_ai(
            app,
            AiTaskKind::ScreenshotTranslation,
            AiSource::Image(Arc::new(bmp)),
        ),
        PostCaptureAction::Ask | PostCaptureAction::CopyPath => save_and_path(&bmp, &app.settings),
    }
}

fn save_image(bmp: &crate::bitmap::Bitmap, settings: &Settings) {
    match try_save_png(bmp, settings) {
        Ok(p) => crate::toast::show_success("已保存", &p.display().to_string()),
        Err(e) => crate::toast::show_error("保存失败", &e),
    }
}

fn copy_image(bmp: &crate::bitmap::Bitmap) {
    if crate::clipboard::set_image(bmp) {
        crate::toast::show_success("已复制图片", "可直接粘贴到聊天或文档");
    } else {
        crate::toast::show_error("复制图片失败", "剪贴板正被占用，请再截一次");
    }
}

fn save_and_path(bmp: &crate::bitmap::Bitmap, settings: &Settings) {
    match try_save_png(bmp, settings) {
        Ok(p) => {
            let s = p.display().to_string();
            if crate::clipboard::set_path(&s, settings.quote_path) {
                crate::toast::show_success(
                    "已复制路径",
                    &if settings.quote_path {
                        format!("\"{s}\"")
                    } else {
                        s
                    },
                );
            } else {
                crate::toast::show_error("已保存，但写入剪贴板失败", &s);
            }
        }
        Err(e) => crate::toast::show_error("保存失败，剪贴板未改动", &e),
    }
}

fn translate_clipboard(app: &mut App) {
    let source = match crate::clipboard::get_text() {
        Ok(text) => text,
        Err(error) => {
            crate::toast::show_error("无法翻译剪贴板", &error);
            return;
        }
    };
    if let Err(error) = crate::vision::validate_translation_input(&source) {
        crate::toast::show_error("无法翻译剪贴板", &error);
        return;
    }
    start_ai(app, AiTaskKind::ClipboardTranslation, AiSource::Text(source));
}

fn start_ai(app: &mut App, kind: AiTaskKind, source: AiSource) {
    if app.ai_busy {
        crate::translation_window::bring_to_front();
        crate::toast::show_info("AI 任务正在进行", "已为你显示当前任务，请等待完成");
        return;
    }
    if !app.settings.has_vision() {
        crate::toast::show_error("AI 功能尚未配置", "请先在设置中填写 DeepSeek API Key");
        return;
    }
    if let AiSource::Text(text) = &source {
        if let Err(error) = crate::vision::validate_translation_input(text) {
            crate::toast::show_error("无法处理文本", &error);
            return;
        }
    }

    app.next_ai_id = app.next_ai_id.wrapping_add(1).max(1);
    let request = AiRequest {
        id: app.next_ai_id,
        kind,
        source,
    };
    let view_kind = match request.kind {
        AiTaskKind::Ocr => crate::translation_window::AiKind::Ocr,
        AiTaskKind::ScreenshotTranslation => crate::translation_window::AiKind::ScreenshotTranslation,
        AiTaskKind::ClipboardTranslation => crate::translation_window::AiKind::ClipboardTranslation,
    };
    let view_source = match &request.source {
        AiSource::Image(bitmap) => crate::translation_window::SourceView::Image(Arc::clone(bitmap)),
        AiSource::Text(text) => crate::translation_window::SourceView::Text(text.clone()),
    };
    crate::translation_window::show_loading(view_kind, view_source, request.id, app.hwnd);
    app.ai_busy = true;
    app.current_ai = Some(request.clone());

    let settings = app.settings.clone();
    let host = app.hwnd.0 as isize;
    std::thread::spawn(move || {
        let result = match (&request.kind, &request.source) {
            (AiTaskKind::Ocr, AiSource::Image(bitmap)) => crate::vision::ocr(bitmap.as_ref(), &settings),
            (AiTaskKind::ScreenshotTranslation, AiSource::Image(bitmap)) => {
                crate::vision::translate(bitmap.as_ref(), &settings)
            }
            (AiTaskKind::ClipboardTranslation, AiSource::Text(text)) => {
                crate::vision::translate_text(text, &settings)
            }
            _ => Err("AI 任务数据无效，请重新操作".to_string()),
        };
        let payload = Box::new(AiCompletion {
            id: request.id,
            result,
        });
        let raw = Box::into_raw(payload);
        let posted = unsafe {
            PostMessageW(
                HWND(host as *mut _),
                WM_AI_DONE,
                WPARAM(0),
                LPARAM(raw as isize),
            )
        };
        if posted.is_err() {
            unsafe { drop(Box::from_raw(raw)); }
        }
    });
}

fn dispatch_ai_result(app: &mut App, lparam: LPARAM) {
    let raw = lparam.0 as *mut AiCompletion;
    if raw.is_null() {
        app.ai_busy = false;
        return;
    }
    let payload = unsafe { Box::from_raw(raw) };
    let AiCompletion { id, result } = *payload;
    if app.current_ai.as_ref().map(|request| request.id) != Some(id) {
        return;
    }
    app.ai_busy = false;
    match result {
        Ok(text) => {
            let _ = crate::translation_window::show_success(id, &text);
            app.current_ai = None;
        }
        Err(error) => {
            if !crate::translation_window::show_error(id, &error) {
                app.current_ai = None;
            }
        }
    }
}

fn dispatch_ai_start(app: &mut App, lparam: LPARAM) {
    let raw = lparam.0 as *mut ImageAiStart;
    if raw.is_null() {
        return;
    }
    let payload = unsafe { Box::from_raw(raw) };
    let kind = match payload.kind {
        ImageAiKind::Ocr => AiTaskKind::Ocr,
        ImageAiKind::Translate => AiTaskKind::ScreenshotTranslation,
    };
    start_ai(app, kind, AiSource::Image(Arc::new(payload.bitmap)));
}

fn retry_ai(app: &mut App, request_id: u64) {
    if app.ai_busy {
        return;
    }
    let Some(request) = app
        .current_ai
        .as_ref()
        .filter(|request| request.id == request_id)
        .cloned()
    else {
        return;
    };
    start_ai(app, request.kind, request.source);
}

fn open_dir(app: &App) {
    let dir = app.settings.resolved_save_directory();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
}

fn open_settings(app: &mut App) {
    unregister_hotkey(app.hwnd, HOTKEY_CAPTURE_ID);
    unregister_hotkey(app.hwnd, HOTKEY_TRANSLATE_ID);
    if let Some(s) = crate::settings_ui::run(app.settings.clone(), || {}) {
        app.settings = s;
        let _ = crate::install::apply_startup(app.settings.start_with_windows, None);
        update_tray_tip(app);
        crate::toast::show_success("设置已保存", "新的偏好和快捷键已生效");
    }
    apply_hotkeys(app);
}

fn update_tray_tip(app: &mut App) {
    let tip = wide(&format!(
        "TermShot · 截图 {} · 翻译 {}",
        app.settings.format_hotkey(),
        app.settings.format_translate_hotkey()
    ));
    app.nid.szTip = [0; 128];
    let count = tip.len().min(127);
    app.nid.szTip[..count].copy_from_slice(&tip[..count]);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &app.nid);
    }
}
