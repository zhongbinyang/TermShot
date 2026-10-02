#![windows_subsystem = "windows"]

mod annotation;
mod app;
mod bitmap;
mod capture_pin;
mod clipboard;
mod draw;
mod geom;
mod icon;
mod install;
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

use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::CreateMutexW;

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    crate::native::enable_dpi();

    let args: Vec<String> = std::env::args().collect();
    if has(&args, "--write-icon") {
        let dest = args
            .iter()
            .position(|a| a == "--write-icon")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| "assets/app.ico".into());
        if let Err(e) = icon::write_ico(std::path::Path::new(&dest)) {
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

    unsafe {
        let name = util::wide("Local\\TermShot.SingleInstance");
        let _mutex = CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr()));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    app::run_tray();
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
