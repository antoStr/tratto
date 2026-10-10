#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod doc;
mod editor;
mod export;
mod geom;
mod ink;
mod model;
mod net;
mod paint;
mod pen;
mod platform;
mod prefs;
mod prims;
mod raster;
#[cfg(not(target_arch = "wasm32"))]
mod share;
#[cfg(not(target_arch = "wasm32"))]
mod store;
mod templates;
mod text;
mod ui;
#[cfg(not(target_arch = "wasm32"))]
mod updates;
#[cfg(target_arch = "wasm32")]
mod guest;

/// Where boards live: the folder of the first Tratto (Electron's userData), so they carry over.
#[cfg(not(target_arch = "wasm32"))]
fn data_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("TRATTO_DATA") {
        return dir.into();
    }
    let base = dirs::config_dir().unwrap_or_else(|| ".".into());
    let packaged = base.join("Tratto");
    let dev = base.join("tratto");
    if !packaged.join("tratto.db").exists() && dev.join("tratto.db").exists() { dev } else { packaged }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    if std::env::var_os("TRATTO_BENCH").is_some() {
        println!("T main {:.0}", platform::now_ms());
    }
    let store = match store::Store::open(&data_dir().join("tratto.db")) {
        Ok(s) => s,
        Err(e) => {
            rfd::MessageDialog::new().set_title("Tratto").set_description(format!("Non riesco ad aprire le lavagne: {e}")).show();
            std::process::exit(1);
        }
    };
    let icon = assets::rasterize_svg(assets::LOGO, 256).map(|p| egui::IconData { rgba: p.data().to_vec(), width: 256, height: 256 });
    let mut viewport = egui::ViewportBuilder::default().with_title("Tratto").with_app_id("tratto").with_inner_size([1440.0, 900.0]).with_min_inner_size([960.0, 640.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        // A whiteboard does not need the gaming card of a laptop: the integrated one saves battery.
        if std::env::var_os("WGPU_POWER_PREF").is_none() {
            setup.power_preference = eframe::wgpu::PowerPreference::LowPower;
        }
        // On Linux just asking Vulkan for its cards wakes a sleeping NVIDIA card (2 s before the
        // first frame); OpenGL stays on the integrated one.
        #[cfg(target_os = "linux")]
        if std::env::var_os("WGPU_BACKEND").is_none() {
            setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
    }
    let options = eframe::NativeOptions {
        wgpu_options,
        viewport,
        multisampling: 4,
        #[cfg(windows)]
        event_loop_builder: Some(Box::new(|b| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            b.with_msg_hook(pen::hook);
        })),
        ..Default::default()
    };
    if std::env::var_os("TRATTO_BENCH").is_some() {
        println!("T run_native {:.0}", platform::now_ms());
    }
    eframe::run_native("Tratto", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, store)))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    guest::start();
}
