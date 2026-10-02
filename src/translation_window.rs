use crate::bitmap::Bitmap;
use crate::geom::{Color, Rect};
use crate::native::{
    cursor_pos, dpi_scale_at, enable_dark_title, hinstance, sc, work_area_from_point,
};
use crate::theme;
use crate::util::wide;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
    RoundRect, ScreenToClient, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DRAW_TEXT_FORMAT, DT_CENTER, DT_LEFT,
    DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HBRUSH, HDC, HFONT, HPEN,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetWindowLongPtrW, IsWindow, LoadCursorW, MoveWindow, RegisterClassExW, SendMessageW,
    SetCursor, SetTimer, KillTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, BN_CLICKED,
    BS_OWNERDRAW, CREATESTRUCTW, ES_AUTOVSCROLL, ES_MULTILINE, ES_NOHIDESEL, ES_READONLY,
    GWLP_USERDATA, HMENU, HWND_TOPMOST, IDC_ARROW, IDC_HAND, MINMAXINFO, SW_HIDE, SW_SHOW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WM_COMMAND,
    WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_SETCURSOR, WM_SETFONT, WM_SIZE, WM_TIMER, WNDCLASSEXW,
    WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED,
    WS_SIZEBOX, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

const ID_COPY: usize = 1;
const ID_CLOSE: usize = 2;
const ID_RETRY: usize = 3;
const EM_SETSEL: u32 = 0x00B1;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_ANIMATION: usize = 1;
const TIMER_COPY_FEEDBACK: usize = 2;
pub const WM_AI_RETRY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiKind {
    Ocr,
    ScreenshotTranslation,
    ClipboardTranslation,
}

