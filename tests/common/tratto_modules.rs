#[allow(dead_code, unused_imports, unused_variables, unused_mut)]
#[path = "../../src/app/mod.rs"]
pub mod app;
#[path = "../../src/assets.rs"]
pub mod assets;
#[path = "../../src/audio.rs"]
pub mod audio;
#[path = "../../src/brush.rs"]
pub mod brush;
#[path = "../../src/code.rs"]
pub mod code;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../src/diag.rs"]
pub mod diag;
#[path = "../../src/doc.rs"]
pub mod doc;
#[path = "../../src/editor/mod.rs"]
pub mod editor;
#[path = "../../src/export.rs"]
pub mod export;
#[path = "../../src/geom.rs"]
pub mod geom;
#[path = "../../src/ink.rs"]
pub mod ink;
#[cfg(target_os = "linux")]
#[path = "../../src/install.rs"]
pub mod install;
#[path = "../../src/model.rs"]
pub mod model;
#[path = "../../src/music.rs"]
pub mod music;
#[path = "../../src/net.rs"]
pub mod net;
#[path = "../../src/paint.rs"]
pub mod paint;
#[path = "../../src/pen.rs"]
pub mod pen;
#[path = "../../src/platform.rs"]
pub mod platform;
#[path = "../../src/prefs.rs"]
pub mod prefs;
#[path = "../../src/prims.rs"]
pub mod prims;
#[path = "../../src/raster.rs"]
pub mod raster;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../src/share.rs"]
pub mod share;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../src/store.rs"]
pub mod store;
#[path = "../../src/style.rs"]
pub mod style;
#[path = "../../src/table.rs"]
pub mod table;
#[path = "../../src/templates.rs"]
pub mod templates;
#[path = "../../src/text.rs"]
pub mod text;
#[path = "../../src/ui/mod.rs"]
pub mod ui;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../src/updates.rs"]
pub mod updates;
#[path = "../../src/widgets.rs"]
pub mod widgets;

#[cfg(not(target_arch = "wasm32"))]
pub fn data_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("TRATTO_DATA") {
        return dir.into();
    }
    let base = dirs::config_dir().unwrap_or_else(|| ".".into());
    let packaged = base.join("Tratto");
    let dev = base.join("tratto");
    if !packaged.join("tratto.db").exists() && dev.join("tratto.db").exists() { dev } else { packaged }
}
