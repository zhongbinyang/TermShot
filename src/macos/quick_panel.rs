use crate::geom::{Color, Point, Rect};
use crate::macos::ffi::{self, TSCanvasCallbacks};
use crate::macos::native::{
    cursor_arrow, cursor_hand, cursor_pos, dpi_scale_at, draw_text_canvas, fill_ellipse_canvas,
    round_rect_canvas, sc, work_area_from_point, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE,
    VK_TAB, VK_UP,
};
use crate::quick_panel::{QuickPanel, Snapshot, ACTION_CLOSE_PINS, BUTTON_COUNT};
use crate::theme;
use std::cell::Cell;
use std::os::raw::c_void;

struct PanelState {
    win: *mut c_void,
    panel: QuickPanel,
    closing: bool,
}

thread_local! {
    static CURRENT: Cell<*mut PanelState> = const { Cell::new(std::ptr::null_mut()) };
}

pub fn close() {
    let cur = CURRENT.with(|c| c.replace(std::ptr::null_mut()));
    if cur.is_null() {
        return;
    }
    unsafe {
        if (*cur).closing {
            return;
        }
        (*cur).closing = true;
        let win = (*cur).win;
        (*cur).win = std::ptr::null_mut();
        if !win.is_null() {
            ffi::ts_canvas_close(win);
        }
        drop(Box::from_raw(cur));
    }
}

pub fn toggle(snapshot: Snapshot) {
    let existing = CURRENT.with(|c| c.get());
    if !existing.is_null() {
        close();
        return;
    }

    let mut ax = 0i32;
    let mut ay = 0i32;
    let has_anchor = unsafe { ffi::ts_tray_anchor(&mut ax, &mut ay) };
    let anchor = if has_anchor {
        Point::new(ax, ay)
    } else {
        cursor_pos()
    };
    let work = work_area_from_point(anchor);
    let scale = dpi_scale_at(anchor).max(1.0);
    let width = sc(356, scale);
    let height = sc(426, scale);
    let margin = sc(12, scale);
    let x = (anchor.x - width / 2).clamp(work.x + margin, work.right() - width - margin);
    let y = if has_anchor {
        (anchor.y + sc(6, scale)).clamp(work.y + margin, work.bottom() - height - margin)
    } else if anchor.y > work.y + work.h / 2 {
        (anchor.y - height - margin).max(work.y + margin)
    } else {
        (anchor.y + margin).min(work.bottom() - height - margin)
    };

    let mut qp = QuickPanel {
        host: 0,
        snapshot,
        scale,
        width,
        height,
        buttons: [Rect::default(); BUTTON_COUNT],
        hover: -1,
        focus: 0,
    };
    qp.layout();

    let state = Box::new(PanelState {
        win: std::ptr::null_mut(),
        panel: qp,
        closing: false,
    });
    let raw = Box::into_raw(state);

    let cbs = TSCanvasCallbacks {
        on_paint: Some(on_paint),
        on_mouse_move: Some(on_mouse_move),
        on_mouse_leave: Some(on_mouse_leave),
        on_mouse_down: Some(on_mouse_down),
        on_key_down: Some(on_key_down),
        on_resign_key: Some(on_resign_key),
        on_close: Some(on_close),
        ..Default::default()
    };

    unsafe {
        let win = ffi::ts_canvas_create(
            ffi::WIN_POPUP,
            x,
            y,
            width,
            height,
            sc(theme::RADIUS_LG, scale) as f64,
            true,
            &cbs,
            raw as *mut c_void,
        );
        (*raw).win = win;
        CURRENT.with(|c| c.set(raw));
        ffi::ts_canvas_show(win, true);
    }
}

fn activate_index(raw: *mut PanelState, index: i32) {
    if raw.is_null() {
        return;
    }
    let (action, disabled) = unsafe {
        let p = &(*raw).panel;
        let act = QuickPanel::action(index);
        let dis = act == 0 || (act == ACTION_CLOSE_PINS && p.snapshot.pin_count == 0);
        (act, dis)
    };
    if disabled {
        return;
    }
    close();
    ffi::dispatch_main_fn(move || {
        crate::macos::app::handle_quick_action(action);
    });
}

