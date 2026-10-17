#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod audio;
mod code;
mod doc;
mod editor;
mod export;
mod geom;
mod ink;
#[cfg(target_os = "linux")]
mod install;
mod model;
mod music;
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
mod style;
mod table;
mod templates;
mod text;
mod ui;
#[cfg(not(target_arch = "wasm32"))]
mod updates;
mod widgets;
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
    let dir = data_dir();
    let log_path = dir.join("startup.log");
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&log_path, format!("Panic:\n{info}\n"));
        default_hook(info);
    }));
    let store = match store::Store::open(&data_dir().join("tratto.db")) {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::write(data_dir().join("startup.log"), format!("Store error: {e}\n"));
            rfd::MessageDialog::new().set_title("Tratto").set_description(format!("Non riesco ad aprire le lavagne: {e}")).show();
            std::process::exit(1);
        }
    };
    #[cfg(target_os = "linux")]
    std::thread::spawn(install::refresh);
    let icon = assets::rasterize_svg(assets::LOGO, 256).map(|p| egui::IconData { rgba: p.data().to_vec(), width: 256, height: 256 });
    let make_viewport = || {
        let mut viewport = egui::ViewportBuilder::default().with_title("Tratto").with_app_id("tratto").with_inner_size([1440.0, 900.0]).with_min_inner_size([800.0, 540.0]);
        if let Some(icon) = icon.clone() {
            viewport = viewport.with_icon(icon);
        }
        viewport
    };
    let make_options = |renderer: eframe::Renderer, multisampling: u16, all_backends: bool| {
        let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
            // A whiteboard does not need the gaming card of a laptop: the integrated one saves battery.
            if std::env::var_os("WGPU_POWER_PREF").is_none() {
                setup.power_preference = eframe::wgpu::PowerPreference::LowPower;
            }
            if all_backends {
                setup.instance_descriptor.backends = eframe::wgpu::Backends::all();
            }
            // On Linux just asking Vulkan for its cards wakes a sleeping NVIDIA card (2 s before the
            // first frame); OpenGL stays on the integrated one.
            #[cfg(target_os = "linux")]
            if std::env::var_os("WGPU_BACKEND").is_none() && !all_backends {
                setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
            }
        }
        eframe::NativeOptions {
            renderer,
            wgpu_options,
            viewport: make_viewport(),
            multisampling,
            #[cfg(windows)]
            event_loop_builder: Some(Box::new(|b| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                b.with_msg_hook(pen::hook);
            })),
            ..Default::default()
        }
    };
    if std::env::var_os("TRATTO_BENCH").is_some() {
        println!("T run_native {:.0}", platform::now_ms());
    }
    let use_glow = std::env::var("TRATTO_RENDERER").is_ok_and(|v| v.eq_ignore_ascii_case("glow") || v.eq_ignore_ascii_case("gl"));
    let first = if use_glow {
        make_options(eframe::Renderer::Glow, 0, true)
    } else {
        make_options(eframe::Renderer::Wgpu, 4, false)
    };
    let s1 = store.clone();
    let res = eframe::run_native("Tratto", first, Box::new(move |cc| Ok(Box::new(app::App::new(cc, s1)))));
    let Err(e1) = res else { return Ok(()) };
    // Fallback 1: Wgpu without multisampling and with all backends (including GL/DX12/Vulkan)
    if !use_glow {
        let s2 = store.clone();
        if eframe::run_native("Tratto", make_options(eframe::Renderer::Wgpu, 1, true), Box::new(move |cc| Ok(Box::new(app::App::new(cc, s2))))).is_ok() {
            return Ok(());
        }
        // Fallback 2: Glow (OpenGL) for older integrated GPUs without working Vulkan/DX12 drivers
        let s3 = store.clone();
        if eframe::run_native("Tratto", make_options(eframe::Renderer::Glow, 0, true), Box::new(move |cc| Ok(Box::new(app::App::new(cc, s3))))).is_ok() {
            return Ok(());
        }
    }
    let _ = std::fs::write(data_dir().join("startup.log"), format!("Graphics init error: {e1}\n"));
    rfd::MessageDialog::new()
        .set_title("Tratto")
        .set_description(format!("Impossibile avviare l'interfaccia grafica di Tratto:\n{e1}"))
        .show();
    Err(e1)
}

#[cfg(target_arch = "wasm32")]
fn main() {
    guest::start();
}
