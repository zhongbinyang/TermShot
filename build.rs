fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        println!("cargo:rerun-if-changed=app.manifest");
        println!("cargo:rerun-if-changed=app.rc");
        println!("cargo:rerun-if-changed=assets/app.ico");
        let ico = std::path::Path::new("assets/app.ico");
        if ico.exists() {
            embed_resource::compile("app.rc", embed_resource::NONE);
        } else {
            let rc = std::path::Path::new("app.manifest.rc");
            std::fs::write(rc, "1 24 \"app.manifest\"\n").ok();
            embed_resource::compile("app.manifest.rc", embed_resource::NONE);
        }
    } else if target_os == "macos" {
        println!("cargo:rerun-if-changed=src/macos/bridge.m");
        let mut build = cc::Build::new();
        build.file("src/macos/bridge.m");
        if std::path::Path::new("/usr/bin/clang").exists() {
            build.compiler("/usr/bin/clang");
        }
        build
            .flag("-fobjc-arc")
            .flag("-Wno-deprecated-declarations")
            .flag("-Wno-unused-parameter")
            .compile("termshot_macos_bridge");

        for fw in [
            "AppKit",
            "Foundation",
            "CoreGraphics",
            "ScreenCaptureKit",
            "Carbon",
            "QuartzCore",
        ] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    }
}
