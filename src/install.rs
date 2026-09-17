use crate::settings::Settings;
use crate::util::{exe_path, local_app_data, start_menu_programs, wide};
use std::path::{Path, PathBuf};
use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Com::IPersistFile;
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use winreg::enums::*;
use winreg::RegKey;

pub fn install_dir() -> PathBuf {
    local_app_data().join("TermShot")
}

pub fn installed_exe() -> PathBuf {
    install_dir().join("TermShot.exe")
}

pub fn start_menu_link() -> PathBuf {
    start_menu_programs().join("TermShot.lnk")
}

pub fn install(launch: bool, start_with_windows: bool) -> Result<(), String> {
    let src = exe_path().map_err(|e| e.to_string())?;
    stop_others();
    std::fs::create_dir_all(install_dir()).map_err(|e| e.to_string())?;
    let dest = installed_exe();
    if !same_path(&src, &dest) {
        let tmp = install_dir().join("TermShot.exe.new");
        std::fs::copy(&src, &tmp).map_err(|e| e.to_string())?;
        std::fs::copy(&tmp, &dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
    }
    let _ = create_shortcut(
        &start_menu_link(),
        &dest,
        "终端截图 - 框选保存 PNG，路径写入剪贴板",
    );
    let _ = apply_startup(start_with_windows, Some(&dest));
    let mut s = Settings::load();
    s.start_with_windows = start_with_windows;
    s.save();
    if launch {
        launch_installed();
    }
    Ok(())
}

pub fn uninstall() {
    let _ = apply_startup(false, None);
    let _ = std::fs::remove_file(start_menu_link());
    stop_others();
    let _ = std::fs::remove_dir_all(install_dir());
}

pub fn apply_startup(enabled: bool, exe: Option<&Path>) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .map_err(|e| e.to_string())?;
    if enabled {
        let path = exe
            .map(|p| p.to_path_buf())
            .or_else(|| exe_path().ok())
            .ok_or_else(|| "no exe".to_string())?;
        key.set_value("TermShot", &format!("\"{}\"", path.display()))
            .map_err(|e| e.to_string())?;
    } else {
        let _ = key.delete_value("TermShot");
    }
    Ok(())
}

pub fn launch_installed() {
    let dest = installed_exe();
    let _ = std::process::Command::new(&dest)
        .current_dir(install_dir())
        .spawn();
}

fn stop_others() {
    let pid = std::process::id();
    if let Ok(out) = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "TermShot.exe", "/FI", &format!("PID ne {pid}")])
        .output()
    {
        let _ = out;
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let fa = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let fb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    fa == fb
}

struct ComScope;
impl Drop for ComScope {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn create_shortcut(link: &Path, target: &Path, desc: &str) -> Result<(), String> {
    if let Some(p) = link.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().map_err(|e| e.to_string())?;
        let _guard = ComScope;
        let sl: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| e.to_string())?;
        let t = wide(&target.display().to_string());
        sl.SetPath(windows::core::PCWSTR(t.as_ptr()))
            .map_err(|e| e.to_string())?;
        if let Some(dir) = target.parent() {
            let d = wide(&dir.display().to_string());
            sl.SetWorkingDirectory(windows::core::PCWSTR(d.as_ptr()))
                .map_err(|e| e.to_string())?;
        }
        let ds = wide(desc);
        sl.SetDescription(windows::core::PCWSTR(ds.as_ptr()))
            .map_err(|e| e.to_string())?;
        sl.SetIconLocation(windows::core::PCWSTR(t.as_ptr()), 0)
            .map_err(|e| e.to_string())?;
        let pf: IPersistFile = sl.cast().map_err(|e| e.to_string())?;
        let lp = wide(&link.display().to_string());
        pf.Save(windows::core::PCWSTR(lp.as_ptr()), true)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
