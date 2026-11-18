use crate::bitmap::{capture_virtual_screen, try_save_png, Bitmap};
use crate::geom::{Point, Rect};
use crate::macos::ffi;
use crate::macos::native::{
    apply_cursor_cross, enable_dpi, sleep_ms, HOTKEY_CAPTURE_ID, HOTKEY_TRANSLATE_ID,
};
use crate::quick_panel::{
    ACTION_CAPTURE, ACTION_CLOSE_PINS, ACTION_EXIT, ACTION_OPEN_DIR, ACTION_SETTINGS,
    ACTION_TRANSLATE,
};
use crate::settings::{PostCaptureAction, Settings};
use crate::windows_enum::snapshot;
use std::cell::RefCell;
use std::os::raw::c_void;
use std::sync::Arc;

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
    Image(Arc<Bitmap>),
    Text(String),
}

#[derive(Clone)]
struct AiRequest {
    id: u64,
    kind: AiTaskKind,
    source: AiSource,
}

struct AppState {
    settings: Settings,
    busy: bool,
    ai_busy: bool,
    next_ai_id: u64,
    current_ai: Option<AiRequest>,
}

thread_local! {
    static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
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

    let tip = tray_tooltip(&settings);
    let icon_bmp = crate::icon::draw_app_bitmap(64);

    APP.with(|a| {
        *a.borrow_mut() = Some(AppState {
            settings: settings.clone(),
            busy: false,
            ai_busy: false,
            next_ai_id: 0,
            current_ai: None,
        });
    });

    let tip_c = ffi::to_cstr(&tip);
    unsafe {
        ffi::ts_tray_init(
            icon_bmp.pixels.as_ptr(),
            icon_bmp.width,
            icon_bmp.height,
            tip_c.as_ptr(),
            on_tray_left_click,
            on_tray_menu_action,
            std::ptr::null_mut(),
        );
    }

    apply_hotkeys(&settings);
    if first {
        crate::toast::show(
            "终端截图已驻留菜单栏",
            &format!(
                "{} 框选截图；复制文字后按 {} 翻译",
                settings.format_hotkey(),
                settings.format_translate_hotkey()
            ),
        );
    }

    unsafe {
        ffi::ts_app_run();
    }
}

fn tray_tooltip(s: &Settings) -> String {
    format!(
        "TermShot · 截图 {} · 翻译 {}",
        s.format_hotkey(),
        s.format_translate_hotkey()
    )
}

fn apply_hotkeys(s: &Settings) {
    unsafe {
        ffi::ts_unregister_hotkey(HOTKEY_CAPTURE_ID);
        ffi::ts_unregister_hotkey(HOTKEY_TRANSLATE_ID);
        if !ffi::ts_register_hotkey(
            HOTKEY_CAPTURE_ID,
            s.hotkey_modifiers,
            s.hotkey_key,
            on_hotkey_pressed,
            std::ptr::null_mut(),
        ) {
            crate::toast::show_error(
                "截图快捷键注册失败",
                &(s.format_hotkey() + " 可能已被占用"),
            );
        }
        if !ffi::ts_register_hotkey(
            HOTKEY_TRANSLATE_ID,
            s.translate_hotkey_modifiers,
            s.translate_hotkey_key,
            on_hotkey_pressed,
            std::ptr::null_mut(),
        ) {
            crate::toast::show_error(
                "翻译快捷键注册失败",
                &(s.format_translate_hotkey() + " 可能已被占用或与截图快捷键冲突"),
            );
        }
    }
}

extern "C" fn on_hotkey_pressed(id: i32, _ud: *mut c_void) {
    if id == HOTKEY_CAPTURE_ID {
        start_capture(false);
    } else if id == HOTKEY_TRANSLATE_ID {
        translate_clipboard();
    }
}

extern "C" fn on_tray_left_click(_ud: *mut c_void) {
    let snap = APP.with(|a| {
        a.borrow().as_ref().map(|app| crate::quick_panel::Snapshot {
            capture_hotkey: app.settings.format_hotkey(),
            translate_hotkey: app.settings.format_translate_hotkey(),
            api_ready: app.settings.has_vision(),
            ai_busy: app.ai_busy,
            pin_count: crate::pin::count(),
        })
    });
    if let Some(snap) = snap {
        crate::macos::quick_panel::toggle(snap);
    }
}

extern "C" fn on_tray_menu_action(action: i32, _ud: *mut c_void) {
    if action == ACTION_CAPTURE as i32 {
        sleep_ms(150);
        start_capture(false);
    } else {
        handle_quick_action(action as usize);
    }
}

pub fn handle_quick_action(action: usize) {
    match action {
        ACTION_CAPTURE => {
            sleep_ms(120);
            start_capture(false);
        }
        ACTION_TRANSLATE => translate_clipboard(),
        ACTION_CLOSE_PINS => {
            crate::pin::close_all();
            crate::toast::show_success("已关闭全部贴图", "");
        }
        ACTION_OPEN_DIR => open_dir(),
        ACTION_SETTINGS => open_settings(),
        ACTION_EXIT => quit_app(),
        _ => {}
    }
}

