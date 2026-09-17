fn main() {
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
}
