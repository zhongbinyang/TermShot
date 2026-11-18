use crate::bitmap::Bitmap;
use crate::geom::{Color, Point, Rect};
use crate::macos::ffi::{self, TSCanvasCallbacks};
use crate::macos::native::{
    blit_bitmap_canvas, cursor_arrow, cursor_hand, cursor_pos, dpi_scale_at, draw_text_canvas,
    line_canvas, round_rect_canvas, sc, work_area_from_point, VK_ESCAPE, VK_RETURN, VK_SPACE,
};
use crate::theme;
use std::os::raw::c_void;

struct SetupState {
    win: *mut c_void,
    scale: f32,
    w: i32,
    h: i32,
    startup: bool,
    installing: bool,
    status_text: String,
    install_path: String,
    app_bmp: Bitmap,
    checkbox_rect: Rect,
    install_btn: Rect,
    cancel_btn: Rect,
    hover_id: i32, // 0 none, 1 install, 2 cancel, 3 checkbox
    done: bool,
}

pub fn run() {
    let cur = cursor_pos();
    let scale = dpi_scale_at(cur).max(1.0);
    let work = work_area_from_point(cur);
    let p = |v: i32| sc(v, scale);

    let w = p(500);
    let h = p(410);
    let x = work.x + ((work.w - w) / 2).max(0);
    let y = work.y + ((work.h - h) / 2).max(0);

    let btn_y = p(354);
    let btn_h = p(34);
    let cancel_w = p(84);
    let install_w = p(102);
    let install_btn = Rect::new(w - p(26) - install_w, btn_y, install_w, btn_h);
    let cancel_btn = Rect::new(install_btn.x - p(10) - cancel_w, btn_y, cancel_w, btn_h);

    let card_x = p(24);
    let card_y = p(74);
    let card_w = w - p(48);
    let chk_box_y = card_y + p(212);
    let chk_box_s = p(18);
    let checkbox_rect = Rect::from_ltrb(
        card_x + p(16),
        chk_box_y - p(2),
        card_x + card_w - p(16),
        chk_box_y + chk_box_s + p(4),
    );

    let state = Box::new(SetupState {
        win: std::ptr::null_mut(),
        scale,
        w,
        h,
        startup: true,
        installing: false,
        status_text: String::new(),
        install_path: crate::install::install_dir().display().to_string(),
        app_bmp: crate::icon::draw_app_bitmap(p(40)),
        checkbox_rect,
        install_btn,
        cancel_btn,
        hover_id: 0,
        done: false,
    });
    let raw = Box::into_raw(state);

    let cbs = TSCanvasCallbacks {
        on_paint: Some(on_paint),
        on_mouse_move: Some(on_mouse_move),
        on_mouse_leave: Some(on_mouse_leave),
        on_mouse_down: Some(on_mouse_down),
        on_key_down: Some(on_key_down),
        on_close: Some(on_close),
        ..Default::default()
    };

    unsafe {
        let win = ffi::ts_canvas_create(
            ffi::WIN_DIALOG,
            x,
            y,
            w,
            h,
            sc(10, scale) as f64,
            true,
            &cbs,
            raw as *mut c_void,
        );
        (*raw).win = win;
        ffi::ts_canvas_show(win, true);

        loop {
            if (*raw).done {
                break;
            }
            ffi::ts_pump_events(12);
        }

        let win_to_close = (*raw).win;
        (*raw).win = std::ptr::null_mut();
        if !win_to_close.is_null() {
            ffi::ts_canvas_close(win_to_close);
        }
        cursor_arrow();
        drop(Box::from_raw(raw));
    }
}

impl SetupState {
    fn invalidate(&self) {
        if !self.win.is_null() {
            unsafe {
                ffi::ts_canvas_invalidate(self.win);
            }
        }
    }

    fn start_install(&mut self) {
        if self.installing {
            return;
        }
        self.installing = true;
        self.invalidate();
        unsafe {
            ffi::ts_pump_events(0);
        }
        match crate::install::install(true, self.startup) {
            Ok(()) => self.done = true,
            Err(e) => {
                self.installing = false;
                self.status_text = format!("安装失败: {e}");
                self.invalidate();
            }
        }
    }

