#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(dead_code, unused_variables)]

mod app;
mod assets;
mod audio;
mod brush;
mod code;
#[cfg(not(target_arch = "wasm32"))]
mod diag;
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

/// The ways of drawing the window. Graphics drivers fail in their own ways (an error, a crash,
/// or a window that never appears because the driver hangs), so they are tried in turn until
/// one shows its first frame; the one that worked is remembered for the next start.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Gfx {
    /// wgpu as the system prefers it (Mac: Metal; Linux: OpenGL on the integrated card, as
    /// Vulkan would wake a sleeping NVIDIA card for 2 s before the first frame).
    Wgpu,
    /// wgpu with every API it knows, without multisampling.
    WgpuAny,
    /// Direct3D 12 on a real graphics card (the software one comes last).
    Dx12,
    Vulkan,
    /// OpenGL, with multisampling and then without (some old cards have no such mode).
    Gl,
    GlPlain,
    /// Direct3D 12 drawn by the processor (WARP): present on every Windows 10 and 11, slower.
    Software,
}

/// Windows: OpenGL first. It only wakes the card the screen is on, so the window shows in a
/// tenth of a second (Direct3D 12 and Vulkan look at every card, waking a laptop's gaming card:
/// one or two seconds), and frames come as evenly.
#[cfg(windows)]
const CHAIN: &[Gfx] = &[Gfx::Gl, Gfx::Dx12, Gfx::Vulkan, Gfx::GlPlain, Gfx::Software];
#[cfg(target_os = "linux")]
const CHAIN: &[Gfx] = &[Gfx::Wgpu, Gfx::WgpuAny, Gfx::Gl, Gfx::GlPlain];
#[cfg(target_os = "macos")]
const CHAIN: &[Gfx] = &[Gfx::Wgpu, Gfx::Gl, Gfx::GlPlain];

/// How long the window may take to show its first frame before the next way is tried. A
/// normal start takes well under a second, a slow disk a few.
#[cfg(not(target_arch = "wasm32"))]
const WATCHDOG: std::time::Duration = std::time::Duration::from_secs(12);

/// Whether the desktop can be on screen: not while the computer is locked, a permission prompt
/// covers it or the screen saver runs. A window can't show then, and that's no fault of the
/// graphics driver: that time doesn't count towards the wait.
#[cfg(windows)]
fn desktop_on_screen() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, DESKTOP_SWITCHDESKTOP, OpenInputDesktop, SwitchDesktop};
    // SAFETY: plain calls on a handle this function opens and closes; switching to the desktop
    // that already has the screen changes nothing, and fails when another one has it.
    unsafe {
        let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if desk.is_null() {
            return false;
        }
        let on = SwitchDesktop(desk) != 0;
        CloseDesktop(desk);
        on
    }
}

#[cfg(all(not(windows), not(target_arch = "wasm32")))]
fn desktop_on_screen() -> bool {
    true
}

#[cfg(not(target_arch = "wasm32"))]
impl Gfx {
    const ALL: [Gfx; 7] = [Gfx::Wgpu, Gfx::WgpuAny, Gfx::Dx12, Gfx::Vulkan, Gfx::Gl, Gfx::GlPlain, Gfx::Software];