fn quit_app() {
    unsafe {
        ffi::ts_unregister_hotkey(HOTKEY_CAPTURE_ID);
        ffi::ts_unregister_hotkey(HOTKEY_TRANSLATE_ID);
    }
    crate::pin::close_all();
    crate::quick_panel::close();
    crate::translation_window::close();
    crate::toast::close();
    unsafe {
        ffi::ts_app_quit();
    }
}

pub fn request_image_ai(kind: ImageAiKind, bitmap: Bitmap) -> bool {
    let is_main = unsafe { libc::pthread_main_np() != 0 };
    let task = match kind {
        ImageAiKind::Ocr => AiTaskKind::Ocr,
        ImageAiKind::Translate => AiTaskKind::ScreenshotTranslation,
    };
    if is_main {
        start_ai(task, AiSource::Image(Arc::new(bitmap)));
    } else {
        ffi::dispatch_main_fn(move || {
            start_ai(task, AiSource::Image(Arc::new(bitmap)));
        });
    }
    true
}

fn start_capture(scroll_after: bool) {
    let settings = APP.with(|a| {
        let mut b = a.borrow_mut();
        let app = b.as_mut()?;
        if app.busy {
            return None;
        }
        app.busy = true;
        Some(app.settings.clone())
    });
    let Some(settings) = settings else {
        return;
    };

    crate::quick_panel::close();
    unsafe {
        ffi::ts_pump_events(20);
    }
    apply_cursor_cross();
    let prev_pid = unsafe { ffi::ts_frontmost_pid() };
    let shot = match capture_virtual_screen() {
        Ok(s) => s,
        Err(e) => {
            crate::toast::show_error("截图失败", &e);
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    app.busy = false;
                }
            });
            return;
        }
    };

    let windows = snapshot();
    let ask = !scroll_after && settings.post_capture_action == PostCaptureAction::Ask;
    let result = crate::macos::overlay::run(shot.clone(), windows, ask, scroll_after);

    if prev_pid > 0 {
        unsafe {
            ffi::ts_activate_pid(prev_pid);
        }
    }

    if let Some(rect) = result.selected {
        if result.scroll {
            if let Some(bmp) = crate::scroll::run_scroll(rect) {
                finish(bmp, rect, None, &settings);
            }
        } else {
            let vs = crate::native::virtual_screen();
            let bmp_rect = Rect::new(rect.x - vs.x, rect.y - vs.y, rect.w, rect.h);
            let mut crop = shot.crop(bmp_rect);
            let mut session = result.session;
            session.stamp(&mut crop, Point::new(bmp_rect.x, bmp_rect.y), Some(&shot));
            finish(crop, rect, result.action, &settings);
        }
    }

    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            app.busy = false;
        }
    });
}

fn finish(
    mut bmp: Bitmap,
    screen: Rect,
    chosen: Option<PostCaptureAction>,
    settings: &Settings,
) {
    let mut action = chosen.unwrap_or(settings.post_capture_action);
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
        PostCaptureAction::SaveImage => save_image(&bmp, settings),
        PostCaptureAction::CopyImage => copy_image(&bmp),
        PostCaptureAction::Pin => {
            crate::pin::open(bmp, Point::new(screen.x, screen.y), settings);
        }
        PostCaptureAction::CopyText => start_ai(AiTaskKind::Ocr, AiSource::Image(Arc::new(bmp))),
        PostCaptureAction::Translate => {
            start_ai(AiTaskKind::ScreenshotTranslation, AiSource::Image(Arc::new(bmp)))
        }
        PostCaptureAction::Ask | PostCaptureAction::CopyPath => save_and_path(&bmp, settings),
    }
}

fn save_image(bmp: &Bitmap, settings: &Settings) {
    match try_save_png(bmp, settings) {
        Ok(p) => crate::toast::show_success("已保存", &p.display().to_string()),
        Err(e) => crate::toast::show_error("保存失败", &e),
    }
}

fn copy_image(bmp: &Bitmap) {
    if crate::clipboard::set_image(bmp) {
        crate::toast::show_success("已复制图片", "可直接粘贴到聊天或文档");
    } else {
        crate::toast::show_error("复制图片失败", "剪贴板正被占用，请再截一次");
    }
}