    fn paint(&self, ctx: *mut c_void) {
        let p = |v: i32| sc(v, self.scale);

        round_rect_canvas(
            ctx,
            Rect::new(0, 0, self.w, self.h),
            p(10),
            theme::WIN_BG,
            theme::BORDER,
            1,
        );

        blit_bitmap_canvas(ctx, &self.app_bmp, Rect::new(p(26), p(22), p(38), p(38)));

        draw_text_canvas(
            ctx,
            Rect::new(p(74), p(18), p(300), p(26)),
            "TermShot 终端截图",
            Color::rgb(255, 255, 255),
            p(18),
            true,
            0,
            false,
        );
        draw_text_canvas(
            ctx,
            Rect::new(p(74), p(44), p(300), p(22)),
            "轻量 · 极速 · 增量长图 · 智能识别",
            theme::DIM,
            p(12),
            false,
            0,
            false,
        );

        let badge = Rect::new(self.w - p(26) - p(62), p(26), p(62), p(22));
        round_rect_canvas(ctx, badge, p(4), theme::PANEL, theme::BORDER, 1);
        draw_text_canvas(ctx, badge, "v1.0.0", theme::ACCENT, p(11), false, 1, false);

        let card_x = p(24);
        let card_y = p(74);
        let card_w = self.w - p(48);
        let card_h = p(264);
        round_rect_canvas(
            ctx,
            Rect::new(card_x, card_y, card_w, card_h),
            p(8),
            theme::PANEL,
            theme::BORDER,
            1,
        );

        draw_text_canvas(
            ctx,
            Rect::new(card_x + p(16), card_y + p(14), p(200), p(18)),
            "安装目标目录",
            theme::DIM,
            p(11),
            false,
            0,
            false,
        );

        let path_rect = Rect::new(card_x + p(16), card_y + p(32), card_w - p(32), p(32));
        round_rect_canvas(ctx, path_rect, p(6), theme::INPUT_BG, theme::BORDER, 1);
        draw_text_canvas(
            ctx,
            Rect::new(path_rect.x + p(10), path_rect.y, path_rect.w - p(95), path_rect.h),
            &self.install_path,
            theme::TEXT,
            p(12),
            false,
            0,
            false,
        );
        draw_text_canvas(
            ctx,
            Rect::new(path_rect.right() - p(88), path_rect.y, p(78), path_rect.h),
            "本地用户目录",
            theme::DIM,
            p(11),
            false,
            2,
            false,
        );

        let feats = [
            ("独立绿色运行", "无需管理员权限，安装到 ~/Applications，不写系统残留"),
            ("菜单栏轻量常驻", "快捷面板与全局快捷键随时唤起，截完可复制、保存或继续处理"),
            ("📜 增量长图拼接", "逐帧实时追踪滚轮，告别错位压扁，代码网页长图极速截取"),
        ];
        let feat_start_y = card_y + p(78);
        let feat_step = p(34);
        for (i, (tag, desc)) in feats.iter().enumerate() {
            let fy = feat_start_y + i as i32 * feat_step;
            draw_text_canvas(
                ctx,
                Rect::new(card_x + p(16), fy, p(116), p(20)),
                tag,
                theme::TEXT,
                p(12),
                true,
                0,
                false,
            );
            draw_text_canvas(
                ctx,
                Rect::new(card_x + p(134), fy + p(1), card_w - p(150), p(20)),
                desc,
                theme::DIM,
                p(11),
                false,
                0,
                false,
            );
        }

        let div_y = card_y + p(194);
        line_canvas(ctx, card_x + p(16), div_y, card_x + card_w - p(16), div_y, theme::BORDER, 1);

        let chk_box_x = card_x + p(16);
        let chk_box_y = card_y + p(212);
        let chk_box_s = p(18);
        let chk_rect = Rect::new(chk_box_x, chk_box_y, chk_box_s, chk_box_s);
        let (chk_bg, chk_border) = if self.startup {
            (theme::ACCENT, theme::ACCENT)
        } else {
            (theme::INPUT_BG, theme::BORDER)
        };
        round_rect_canvas(ctx, chk_rect, p(4), chk_bg, chk_border, 1);
        if self.startup {
            draw_text_canvas(
                ctx,
                chk_rect,
                "✓",
                Color::rgb(0x06, 0x22, 0x1B),
                p(12),
                true,
                1,
                false,
            );
        }
        draw_text_canvas(
            ctx,
            Rect::new(
                chk_box_x + chk_box_s + p(8),
                chk_box_y - p(2),
                card_w - p(48),
                chk_box_s + p(4),
            ),
            "开机时自动启动 TermShot (随 macOS 登录并在菜单栏常驻)",
            theme::TEXT,
            p(12),
            false,
            0,
            false,
        );

        let footer_rect = Rect::new(p(26), p(358), p(250), p(24));
        if self.installing {
            draw_text_canvas(
                ctx,
                footer_rect,
                "正在为您安装 TermShot，请稍候...",
                theme::ACCENT,
                p(12),
                true,
                0,
                false,
            );
        } else if !self.status_text.is_empty() {
            draw_text_canvas(
                ctx,
                footer_rect,
                &self.status_text,
                theme::DANGER,
                p(11),
                false,
                0,
                false,
            );
        } else {
            draw_text_canvas(
                ctx,
                footer_rect,
                "绿色便携 · 当前用户隔离安装",
                theme::DIM,
                p(11),
                false,
                0,
                false,
            );
        }

        // Cancel button
        let cancel_bg = if self.hover_id == 2 {
            theme::HOVER_BG
        } else {
            theme::PANEL
        };
        let cancel_border = if self.hover_id == 2 {
            Color::rgb(0x3D, 0x4B, 0x5E)
        } else {
            theme::BORDER
        };
        round_rect_canvas(ctx, self.cancel_btn, p(6), cancel_bg, cancel_border, 1);
        draw_text_canvas(
            ctx,
            self.cancel_btn,
            "取消",
            if self.installing { theme::DIM } else { theme::TEXT },
            p(12),
            false,
            1,
            false,
        );

        // Install button
        let inst_bg = if self.installing {
            theme::PANEL
        } else if self.hover_id == 1 {
            theme::ACCENT_HI
        } else {
            theme::ACCENT
        };
        let inst_fg = if self.installing {
            theme::DIM
        } else {
            Color::rgb(0x06, 0x22, 0x1B)
        };
        round_rect_canvas(ctx, self.install_btn, p(6), inst_bg, inst_bg, 1);
        draw_text_canvas(ctx, self.install_btn, "立即安装", inst_fg, p(12), true, 1, false);
    }
}

