use crate::native::{
    cursor_pos, dpi_scale_at, enable_dark_title, hinstance, sc, set_font, ui_font, work_area_from_point,
};
use crate::theme;
use crate::util::wide;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, RegisterClassExW,
    SendMessageW, SetWindowLongPtrW, TranslateMessage, BS_AUTOCHECKBOX, BS_PUSHBUTTON, GWLP_USERDATA,
    MSG, SW_SHOW, WM_COMMAND, WM_DESTROY, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU,
    WS_VISIBLE,
};

pub fn run() {
    unsafe {
        let scale = dpi_scale_at(cursor_pos()).max(1.0);
        let p = |v: i32| sc(v, scale);
        let font = ui_font(p(16));
        let class = wide("TermShotSetup");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance().0),
            lpszClassName: windows::core::PCWSTR(class.as_ptr()),
            hbrBackground: CreateSolidBrush(windows::Win32::Foundation::COLORREF(theme::WIN_BG.colorref())),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let mut startup = Box::new(true);
        let work = work_area_from_point(cursor_pos());
        let w = p(460);
        let h = p(300);
        let x = work.x + ((work.w - w) / 2).max(0);
        let y = work.y + ((work.h - h) / 2).max(0);
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            windows::core::PCWSTR(class.as_ptr()),
            windows::core::w!("安装终端截图"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            hinstance(),
            Some(startup.as_mut() as *mut bool as *mut _),
        )
        .unwrap_or_default();
        enable_dark_title(hwnd);
        let inst = hinstance();
        let mk = |cls, text, style, x, y, cw, ch, id| {
            let hwnd = CreateWindowExW(
                Default::default(),
                cls,
                text,
                style,
                x,
                y,
                cw,
                ch,
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::HMENU(id),
                inst,
                None,
            )
            .unwrap_or_default();
            set_font(hwnd, font);
            hwnd
        };
        let _ = mk(
            windows::core::w!("STATIC"),
            windows::core::w!("终端截图"),
            WS_CHILD | WS_VISIBLE,
            p(28),
            p(22),
            p(380),
            p(28),
            0isize as _,
        );
        let _ = mk(
            windows::core::w!("STATIC"),
            windows::core::w!("托盘常驻，框选存 PNG。无需管理员，安装到当前用户。"),
            WS_CHILD | WS_VISIBLE,
            p(30),
            p(58),
            p(380),
            p(36),
            0isize as _,
        );
        let path = format!("位置  {}", crate::install::install_dir().display());
        let pw = wide(&path);
        let _ = mk(
            windows::core::w!("STATIC"),
            windows::core::PCWSTR(pw.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            p(30),
            p(102),
            p(380),
            p(36),
            0isize as _,
        );
        let chk = mk(
            windows::core::w!("BUTTON"),
            windows::core::w!("开机启动（默认开启）"),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | BS_AUTOCHECKBOX as u32,
            ),
            p(30),
            p(142),
            p(280),
            p(24),
            201isize as _,
        );
        SendMessageW(
            chk,
            windows::Win32::UI::WindowsAndMessaging::BM_SETCHECK,
            WPARAM(1),
            LPARAM(0),
        );
        let _ = mk(
            windows::core::w!("BUTTON"),
            windows::core::w!("安装"),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32,
            ),
            p(250),
            p(214),
            p(88),
            p(32),
            1isize as _,
        );
        let _ = mk(
            windows::core::w!("BUTTON"),
            windows::core::w!("取消"),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32,
            ),
            p(346),
            p(214),
            p(88),
            p(32),
            2isize as _,
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, SW_SHOW);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(hwnd).as_bool() {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = startup;
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(font);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut bool;
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 as u32) & 0xFFFF;
            if id == 201 && !ptr.is_null() {
                *ptr = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, 201).unwrap_or_default(),
                    windows::Win32::UI::WindowsAndMessaging::BM_GETCHECK,
                    WPARAM(0),
                    LPARAM(0),
                )
                .0 == 1;
            } else if id == 1 {
                let start = if ptr.is_null() { true } else { *ptr };
                let _ = crate::install::install(true, start);
                let _ = DestroyWindow(hwnd);
            } else if id == 2 {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
