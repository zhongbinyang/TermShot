#[cfg(target_os = "windows")]
mod imp {
    use crate::settings::Settings;
    use crate::util::{exe_path, local_app_data, start_menu_programs, wide};
    use std::path::{Path, PathBuf};
    use windows::core::Interface;
    use windows::Win32::System::Com::IPersistFile;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
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
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
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
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::settings::Settings;
    use crate::util::{exe_path, start_menu_programs};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    const BUNDLE_ID: &str = "com.termshot.app";

    pub fn install_dir() -> PathBuf {
        start_menu_programs().join("TermShot.app")
    }

    pub fn installed_exe() -> PathBuf {
        install_dir().join("Contents").join("MacOS").join("TermShot")
    }

    pub fn launch_agent_plist() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{BUNDLE_ID}.plist"))
    }

    pub fn install(launch: bool, start_at_login: bool) -> Result<(), String> {
        let src = exe_path().map_err(|e| e.to_string())?;
        stop_others();

        let app_dir = install_dir();
        let contents = app_dir.join("Contents");
        let macos_dir = contents.join("MacOS");
        let res_dir = contents.join("Resources");
        std::fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&res_dir).map_err(|e| e.to_string())?;

        let dest = installed_exe();
        if !same_path(&src, &dest) {
            let tmp = macos_dir.join("TermShot.new");
            std::fs::copy(&src, &tmp).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
        }
        let mut perms = std::fs::metadata(&dest)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms).map_err(|e| e.to_string())?;

        crate::icon::write_icns(&res_dir.join("AppIcon.icns"))?;

        let plist = info_plist();
        std::fs::write(contents.join("Info.plist"), plist).map_err(|e| e.to_string())?;

        let _ = std::process::Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&app_dir)
            .output();

        let _ = apply_startup(start_at_login, Some(&dest));
        let mut s = Settings::load();
        s.start_with_windows = start_at_login;
        s.save();
        if launch {
            launch_installed();
        }
        Ok(())
    }

    pub fn uninstall() {
        let plist = launch_agent_plist();
        if plist.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w"])
                .arg(&plist)
                .output();
            let _ = std::fs::remove_file(&plist);
        }
        stop_others();
        let _ = std::fs::remove_dir_all(install_dir());
        let _ = std::fs::remove_dir_all(Settings::dir());
    }

    pub fn apply_startup(enabled: bool, exe: Option<&Path>) -> Result<(), String> {
        let plist = launch_agent_plist();
        if enabled {
            let bin = exe
                .map(|p| p.to_path_buf())
                .filter(|p| p.exists())
                .or_else(|| {
                    let inst = installed_exe();
                    inst.exists().then_some(inst)
                })
                .or_else(|| exe_path().ok())
                .ok_or_else(|| "no exe".to_string())?;
            if let Some(parent) = plist.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{BUNDLE_ID}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
                xml_escape(&bin.display().to_string())
            );
            std::fs::write(&plist, content).map_err(|e| e.to_string())?;
        } else if plist.exists() {
            let _ = std::fs::remove_file(&plist);
        }
        Ok(())
    }

    pub fn launch_installed() {
        let app_dir = install_dir();
        if app_dir.exists() {
            let _ = std::process::Command::new("open").arg(&app_dir).spawn();
        }
    }

    fn stop_others() {
        let my_pid = std::process::id().to_string();
        if let Ok(out) = std::process::Command::new("pgrep")
            .args(["-x", "TermShot"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let pid = line.trim();
                if !pid.is_empty() && pid != my_pid {
                    let _ = std::process::Command::new("kill").args(["-TERM", pid]).output();
                }
            }
        }
    }

    fn same_path(a: &Path, b: &Path) -> bool {
        let fa = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
        let fb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
        fa == fb
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    pub fn info_plist() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleExecutable</key>
    <string>TermShot</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>{BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>TermShot</string>
    <key>CFBundleDisplayName</key>
    <string>TermShot</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSScreenCaptureUsageDescription</key>
    <string>TermShot 需要屏幕录制权限以截取选区、窗口和滚动长图。</string>
</dict>
</plist>
"#
        )
    }
}

pub use imp::*;