extern "C" fn on_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let panel = unsafe { &(*(ud as *const PanelState)).panel };
    let p = |v: i32| sc(v, panel.scale);

    round_rect_canvas(
        ctx,
        Rect::new(0, 0, panel.width, panel.height),
        p(12),
        theme::WIN_BG,
        theme::BORDER,
        1,
    );

    draw_text_canvas(
        ctx,
        Rect::new(p(20), p(14), p(190), p(28)),
        "TermShot",
        theme::TEXT,
        p(18),
        true,
        0,
        false,
    );
    draw_text_canvas(
        ctx,
        Rect::new(p(20), p(43), p(220), p(20)),
        "截图、识别与翻译",
        theme::DIM,
        p(11),
        false,
        0,
        false,
    );

    let status_color = if panel.snapshot.ai_busy {
        theme::WARNING
    } else if panel.snapshot.api_ready {
        theme::SUCCESS
    } else {
        theme::DIM
    };
    let status = if panel.snapshot.ai_busy {
        "AI 处理中"
    } else if panel.snapshot.api_ready {
        "AI 已就绪"
    } else {
        "AI 未配置"
    };
    fill_ellipse_canvas(ctx, Rect::new(p(250), p(25), p(8), p(8)), status_color);
    draw_text_canvas(
        ctx,
        Rect::new(p(265), p(17), p(74), p(24)),
        status,
        status_color,
        p(10),
        false,
        0,
        false,
    );

    draw_button(
        ctx,
        panel,
        0,
        "区域截图",
        &panel.snapshot.capture_hotkey,
        true,
        true,
    );
    draw_button(
        ctx,
        panel,
        1,
        "翻译剪贴板",
        &panel.snapshot.translate_hotkey,
        false,
        true,
    );

    draw_text_canvas(
        ctx,
        Rect::new(p(20), p(218), p(100), p(22)),
        "贴图",
        theme::TEXT,
        p(12),
        true,
        0,
        false,
    );
    let pin_status = if panel.snapshot.pin_count == 0 {
        "当前没有桌面贴图".to_string()
    } else {
        format!("当前有 {} 张桌面贴图", panel.snapshot.pin_count)
    };
    draw_text_canvas(
        ctx,
        Rect::new(p(20), p(240), p(260), p(20)),
        &pin_status,
        theme::DIM,
        p(10),
        false,
        0,
        false,
    );
    draw_button(
        ctx,
        panel,
        2,
        "关闭全部贴图",
        if panel.snapshot.pin_count == 0 {
            "无贴图"
        } else {
            ""
        },
        false,
        panel.snapshot.pin_count > 0,
    );

    draw_text_canvas(
        ctx,
        Rect::new(p(20), p(320), p(120), p(22)),
        "快捷入口",
        theme::DIM,
        p(11),
        true,
        0,
        false,
    );
    draw_small_button(ctx, panel, 3, "保存目录");
    draw_small_button(ctx, panel, 4, "设置");
    draw_small_button(ctx, panel, 5, "退出");
}

fn draw_button(
    ctx: *mut c_void,
    panel: &QuickPanel,
    index: usize,
    label: &str,
    shortcut: &str,
    primary: bool,
    enabled: bool,
) {
    let rect = panel.buttons[index];
    let active = panel.hover == index as i32 || panel.focus == index as i32;
    let bg = if !enabled {
        theme::INPUT_BG
    } else if primary {
        if active {
            theme::ACCENT_HI
        } else {
            theme::ACCENT
        }
    } else if active {
        theme::HOVER_BG
    } else {
        theme::PANEL
    };
    let fg = if !enabled {
        theme::DIM
    } else if primary {
        Color::rgb(0x05, 0x22, 0x1B)
    } else {
        theme::TEXT
    };
    round_rect_canvas(
        ctx,
        rect,
        sc(8, panel.scale),
        bg,
        if active { theme::ACCENT } else { theme::BORDER },
        1,
    );
    draw_text_canvas(
        ctx,
        Rect::new(rect.x + sc(16, panel.scale), rect.y, rect.w / 2, rect.h),
        label,
        fg,
        sc(13, panel.scale),
        true,
        0,
        false,
    );
    draw_text_canvas(
        ctx,
        Rect::new(
            rect.x + rect.w / 2,
            rect.y,
            rect.w / 2 - sc(16, panel.scale),
            rect.h,
        ),
        shortcut,
        if primary { fg } else { theme::DIM },
        sc(10, panel.scale),
        false,
        2,
        false,
    );
}

fn draw_small_button(ctx: *mut c_void, panel: &QuickPanel, index: usize, label: &str) {
    let rect = panel.buttons[index];
    let active = panel.hover == index as i32 || panel.focus == index as i32;
    round_rect_canvas(
        ctx,
        rect,
        sc(7, panel.scale),
        if active { theme::HOVER_BG } else { theme::PANEL },
        if active { theme::ACCENT } else { theme::BORDER },
        1,
    );
    draw_text_canvas(
        ctx,
        rect,
        label,
        if active {
            theme::ACCENT_HI
        } else {
            theme::TEXT
        },
        sc(11, panel.scale),
        false,
        1,
        false,
    );
}

extern "C" fn on_mouse_move(x: i32, y: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut PanelState);
        let next = st.panel.hit(Point::new(x, y));
        if next >= 0 {
            cursor_hand();
        } else {
            cursor_arrow();
        }
        if next != st.panel.hover {
            st.panel.hover = next;
            ffi::ts_canvas_invalidate(st.win);
        }
    }
}

extern "C" fn on_mouse_leave(ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut PanelState);
        if st.panel.hover != -1 {
            st.panel.hover = -1;
            ffi::ts_canvas_invalidate(st.win);
        }
    }
}

extern "C" fn on_mouse_down(x: i32, y: i32, btn: i32, _clicks: i32, ud: *mut c_void) {
    if btn != 0 || ud.is_null() {
        return;
    }
    let raw = ud as *mut PanelState;
    let index = unsafe { (*raw).panel.hit(Point::new(x, y)) };
    if index >= 0 {
        activate_index(raw, index);
    }
}

extern "C" fn on_key_down(vk: u32, _mods: u32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    let raw = ud as *mut PanelState;
    if vk == VK_ESCAPE {
        close();
        return;
    }
    unsafe {
        let st = &mut *raw;
        if vk == VK_TAB || vk == VK_DOWN {
            st.panel.focus = (st.panel.focus + 1).rem_euclid(BUTTON_COUNT as i32);
            ffi::ts_canvas_invalidate(st.win);
        } else if vk == VK_UP {
            st.panel.focus = (st.panel.focus - 1).rem_euclid(BUTTON_COUNT as i32);
            ffi::ts_canvas_invalidate(st.win);
        } else if vk == VK_RETURN || vk == VK_SPACE {
            let focus = st.panel.focus;
            activate_index(raw, focus);
        }
    }
}

extern "C" fn on_resign_key(_ud: *mut c_void) {
    close();
}

extern "C" fn on_close(_ud: *mut c_void) {
    close();
}
