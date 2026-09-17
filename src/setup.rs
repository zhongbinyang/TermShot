#![allow(clippy::too_many_arguments)]
use crate::geom::Color;
use crate::native::{
    cursor_pos, dpi_scale_at, enable_dark_title, hinstance, sc, set_font, work_area_from_point,
};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, InvalidateRect, RoundRect,
    SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HFONT,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    DrawIconEx, GetClientRect, GetMessageW, GetWindowLongPtrW, GetWindowTextW, LoadCursorW,
    PostMessageW, RegisterClassExW, SendMessageW, SetCursor, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, BS_OWNERDRAW, CS_DBLCLKS, DI_NORMAL, GWLP_USERDATA, IDC_HAND,
    MSG, SW_SHOW, WINDOW_EX_STYLE, WM_COMMAND, WM_DESTROY, WM_DRAWITEM, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SETICON, WNDCLASSEXW, WS_CAPTION,
    WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
};

const WM_MOUSELEAVE: u32 = 0x02A3;

struct SetupState {
    scale: f32,
    startup: bool,
    installing: bool,
    status_text: String,
    install_path: String,
    checkbox_rect: RECT,
    install_btn: HWND,
    cancel_btn: HWND,
    font_title: HFONT,
    font_sub: HFONT,
    font_bold: HFONT,
    font_text: HFONT,
    font_small: HFONT,
    app_icon: Option<windows::Win32::UI::WindowsAndMessaging::HICON>,
}

