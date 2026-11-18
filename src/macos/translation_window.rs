use crate::bitmap::Bitmap;
use crate::macos::ffi;
use crate::macos::native::{cursor_pos, dpi_scale_at, sc, work_area_from_point};
use crate::translation_window::{AiKind, SourceView};
use std::cell::Cell;
use std::os::raw::c_void;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewState {
    Loading = 0,
    Success = 1,
    Error = 2,
}

struct TransState {
    win: *mut c_void,
    request_id: u64,
    kind: AiKind,
    view_state: ViewState,
    source_text: String,
    source_image: Option<Arc<Bitmap>>,
    translated_text: String,
    error_text: String,
    copied: bool,
    copy_seq: u64,
}

thread_local! {
    static CURRENT: Cell<*mut TransState> = const { Cell::new(std::ptr::null_mut()) };
}

pub fn show_loading(kind: AiKind, source: SourceView, request_id: u64) {
    let (source_text, source_image) = match source {
        SourceView::Text(text) => (text, None),
        SourceView::Image(image) => (
            format!("截图 · {} × {} px", image.width, image.height),
            Some(image),
        ),
    };

    let existing = CURRENT.with(|c| c.get());
    if !existing.is_null() {
        unsafe {
            let st = &mut *existing;
            st.request_id = request_id;
            st.kind = kind;
            st.view_state = ViewState::Loading;
            st.source_text = source_text;
            st.source_image = source_image;
            st.translated_text.clear();
            st.error_text.clear();
            st.copied = false;
            sync_window(st);
            ffi::ts_trans_show(st.win);
            return;
        }
    }

    let cur = cursor_pos();
    let work = work_area_from_point(cur);
    let scale = dpi_scale_at(cur).max(1.0);
    let w = sc(680, scale);
    let h = sc(540, scale);
    let x = work.x + ((work.w - w) / 2).max(0);
    let y = work.y + ((work.h - h) / 3).max(0);

    let state = Box::new(TransState {
        win: std::ptr::null_mut(),
        request_id,
        kind,
        view_state: ViewState::Loading,
        source_text,
        source_image,
        translated_text: String::new(),
        error_text: String::new(),
        copied: false,
        copy_seq: 0,
    });
    let raw = Box::into_raw(state);

    unsafe {
        let win = ffi::ts_trans_create(
            x,
            y,
            w,
            h,
            on_copy,
            on_retry,
            on_close,
            raw as *mut c_void,
        );
        (*raw).win = win;
        CURRENT.with(|c| c.set(raw));
        sync_window(&*raw);
        ffi::ts_trans_show(win);
    }
}

pub fn show_success(request_id: u64, text: &str) -> bool {
    let cur = CURRENT.with(|c| c.get());
    if cur.is_null() {
        return false;
    }
    unsafe {
        let st = &mut *cur;
        if st.request_id != request_id {
            return false;
        }
        st.view_state = ViewState::Success;
        st.translated_text = text.to_string();
        st.error_text.clear();
        st.copied = false;
        sync_window(st);
        true
    }
}

pub fn show_error(request_id: u64, error: &str) -> bool {
    let cur = CURRENT.with(|c| c.get());
    if cur.is_null() {
        return false;
    }
    unsafe {
        let st = &mut *cur;
        if st.request_id != request_id {
            return false;
        }
        st.view_state = ViewState::Error;
        st.translated_text.clear();
        st.error_text = error.chars().take(500).collect();
        sync_window(st);
        true
    }
}

pub fn bring_to_front() {
    let cur = CURRENT.with(|c| c.get());
    if !cur.is_null() {
        unsafe {
            ffi::ts_trans_show((*cur).win);
        }
    }
}

pub fn close() {
    let cur = CURRENT.with(|c| c.replace(std::ptr::null_mut()));
    if cur.is_null() {
        return;
    }
    unsafe {
        let win = (*cur).win;
        (*cur).win = std::ptr::null_mut();
        if !win.is_null() {
            ffi::ts_trans_close(win);
        }
        drop(Box::from_raw(cur));
    }
}

unsafe fn sync_window(st: &TransState) {
    if st.win.is_null() {
        return;
    }
    let kind_id = match st.kind {
        AiKind::Ocr => 0,
        AiKind::ScreenshotTranslation => 1,
        AiKind::ClipboardTranslation => 2,
    };
    let src_c = ffi::to_cstr(&st.source_text);
    let res_c = ffi::to_cstr(&st.translated_text);
    let err_c = ffi::to_cstr(&st.error_text);
    let (img_ptr, img_w, img_h) = match &st.source_image {
        Some(bmp) => (bmp.pixels.as_ptr(), bmp.width, bmp.height),
        None => (std::ptr::null(), 0, 0),
    };
    ffi::ts_trans_update(
        st.win,
        kind_id,
        st.view_state as i32,
        src_c.as_ptr(),
        img_ptr,
        img_w,
        img_h,
        res_c.as_ptr(),
        err_c.as_ptr(),
        st.copied,
    );
}

extern "C" fn on_copy(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut TransState);
        if st.view_state != ViewState::Success {
            return;
        }
        if crate::clipboard::set_text(&st.translated_text) {
            st.copied = true;
            st.copy_seq = st.copy_seq.wrapping_add(1);
            let seq = st.copy_seq;
            let req_id = st.request_id;
            sync_window(st);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                ffi::dispatch_main_fn(move || {
                    let cur = CURRENT.with(|c| c.get());
                    if !cur.is_null() {
                        let st = &mut *cur;
                        if st.request_id == req_id && st.copy_seq == seq {
                            st.copied = false;
                            sync_window(st);
                        }
                    }
                });
            });
        } else {
            crate::toast::show_error("复制失败", "剪贴板正被占用，请稍后重试");
        }
    }
}

extern "C" fn on_retry(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let req_id = unsafe { (*(ud as *const TransState)).request_id };
    crate::macos::app::retry_ai(req_id);
}

extern "C" fn on_close(ud: *mut c_void) {
    let cur = CURRENT.with(|c| c.get());
    if !cur.is_null() && cur == ud as *mut TransState {
        CURRENT.with(|c| c.set(std::ptr::null_mut()));
        unsafe {
            (*cur).win = std::ptr::null_mut();
            drop(Box::from_raw(cur));
        }
    }
}