    fn name(self) -> &'static str {
        match self {
            Gfx::Wgpu => "wgpu",
            Gfx::WgpuAny => "wgpu-any",
            Gfx::Dx12 => "dx12",
            Gfx::Vulkan => "vulkan",
            Gfx::Gl => "gl",
            Gfx::GlPlain => "gl-plain",
            Gfx::Software => "software",
        }
    }

    fn from_name(s: &str) -> Option<Gfx> {
        let s = s.trim().to_ascii_lowercase();
        if s == "glow" {
            return Some(Gfx::Gl);
        }
        Gfx::ALL.into_iter().find(|g| g.name() == s)
    }

    fn options(self, viewport: egui::ViewportBuilder) -> eframe::NativeOptions {
        use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};
        use eframe::wgpu;
        let mut wgpu_options = WgpuConfiguration::default();
        if let WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
            // A whiteboard does not need the gaming card of a laptop: the integrated one saves battery.
            let high = std::env::var("WGPU_POWER_PREF").is_ok_and(|v| v.eq_ignore_ascii_case("high"));
            setup.power_preference = if high { wgpu::PowerPreference::HighPerformance } else { wgpu::PowerPreference::LowPower };
            let pick = move |software: bool| -> eframe::egui_wgpu::NativeAdapterSelectorMethod {
                std::sync::Arc::new(move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
                    let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
                        wgpu::DeviceType::IntegratedGpu => u8::from(high),
                        wgpu::DeviceType::DiscreteGpu => u8::from(!high),
                        _ => 2,
                    };
                    adapters
                        .iter()
                        .filter(|a| (a.get_info().device_type == wgpu::DeviceType::Cpu) == software && surface.is_none_or(|s| a.is_surface_supported(s)))
                        .min_by_key(|a| rank(a))
                        .cloned()
                        .ok_or_else(|| {
                            let seen: Vec<String> = adapters.iter().map(|a| format!("{} ({:?})", a.get_info().name, a.get_info().device_type)).collect();
                            format!("nessuna scheda adatta tra: {}", if seen.is_empty() { "nessuna".into() } else { seen.join(", ") })
                        })
                })
            };
            let backends = &mut setup.instance_descriptor.backends;
            match self {
                Gfx::Wgpu => {
                    #[cfg(target_os = "linux")]
                    if std::env::var_os("WGPU_BACKEND").is_none() {
                        *backends = wgpu::Backends::GL;
                    }
                }
                Gfx::WgpuAny => *backends = wgpu::Backends::all(),
                Gfx::Dx12 | Gfx::Software => {
                    *backends = wgpu::Backends::DX12;
                    setup.native_adapter_selector = Some(pick(self == Gfx::Software));
                }
                Gfx::Vulkan => {
                    *backends = wgpu::Backends::VULKAN;
                    setup.native_adapter_selector = Some(pick(false));
                }
                Gfx::Gl | Gfx::GlPlain => {}
            }
        }
        let (renderer, multisampling) = match self {
            Gfx::Gl => (eframe::Renderer::Glow, 4),
            Gfx::GlPlain => (eframe::Renderer::Glow, 0),
            Gfx::WgpuAny | Gfx::Software => (eframe::Renderer::Wgpu, 1),
            _ => (eframe::Renderer::Wgpu, 4),
        };
        eframe::NativeOptions {
            renderer,
            wgpu_options,
            viewport,
            multisampling,
            #[cfg(windows)]
            event_loop_builder: Some(Box::new(|b| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                b.with_msg_hook(pen::hook);
            })),
            ..Default::default()
        }
    }
}

/// Ends the process at once, even with the main thread stuck inside a graphics driver.
#[cfg(not(target_arch = "wasm32"))]
fn quit(code: i32) -> ! {
    #[cfg(windows)]
    // SAFETY: ends this very process; nothing is left half-written (the boards are in SQLite,
    // which survives this, and the log is closed after every line).
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        TerminateProcess(GetCurrentProcess(), code as u32);
    }
    std::process::exit(code)
}

#[cfg(not(target_arch = "wasm32"))]
fn error_box(text: &str) {
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("Tratto").set_description(text).show();
}

/// This way of drawing failed before the window appeared: Tratto starts again with the next,
/// or, when none is left, says so and how to get help.
#[cfg(not(target_arch = "wasm32"))]
fn next_gfx(attempt: usize, forced: bool, why: &str) -> ! {
    if !forced && attempt + 1 < CHAIN.len() {
        diag::log(format!("Riavvio con il modo di disegno successivo ({} di {})", attempt + 2, CHAIN.len()));
        if let Ok(exe) = std::env::current_exe()
            && std::process::Command::new(exe).arg("--gfx-attempt").arg((attempt + 1).to_string()).spawn().is_ok()
        {
            quit(0);
        }
        diag::log("Non riesco a riavviare Tratto");
    }
    diag::log("Nessun modo di disegno ha funzionato");
    error_box(&format!(
        "Tratto non riesce ad aprire la finestra su questo computer.\n\nUltimo errore: {why}\n\nAggiorna i driver della scheda video (dal sito di Intel, AMD o NVIDIA, o da Windows Update) e riprova. Se non basta, manda questo file a chi ti ha dato Tratto:\n{}",
        diag::log_path(&data_dir()).display()
    ));
    quit(1)
}