extern "C" fn on_paint(ctx: *mut c_void, _w: i32, _h: i32, ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *const SetupState)).paint(ctx);
        }
    }
}

extern "C" fn on_mouse_move(x: i32, y: i32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SetupState);
        let p = Point::new(x, y);
        let next = if st.install_btn.contains(p) {
            1
        } else if st.cancel_btn.contains(p) {
            2
        } else if st.checkbox_rect.contains(p) {
            3
        } else {
            0
        };
        if next > 0 {
            cursor_hand();
        } else {
            cursor_arrow();
        }
        if next != st.hover_id {
            st.hover_id = next;
            st.invalidate();
        }
    }
}

extern "C" fn on_mouse_leave(ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            let st = &mut *(ud as *mut SetupState);
            if st.hover_id != 0 {
                st.hover_id = 0;
                st.invalidate();
            }
        }
    }
}

extern "C" fn on_mouse_down(x: i32, y: i32, btn: i32, _clicks: i32, ud: *mut c_void) {
    if btn != 0 || ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SetupState);
        if st.installing {
            return;
        }
        let p = Point::new(x, y);
        if st.checkbox_rect.contains(p) {
            st.startup = !st.startup;
            st.invalidate();
        } else if st.cancel_btn.contains(p) {
            st.done = true;
        } else if st.install_btn.contains(p) {
            st.start_install();
        }
    }
}

extern "C" fn on_key_down(vk: u32, _mods: u32, ud: *mut c_void) {
    if ud.is_null() {
        return;
    }
    unsafe {
        let st = &mut *(ud as *mut SetupState);
        if st.installing {
            return;
        }
        if vk == VK_ESCAPE {
            st.done = true;
        } else if vk == VK_RETURN {
            st.start_install();
        } else if vk == VK_SPACE {
            st.startup = !st.startup;
            st.invalidate();
        }
    }
}

extern "C" fn on_close(ud: *mut c_void) {
    if !ud.is_null() {
        unsafe {
            (*(ud as *mut SetupState)).done = true;
        }
    }
}