pub fn run() {
    unsafe {
        let scale = dpi_scale_at(cursor_pos()).max(1.0);
        let p = |v: i32| sc(v, scale);

        let font_title = make_font(p(18), true);
        let font_sub = make_font(p(12), false);
        let font_bold = make_font(p(12), true);
        let font_text = make_font(p(12), false);
        let font_small = make_font(p(11), false);
        let app_icon = crate::icon::create_hicon();

        let class = wide("TermShotSetup");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            hbrBackground: CreateSolidBrush(COLORREF(theme::WIN_BG.colorref())),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let client_w = p(500);
        let client_h = p(410);
        let mut rc_win = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let _ = AdjustWindowRectEx(&mut rc_win, style, false, WINDOW_EX_STYLE(0));
        let win_w = rc_win.right - rc_win.left;
        let win_h = rc_win.bottom - rc_win.top;

        let work = work_area_from_point(cursor_pos());
        let win_x = work.x + ((work.w - win_w) / 2).max(0);
        let win_y = work.y + ((work.h - win_h) / 2).max(0);

        let mut state = Box::new(SetupState {
            scale,
            startup: true,
            installing: false,
            status_text: String::new(),
            install_path: crate::install::install_dir().display().to_string(),
            checkbox_rect: RECT::default(),
            install_btn: HWND::default(),
            cancel_btn: HWND::default(),
            font_title,
            font_sub,
            font_bold,
            font_text,
            font_small,
            app_icon,
        });

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("安装 TermShot 终端截图"),
            style | WS_VISIBLE,
            win_x,
            win_y,
            win_w,
            win_h,
            None,
            None,
            hinstance(),
            Some(state.as_mut() as *mut SetupState as *mut _),
        )
        .unwrap_or_default();

        enable_dark_title(hwnd);

        if let Some(icon) = state.app_icon {
            let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(1), LPARAM(icon.0 as isize));
            let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(0), LPARAM(icon.0 as isize));
        }

        // Action Buttons
        let btn_y = p(354);
        let btn_h = p(34);
        let cancel_w = p(84);
        let install_w = p(102);

        let cancel_btn = owner_button(
            hwnd,
            client_w - p(26) - install_w - p(10) - cancel_w,
            btn_y,
            cancel_w,
            btn_h,
            "取消",
            2,
            font_text,
        );
        let install_btn = owner_button(
            hwnd,
            client_w - p(26) - install_w,
            btn_y,
            install_w,
            btn_h,
            "立即安装",
            1,
            font_bold,
        );

        state.cancel_btn = cancel_btn;
        state.install_btn = install_btn;

        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
                break;
            }
            if msg.message == WM_KEYDOWN {
                let key = msg.wParam.0 as i32;
                if key == VK_RETURN.0 as i32 {
                    let _ = PostMessageW(hwnd, WM_COMMAND, WPARAM(1), LPARAM(0));
                    continue;
                } else if key == VK_ESCAPE.0 as i32 {
                    let _ = PostMessageW(hwnd, WM_COMMAND, WPARAM(2), LPARAM(0));
                    continue;
                } else if key == 0x20 {
                    let _ = PostMessageW(hwnd, WM_COMMAND, WPARAM(201), LPARAM(0));
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = DeleteObject(font_title);
        let _ = DeleteObject(font_sub);
        let _ = DeleteObject(font_bold);
        let _ = DeleteObject(font_text);
        let _ = DeleteObject(font_small);
        if let Some(icon) = state.app_icon {
            crate::icon::destroy_icon(icon);
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SetupState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;

    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let cw = rc.right - rc.left;
            let ch = rc.bottom - rc.top;

            let p = |v: i32| sc(v, state.scale);

            // Double buffered painting
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, cw, ch);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            // 1. Background
            let bg_brush = CreateSolidBrush(COLORREF(theme::WIN_BG.colorref()));
            let _ = windows::Win32::Graphics::Gdi::FillRect(mem_dc, &rc, bg_brush);
            let _ = DeleteObject(bg_brush);

            // 2. Header: App Icon
            if let Some(icon) = state.app_icon {
                let _ = DrawIconEx(
                    mem_dc,
                    p(26),
                    p(22),
                    icon,
                    p(38),
                    p(38),
                    0,
                    None,
                    DI_NORMAL,
                );
            }

            // 2. Header: Title & Subtitle
            SetBkMode(mem_dc, TRANSPARENT);
            SetTextColor(mem_dc, COLORREF(Color::rgb(255, 255, 255).colorref()));
            let old_font = SelectObject(mem_dc, state.font_title);

            let mut wt_title = wide("TermShot 终端截图");
            let mut rc_title = RECT {
                left: p(74),
                top: p(18),
                right: p(380),
                bottom: p(44),
            };
            DrawTextW(mem_dc, &mut wt_title, &mut rc_title, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);

            SelectObject(mem_dc, state.font_sub);
            SetTextColor(mem_dc, COLORREF(theme::DIM.colorref()));
            let mut wt_sub = wide("轻量 · 极速 · 增量长图 · 智能识别");
            let mut rc_sub = RECT {
                left: p(74),
                top: p(44),
                right: p(380),
                bottom: p(66),
            };
            DrawTextW(mem_dc, &mut wt_sub, &mut rc_sub, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);

            // 2. Header: Version pill badge
            let badge_w = p(62);
            let badge_h = p(22);
            let badge_x = cw - p(26) - badge_w;
            let badge_y = p(26);
            let badge_brush = CreateSolidBrush(COLORREF(theme::PANEL.colorref()));
            let badge_pen = CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref()));
            let old_b = SelectObject(mem_dc, badge_brush);
            let old_p = SelectObject(mem_dc, badge_pen);
            let _ = RoundRect(mem_dc, badge_x, badge_y, badge_x + badge_w, badge_y + badge_h, p(4), p(4));
            SelectObject(mem_dc, old_b);
            SelectObject(mem_dc, old_p);
            let _ = DeleteObject(badge_brush);
            let _ = DeleteObject(badge_pen);

            SelectObject(mem_dc, state.font_small);
            SetTextColor(mem_dc, COLORREF(theme::ACCENT.colorref()));
            let mut wt_v = wide("v1.0.0");
            let mut rc_v = RECT {
                left: badge_x,
                top: badge_y,
                right: badge_x + badge_w,
                bottom: badge_y + badge_h,
            };
            DrawTextW(mem_dc, &mut wt_v, &mut rc_v, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

            // 3. Center Main Card
            let card_x = p(24);
            let card_y = p(74);
            let card_w = cw - p(48);
            let card_h = p(264);

            let card_brush = CreateSolidBrush(COLORREF(theme::PANEL.colorref()));
            let card_pen = CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref()));
            let old_b = SelectObject(mem_dc, card_brush);
            let old_p = SelectObject(mem_dc, card_pen);
            let _ = RoundRect(mem_dc, card_x, card_y, card_x + card_w, card_y + card_h, p(8), p(8));
            SelectObject(mem_dc, old_b);
            SelectObject(mem_dc, old_p);
            let _ = DeleteObject(card_brush);
            let _ = DeleteObject(card_pen);

            // Card - Section A: Install Location
            SelectObject(mem_dc, state.font_small);
            SetTextColor(mem_dc, COLORREF(theme::DIM.colorref()));
            let mut wt_loc_lbl = wide("安装目标目录");
            let mut rc_loc_lbl = RECT {
                left: card_x + p(16),
                top: card_y + p(14),
                right: card_x + p(200),
                bottom: card_y + p(32),
            };
            DrawTextW(mem_dc, &mut wt_loc_lbl, &mut rc_loc_lbl, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);

            let path_x = card_x + p(16);
            let path_y = card_y + p(32);
            let path_w = card_w - p(32);
            let path_h = p(32);

            let path_brush = CreateSolidBrush(COLORREF(theme::INPUT_BG.colorref()));
            let path_pen = CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref()));
            let old_b = SelectObject(mem_dc, path_brush);
            let old_p = SelectObject(mem_dc, path_pen);
            let _ = RoundRect(mem_dc, path_x, path_y, path_x + path_w, path_y + path_h, p(6), p(6));
            SelectObject(mem_dc, old_b);
            SelectObject(mem_dc, old_p);
            let _ = DeleteObject(path_brush);
            let _ = DeleteObject(path_pen);

            SelectObject(mem_dc, state.font_text);
            SetTextColor(mem_dc, COLORREF(theme::TEXT.colorref()));
            let mut wt_path = wide(&state.install_path);
            let mut rc_path = RECT {
                left: path_x + p(10),
                top: path_y,
                right: path_x + path_w - p(90),
                bottom: path_y + path_h,
            };
            DrawTextW(mem_dc, &mut wt_path, &mut rc_path, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

            SelectObject(mem_dc, state.font_small);
            SetTextColor(mem_dc, COLORREF(theme::DIM.colorref()));
            let mut wt_tag = wide("本地用户目录");
            let mut rc_tag = RECT {
                left: path_x + path_w - p(85),
                top: path_y,
                right: path_x + path_w - p(10),
                bottom: path_y + path_h,
            };
            DrawTextW(mem_dc, &mut wt_tag, &mut rc_tag, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

            // Card - Section B: Feature Highlights
            let feats = [
                ("⚡ 独立绿色运行", "无需管理员权限，安装到当前用户，不写系统注册表残留"),
                ("📌 托盘静默常驻", "后台无感驻留，支持快捷键随时唤起，截完自动复制或保存"),
                ("📜 增量长图拼接", "逐帧实时追踪滚轮，告别错位压扁，代码网页长图极速截取"),
            ];

            let feat_start_y = card_y + p(78);
            let feat_step = p(34);
            for (i, (tag, desc)) in feats.iter().enumerate() {
                let fy = feat_start_y + (i as i32) * feat_step;

                SelectObject(mem_dc, state.font_bold);
                SetTextColor(mem_dc, COLORREF(theme::TEXT.colorref()));
                let mut wt_t = wide(tag);
                let mut rc_t = RECT {
                    left: card_x + p(16),
                    top: fy,
                    right: card_x + p(130),
                    bottom: fy + p(20),
                };
                DrawTextW(mem_dc, &mut wt_t, &mut rc_t, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);

                SelectObject(mem_dc, state.font_small);
                SetTextColor(mem_dc, COLORREF(theme::DIM.colorref()));
                let mut wt_d = wide(desc);
                let mut rc_d = RECT {
                    left: card_x + p(134),
                    top: fy + p(1),
                    right: card_x + card_w - p(16),
                    bottom: fy + p(22),
                };
                DrawTextW(mem_dc, &mut wt_d, &mut rc_d, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
            }

            // Card - Divider line
            let div_y = card_y + p(194);
            let div_pen = CreatePen(PS_SOLID, 1, COLORREF(theme::BORDER.colorref()));
            let old_p = SelectObject(mem_dc, div_pen);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::MoveToEx(mem_dc, card_x + p(16), div_y, None);
                let _ = windows::Win32::Graphics::Gdi::LineTo(mem_dc, card_x + card_w - p(16), div_y);
            }
            SelectObject(mem_dc, old_p);
            let _ = DeleteObject(div_pen);

            // Card - Section C: Interactive Checkbox
            let chk_box_x = card_x + p(16);
            let chk_box_y = card_y + p(212);
            let chk_box_s = p(18);

            state.checkbox_rect = RECT {
                left: chk_box_x,
                top: chk_box_y - p(2),
                right: card_x + card_w - p(16),
                bottom: chk_box_y + chk_box_s + p(4),
            };

            let (chk_bg, chk_border) = if state.startup {
                (theme::ACCENT, theme::ACCENT)
            } else {
                (theme::INPUT_BG, theme::BORDER)
            };
            let chk_brush = CreateSolidBrush(COLORREF(chk_bg.colorref()));
            let chk_pen = CreatePen(PS_SOLID, 1, COLORREF(chk_border.colorref()));
            let old_b = SelectObject(mem_dc, chk_brush);
            let old_p = SelectObject(mem_dc, chk_pen);
            let _ = RoundRect(mem_dc, chk_box_x, chk_box_y, chk_box_x + chk_box_s, chk_box_y + chk_box_s, p(4), p(4));
            SelectObject(mem_dc, old_b);
            SelectObject(mem_dc, old_p);
            let _ = DeleteObject(chk_brush);
            let _ = DeleteObject(chk_pen);

            if state.startup {
                SelectObject(mem_dc, state.font_bold);
                SetTextColor(mem_dc, COLORREF(Color::rgb(0x06, 0x22, 0x1B).colorref()));
                let mut wt_check = wide("✓");
                let mut rc_check = RECT {
                    left: chk_box_x,
                    top: chk_box_y,
                    right: chk_box_x + chk_box_s,
                    bottom: chk_box_y + chk_box_s,
                };
                DrawTextW(mem_dc, &mut wt_check, &mut rc_check, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            }

            SelectObject(mem_dc, state.font_text);
            SetTextColor(mem_dc, COLORREF(theme::TEXT.colorref()));
            let mut wt_chk_lbl = wide("开机时自动启动 TermShot (随 Windows 登录并在托盘常驻)");
            let mut rc_chk_lbl = RECT {
                left: chk_box_x + chk_box_s + p(8),
                top: chk_box_y - p(2),
                right: card_x + card_w - p(16),
                bottom: chk_box_y + chk_box_s + p(2),
            };
            DrawTextW(mem_dc, &mut wt_chk_lbl, &mut rc_chk_lbl, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

            // 4. Bottom Footer Info
            if state.installing {
                SelectObject(mem_dc, state.font_bold);
                SetTextColor(mem_dc, COLORREF(theme::ACCENT.colorref()));
                let mut wt_info = wide("正在为您安装 TermShot，请稍候...");
                let mut rc_info = RECT {
                    left: p(26),
                    top: p(360),
                    right: p(260),
                    bottom: p(382),
                };
                DrawTextW(mem_dc, &mut wt_info, &mut rc_info, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
            } else if !state.status_text.is_empty() {
                SelectObject(mem_dc, state.font_small);
                SetTextColor(mem_dc, COLORREF(theme::DANGER.colorref()));
                let mut wt_err = wide(&state.status_text);
                let mut rc_err = RECT {
                    left: p(26),
                    top: p(360),
                    right: p(260),
                    bottom: p(382),
                };
                DrawTextW(mem_dc, &mut wt_err, &mut rc_err, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
            } else {
                SelectObject(mem_dc, state.font_small);
                SetTextColor(mem_dc, COLORREF(theme::DIM.colorref()));
                let mut wt_info = wide("绿色便携 · 当前用户隔离安装");
                let mut rc_info = RECT {
                    left: p(26),
                    top: p(360),
                    right: p(260),
                    bottom: p(382),
                };
                DrawTextW(mem_dc, &mut wt_info, &mut rc_info, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
            }

            // Blit to screen DC
            let _ = BitBlt(hdc, 0, 0, cw, ch, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_font);
            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(mem_bmp);
            let _ = DeleteDC(mem_dc);

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            draw_setup_button(state.scale, dis, state.font_text, state.font_bold);
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let p = lp(lparam);
            if p.x >= state.checkbox_rect.left
                && p.x <= state.checkbox_rect.right
                && p.y >= state.checkbox_rect.top
                && p.y <= state.checkbox_rect.bottom
            {
                state.startup = !state.startup;
                let _ = InvalidateRect(hwnd, Some(&state.checkbox_rect), false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cur = cursor_pos();
            let mut pt = windows::Win32::Foundation::POINT { x: cur.x, y: cur.y };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
            if pt.x >= state.checkbox_rect.left
                && pt.x <= state.checkbox_rect.right
                && pt.y >= state.checkbox_rect.top
                && pt.y <= state.checkbox_rect.bottom
            {
                let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                SetCursor(cursor);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xFFFF;
            if id == 201 {
                state.startup = !state.startup;
                let _ = InvalidateRect(hwnd, Some(&state.checkbox_rect), false);
            } else if id == 1 && !state.installing {
                state.installing = true;
                let _ = InvalidateRect(hwnd, None, false);
                let _ = EnableWindow(state.install_btn, false);
                let _ = EnableWindow(state.cancel_btn, false);

                let startup = state.startup;
                match crate::install::install(true, startup) {
                    Ok(_) => {
                        let _ = DestroyWindow(hwnd);
                    }
                    Err(e) => {
                        state.installing = false;
                        state.status_text = format!("安装失败: {e}");
                        let _ = EnableWindow(state.install_btn, true);
                        let _ = EnableWindow(state.cancel_btn, true);
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
            } else if id == 2 {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn lp(lparam: LPARAM) -> crate::geom::Point {
    let v = lparam.0 as u32;
    crate::geom::Point::new((v & 0xFFFF) as i16 as i32, ((v >> 16) & 0xFFFF) as i16 as i32)
}

fn make_font(px: i32, bold: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
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
    let t = wide(text);
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::w!("BUTTON"),
            windows::core::PCWSTR(t.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | BS_OWNERDRAW as u32,
            ),
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
        let _ = SetWindowSubclass(hwnd, Some(button_subclass_proc), id as usize, 0);
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

fn draw_setup_button(scale: f32, dis: &DRAWITEMSTRUCT, font: HFONT, font_bold: HFONT) {
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let is_pressed = (dis.itemState.0 & ODS_SELECTED.0) != 0;
    let is_disabled = (dis.itemState.0 & ODS_DISABLED.0) != 0;
    let is_accent = dis.CtlID == 1;

    let cur = cursor_pos();
    let mut pt = windows::Win32::Foundation::POINT { x: cur.x, y: cur.y };
    let _ = unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(dis.hwndItem, &mut pt) };
    let is_hover = pt.x >= 0
        && pt.x < (rc.right - rc.left)
        && pt.y >= 0
        && pt.y < (rc.bottom - rc.top);

    let (bg_color, border_color, text_color, use_font) = if is_disabled {
        (
            theme::PANEL,
            theme::BORDER,
            theme::DIM,
            font,
        )
    } else if is_accent {
        if is_pressed {
            (
                Color::rgb(0x22, 0xB0, 0x82),
                Color::rgb(0x22, 0xB0, 0x82),
                Color::rgb(0x06, 0x22, 0x1B),
                font_bold,
            )
        } else if is_hover {
            (
                theme::ACCENT_HI,
                theme::ACCENT_HI,
                Color::rgb(0x06, 0x22, 0x1B),
                font_bold,
            )
        } else {
            (
                theme::ACCENT,
                theme::ACCENT,
                Color::rgb(0x06, 0x22, 0x1B),
                font_bold,
            )
        }
    } else if is_pressed {
        (
            Color::rgb(0x12, 0x18, 0x20),
            theme::ACCENT,
            theme::TEXT,
            font,
        )
    } else if is_hover {
        (
            theme::HOVER_BG,
            Color::rgb(0x3D, 0x4B, 0x5E),
            theme::TEXT,
            font,
        )
    } else {
        (
            theme::PANEL,
            theme::BORDER,
            theme::TEXT,
            font,
        )
    };

    let p = |v: i32| sc(v, scale);
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
        let old_font = SelectObject(hdc, use_font);

        let mut buf = [0u16; 64];
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