#[cfg(not(target_arch = "wasm32"))]
fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "errore imprevisto".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    if std::env::var_os("TRATTO_BENCH").is_some() {
        println!("T main {:.0}", platform::now_ms());
    }
    let dir = data_dir();
    diag::init(&dir);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        diag::log(format!("Errore: {info}"));
        default_hook(info);
    }));
    let store = match store::Store::open(&dir.join("tratto.db")) {
        Ok(s) => s,
        Err(e) => {
            diag::log(format!("Non riesco ad aprire le lavagne: {e}"));
            error_box(&format!("Non riesco ad aprire le lavagne: {e}"));
            std::process::exit(1);
        }
    };
    #[cfg(target_os = "linux")]
    std::thread::spawn(install::refresh);

    // Which way of drawing: the one asked for (TRATTO_RENDERER), or the chain from the one that
    // worked last, moved along by the restarts after a failure.
    let mut args = std::env::args().skip(1);
    let mut attempt = 0;
    while let Some(a) = args.next() {
        if a == "--gfx-attempt" {
            attempt = args.next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(0).min(CHAIN.len() - 1);
        }
    }
    let forced = std::env::var("TRATTO_RENDERER").ok().and_then(|v| Gfx::from_name(&v));
    let start = diag::remembered_gfx(&dir).and_then(|n| Gfx::from_name(&n)).and_then(|g| CHAIN.iter().position(|c| *c == g)).unwrap_or(0);
    let gfx = forced.unwrap_or(CHAIN[(start + attempt) % CHAIN.len()]);
    diag::log(format!("Modo di disegno: {}{}", gfx.name(), if forced.is_some() { " (scelto con TRATTO_RENDERER)" } else { "" }));
    diag::set_gfx(gfx.name(), forced.is_none());
    if forced.is_none() {
        std::thread::spawn(move || {
            let step = std::time::Duration::from_millis(100);
            let mut waited = std::time::Duration::ZERO;
            while waited < WATCHDOG {
                if diag::shown() {
                    return;
                }
                std::thread::sleep(step);
                if desktop_on_screen() {
                    waited += step;
                }
            }
            diag::log(format!("Dopo {} s la finestra non è ancora comparsa ({})", WATCHDOG.as_secs(), gfx.name()));
            next_gfx(attempt, false, "la finestra non compare (il driver della scheda video non risponde)");
        });
    }

    let icon = assets::rasterize_svg(assets::LOGO, 256).map(|p| egui::IconData { rgba: p.data().to_vec(), width: 256, height: 256 });
    let mut viewport = egui::ViewportBuilder::default().with_title("Tratto").with_app_id("tratto").with_inner_size([1440.0, 900.0]).with_min_inner_size([800.0, 540.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    if std::env::var_os("TRATTO_BENCH").is_some() {
        println!("T run_native {:.0}", platform::now_ms());
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| eframe::run_native("Tratto", gfx.options(viewport), Box::new(move |cc| Ok(Box::new(app::App::new(cc, store)))))));
    let why = match result {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(e)) => e.to_string(),
        Err(p) => panic_text(&*p),
    };
    if diag::shown() {
        // It had opened: something broke while in use.
        diag::log(format!("Chiuso per un errore: {why}"));
        error_box(&format!("Tratto si è chiuso per un errore imprevisto:\n{why}\n\nI dettagli sono in {}", diag::log_path(&dir).display()));
        quit(1);
    }
    diag::log(format!("{} non funziona: {why}", gfx.name()));
    next_gfx(attempt, forced.is_some(), &why)
}

#[cfg(target_arch = "wasm32")]
fn main() {
    guest::start();
}
