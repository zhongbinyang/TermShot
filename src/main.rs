#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod annotation;
mod app;
mod bitmap;
mod capture_pin;
mod clipboard;
mod draw;
mod geom;
mod icon;
mod install;
#[cfg(target_os = "macos")]
mod macos;
mod native;
mod overlay;
mod pin;
mod quick_panel;
mod scroll;
mod settings;
mod settings_ui;
mod setup;
mod theme;
mod toast;
mod toolbar;
mod translation_window;
mod util;
mod vision;
mod windows_enum;

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
    crate::native::enable_dpi();

    let args: Vec<String> = std::env::args().collect();
    if has(&args, "--write-icon") {
        let default_dest = if cfg!(target_os = "macos") {
            "assets/AppIcon.icns"
        } else {
            "assets/app.ico"
        };
        let dest = args
            .iter()
            .position(|a| a == "--write-icon")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| default_dest.into());
        let path = std::path::Path::new(&dest);
        let res = if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("icns"))
        {
            icon::write_icns(path)
        } else {
            icon::write_ico(path)
        };
        if let Err(e) = res {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    if has(&args, "--uninstall") {
        install::uninstall();
        return;
    }
    if is_setup(&args) {
        if has(&args, "--silent") {
            let start = !has(&args, "--no-startup");
            let _ = install::install(true, start);
            return;
        }
        setup::run();
        return;
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        let name = util::wide("Local\\TermShot.SingleInstance");
        let _mutex = CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr()));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if !acquire_macos_single_instance_lock() {
            return;
        }
    }

    app::run_tray();
}

#[cfg(target_os = "macos")]
fn acquire_macos_single_instance_lock() -> bool {
    use std::os::unix::io::AsRawFd;
    let dir = settings::Settings::dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return true;
    }
    let lock_path = dir.join("termshot.lock");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
    else {
        return true;
    };
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        return false;
    }
    std::mem::forget(file);
    true
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a.eq_ignore_ascii_case(flag))
}

fn is_setup(args: &[String]) -> bool {
    if has(args, "--install") || has(args, "--setup") {
        return true;
    }
    let name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    name.to_ascii_lowercase().contains("setup") || name.to_ascii_lowercase().contains("install")
}