fn save_and_path(bmp: &Bitmap, settings: &Settings) {
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

fn translate_clipboard() {
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
    start_ai(AiTaskKind::ClipboardTranslation, AiSource::Text(source));
}

fn start_ai(kind: AiTaskKind, source: AiSource) {
    let prep = APP.with(|a| {
        let mut b = a.borrow_mut();
        let app = b.as_mut()?;
        if app.ai_busy {
            crate::translation_window::bring_to_front();
            crate::toast::show_info("AI 任务正在进行", "已为你显示当前任务，请等待完成");
            return None;
        }
        if !app.settings.has_vision() {
            crate::toast::show_error("AI 功能尚未配置", "请先在设置中填写 DeepSeek API Key");
            return None;
        }
        if let AiSource::Text(text) = &source {
            if let Err(error) = crate::vision::validate_translation_input(text) {
                crate::toast::show_error("无法处理文本", &error);
                return None;
            }
        }
        app.next_ai_id = app.next_ai_id.wrapping_add(1).max(1);
        let request = AiRequest {
            id: app.next_ai_id,
            kind,
            source,
        };
        app.ai_busy = true;
        app.current_ai = Some(request.clone());
        Some((request, app.settings.clone()))
    });

    let Some((request, settings)) = prep else {
        return;
    };

    let view_kind = match request.kind {
        AiTaskKind::Ocr => crate::translation_window::AiKind::Ocr,
        AiTaskKind::ScreenshotTranslation => {
            crate::translation_window::AiKind::ScreenshotTranslation
        }
        AiTaskKind::ClipboardTranslation => {
            crate::translation_window::AiKind::ClipboardTranslation
        }
    };
    let view_source = match &request.source {
        AiSource::Image(bitmap) => crate::translation_window::SourceView::Image(Arc::clone(bitmap)),
        AiSource::Text(text) => crate::translation_window::SourceView::Text(text.clone()),
    };
    crate::macos::translation_window::show_loading(view_kind, view_source, request.id);

    std::thread::spawn(move || {
        let result = match (&request.kind, &request.source) {
            (AiTaskKind::Ocr, AiSource::Image(bitmap)) => {
                crate::vision::ocr(bitmap.as_ref(), &settings)
            }
            (AiTaskKind::ScreenshotTranslation, AiSource::Image(bitmap)) => {
                crate::vision::translate(bitmap.as_ref(), &settings)
            }
            (AiTaskKind::ClipboardTranslation, AiSource::Text(text)) => {
                crate::vision::translate_text(text, &settings)
            }
            _ => Err("AI 任务数据无效，请重新操作".to_string()),
        };
        let req_id = request.id;
        ffi::dispatch_main_fn(move || {
            dispatch_ai_result(req_id, result);
        });
    });
}

fn dispatch_ai_result(id: u64, result: Result<String, String>) {
    let matches_current = APP.with(|a| {
        let mut b = a.borrow_mut();
        let Some(app) = b.as_mut() else {
            return false;
        };
        if app.current_ai.as_ref().map(|r| r.id) != Some(id) {
            return false;
        }
        app.ai_busy = false;
        true
    });
    if !matches_current {
        return;
    }
    match result {
        Ok(text) => {
            let _ = crate::translation_window::show_success(id, &text);
            APP.with(|a| {
                if let Some(app) = a.borrow_mut().as_mut() {
                    app.current_ai = None;
                }
            });
        }
        Err(error) => {
            if !crate::translation_window::show_error(id, &error) {
                APP.with(|a| {
                    if let Some(app) = a.borrow_mut().as_mut() {
                        app.current_ai = None;
                    }
                });
            }
        }
    }
}

pub fn retry_ai(request_id: u64) {
    let req = APP.with(|a| {
        let b = a.borrow();
        let app = b.as_ref()?;
        if app.ai_busy {
            return None;
        }
        app.current_ai
            .as_ref()
            .filter(|r| r.id == request_id)
            .cloned()
    });
    if let Some(request) = req {
        start_ai(request.kind, request.source);
    }
}

fn open_dir() {
    let dir = APP.with(|a| {
        a.borrow()
            .as_ref()
            .map(|app| app.settings.resolved_save_directory())
            .unwrap_or_else(|| Settings::load().resolved_save_directory())
    });
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::process::Command::new("open").arg(&dir).spawn();
}

fn open_settings() {
    let current_settings = APP.with(|a| {
        let mut b = a.borrow_mut();
        let app = b.as_mut()?;
        if app.busy {
            return None;
        }
        app.busy = true;
        Some(app.settings.clone())
    });
    let Some(current_settings) = current_settings else {
        return;
    };

    unsafe {
        ffi::ts_unregister_hotkey(HOTKEY_CAPTURE_ID);
        ffi::ts_unregister_hotkey(HOTKEY_TRANSLATE_ID);
    }

    let updated = crate::settings_ui::run(current_settings.clone(), || {});
    let final_settings = if let Some(s) = updated {
        let _ = crate::install::apply_startup(s.start_with_windows, None);
        let tip_c = ffi::to_cstr(&tray_tooltip(&s));
        unsafe {
            ffi::ts_tray_set_tooltip(tip_c.as_ptr());
        }
        crate::toast::show_success("设置已保存", "新的偏好和快捷键已生效");
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().as_mut() {
                app.settings = s.clone();
            }
        });
        s
    } else {
        current_settings
    };

    apply_hotkeys(&final_settings);
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().as_mut() {
            app.busy = false;
        }
    });
}
