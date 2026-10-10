// The guests' page (web/pkg) is built separately by scripts/web.sh and bundled into the desktop
// app. Empty stand-ins let the desktop app build before it exists; sharing then shows an error.
fn main() {
    for f in ["web/pkg/tratto.js", "web/pkg/tratto_bg.wasm"] {
        let p = std::path::Path::new(f);
        if !p.exists() {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
    }
    println!("cargo:rerun-if-changed=web");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new().set_icon("assets/icon.ico").compile().unwrap();
    }
}