#[derive(Clone)]
pub enum SourceView {
    Text(String),
    Image(Arc<Bitmap>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewState {
    Loading,
    Success,
    Error,
}

#[derive(Clone, Copy, Debug)]
struct TranslationLayout {
    header: Rect,
    source_card: Rect,
    source_edit: Rect,
    translated_card: Rect,
    translated_edit: Rect,
    action_bar: Rect,
    copy_btn: Rect,
    retry_btn: Rect,
    close_btn: Rect,
}

struct TranslationWindow {
    source_text: String,
    translated_text: String,
    source_edit: HWND,
    translated_edit: HWND,
    copy_btn: HWND,
    retry_btn: HWND,
    close_btn: HWND,
    font: HFONT,
    font_title: HFONT,
    font_bold: HFONT,
    font_small: HFONT,
    bg_brush: HBRUSH,
    panel_brush: HBRUSH,
    bottom_brush: HBRUSH,
    accent_brush: HBRUSH,
    accent_dark_brush: HBRUSH,
    border_brush: HBRUSH,
    border_pen: HPEN,
    accent_pen: HPEN,
    scale: f32,
    request_id: u64,
    host: isize,
    kind: AiKind,
    view_state: ViewState,
    source_image: Option<Arc<Bitmap>>,
    error_text: String,
    pulse: u8,
    copied: bool,
}

static CURRENT: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn current() -> HWND {
    HWND(CURRENT.load(Ordering::SeqCst))
}

fn set_current(hwnd: HWND) {
    CURRENT.store(hwnd.0, Ordering::SeqCst);
}

pub fn show_loading(kind: AiKind, source: SourceView, request_id: u64, host: HWND) {
    unsafe {
        let (source_text, source_image) = match source {
            SourceView::Text(text) => (text, None),
            SourceView::Image(image) => (
                format!("截图 · {} × {} px", image.width, image.height),
                Some(image),
            ),
        };
        let existing = current();
        if !existing.0.is_null() && IsWindow(existing).as_bool() {
            let ptr = GetWindowLongPtrW(existing, GWLP_USERDATA) as *mut TranslationWindow;
            if !ptr.is_null() {
                let state = &mut *ptr;
                state.source_text = source_text;
                state.source_image = source_image;
                state.translated_text.clear();
                state.error_text.clear();
                state.request_id = request_id;
                state.host = host.0 as isize;
                state.kind = kind;
                state.view_state = ViewState::Loading;
                state.pulse = 0;
                state.copied = false;
                let source_value = wide(&state.source_text);
                let translated_value = wide("");
                let _ = SetWindowTextW(
                    state.source_edit,
                    windows::core::PCWSTR(source_value.as_ptr()),
                );
                let _ = SetWindowTextW(
                    state.translated_edit,
                    windows::core::PCWSTR(translated_value.as_ptr()),
                );
                let _ = SendMessageW(state.source_edit, EM_SETSEL, WPARAM(0), LPARAM(0));
                let _ = SendMessageW(state.translated_edit, EM_SETSEL, WPARAM(0), LPARAM(0));
                update_title(existing, state.kind);
                update_controls(state);
                let _ = SetTimer(existing, TIMER_ANIMATION, 90, None);
                let _ = InvalidateRect(existing, None, false);
                let _ = ShowWindow(existing, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    existing,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                return;
            }
        }

        let point = cursor_pos();
        let work = work_area_from_point(point);
        let scale = dpi_scale_at(point).max(1.0);
        let font = make_font(sc(14, scale), false);
        let font_title = make_font(sc(18, scale), true);
        let font_bold = make_font(sc(13, scale), true);
        let font_small = make_font(sc(11, scale), false);
        let state = Box::new(TranslationWindow {
            source_text,
            translated_text: String::new(),
            source_edit: HWND::default(),
            translated_edit: HWND::default(),
            copy_btn: HWND::default(),
            retry_btn: HWND::default(),
            close_btn: HWND::default(),
            font,
            font_title,
            font_bold,
            font_small,
            bg_brush: CreateSolidBrush(COLORREF(theme::WIN_BG.colorref())),
            panel_brush: CreateSolidBrush(COLORREF(theme::PANEL.colorref())),
            bottom_brush: CreateSolidBrush(COLORREF(theme::BOTTOM_BAR_BG.colorref())),
            accent_brush: CreateSolidBrush(COLORREF(theme::ACCENT.colorref())),
            accent_dark_brush: CreateSolidBrush(COLORREF(theme::ACCENT_DARK.colorref())),
            border_brush: CreateSolidBrush(COLORREF(theme::BORDER.colorref())),
            border_pen: CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref())),
            accent_pen: CreatePen(PS_SOLID, 1, COLORREF(theme::ACCENT.colorref())),
            scale,
            request_id,
            host: host.0 as isize,
            kind,
            view_state: ViewState::Loading,
            source_image,
            error_text: String::new(),
            pulse: 0,
            copied: false,
        });

        let class = wide("TermShotTranslationWindow");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let style = WS_OVERLAPPED
            | WS_CAPTION
            | WS_SYSMENU
            | WS_SIZEBOX
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX
            | WS_CLIPCHILDREN;
        let mut window_rect = RECT {
            left: 0,
            top: 0,
            right: sc(680, scale),
            bottom: sc(540, scale),
        };
        let _ = AdjustWindowRectEx(&mut window_rect, style, false, WINDOW_EX_STYLE(0));
        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;
        let x = work.x + ((work.w - width) / 2).max(0);
        let y = work.y + ((work.h - height) / 3).max(0);
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST,
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("TermShot — AI 结果"),
            style,
            x,
            y,
            width,
            height,
            None,
            None,
            hinstance(),
            Some(Box::into_raw(state) as *mut _),
        )
        .unwrap_or_default();
        if hwnd.0.is_null() {
            return;
        }

        set_current(hwnd);
        enable_dark_title(hwnd);
        build_controls(hwnd);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TranslationWindow;
        if !ptr.is_null() {
            update_title(hwnd, (*ptr).kind);
            update_controls(&*ptr);
        }
        layout(hwnd);
        let _ = SetTimer(hwnd, TIMER_ANIMATION, 90, None);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

pub fn show_success(request_id: u64, text: &str) -> bool {
    unsafe {
        let hwnd = current();
        if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() {
            return false;
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TranslationWindow;
        if ptr.is_null() || (*ptr).request_id != request_id {
            return false;
        }
        let state = &mut *ptr;
        state.view_state = ViewState::Success;
        state.translated_text = text.to_string();
        state.error_text.clear();
        state.copied = false;
        let value = wide(text);
        let _ = SetWindowTextW(state.translated_edit, windows::core::PCWSTR(value.as_ptr()));
        let _ = SendMessageW(state.translated_edit, EM_SETSEL, WPARAM(0), LPARAM(0));
        let _ = KillTimer(hwnd, TIMER_ANIMATION);
        update_controls(state);
        let _ = InvalidateRect(hwnd, None, false);
        true
    }
}

pub fn show_error(request_id: u64, error: &str) -> bool {
    unsafe {
        let hwnd = current();
        if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() {
            return false;
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TranslationWindow;
        if ptr.is_null() || (*ptr).request_id != request_id {
            return false;
        }
        let state = &mut *ptr;
        state.view_state = ViewState::Error;
        state.translated_text.clear();
        state.error_text = error.chars().take(500).collect();
        let _ = KillTimer(hwnd, TIMER_ANIMATION);
        update_controls(state);
        let _ = InvalidateRect(hwnd, None, false);
        true
    }
}

pub fn bring_to_front() {
    unsafe {
        let hwnd = current();
        if !hwnd.0.is_null() && IsWindow(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

unsafe fn update_title(hwnd: HWND, kind: AiKind) {
    let title = match kind {
        AiKind::Ocr => "TermShot — 文字识别",
        AiKind::ScreenshotTranslation => "TermShot — 截图翻译",
        AiKind::ClipboardTranslation => "TermShot — 剪贴板翻译",
    };
    let value = wide(title);
    let _ = SetWindowTextW(hwnd, windows::core::PCWSTR(value.as_ptr()));
}

unsafe fn update_controls(state: &TranslationWindow) {
    let is_text_source = state.source_image.is_none();
    let _ = ShowWindow(state.source_edit, if is_text_source { SW_SHOW } else { SW_HIDE });
    let success = state.view_state == ViewState::Success;
    let failed = state.view_state == ViewState::Error;
    let _ = ShowWindow(state.translated_edit, if success { SW_SHOW } else { SW_HIDE });
    let _ = ShowWindow(state.copy_btn, if success { SW_SHOW } else { SW_HIDE });
    let _ = EnableWindow(state.copy_btn, success);
    let _ = ShowWindow(state.retry_btn, if failed { SW_SHOW } else { SW_HIDE });
    let copy_label = wide(if state.copied { "✓ 已复制" } else { "复制结果" });
    let _ = SetWindowTextW(state.copy_btn, windows::core::PCWSTR(copy_label.as_ptr()));
}

pub fn close() {
    unsafe {
        let hwnd = current();
        if !hwnd.0.is_null() && IsWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
    }
}

fn make_font(px: i32, bold: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            if bold {
                FW_BOLD.0 as i32
            } else {
                FW_NORMAL.0 as i32
            },
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            windows::core::w!("Microsoft YaHei UI"),
        )
    }
}

unsafe fn build_controls(hwnd: HWND) {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TranslationWindow;
    if ptr.is_null() {
        return;
    }
    let state = &mut *ptr;
    state.source_edit = create_text_box(hwnd, &state.source_text, state.font);
    state.translated_edit = create_text_box(hwnd, &state.translated_text, state.font);
    state.copy_btn = create_button(hwnd, "复制结果", ID_COPY, state.font_bold);
    state.retry_btn = create_button(hwnd, "重新尝试", ID_RETRY, state.font_bold);
    state.close_btn = create_button(hwnd, "关闭", ID_CLOSE, state.font);

    let _ = SendMessageW(state.source_edit, EM_SETSEL, WPARAM(0), LPARAM(0));
    let _ = SendMessageW(state.translated_edit, EM_SETSEL, WPARAM(0), LPARAM(0));
}

unsafe fn create_text_box(parent: HWND, text: &str, font: HFONT) -> HWND {
    let value = wide(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        windows::core::w!("EDIT"),
        windows::core::PCWSTR(value.as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | WS_VSCROLL
            | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_READONLY as u32
                    | ES_NOHIDESEL as u32,
            ),
        0,
        0,
        0,
        0,
        parent,
        None,
        hinstance(),
        None,
    )
    .unwrap_or_default();
    let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    let _ = SetWindowSubclass(hwnd, Some(text_subclass_proc), 0, parent.0 as usize);
    hwnd
}

unsafe fn create_button(parent: HWND, text: &str, id: usize, font: HFONT) -> HWND {
    let value = wide(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        windows::core::w!("BUTTON"),
        windows::core::PCWSTR(value.as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_OWNERDRAW as u32),
        0,
        0,
        0,
        0,
        parent,
        HMENU(id as *mut _),
        hinstance(),
        None,
    )
    .unwrap_or_default();
    let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    let _ = SetWindowSubclass(hwnd, Some(button_subclass_proc), id, parent.0 as usize);
    hwnd
}

fn compute_layout(width: i32, height: i32, scale: f32) -> TranslationLayout {
    let p = |v: i32| sc(v, scale);
    let margin = p(16);
    let header_h = p(68);
    let action_h = p(62);
    let card_gap = p(12);
    let cards_top = header_h + p(4);
    let action_y = (height - action_h).max(cards_top + p(2));
    let cards_bottom = (action_y - p(12)).max(cards_top + p(2));
    let available_h = (cards_bottom - cards_top - card_gap).max(2);
    let source_h = (available_h * 2 / 5).max(1);
    let translated_h = (available_h - source_h).max(1);
    let card_w = (width - margin * 2).max(1);
    let source_card = Rect::new(margin, cards_top, card_w, source_h);
    let translated_card = Rect::new(
        margin,
        source_card.bottom() + card_gap,
        card_w,
        translated_h,
    );
    let edit_x = margin + p(14);
    let edit_w = (card_w - p(28)).max(1);
    let source_edit = Rect::new(
        edit_x,
        source_card.y + p(40),
        edit_w,
        (source_card.h - p(54)).max(1),
    );
    let translated_edit = Rect::new(
        edit_x,
        translated_card.y + p(40),
        edit_w,
        (translated_card.h - p(54)).max(1),
    );
    let close_w = p(88);
    let copy_w = p(112);
    let button_h = p(34);
    let button_y = action_y + (action_h - button_h) / 2;
    let close_btn = Rect::new(width - margin - close_w, button_y, close_w, button_h);
    let copy_btn = Rect::new(close_btn.x - p(10) - copy_w, button_y, copy_w, button_h);
    TranslationLayout {
        header: Rect::new(0, 0, width.max(1), header_h),
        source_card,
        source_edit,
        translated_card,
        translated_edit,
        action_bar: Rect::new(0, action_y, width.max(1), action_h),
        copy_btn,
        retry_btn: copy_btn,
        close_btn,
    }
}

unsafe fn layout(hwnd: HWND) {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TranslationWindow;
    if ptr.is_null() {
        return;
    }
    let state = &*ptr;
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let layout = compute_layout(rc.right - rc.left, rc.bottom - rc.top, state.scale);
    move_control(state.source_edit, layout.source_edit);
    move_control(state.translated_edit, layout.translated_edit);
    move_control(state.copy_btn, layout.copy_btn);
    move_control(state.retry_btn, layout.retry_btn);
    move_control(state.close_btn, layout.close_btn);
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe fn move_control(hwnd: HWND, rect: Rect) {
    let _ = MoveWindow(hwnd, rect.x, rect.y, rect.w, rect.h, true);
}

unsafe fn paint_window(state: &TranslationWindow, hdc: HDC, width: i32, height: i32) {
    let layout = compute_layout(width, height, state.scale);
    let full = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let _ = FillRect(hdc, &full, state.bg_brush);

    let (title, subtitle, source_label, result_label) = match state.kind {
        AiKind::Ocr => (
            "文字识别",
            "从截图中提取可复制文本",
            "原始截图",
            "识别结果",
        ),
        AiKind::ScreenshotTranslation => (
            "截图翻译",
            "识别截图内容并翻译",
            "原始截图",
            "翻译结果",
        ),
        AiKind::ClipboardTranslation => (
            "剪贴板翻译",
            "自动识别语言 · 中英互译",
            "原文",
            "翻译结果",
        ),
    };
    draw_text(
        hdc,
        title,
        RECT {
            left: sc(18, state.scale),
            top: sc(12, state.scale),
            right: width - sc(120, state.scale),
            bottom: sc(38, state.scale),
        },
        state.font_title,
        theme::TEXT,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        subtitle,
        RECT {
            left: sc(18, state.scale),
            top: sc(39, state.scale),
            right: width - sc(120, state.scale),
            bottom: sc(61, state.scale),
        },
        state.font_small,
        theme::DIM,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_status_badge(state, hdc, width);
    fill_color_rect(
        hdc,
        Rect::new(0, layout.header.bottom() - 1, width, 1),
        state.border_brush,
    );

    draw_card(state, hdc, layout.source_card, false);
    draw_card(state, hdc, layout.translated_card, true);
    draw_card_header(
        state,
        hdc,
        layout.source_card,
        source_label,
        state.source_text.chars().count(),
        false,
    );
    draw_card_header(
        state,
        hdc,
        layout.translated_card,
        result_label,
        state.translated_text.chars().count(),
        true,
    );

    if let Some(image) = &state.source_image {
        paint_image_preview(hdc, image, layout.source_edit);
    }
    match state.view_state {
        ViewState::Loading => paint_loading(state, hdc, layout.translated_edit),
        ViewState::Error => paint_error(state, hdc, layout.translated_edit),
        ViewState::Success => {}
    }

    fill_color_rect(hdc, layout.action_bar, state.bottom_brush);
    fill_color_rect(
        hdc,
        Rect::new(0, layout.action_bar.y, width, 1),
        state.border_brush,
    );
    draw_text(
        hdc,
        match state.view_state {
            ViewState::Loading => "处理中可关闭窗口，完成后不会自动覆盖剪贴板",
            ViewState::Error => "检查网络或 API 配置后可重新尝试  ·  Ctrl+R",
            ViewState::Success => "仅点击“复制结果”时写入剪贴板  ·  Ctrl+Enter",
        },
        RECT {
            left: sc(18, state.scale),
            top: layout.action_bar.y,
            right: layout.copy_btn.x - sc(12, state.scale),
            bottom: layout.action_bar.bottom(),
        },
        state.font_small,
        theme::DIM,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

unsafe fn draw_status_badge(state: &TranslationWindow, hdc: HDC, width: i32) {
    let badge = Rect::new(
        width - sc(100, state.scale),
        sc(20, state.scale),
        sc(82, state.scale),
        sc(26, state.scale),
    );
    let (label, fg, bg, stroke) = match state.view_state {
        ViewState::Loading => ("●  处理中", theme::WARNING, theme::INPUT_BG, theme::WARNING),
        ViewState::Success => ("●  已完成", theme::ACCENT_HI, theme::ACCENT_DARK, theme::ACCENT),
        ViewState::Error => ("●  失败", theme::DANGER, theme::ERROR_BG, theme::DANGER),
    };
    let brush = CreateSolidBrush(COLORREF(bg.colorref()));
    let pen = CreatePen(PS_SOLID, 1, COLORREF(stroke.colorref()));
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);
    let round = sc(13, state.scale);
    let _ = RoundRect(
        hdc,
        badge.x,
        badge.y,
        badge.right(),
        badge.bottom(),
        round,
        round,
    );
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
    draw_text(
        hdc,
        label,
        rect_to_win(badge),
        state.font_small,
        fg,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

unsafe fn paint_image_preview(hdc: HDC, image: &Bitmap, area: Rect) {
    if image.width <= 0 || image.height <= 0 || area.w <= 2 || area.h <= 2 {
        return;
    }
    let scale = (area.w as f32 / image.width as f32)
        .min(area.h as f32 / image.height as f32)
        .min(1.0);
    let width = (image.width as f32 * scale).round() as i32;
    let height = (image.height as f32 * scale).round() as i32;
    let dest = Rect::new(
        area.x + (area.w - width) / 2,
        area.y + (area.h - height) / 2,
        width.max(1),
        height.max(1),
    );
    image.blit_to_hdc(hdc, dest);
}

unsafe fn paint_loading(state: &TranslationWindow, hdc: HDC, area: Rect) {
    let bright = state.pulse < 10;
    let color = if bright { theme::HOVER_BG } else { theme::BORDER };
    let brush = CreateSolidBrush(COLORREF(color.colorref()));
    let widths = [72, 92, 84, 64];
    for (index, percent) in widths.iter().enumerate() {
        let rect = Rect::new(
            area.x,
            area.y + sc(8 + index as i32 * 26, state.scale),
            area.w * percent / 100,
            sc(11, state.scale),
        );
        let old = SelectObject(hdc, brush);
        let round = sc(5, state.scale);
        let _ = RoundRect(hdc, rect.x, rect.y, rect.right(), rect.bottom(), round, round);
        SelectObject(hdc, old);
    }
    let _ = DeleteObject(brush);
    draw_text(
        hdc,
        "AI 正在处理，请稍候…",
        RECT {
            left: area.x,
            top: area.bottom() - sc(36, state.scale),
            right: area.right(),
            bottom: area.bottom(),
        },
        state.font_small,
        theme::DIM,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

unsafe fn paint_error(state: &TranslationWindow, hdc: HDC, area: Rect) {
    draw_text(
        hdc,
        "处理失败",
        RECT {
            left: area.x,
            top: area.y + sc(8, state.scale),
            right: area.right(),
            bottom: area.y + sc(38, state.scale),
        },
        state.font_bold,
        theme::DANGER,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        &state.error_text,
        RECT {
            left: area.x,
            top: area.y + sc(44, state.scale),
            right: area.right(),
            bottom: area.bottom(),
        },
        state.font,
        theme::TEXT,
        DT_LEFT | windows::Win32::Graphics::Gdi::DT_WORDBREAK | DT_NOPREFIX,
    );
}

unsafe fn draw_card(state: &TranslationWindow, hdc: HDC, rect: Rect, accented: bool) {
    let old_brush = SelectObject(hdc, state.panel_brush);
    let old_pen = SelectObject(hdc, state.border_pen);
    let round = sc(10, state.scale);
    let _ = RoundRect(
        hdc,
        rect.x,
        rect.y,
        rect.right(),
        rect.bottom(),
        round,
        round,
    );
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    if accented {
        let bar_w = sc(3, state.scale).max(2);
        let bar = Rect::new(
            rect.x + sc(6, state.scale),
            rect.y + sc(12, state.scale),
            bar_w,
            (rect.h - sc(24, state.scale)).max(sc(12, state.scale)),
        );
        let old = SelectObject(hdc, state.accent_brush);
        let old_pen = SelectObject(hdc, state.accent_pen);
        let _ = RoundRect(hdc, bar.x, bar.y, bar.right(), bar.bottom(), bar_w, bar_w);
        SelectObject(hdc, old);
        SelectObject(hdc, old_pen);
    }
}

unsafe fn draw_card_header(
    state: &TranslationWindow,
    hdc: HDC,
    rect: Rect,
    title: &str,
    count: usize,
    accented: bool,
) {
    let left = rect.x
        + if accented {
            sc(18, state.scale)
        } else {
            sc(14, state.scale)
        };
    let top = rect.y + sc(8, state.scale);
    draw_text(
        hdc,
        title,
        RECT {
            left,
            top,
            right: rect.right() - sc(100, state.scale),
            bottom: top + sc(26, state.scale),
        },
        state.font_bold,
        if accented {
            theme::ACCENT_HI
        } else {
            theme::TEXT
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        &format!("{count} 字符"),
        RECT {
            left: rect.right() - sc(110, state.scale),
            top,
            right: rect.right() - sc(14, state.scale),
            bottom: top + sc(26, state.scale),
        },
        state.font_small,
        theme::DIM,
        DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

unsafe fn draw_text(
    hdc: HDC,
    text: &str,
    mut rect: RECT,
    font: HFONT,
    color: Color,
    format: DRAW_TEXT_FORMAT,
) {
    let old_font = SelectObject(hdc, font);
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, COLORREF(color.colorref()));
    let mut value = wide(text);
    let _ = DrawTextW(hdc, &mut value, &mut rect, format);
    SelectObject(hdc, old_font);
}

unsafe fn fill_color_rect(hdc: HDC, rect: Rect, brush: HBRUSH) {
    let win = rect_to_win(rect);
    let _ = FillRect(hdc, &win, brush);
}

fn rect_to_win(rect: Rect) -> RECT {
    RECT {
        left: rect.x,
        top: rect.y,
        right: rect.right(),
        bottom: rect.bottom(),
    }
}

unsafe fn draw_owner_button(state: &TranslationWindow, dis: &DRAWITEMSTRUCT) {
    let rc = dis.rcItem;
    let primary = dis.CtlID == ID_COPY as u32 || dis.CtlID == ID_RETRY as u32;
    let pressed = (dis.itemState.0 & ODS_SELECTED.0) != 0;
    let disabled = (dis.itemState.0 & ODS_DISABLED.0) != 0;
    let cursor = cursor_pos();
    let mut point = windows::Win32::Foundation::POINT {
        x: cursor.x,
        y: cursor.y,
    };
    let _ = ScreenToClient(dis.hwndItem, &mut point);
    let hovered = point.x >= 0
        && point.x < rc.right - rc.left
        && point.y >= 0
        && point.y < rc.bottom - rc.top;

    let (bg, border, text, font) = if disabled {
        (theme::PANEL, theme::BORDER, theme::DIM, state.font)
    } else if primary {
        if pressed {
            (
                Color::rgb(0x22, 0xB0, 0x82),
                Color::rgb(0x22, 0xB0, 0x82),
                Color::rgb(0x06, 0x22, 0x1B),
                state.font_bold,
            )
        } else if hovered {
            (
                theme::ACCENT_HI,
                theme::ACCENT_HI,
                Color::rgb(0x06, 0x22, 0x1B),
                state.font_bold,
            )
        } else {
            (
                theme::ACCENT,
                theme::ACCENT,
                Color::rgb(0x06, 0x22, 0x1B),
                state.font_bold,
            )
        }
    } else if pressed {
        (theme::ACCENT_DARK, theme::ACCENT, theme::TEXT, state.font)
    } else if hovered {
        (theme::HOVER_BG, theme::ACCENT, theme::TEXT, state.font)
    } else {
        (theme::PANEL, theme::BORDER, theme::TEXT, state.font)
    };

    let brush = CreateSolidBrush(COLORREF(bg.colorref()));
    let pen = CreatePen(PS_SOLID, 1, COLORREF(border.colorref()));
    let old_brush = SelectObject(dis.hDC, brush);
    let old_pen = SelectObject(dis.hDC, pen);
    let round = sc(7, state.scale);
    let _ = RoundRect(dis.hDC, rc.left, rc.top, rc.right, rc.bottom, round, round);
    SelectObject(dis.hDC, old_brush);
    SelectObject(dis.hDC, old_pen);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);

    let mut label = [0u16; 64];
    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(dis.hwndItem, &mut label);
    let label = crate::util::from_wide(&label);
    draw_text(
        dis.hDC,
        &label,
        rc,
        font,
        text,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

unsafe extern "system" fn button_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    parent_raw: usize,
) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            let key = wparam.0 as u32;
            let control = crate::native::key_down(0x11);
            if key == 0x1B || (control && (key == 0x0D || key == 0x52)) {
                let parent = HWND(parent_raw as *mut core::ffi::c_void);
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    parent,
                    WM_KEYDOWN,
                    wparam,
                    lparam,
                );
                return LRESULT(0);
            }
        }
        WM_MOUSEMOVE => {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            let _ = InvalidateRect(hwnd, None, false);
        }
        WM_MOUSELEAVE => {
            let _ = InvalidateRect(hwnd, None, false);
        }
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(None, IDC_HAND).unwrap_or_default());
            return LRESULT(1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn text_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    parent_raw: usize,
) -> LRESULT {
    if msg == WM_KEYDOWN {
        let key = wparam.0 as u32;
        let control = crate::native::key_down(0x11);
        if key == 0x1B || (control && (key == 0x0D || key == 0x52)) {
            let parent = HWND(parent_raw as *mut core::ffi::c_void);
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                parent,
                WM_KEYDOWN,
                wparam,
                lparam,
            );
            return LRESULT(0);
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe fn destroy_resources(state: &mut TranslationWindow) {
    for font in [
        state.font,
        state.font_title,
        state.font_bold,
        state.font_small,
    ] {
        if !font.is_invalid() {
            let _ = DeleteObject(font);
        }
    }
    for brush in [
        state.bg_brush,
        state.panel_brush,
        state.bottom_brush,
        state.accent_brush,
        state.accent_dark_brush,
        state.border_brush,
    ] {
        if !brush.is_invalid() {
            let _ = DeleteObject(brush);
        }
    }
    if !state.border_pen.is_invalid() {
        let _ = DeleteObject(state.border_pen);
    }
    if !state.accent_pen.is_invalid() {
        let _ = DeleteObject(state.accent_pen);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TranslationWindow;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let width = (rc.right - rc.left).max(1);
            let height = (rc.bottom - rc.top).max(1);
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
            let old_bmp = SelectObject(mem_dc, mem_bmp);
            paint_window(state, mem_dc, width, height);
            let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);
            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(mem_bmp);
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_SIZE => {
            layout(hwnd);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam.0 as *mut MINMAXINFO);
            info.ptMinTrackSize.x = sc(540, state.scale);
            info.ptMinTrackSize.y = sc(460, state.scale);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
            let control = HWND(lparam.0 as *mut _);
            if control == state.source_edit || control == state.translated_edit {
                let hdc = HDC(wparam.0 as *mut _);
                let _ = SetTextColor(hdc, COLORREF(theme::TEXT.colorref()));
                let _ = SetBkColor(hdc, COLORREF(theme::PANEL.colorref()));
                return LRESULT(state.panel_brush.0 as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            draw_owner_button(state, dis);
            LRESULT(1)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32 & 0xFFFF) as usize;
            let code = (wparam.0 as u32 >> 16) & 0xFFFF;
            if code == BN_CLICKED {
                match id {
                    ID_COPY => {
                        if crate::clipboard::set_text(&state.translated_text) {
                            state.copied = true;
                            update_controls(state);
                            let _ = SetTimer(hwnd, TIMER_COPY_FEEDBACK, 1500, None);
                        } else {
                            crate::toast::show_error("复制失败", "剪贴板正被占用，请稍后重试");
                        }
                    }
                    ID_RETRY => request_retry(state),
                    ID_CLOSE => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = wparam.0 as u32;
            if key == 0x1B {
                let _ = DestroyWindow(hwnd);
            } else if crate::native::key_down(0x11) && key == 0x0D {
                if state.view_state == ViewState::Success
                    && crate::clipboard::set_text(&state.translated_text)
                {
                    state.copied = true;
                    update_controls(state);
                    let _ = SetTimer(hwnd, TIMER_COPY_FEEDBACK, 1500, None);
                }
            } else if crate::native::key_down(0x11)
                && key == 0x52
                && state.view_state == ViewState::Error
            {
                request_retry(state);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            match wparam.0 {
                TIMER_ANIMATION => {
                    state.pulse = (state.pulse + 1) % 20;
                    let _ = InvalidateRect(hwnd, None, false);
                }
                TIMER_COPY_FEEDBACK => {
                    let _ = KillTimer(hwnd, TIMER_COPY_FEEDBACK);
                    state.copied = false;
                    update_controls(state);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, TIMER_ANIMATION);
            let _ = KillTimer(hwnd, TIMER_COPY_FEEDBACK);
            destroy_resources(state);
            drop(Box::from_raw(ptr));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if current() == hwnd {
                set_current(HWND::default());
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn request_retry(state: &TranslationWindow) {
    if state.host == 0 {
        return;
    }
    let host = HWND(state.host as *mut core::ffi::c_void);
    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
        host,
        WM_AI_RETRY,
        WPARAM(state.request_id as usize),
        LPARAM(0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_uses_a_forty_sixty_content_split() {
        let layout = compute_layout(680, 540, 1.0);
        let ratio = layout.translated_card.h as f32 / layout.source_card.h as f32;
        assert!((1.45..=1.55).contains(&ratio));
        assert!(layout.source_card.bottom() < layout.translated_card.y);
        assert!(layout.translated_card.bottom() < layout.action_bar.y);
    }

    #[test]
    fn layout_keeps_controls_inside_their_regions_at_high_dpi() {
        let layout = compute_layout(1020, 810, 1.5);
        assert!(layout.source_card.contains(crate::geom::Point::new(
            layout.source_edit.x,
            layout.source_edit.y,
        )));
        assert!(layout.translated_card.contains(crate::geom::Point::new(
            layout.translated_edit.x,
            layout.translated_edit.y,
        )));
        assert!(layout.copy_btn.y >= layout.action_bar.y);
        assert_eq!(layout.retry_btn, layout.copy_btn);
        assert!(layout.close_btn.right() <= layout.action_bar.right());
    }
}
