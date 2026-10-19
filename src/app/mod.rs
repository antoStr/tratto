//! The desktop app: the board list and the open board, with theme, settings and messages.

pub mod board;
pub mod context_bar;
pub mod dialogs;
#[cfg(not(target_arch = "wasm32"))]
pub mod home;
pub mod live;
pub mod panels;
pub mod toolbar;

use egui::{Color32, Id, RichText};

use crate::prefs::{self, Prefs};
#[cfg(not(target_arch = "wasm32"))]
use crate::store::Store;
use crate::ui::{self, Theme};

/* ---------------- messages ---------------- */

pub struct Toast {
    text: String,
    error: bool,
    born: f64,
    until: f64,
}

/// Short messages at the bottom of the window.
#[derive(Default)]
pub struct Toasts(Vec<Toast>);

impl Toasts {
    pub fn info(&mut self, text: impl Into<String>) {
        let now = crate::platform::now_ms();
        self.0.push(Toast { text: text.into(), error: false, born: now, until: now + 3500.0 });
    }
    pub fn error(&mut self, text: impl Into<String>) {
        let now = crate::platform::now_ms();
        self.0.push(Toast { text: text.into(), error: true, born: now, until: now + 6000.0 });
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        let now = crate::platform::now_ms();
        self.0.retain(|t| t.until > now);
        if self.0.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        let t = ui::theme(ctx);
        // Each message rises in on a spring, the older ones glide up to make room, and it fades
        // away at the end.
        let mut y = 0.0;
        for toast in self.0.iter().rev().take(3) {
            let id = Id::new(("toast", toast.born.to_bits()));
            let k = ui::motion::appear(ctx, id, true);
            let out = ((toast.until - now) / 220.0).clamp(0.0, 1.0) as f32;
            let lift = ui::motion::spring(ctx, id.with("y"), y, 0.32, 0.86);
            let resp = egui::Area::new(id).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -88.0 - lift + ui::motion::travel(ctx, k) * 16.0]).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
                ui.set_opacity((k * out).clamp(0.0, 1.0));
                ui::menu_frame(&t).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if toast.error {
                            ui::icon(ui, "x", ui.cursor().min + egui::vec2(7.0, 8.0), 14.0, Color32::from_rgb(0xFF, 0x7A, 0x5E));
                            ui.add_space(18.0);
                        }
                        ui.label(RichText::new(&toast.text).color(t.menu_text));
                    });
                });
            });
            y += resp.response.rect.height() + 6.0;
        }
    }
}

/// Theme, text size and interface zoom from the preferences; `applied` remembers what is set.
pub fn apply_theme(ctx: &egui::Context, p: &Prefs, applied: &mut Option<(Theme, f32)>) {
    let dark = match p.theme {
        prefs::Theme::Dark => true,
        prefs::Theme::Light => false,
        prefs::Theme::System => crate::platform::system_dark(ctx),
    };
    let theme = Theme::new(dark, p.accent(), p.high_contrast);
    let key = (theme, p.text_scale);
    if *applied != Some(key) {
        ui::setup(ctx, theme, p.text_scale);
        *applied = Some(key);
    }
    let zoom = p.ui_scale.clamp(0.75, 2.0);
    if (ctx.zoom_factor() - zoom).abs() > 1e-3 {
        ctx.set_zoom_factor(zoom);
    }
    ui::motion::set_reduced(ctx, p.reduce_motion);
}

/* ---------------- the app ---------------- */

#[cfg(not(target_arch = "wasm32"))]
pub enum Screen {
    Home(home::Home),
    Board(Box<board::BoardScreen>),
}

#[cfg(not(target_arch = "wasm32"))]
pub struct App {
    pub prefs: Prefs,
    saved_prefs: String,
    pub store: Store,
    screen: Screen,
    pub toasts: Toasts,
    pub dialogs: dialogs::Dialogs,
    applied: Option<(Theme, f32)>,
    last_save: f64,
    bench: Option<Bench>,
    /// Whether the window had the focus last frame (the tablet is told when it comes back).
    focused: Option<bool>,
    /// TRATTO_TRACE=file.csv: a line per frame (how long it took, what was on screen, the stroke
    /// being drawn), to find where it slows down on someone's computer.
    trace: Option<Trace>,
}

#[cfg(not(target_arch = "wasm32"))]
struct Trace {
    out: std::io::BufWriter<std::fs::File>,
    start: f64,
    last: f64,
}

/// Measurements against the first Tratto (TRATTO_BENCH=1): when the first frame is drawn and,
/// with TRATTO_OPEN=<board> TRATTO_PAN=1, frame times while panning across that board.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Bench {
    frame: u32,
    open: Option<String>,
    pan: bool,
    ui_ms: Vec<f64>,
    gaps: Vec<f64>,
    last: f64,
    vertices: usize,
}

#[cfg(not(target_arch = "wasm32"))]
fn percentile(v: &[f64], p: f64) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v.get(((v.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0)
}

/// What a screen asks of the app.
pub enum Action {
    Open { id: String, template: Option<String> },
    New { template: Option<String> },
    Home,
}

#[cfg(not(target_arch = "wasm32"))]
impl App {
    pub fn new(cc: &eframe::CreationContext, store: Store) -> App {
        if std::env::var_os("TRATTO_BENCH").is_some() {
            println!("T app_new {:.0}", crate::platform::now_ms());
        }
        // The graphics card and driver in use, for the start-up log.
        if let Some(rs) = &cc.wgpu_render_state {
            let i = rs.adapter.get_info();
            crate::diag::log(format!("Scheda video: {} ({:?}, {:?}, driver {} {})", i.name, i.backend, i.device_type, i.driver, i.driver_info));
        }
        if let Some(gl) = &cc.gl {
            use eframe::glow::HasContext;
            // SAFETY: plain queries on the context eframe just made current.
            let (renderer, version) = unsafe { (gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION)) };
            crate::diag::log(format!("Scheda video: {renderer} (OpenGL {version})"));
        }
        ui::install_fonts(&cc.egui_ctx);
        #[cfg(target_os = "linux")]
        crate::platform::watch_color_scheme(cc.egui_ctx.clone());
        // A new version shows up on the home screen (and in Settings ▸ Updates).
        if std::env::var_os("TRATTO_BENCH").is_none() {
            crate::updates::check(&cc.egui_ctx);
        }
        let prefs = store.setting("prefs").map(|s| Prefs::from_json(&s)).unwrap_or_default();
        let saved_prefs = prefs.to_json();
        let home = home::Home::new(&store);
        let bench = std::env::var_os("TRATTO_BENCH").map(|_| Bench { open: std::env::var("TRATTO_OPEN").ok(), pan: std::env::var_os("TRATTO_PAN").is_some(), ..Default::default() });
        let trace = std::env::var_os("TRATTO_TRACE").and_then(|p| std::fs::File::create(p).ok()).map(|f| {
            use std::io::Write;
            let mut out = std::io::BufWriter::new(f);
            let _ = writeln!(out, "t_ms,gap_ms,ui_ms,vertices,indices,elements,live_points,events,zoom");
            let now = crate::platform::now_ms();
            Trace { out, start: now, last: now }
        });
        App { prefs, saved_prefs, store, screen: Screen::Home(home), toasts: Toasts::default(), dialogs: dialogs::Dialogs::default(), applied: None, last_save: 0.0, bench, focused: None, trace }
    }

    /// The preferences in use: the open board keeps its own copy while it is open.
    fn prefs(&self) -> &Prefs {
        match &self.screen {
            Screen::Board(b) => &b.editor.prefs,
            Screen::Home(_) => &self.prefs,
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        let p = self.prefs().clone();
        apply_theme(ctx, &p, &mut self.applied);
    }

    fn save_prefs(&mut self, force: bool) {
        let now = crate::platform::now_ms();
        if !force && now - self.last_save < 1000.0 {
            return;
        }
        self.last_save = now;
        let json = self.prefs().to_json();
        if json != self.saved_prefs {
            if let Err(e) = self.store.set_setting("prefs", &json) {
                self.toasts.error(e);
            }
            self.saved_prefs = json;
        }
    }

    fn act(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Home => {
                if let Screen::Board(b) = &mut self.screen {
                    b.close();
                    self.prefs = b.editor.prefs.clone();
                }
                self.screen = Screen::Home(home::Home::new(&self.store));
            }
            Action::New { template } => {
                let title = template.as_deref().and_then(|t| crate::templates::TEMPLATES.iter().find(|x| x.id == t)).map_or("Lavagna senza titolo", |t| t.name);
                match self.store.create(title, None) {
                    Ok(id) => self.act(ctx, Action::Open { id, template }),
                    Err(e) => self.toasts.error(e),
                }
            }
            Action::Open { id, template } => {
                if let Screen::Board(b) = &mut self.screen {
                    b.close();
                    self.prefs = b.editor.prefs.clone();
                }
                match board::BoardScreen::open(ctx, &self.store, &id, self.prefs.clone(), template) {
                    Ok(b) => self.screen = Screen::Board(Box::new(b)),
                    Err(e) => {
                        self.toasts.error(e);
                        self.screen = Screen::Home(home::Home::new(&self.store));
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::diag::frame(&ctx);
        // A drawing tablet's pressure through Wintab (Windows Ink turned off in its driver):
        // opened once the window is on screen, and told when the window comes back to the front.
        let focused = ctx.input(|i| i.focused);
        if crate::diag::shown() && self.focused != Some(focused) {
            #[cfg(windows)]
            {
                use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if self.focused.is_none()
                    && let Ok(h) = frame.window_handle()
                    && let RawWindowHandle::Win32(w) = h.as_raw()
                {
                    crate::pen::wintab_open(w.hwnd.get(), &crate::data_dir());
                }
                crate::pen::wintab_focus(focused);
            }
            #[cfg(not(windows))]
            let _ = &frame;
            self.focused = Some(focused);
        }
        let t0 = crate::platform::now_ms();
        if let Some(open) = self.bench.as_mut().and_then(|b| {
            if b.frame == 0 {
                println!("FIRST_FRAME {t0:.0}");
            }
            b.frame += 1;
            b.open.take()
        }) {
            self.act(&ctx, Action::Open { id: open, template: None });
        }
        self.apply_theme(&ctx);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.dialogs.settings = true;
        }
        let action = match &mut self.screen {
            Screen::Home(h) => h.ui(ui, &self.store, &mut self.prefs, &mut self.toasts, &mut self.dialogs),
            Screen::Board(b) => b.ui(ui, &mut self.toasts, &mut self.dialogs),
        };
        let prefs = match &mut self.screen {
            Screen::Board(b) => &mut b.editor.prefs,
            Screen::Home(_) => &mut self.prefs,
        };
        dialogs::settings(&ctx, &mut self.dialogs.settings, prefs, true);
        if let Some(a) = action {
            self.act(&ctx, a);
        }
        self.toasts.show(&ctx);
        self.save_prefs(false);
        if let (Some(bench), Screen::Board(b)) = (&mut self.bench, &mut self.screen)
            && bench.pan
            && bench.frame > 120
        {
            // Two seconds to settle, then 600 frames sliding sideways across the board.
            if bench.ui_ms.len() < 600 {
                let now = crate::platform::now_ms();
                if bench.last > 0.0 {
                    bench.gaps.push(now - bench.last);
                }
                bench.last = now;
                bench.ui_ms.push(now - t0);
                bench.vertices = bench.vertices.max(b.editor.painter.last_len.0);
                let c = b.editor.cam;
                b.editor.cam = crate::geom::Camera { x: c.x - 4.0, ..c };
                ctx.request_repaint();
            } else {
                let fps = 1000.0 * bench.gaps.len() as f64 / bench.gaps.iter().sum::<f64>();
                println!("PAN frames={} fps={fps:.1} frame_ms_p50={:.2} p95={:.2} p99={:.2} ui_ms_p50={:.2} p95={:.2} vertices={}", bench.gaps.len(), percentile(&bench.gaps, 0.5), percentile(&bench.gaps, 0.95), percentile(&bench.gaps, 0.99), percentile(&bench.ui_ms, 0.5), percentile(&bench.ui_ms, 0.95), bench.vertices);
                bench.pan = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.bench.as_ref().is_some_and(|b| b.pan) {
            ctx.request_repaint();
        }
        if let Some(tr) = &mut self.trace {
            use std::io::Write;
            let now = crate::platform::now_ms();
            let events = ctx.input(|i| i.events.len());
            let (verts, idx, els, live, zoom) = match &mut self.screen {
                Screen::Board(b) => (b.editor.painter.last_len.0, b.editor.painter.last_len.1, b.editor.board.len(), b.editor.live_points(), b.editor.cam.z),
                Screen::Home(_) => (0, 0, 0, 0, 1.0),
            };
            let _ = writeln!(tr.out, "{:.1},{:.2},{:.2},{verts},{idx},{els},{live},{events},{zoom:.3}", now - tr.start, now - tr.last, now - t0);
            tr.last = now;
            let _ = tr.out.flush();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Screen::Board(b) = &mut self.screen {
            b.close();
        }
        self.save_prefs(true);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgb(0x2C, 0x2C, 0x2C).to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod picture {
    use super::*;

    /// The benchmark board in a new database: `TRATTO_BENCH_DB=dir/tratto.db cargo test make_bench_db -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn make_bench_db() {
        let path = std::env::var("TRATTO_BENCH_DB").expect("TRATTO_BENCH_DB");
        let store = Store::open(std::path::Path::new(&path)).unwrap();
        let doc = yrs::Doc::new();
        let mut b = crate::doc::Board::new(doc.clone(), std::sync::Arc::new(|| {}));
        b.put(crate::templates::bench());
        let id = store.create("Benchmark", Some(&crate::doc::encode_state(&doc))).unwrap();
        println!("BOARD {id}");
    }

    /// A board with every brush in a new database, to look at in the real app:
    /// `TRATTO_BENCH_DB=dir/tratto.db cargo test make_brush_db -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn make_brush_db() {
        let path = std::env::var("TRATTO_BENCH_DB").expect("TRATTO_BENCH_DB");
        let store = Store::open(std::path::Path::new(&path)).unwrap();
        let doc = yrs::Doc::new();
        let mut b = crate::doc::Board::new(doc.clone(), std::sync::Arc::new(|| {}));
        for (row, (brush, _)) in crate::model::BRUSHES.iter().enumerate() {
            for col in 0..2 {
                let size = if *brush == crate::model::Brush::Airbrush { 36.0 } else { 10.0 };
                let pts: Vec<f32> = (0..=160)
                    .flat_map(|i| {
                        let t = i as f32 / 160.0;
                        [t * 420.0, 24.0 + 22.0 * (t * std::f32::consts::TAU * 1.5).sin(), if col == 0 { (t * std::f32::consts::PI).sin().max(0.0).powf(0.7) } else { -1.0 }]
                    })
                    .collect();
                let mut el = crate::model::El::new(crate::model::Kind::Ink(crate::model::Ink { colors: Vec::new(), points: pts, color: "#1E1E1E".into(), size, brush: Some(*brush) }));
                (el.x, el.y, el.w, el.h, el.z) = (col as f64 * 480.0, row as f64 * 90.0, 420.0, 48.0, (row * 2 + col) as f64);
                b.put([el]);
            }
        }
        let id = store.create("Pennelli", Some(&crate::doc::encode_state(&doc))).unwrap();
        println!("BOARD {id}");
    }

    /// Interface text size of the pictures (TRATTO_TEXT=1.36 for "Molto grande").
    fn text_scale() -> f32 {
        std::env::var("TRATTO_TEXT").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0)
    }

    /// How long closing a big board and typing in it take:
    /// `TRATTO_BENCH_DB=dir/tratto.db cargo test --release time_close -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn time_close() {
        let path = std::env::var("TRATTO_BENCH_DB").expect("TRATTO_BENCH_DB");
        let store = Store::open(std::path::Path::new(&path)).unwrap();
        let id = store.boards().unwrap()[0].id.clone();
        let ctx = egui::Context::default();
        let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap();
        println!("elements {}", b.editor.board.len());
        let mut s = crate::model::El::new(crate::model::Kind::Sticky(crate::model::Sticky::default()));
        (s.w, s.h) = (220.0, 220.0);
        let sid = s.id.clone();
        b.editor.board.put([s]);
        let t = crate::platform::now_ms();
        for i in 0..20 {
            b.editor.set_text(&sid, "x".repeat(i + 1));
            b.editor.board.refresh();
        }
        println!("typing: {:.2} ms a key", (crate::platform::now_ms() - t) / 20.0);
        let t = crate::platform::now_ms();
        let els: Vec<std::sync::Arc<crate::model::El>> = b.editor.board.paint_order().to_vec();
        let _ = crate::export::thumbnail(&els, &b.editor.board.meta(), &|_| None);
        println!("thumbnail: {:.0} ms", crate::platform::now_ms() - t);
        let t = crate::platform::now_ms();
        let state = crate::doc::encode_state(&b.editor.board.doc);
        let enc = crate::platform::now_ms() - t;
        store.save_ydoc(&id, &state).unwrap();
        println!("save: encode {enc:.0} ms, write {:.0} ms ({} KB)", crate::platform::now_ms() - t - enc, state.len() / 1024);
        let t = crate::platform::now_ms();
        b.close();
        println!("close: {:.0} ms", crate::platform::now_ms() - t);
    }

    /// Device pixels per point of the pictures (TRATTO_PPP=1.3 to see 130%).
    fn ppp() -> f32 {
        std::env::var("TRATTO_PPP").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0)
    }

    /// The board list as a picture: `TRATTO_SHOT=out.png cargo test home_picture`. With
    /// TRATTO_SETTINGS=0…3 the settings are open on that section.
    #[test]
    fn home_picture() {
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let dark = std::env::var("TRATTO_DARK").is_ok();
        let settings: Option<usize> = std::env::var("TRATTO_SETTINGS").ok().and_then(|v| v.parse().ok());
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        for name in ["Riunione di lunedì", "Idee per il progetto", "Mappa del sito", "Retrospettiva"] {
            store.create(name, None).unwrap();
        }
        let mut home: Option<home::Home> = None;
        let (mut toasts, mut dialogs, mut prefs) = (Toasts::default(), dialogs::Dialogs::default(), Prefs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(std::env::var("TRATTO_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some(egui::vec2(w.parse().ok()?, h.parse().ok()?)))).unwrap_or(egui::vec2(1440.0, 900.0))).with_pixels_per_point(ppp()).wgpu().build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if home.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(dark, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), text_scale());
                // TRATTO_UI=1.5: the interface zoomed as in Settings ▸ Accessibilità.
                if let Some(z) = std::env::var("TRATTO_UI").ok().and_then(|v| v.parse::<f32>().ok()) {
                    prefs.ui_scale = z;
                    ctx.set_zoom_factor(z);
                }
                home = Some(home::Home::new(&store));
                if let Some(tab) = settings {
                    dialogs.settings = true;
                    dialogs::open_settings_tab(&ctx, tab);
                }
                return;
            }
            home.as_mut().unwrap().ui(ui, &store, &mut prefs, &mut toasts, &mut dialogs);
            dialogs::settings(&ctx, &mut dialogs.settings, &mut prefs, true);
        });
        harness.run_steps(60);
        // TRATTO_DRAG=x0,y0,x1,y1: a drag from one point to the other, still held.
        if let Some(v) = std::env::var("TRATTO_DRAG").ok() {
            let n: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            let (a, b) = (egui::pos2(n[0], n[1]), egui::pos2(n[2], n[3]));
            harness.event(egui::Event::PointerMoved(a));
            harness.run_steps(2);
            harness.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
            harness.run_steps(2);
            for k in 1..=12 {
                harness.event(egui::Event::PointerMoved(a + (b - a) * (k as f32 / 12.0)));
                harness.run_steps(1);
            }
            harness.run_steps(6);
        }
        harness.render().unwrap().save(path).unwrap();
    }

    /// The toolbar list in the settings: a row dragged three places down and let go lands there.
    #[test]
    fn a_toolbar_button_dragged_in_the_settings_moves_there() {
        let shared: std::rc::Rc<std::cell::RefCell<Prefs>> = Default::default();
        let inner = shared.clone();
        let mut dialogs = dialogs::Dialogs::default();
        let mut first = true;
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if first {
                first = false;
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                dialogs.settings = true;
                dialogs::open_settings_tab(&ctx, 3);
                return;
            }
            dialogs::settings(&ctx, &mut dialogs.settings, &mut inner.borrow_mut(), true);
        });
        harness.run_steps(30);
        let (a, b) = (egui::pos2(700.0, 358.0), egui::pos2(700.0, 462.0));
        harness.event(egui::Event::PointerMoved(a));
        harness.run_steps(2);
        harness.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        harness.run_steps(2);
        for k in 1..=12 {
            harness.event(egui::Event::PointerMoved(a + (b - a) * (k as f32 / 12.0)));
            harness.run_steps(1);
        }
        harness.event(egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        harness.run_steps(4);
        let order = crate::prefs::toolbar_items(&shared.borrow().toolbar_order);
        assert_eq!(&order[..5], &["select", "draw", "pen", "highlighter", "hand"].map(String::from));
    }

    /// The timer's display takes digits like a microwave: 1, 3, 0 and Enter start 1:30 for
    /// everyone; letters are ignored.
    #[test]
    fn digits_typed_on_the_timer_start_it() {
        use egui_kittest::kittest::Queryable;
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Timer", None).unwrap();
        let shared: std::rc::Rc<std::cell::RefCell<Option<board::BoardScreen>>> = Default::default();
        let inner = shared.clone();
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let mut screen = inner.borrow_mut();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs { music_volume: 0.0, ..Prefs::default() }, None).unwrap());
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        harness.get_by_label_contains("Timer:").click();
        harness.run_steps(5);
        harness.ctx.memory_mut(|m| m.request_focus(egui::Id::new("timer-field")));
        harness.run_steps(2);
        for s in ["1", "x", "3", "0"] {
            harness.event(egui::Event::Text(s.into()));
            harness.run_steps(1);
        }
        harness.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
        harness.run_steps(3);
        let b = shared.borrow();
        let total = b.as_ref().unwrap().editor.board.timer_field("total").and_then(|v| v.as_f64());
        assert_eq!(total, Some(90_000.0));
    }

    /// "Modelli" at the top right opens the templates; clicked again, it closes them.
    #[test]
    fn the_templates_button_opens_and_closes_them() {
        use egui_kittest::kittest::Queryable;
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Prova", None).unwrap();
        let shared: std::rc::Rc<std::cell::RefCell<Option<board::BoardScreen>>> = Default::default();
        let inner = shared.clone();
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let mut screen = inner.borrow_mut();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs { music_volume: 0.0, ..Prefs::default() }, None).unwrap());
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        let shown = |h: &egui_kittest::Harness<'_>| h.ctx.data(|d| d.get_temp::<(bool, bool)>(egui::Id::new("panels-shown"))).is_some_and(|s| s.0);
        assert!(!shown(&harness));
        harness.get_by_label("Modelli").click();
        harness.run_steps(5);
        assert!(shown(&harness) && shared.borrow().as_ref().unwrap().ui.left_tab == board::LeftTab::Templates);
        harness.get_by_label("Chiudi i modelli").click();
        harness.run_steps(5);
        assert!(!shown(&harness));
    }

    /// The text tool: a click where the text goes, then the keys write in it straight away.
    #[test]
    fn a_text_placed_takes_the_keys_at_once() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Testo", None).unwrap();
        let shared: std::rc::Rc<std::cell::RefCell<Option<board::BoardScreen>>> = Default::default();
        let inner = shared.clone();
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let mut screen = inner.borrow_mut();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs { music_volume: 0.0, ..Prefs::default() }, None).unwrap());
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        shared.borrow_mut().as_mut().unwrap().editor.set_tool(crate::editor::Tool::Text);
        let at = egui::pos2(700.0, 420.0);
        harness.event(egui::Event::PointerMoved(at));
        harness.run_steps(2);
        harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        harness.run_steps(1);
        harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        harness.run_steps(3);
        for c in ["C", "i", "a", "o"] {
            harness.event(egui::Event::Text(c.into()));
            harness.run_steps(1);
        }
        harness.run_steps(3);
        let mut b = shared.borrow_mut();
        let texts: Vec<String> = b.as_mut().unwrap().editor.board.all().iter().filter_map(|e| e.text().map(|t| t.text.clone())).collect();
        assert_eq!(texts, ["Ciao"]);
    }

    /// Hold E, point at an emoji of the wheel, let go: it floats up from there. With
    /// `TRATTO_SHOT=x.png` the wheel open is saved as a picture.
    #[test]
    fn the_emote_wheel_sends_what_it_points_at() {
        let shot = std::env::var("TRATTO_SHOT").ok();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Emote", None).unwrap();
        let shared: std::rc::Rc<std::cell::RefCell<Option<board::BoardScreen>>> = Default::default();
        let inner = shared.clone();
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let builder = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0));
        let builder = if shot.is_some() { builder.wgpu() } else { builder };
        let mut harness = builder.build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let mut screen = inner.borrow_mut();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap());
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        let at = egui::pos2(600.0, 400.0);
        harness.event(egui::Event::PointerMoved(at));
        harness.run_steps(2);
        let key = |pressed| egui::Event::Key { key: egui::Key::E, physical_key: Some(egui::Key::E), pressed, repeat: false, modifiers: Default::default() };
        harness.event(key(true));
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(400));
        // The wheel's top slot.
        let target = at + egui::vec2(0.0, -54.0);
        for k in 1..=6 {
            harness.event(egui::Event::PointerMoved(at + (target - at) * (k as f32 / 6.0)));
            harness.run_steps(1);
        }
        harness.run_steps(10);
        if let Some(path) = &shot {
            harness.render().unwrap().save(path).unwrap();
        }
        assert!(shared.borrow().as_ref().unwrap().editor.floaters.is_empty());
        harness.event(key(false));
        harness.run_steps(3);
        let b = shared.borrow();
        let ed = &b.as_ref().unwrap().editor;
        assert_eq!(ed.floaters.len(), 1, "one emote sent");
        assert_eq!(ed.floaters[0].0, crate::editor::EMOTES[0]);
        assert!(ed.emote_hold.is_none() && ed.tool != crate::editor::Tool::Eraser);
    }

    /// Writing for a long time, through the real input path: one very long stroke with each
    /// brush (frame times at its start and at its end, to see whether they grow), then a long
    /// session of strokes: `cargo test --release long_writing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn long_writing() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Scrittura lunga", None).unwrap();
        let shared: std::rc::Rc<std::cell::RefCell<Option<board::BoardScreen>>> = Default::default();
        let inner = shared.clone();
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let mut screen = inner.borrow_mut();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap();
                b.editor.tool = crate::editor::Tool::Pen;
                *screen = Some(b);
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        let press = |h: &mut egui_kittest::Harness, p: egui::Pos2, down: bool| {
            h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: egui::Modifiers::NONE });
        };
        // A scribble going back and forth over the middle of the window, as when shading.
        let at = |i: usize| {
            let t = i as f32 * 0.02;
            egui::pos2(720.0 + 300.0 * (t * 0.37).sin() + 40.0 * (t * 5.0).sin(), 450.0 + 200.0 * (t * 0.23).cos() + 40.0 * (t * 4.0).cos())
        };
        let stats = |v: &[f64]| format!("p50 {:5.2}  p95 {:5.2}  max {:5.2} ms", percentile(v, 0.5), percentile(v, 0.95), percentile(v, 1.0));
        for (brush, name) in crate::model::BRUSHES {
            shared.borrow_mut().as_mut().unwrap().editor.prefs.pens[0] = crate::prefs::Pen { brush, size: if brush == crate::model::Brush::Airbrush { 30.0 } else { 6.0 }, ..Default::default() };
            harness.input_mut().events.push(egui::Event::PointerMoved(at(0)));
            harness.step();
            press(&mut harness, at(0), true);
            // 3000 moves, four a frame (a 1000 Hz mouse at 250 frames a second).
            let mut times = Vec::new();
            for f in 0..750 {
                for k in 0..4 {
                    harness.input_mut().events.push(egui::Event::PointerMoved(at(f * 4 + k + 1)));
                }
                let t = std::time::Instant::now();
                harness.step();
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            press(&mut harness, at(3000), false);
            let t = std::time::Instant::now();
            harness.run_steps(3);
            let commit = t.elapsed().as_secs_f64() * 1000.0 / 3.0;
            println!("{name:<13} start {}  end {}  lift {commit:5.1} ms", stats(&times[..150]), stats(&times[600..]));
        }
        // A long session: 300 more strokes, then how a frame costs with all of them on screen.
        shared.borrow_mut().as_mut().unwrap().editor.prefs.pens[0] = crate::prefs::Pen::default();
        for s in 0..300 {
            let base = s * 37;
            harness.input_mut().events.push(egui::Event::PointerMoved(at(base)));
            harness.step();
            press(&mut harness, at(base), true);
            for f in 0..40 {
                for k in 0..4 {
                    harness.input_mut().events.push(egui::Event::PointerMoved(at(base + f * 4 + k + 1)));
                }
                harness.step();
            }
            press(&mut harness, at(base + 161), false);
            harness.step();
        }
        let mut times = Vec::new();
        let builds = shared.borrow_mut().as_mut().unwrap().editor.painter.builds;
        for _ in 0..60 {
            let t = std::time::Instant::now();
            harness.step();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        println!("meshes made in 60 idle frames: {}", shared.borrow_mut().as_mut().unwrap().editor.painter.builds - builds);
        let n = shared.borrow_mut().as_mut().unwrap().editor.board.len();
        let verts = shared.borrow_mut().as_mut().unwrap().editor.painter.last_len;
        println!("{n} strokes, idle frame {}, {verts:?} vertices and indices", stats(&times));
        // Where the time goes: the board's meshes alone.
        let mut b = shared.borrow_mut();
        let b = b.as_mut().unwrap();
        let els = b.editor.board.paint_order().to_vec();
        let ctx = harness.ctx.clone();
        let f = crate::paint::Frame { cam: b.editor.cam, origin: b.editor.origin, view: b.editor.view(), pixels_per_point: 1.0, editing: None, editing_cell: None, glides: None, dark_board: false };
        let mut out = Vec::new();
        let t = std::time::Instant::now();
        for _ in 0..10 {
            out.clear();
            b.editor.painter.paint(&ctx, &mut out, &els, &f);
        }
        println!("painter.paint {:5.2} ms", t.elapsed().as_secs_f64() * 100.0);
    }

    /// Frame times (the interface's own work, no drawing on the card) on a heavy board while
    /// panels open and close: `cargo test --release frame_times -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn frame_times() {
        use crate::model::{El, Ink, Kind, Sticky};
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Pesante", None).unwrap();
        let shared: std::rc::Rc<std::cell::RefCell<Option<board::BoardScreen>>> = Default::default();
        let inner = shared.clone();
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut prefs = Prefs::default();
        (prefs.left_panel, prefs.right_panel) = (false, false);
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let mut screen = inner.borrow_mut();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let mut b = board::BoardScreen::open(&ctx, &store, &id, prefs.clone(), None).unwrap();
                let mut els = Vec::new();
                for i in 0..400 {
                    let mut s = El::new(Kind::Sticky(Sticky { text: format!("Idea numero {i}: qualcosa da ricordare"), color: "#FFE58F".into(), ..Default::default() }));
                    (s.x, s.y, s.w, s.h) = ((i % 20) as f64 * 240.0, (i / 20) as f64 * 240.0, 220.0, 220.0);
                    els.push(s);
                }
                for i in 0..600 {
                    let pts: Vec<f32> = (0..150).flat_map(|k| [(k as f32 * 3.0), ((k as f32 * 0.2 + i as f32).sin() * 30.0), if i % 2 == 0 { -1.0 } else { 0.5 + 0.4 * (k as f32 * 0.1).sin() }]).collect();
                    // Every brush, and strokes of older versions.
                    let brush = crate::model::BRUSHES.get(i % 8).map(|b| b.0);
                    let size = if brush == Some(crate::model::Brush::Airbrush) { 24.0 } else { 4.0 };
                    let mut s = El::new(Kind::Ink(Ink { colors: Vec::new(), points: pts, color: "#1E1E1E".into(), size, brush }));
                    (s.x, s.y, s.w, s.h) = ((i % 30) as f64 * 160.0, 5000.0 + (i / 30) as f64 * 90.0, 450.0, 60.0);
                    els.push(s);
                }
                b.editor.board.put(els);
                *screen = Some(b);
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        let measure = |name: &str, harness: &mut egui_kittest::Harness<'_>, n: usize| {
            let mut v = Vec::new();
            for _ in 0..n {
                let t = std::time::Instant::now();
                harness.step();
                v.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            println!("{name:<22} p50 {:6.2} ms  p95 {:6.2} ms  max {:6.2} ms", percentile(&v, 0.5), percentile(&v, 0.95), percentile(&v, 1.0));
        };
        let set = |f: &dyn Fn(&mut board::BoardScreen)| f(shared.borrow_mut().as_mut().unwrap());
        set(&|b| b.editor.fit_now());
        measure("all in view", &mut harness, 30);
        for i in 0..30 {
            set(&|b| {
                let c = b.editor.cam;
                b.editor.set_cam(crate::geom::Camera { x: c.x + if i % 2 == 0 { 7.0 } else { -7.0 }, ..c });
            });
            let t = std::time::Instant::now();
            harness.step();
            if i == 29 {
                println!("panning frame        {:6.2} ms", t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        set(&|b| {
            let first = b.editor.board.all().iter().find(|e| e.sticky().is_some()).map(|e| e.id.clone()).unwrap();
            b.editor.selection = vec![first];
        });
        measure("panels closed", &mut harness, 30);
        set(&|b| b.editor.prefs.right_panel = true);
        measure("right panel opening", &mut harness, 40);
        set(&|b| b.editor.prefs.left_panel = true);
        measure("left panel opening", &mut harness, 40);
        measure("both open, idle", &mut harness, 30);
        set(&|b| b.editor.selection = b.editor.board.all().iter().take(1).map(|e| e.id.clone()).collect());
        measure("one selected", &mut harness, 30);
        set(&|b| b.editor.selection = b.editor.board.all().iter().map(|e| e.id.clone()).collect());
        measure("all selected", &mut harness, 30);
        set(&|b| {
            b.editor.selection.clear();
            b.editor.prefs.minimap = true;
        });
        measure("minimap on", &mut harness, 30);
        set(&|b| b.ui.left_tab = board::LeftTab::Templates);
        measure("templates tab", &mut harness, 10);
        // At rest nothing should keep asking for frames (that keeps the processor and the card busy).
        for (name, f) in [("idle, everything open", &(|_: &mut board::BoardScreen| {}) as &dyn Fn(&mut board::BoardScreen)), ("idle, panels closed", &|b: &mut board::BoardScreen| (b.editor.prefs.left_panel, b.editor.prefs.right_panel, b.editor.prefs.minimap) = (false, false, false))] {
            set(f);
            harness.run_steps(120);
            match harness.try_run() {
                Ok(n) => println!("{name:<22} settles after {n} frames"),
                Err(e) => println!("{name:<22} NEVER SETTLES: {e}"),
            }
        }
    }

    /// The whole board screen as a picture: `TRATTO_SHOT=out.png cargo test app::picture -- --nocapture`.
    #[test]
    fn board_screen_picture() {
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let dark = std::env::var("TRATTO_DARK").is_ok();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Diagramma di flusso", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        // TRATTO_SIZE=1300x800 for a smaller window, TRATTO_MINIMAP=br (or tl, bl) for the minimap
        // put down in the bottom right (top left, bottom left) corner.
        let size = std::env::var("TRATTO_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some(egui::vec2(w.parse().ok()?, h.parse().ok()?)))).unwrap_or(egui::vec2(1440.0, 900.0));
        let mut prefs = Prefs::default();
        match std::env::var("TRATTO_MINIMAP").as_deref() {
            Ok("br") => prefs.minimap_at = Some([1.0, 1.0]),
            Ok("tl") => prefs.minimap_at = Some([0.0, 0.0]),
            Ok("bl") => prefs.minimap_at = Some([0.0, 1.0]),
            _ => {}
        }
        // TRATTO_CLEAN=1: both side panels closed and no minimap.
        if std::env::var_os("TRATTO_CLEAN").is_some() {
            (prefs.left_panel, prefs.right_panel, prefs.minimap) = (false, false, false);
        }
        // TRATTO_CROWDED=1: everything open at once (panels, minimap, timer banner, voting,
        // presenting), to see that nothing covers anything else.
        let crowded = std::env::var_os("TRATTO_CROWDED").is_some();
        if crowded {
            (prefs.left_panel, prefs.right_panel, prefs.minimap, prefs.timer_banner) = (true, true, true, true);
        }
        // TRATTO_FOLDED=1: the tool's options folded away.
        if std::env::var_os("TRATTO_FOLDED").is_some() {
            prefs.tray = false;
        }
        // TRATTO_PANELS=1: both side panels open.
        if std::env::var_os("TRATTO_PANELS").is_some() {
            (prefs.left_panel, prefs.right_panel) = (true, true);
        }
        // TRATTO_BAR=1.2: the large toolbar.
        if let Some(b) = std::env::var("TRATTO_BAR").ok().and_then(|v| v.parse().ok()) {
            prefs.toolbar_size = b;
        }
        // TRATTO_UI=1.5 TRATTO_TEXT=1.36: the interface at its largest.
        if let Some(z) = std::env::var("TRATTO_UI").ok().and_then(|v| v.parse().ok()) {
            prefs.ui_scale = z;
        }
        // TRATTO_MAP=1: the minimap shown.
        if std::env::var_os("TRATTO_MAP").is_some() {
            prefs.minimap = true;
        }
        let mut frames = 0;
        let mut harness = egui_kittest::Harness::builder().with_size(size).with_pixels_per_point(ppp()).wgpu().build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(dark, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), text_scale());
                let mut b = board::BoardScreen::open(&ctx, &store, &id, prefs.clone(), Some("flow".into())).unwrap();
                // TRATTO_NOSEL=1: nothing selected.
                if std::env::var_os("TRATTO_NOSEL").is_none() {
                    b.editor.selection = vec![b.editor.board.all().iter().find(|e| e.shape().is_some()).unwrap().id.clone()];
                }
                b.editor.tool = if std::env::var_os("TRATTO_SELECT").is_some() { crate::editor::Tool::Select } else { crate::editor::Tool::Pen };
                b.editor.prefs.focus = std::env::var_os("TRATTO_FOCUS").is_some();
                // TRATTO_SMUDGE=1: the first pen a sfumino.
                if std::env::var_os("TRATTO_SMUDGE").is_some() {
                    b.editor.prefs.pens[0] = crate::prefs::Pen { brush: crate::model::Brush::Smudge, size: 24.0, ..Default::default() };
                }
                // TRATTO_PEEK=1: on a phone, the right panel shown over the board.
                if std::env::var_os("TRATTO_PEEK").is_some() {
                    (b.editor.prefs.right_panel, b.ui.peek_right) = (true, true);
                }
                if crowded {
                    let now = crate::platform::shared_now();
                    b.editor.board.transact_plain(|txn, m| {
                        use yrs::Map;
                        m.timer.insert(txn, "total", 300_000.0);
                        m.timer.insert(txn, "left", 300_000.0);
                        m.timer.insert(txn, "end", now + 200_000.0);
                    });
                    b.editor.board.start_voting(3, "test");
                    b.editor.presence.set("spotlight", serde_json::json!(now));
                }
                if prefs.ui_scale != 1.0 {
                    ctx.set_zoom_factor(prefs.ui_scale);
                }
                // TRATTO_PICK=code: a code block, selected. TRATTO_ZOOM=0.04: that zoom.
                // TRATTO_TOOL=shape: that tool in hand (its tray open).
                match std::env::var("TRATTO_TOOL").as_deref() {
                    Ok("shape") => b.editor.set_tool(crate::editor::Tool::Shape),
                    Ok("arrow") => b.editor.set_tool(crate::editor::Tool::Arrow),
                    Ok("eraser") => b.editor.set_tool(crate::editor::Tool::Eraser),
                    Ok("table") => b.editor.set_tool(crate::editor::Tool::Table),
                    Ok("pen") => b.editor.set_tool(crate::editor::Tool::Pen),
                    Ok("lasso") => b.editor.set_tool(crate::editor::Tool::Lasso),
                    Ok("select") => b.editor.set_tool(crate::editor::Tool::Select),
                    _ => {}
                }
                // TRATTO_VOTES=1: a vote just ended, with a few votes cast.
                if std::env::var_os("TRATTO_VOTES").is_some() {
                    let ids: Vec<String> = b.editor.board.all().iter().filter(|e| e.shape().is_some()).map(|e| e.id.clone()).collect();
                    b.editor.board.start_voting(3, &b.editor.voter.clone());
                    for (who, picks) in [("a", [0, 0, 1]), ("b", [0, 2, 3]), ("c", [1, 0, 4])] {
                        for i in picks {
                            b.editor.board.cast_vote(who, &ids[i % ids.len()], 1);
                        }
                    }
                    b.editor.board.end_voting();
                }
                // TRATTO_SHAPES=1: the shapes panel open.
                if std::env::var_os("TRATTO_SHAPES").is_some() {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("shapes-panel"), true));
                }
                // TRATTO_COMMENT=1: a new comment open, being written.
                if std::env::var_os("TRATTO_COMMENT").is_some() {
                    let mut c = crate::model::El::new(crate::model::Kind::Comment { thread: Vec::new() });
                    (c.x, c.y) = (-600.0, 300.0);
                    let cid = c.id.clone();
                    b.editor.board.put([c]);
                    b.editor.comment = Some(cid);
                }
                // TRATTO_PICK=ink: every brush drawn on the board (a pen's pressure rising and
                // falling), the pencil stroke selected.
                if std::env::var("TRATTO_PICK").is_ok_and(|v| v == "ink") {
                    let mut els = Vec::new();
                    for (row, (brush, _)) in crate::model::BRUSHES.iter().enumerate() {
                        let size = if *brush == crate::model::Brush::Airbrush { 30.0 } else { 9.0 };
                        let pts: Vec<f32> = (0..=120)
                            .flat_map(|i| {
                                let t = i as f32 / 120.0;
                                [t * 360.0, 20.0 + 18.0 * (t * std::f32::consts::TAU * 1.5).sin(), (t * std::f32::consts::PI).sin().max(0.0).powf(0.7)]
                            })
                            .collect();
                        let colors = ["#1E1E1E", "#3D3D3D", "#E03131", "#1971C2", "#2F9E44", "#6741D9", "#F76707"];
                        let mut el = crate::model::El::new(crate::model::Kind::Ink(crate::model::Ink { colors: Vec::new(), points: pts, color: colors[row].into(), size, brush: Some(*brush) }));
                        (el.x, el.y, el.w, el.h, el.z) = (-700.0 + (row % 2) as f64 * 420.0, -380.0 + (row / 2) as f64 * 110.0, 360.0, 40.0, 100.0 + row as f64);
                        els.push(el);
                    }
                    let pick = els[1].id.clone();
                    b.editor.board.put(els);
                    b.editor.selection = vec![pick];
                }
                if std::env::var("TRATTO_PICK").is_ok_and(|v| v == "line") {
                    b.editor.selection = b.editor.board.all().iter().filter(|e| e.line().is_some()).take(1).map(|e| e.id.clone()).collect();
                }
                if std::env::var("TRATTO_PICK").is_ok_and(|v| v == "code") {
                    b.editor.insert_code(None);
                    b.editor.stop_editing();
                }
                screen = Some(b);
                return;
            }
            frames += 1;
            // TRATTO_ZOOM=0.04: that zoom about the window's centre, once the board has opened.
            if frames == 10
                && let Some(z) = std::env::var("TRATTO_ZOOM").ok().and_then(|v| v.parse::<f64>().ok())
            {
                let ed = &mut screen.as_mut().unwrap().editor;
                let (w, h) = ed.size;
                ed.zoom_at(w / 2.0, h / 2.0, z);
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        // TRATTO_PEEL=1: a note torn off the pad, being carried over the board.
        if std::env::var_os("TRATTO_PEEL").is_some() {
            harness.run_steps(20);
            let bar = harness.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("toolbar-rect"))).unwrap();
            let (a, b) = (egui::pos2(bar.center().x - 80.0, bar.center().y), egui::pos2(bar.center().x + 60.0, bar.min.y - 260.0));
            harness.event(egui::Event::PointerMoved(a));
            harness.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
            harness.run_steps(1);
            super::toolbar::start_drag(&harness.ctx, super::toolbar::Payload::Sticky, "Nota", "sticky-note");
            for k in 1..=10 {
                harness.event(egui::Event::PointerMoved(a + (b - a) * (k as f32 / 10.0)));
                harness.run_steps(1);
            }
            harness.run_steps(8);
            harness.render().unwrap().save(&path).unwrap();
            return;
        }
        // TRATTO_DRAG=x0,y0,x1,y1: a drag from one point to the other, still held.
        if let Some(v) = std::env::var("TRATTO_DRAG").ok() {
            let n: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            let (a, b) = (egui::pos2(n[0], n[1]), egui::pos2(n[2], n[3]));
            harness.run_steps(20);
            harness.event(egui::Event::PointerMoved(a));
            harness.run_steps(2);
            harness.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
            harness.run_steps(2);
            for k in 1..=12 {
                harness.event(egui::Event::PointerMoved(a + (b - a) * (k as f32 / 12.0)));
                harness.run_steps(1);
            }
            harness.run_steps(6);
            harness.render().unwrap().save(&path).unwrap();
            return;
        }
        // TRATTO_POINTER=x,y: the pointer resting there (hover effects); TRATTO_CLICK too: clicked.
        if let Some((x, y)) = std::env::var("TRATTO_POINTER").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))) {
            harness.run_steps(20);
            let at = egui::pos2(x, y);
            harness.event(egui::Event::PointerMoved(at));
            if std::env::var_os("TRATTO_CLICK").is_some() {
                harness.run_steps(2);
                harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                harness.run_steps(1);
                harness.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
            }
        }
        harness.run_steps(40);
        harness.render().unwrap().save(path).unwrap();
    }

    /// Cursive written with the mouse, through the real input path: three words, each one
    /// stroke with every point of its loops, sampled like a 125 Hz mouse while frames run at
    /// about 60 Hz; then two quick dots over a note, both drawn, the pen still the pen. With
    /// `TRATTO_SHOT=out.png` it also saves the picture.
    #[test]
    fn mouse_writing_stays_whole() {
        let shot = std::env::var("TRATTO_SHOT").ok();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Corsivo", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        // Strokes (points in each), the tool and where the note is on screen, after each frame.
        let state = std::rc::Rc::new(std::cell::RefCell::new((Vec::<usize>::new(), crate::editor::Tool::Pen, egui::Pos2::ZERO)));
        let out = state.clone();
        let builder = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0));
        let builder = if shot.is_some() { builder.wgpu() } else { builder };
        let mut harness = builder.build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap();
                b.editor.tool = crate::editor::Tool::Pen;
                // A note to write over.
                let mut s = crate::model::El::new(crate::model::Kind::Sticky(crate::model::Sticky::default()));
                (s.x, s.y, s.w, s.h) = (300.0, -200.0, 220.0, 220.0);
                b.editor.board.put([s]);
                screen = Some(b);
                return;
            }
            let b = screen.as_mut().unwrap();
            b.ui(ui, &mut toasts, &mut dialogs);
            let sticky = b.editor.board.all().iter().find(|e| e.sticky().is_some()).cloned();
            let note = sticky.map(|e| b.editor.to_screen(e.x + e.w / 2.0, e.y + e.h / 2.0) + b.editor.origin.to_vec2()).unwrap_or_default();
            *out.borrow_mut() = (b.editor.board.all().iter().filter_map(|e| e.ink().map(|i| i.points.len() / 3)).collect(), b.editor.tool, note);
        });
        harness.run_steps(10);
        // A word: loops along a line, like "eeee", at mouse speed (up to 4 px a sample).
        let word = |x0: f32, y0: f32| -> Vec<egui::Pos2> {
            (0..=180)
                .map(|i| {
                    let t = i as f32 / 180.0 * std::f32::consts::TAU * 4.0;
                    egui::pos2(x0 + i as f32 * 1.2 - 20.0 * t.sin(), y0 - 20.0 * (1.0 - t.cos()))
                })
                .collect()
        };
        let press = |h: &mut egui_kittest::Harness, p: egui::Pos2, down: bool| {
            h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: egui::Modifiers::NONE });
        };
        for (x, y) in [(420.0, 300.0), (420.0, 420.0), (420.0, 540.0)] {
            let pts = word(x, y);
            harness.input_mut().events.push(egui::Event::PointerMoved(pts[0]));
            harness.step();
            press(&mut harness, pts[0], true);
            for (i, p) in pts.iter().enumerate() {
                harness.input_mut().events.push(egui::Event::PointerMoved(*p));
                if i % 2 == 1 {
                    harness.step();
                }
            }
            press(&mut harness, *pts.last().unwrap(), false);
            harness.step();
            // A hand takes longer than a bouncing switch to press again.
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        harness.step();
        let (strokes, tool, note) = state.borrow().clone();
        assert_eq!(strokes.len(), 3, "one stroke per word: {strokes:?}");
        assert!(strokes.iter().all(|n| *n > 150), "no point of the loops lost: {strokes:?}");
        assert_eq!(tool, crate::editor::Tool::Pen);
        // Two quick dots over the note (a double click, for egui).
        for k in 0..2 {
            // The second a little beside the first, as when writing: on the note, not on the dot.
            let dot = note + egui::vec2(12.0 * k as f32, 0.0);
            harness.input_mut().events.push(egui::Event::PointerMoved(dot));
            harness.step();
            press(&mut harness, dot, true);
            harness.step();
            press(&mut harness, dot, false);
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        harness.run_steps(5);
        let (strokes, tool, _) = state.borrow().clone();
        assert_eq!(strokes.len(), 5, "both dots drawn: {strokes:?}");
        assert_eq!(tool, crate::editor::Tool::Pen, "still writing after a quick double tap");
        if let Some(path) = shot {
            harness.render().unwrap().save(path).unwrap();
        }
    }

    /// A gaming mouse (1000 Hz) on a fast screen, and on a slow one: the switch bounces (lets go
    /// for a few milliseconds) in the middle of a word, and the word stays one stroke; a real
    /// lift ends it.
    #[test]
    fn a_bouncing_mouse_button_does_not_break_the_stroke() {
        for moves_per_frame in [6usize, 33] {
            let store = Store::open(std::path::Path::new(":memory:")).unwrap();
            let id = store.create("Rimbalzi", None).unwrap();
            let mut screen: Option<board::BoardScreen> = None;
            let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
            let strokes = std::rc::Rc::new(std::cell::RefCell::new(Vec::<usize>::new()));
            let out = strokes.clone();
            let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
                let ctx = ui.ctx().clone();
                if screen.is_none() {
                    ui::install_fonts(&ctx);
                    ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                    let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap();
                    b.editor.tool = crate::editor::Tool::Pen;
                    screen = Some(b);
                    return;
                }
                let b = screen.as_mut().unwrap();
                b.ui(ui, &mut toasts, &mut dialogs);
                *out.borrow_mut() = b.editor.board.all().iter().filter_map(|e| e.ink().map(|i| i.points.len() / 3)).collect();
            });
            harness.run_steps(5);
            let button = |h: &mut egui_kittest::Harness, p: egui::Pos2, down: bool| {
                h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: egui::Modifiers::NONE });
            };
            let pts: Vec<egui::Pos2> = (0..=600).map(|i| {
                let t = i as f32 / 600.0 * std::f32::consts::TAU * 5.0;
                egui::pos2(420.0 + i as f32 * 0.8 - 18.0 * t.sin(), 400.0 - 18.0 * (1.0 - t.cos()))
            }).collect();
            harness.input_mut().events.push(egui::Event::PointerMoved(pts[0]));
            harness.step();
            button(&mut harness, pts[0], true);
            for (i, p) in pts.iter().enumerate() {
                harness.input_mut().events.push(egui::Event::PointerMoved(*p));
                // Bounces: one inside a frame, one across two frames.
                if i == 200 {
                    button(&mut harness, *p, false);
                    button(&mut harness, *p, true);
                }
                if i == 400 {
                    button(&mut harness, *p, false);
                    harness.step();
                    button(&mut harness, *p, true);
                }
                if i % moves_per_frame == moves_per_frame - 1 {
                    harness.step();
                }
            }
            button(&mut harness, *pts.last().unwrap(), false);
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(80));
            harness.run_steps(2);
            let s = strokes.borrow().clone();
            assert_eq!(s.len(), 1, "one stroke at {moves_per_frame} moves a frame: {s:?}");
            assert!(s[0] > 400, "every loop kept: {s:?}");
        }
    }

    /// A widget dragged from the "+" lands where it is let go, not in the middle.
    #[test]
    fn a_widget_dragged_onto_the_board_lands_under_the_pointer() {
        dragged_lands(super::toolbar::Payload::Checklist, |e| e.widget().is_some());
        // A note torn off the pad, likewise; it opens ready to write in.
        dragged_lands(super::toolbar::Payload::Sticky, |e| e.sticky().is_some());
    }

    /// The layers panel: a row dragged between two others goes there in the stack, and one
    /// dropped on a folder goes into it.
    #[test]
    fn layers_move_by_dragging_their_rows() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Livelli", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        type Seen = (Vec<String>, Vec<(String, Option<String>)>);
        let out: std::rc::Rc<std::cell::RefCell<Seen>> = Default::default();
        let seen = out.clone();
        let folder = std::rc::Rc::new(std::cell::Cell::new(false));
        let make_folder = folder.clone();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let prefs = Prefs { left_panel: true, ..Prefs::default() };
                let mut b = board::BoardScreen::open(&ctx, &store, &id, prefs, None).unwrap();
                let els: Vec<crate::model::El> = ["a", "b", "c", "d"]
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        let mut e = crate::model::El::new(crate::model::Kind::Shape(crate::model::Shape::default()));
                        (e.id, e.x, e.y, e.w, e.h, e.z, e.name) = (n.to_string(), i as f64 * 300.0, 0.0, 100.0, 80.0, i as f64, Some(n.to_uppercase()));
                        e
                    })
                    .collect();
                b.editor.board.put(els);
                screen = Some(b);
                return;
            }
            let b = screen.as_mut().unwrap();
            if make_folder.take() {
                b.editor.board.create_group(&["a".into(), "b".into()], Some("Cartella".into()));
            }
            b.ui(ui, &mut toasts, &mut dialogs);
            *seen.borrow_mut() = (b.editor.board.all().iter().map(|e| e.id.clone()).collect(), b.editor.board.all().iter().map(|e| (e.id.clone(), e.group_id.clone())).collect());
        });
        harness.run_steps(5);
        let rows = |h: &egui_kittest::Harness| h.ctx.data(|d| d.get_temp::<Vec<(usize, egui::Rect)>>(egui::Id::new("layer-rows"))).expect("the layers are listed");
        let drag = |h: &mut egui_kittest::Harness, from: egui::Pos2, to: egui::Pos2| {
            h.input_mut().events.push(egui::Event::PointerMoved(from));
            h.step();
            h.input_mut().events.push(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
            h.step();
            for k in 1..=12 {
                h.input_mut().events.push(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 12.0)));
                h.step();
            }
            h.input_mut().events.push(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
            h.run_steps(3);
        };
        // Top to bottom the list shows D, C, B, A. D goes between B and A.
        let r = rows(&harness);
        assert_eq!(r.len(), 4);
        drag(&mut harness, r[0].1.center(), egui::pos2(r[3].1.center().x, r[3].1.min.y + 4.0));
        assert_eq!(out.borrow().0, ["a", "d", "b", "c"], "bottom to top");
        // A folder of A and B; C dropped on its row goes in, at its top.
        folder.set(true);
        harness.run_steps(3);
        let r = rows(&harness);
        let order = out.borrow().0.clone();
        let c_row = order.iter().rev().position(|i| i == "c").unwrap();
        let folder_row = r.iter().find(|(i, _)| *i != c_row && *i != order.iter().rev().position(|x| x == "d").unwrap()).unwrap().1;
        drag(&mut harness, r[c_row].1.center(), folder_row.center());
        let groups = out.borrow().1.clone();
        let g = |id: &str| groups.iter().find(|(i, _)| i == id).unwrap().1.clone();
        assert!(g("c").is_some() && g("c") == g("a") && g("a") == g("b"), "c is in the folder: {groups:?}");
        assert!(g("d").is_none());
    }

    fn dragged_lands(what: super::toolbar::Payload, is: fn(&crate::model::El) -> bool) {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Trascina", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let out = std::rc::Rc::new(std::cell::RefCell::new(None::<egui::Pos2>));
        let seen = out.clone();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap());
                return;
            }
            let b = screen.as_mut().unwrap();
            b.ui(ui, &mut toasts, &mut dialogs);
            let w = b.editor.board.all().iter().find(|e| is(e)).cloned();
            *seen.borrow_mut() = w.map(|e| b.editor.to_screen(e.x + e.w / 2.0, e.y + e.h / 2.0) + b.editor.origin.to_vec2());
        });
        harness.run_steps(5);
        // From the "+" at the end of the toolbar, where its card opens.
        let bar = harness.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("toolbar-rect"))).unwrap();
        let from = egui::pos2(bar.max.x - 26.0, bar.center().y);
        harness.input_mut().events.push(egui::Event::PointerMoved(from));
        harness.input_mut().events.push(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        harness.step();
        super::toolbar::start_drag(&harness.ctx, what, "Lista", "list-checks");
        let to = egui::pos2(600.0, 350.0);
        for k in 1..=10 {
            harness.input_mut().events.push(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 10.0)));
            harness.step();
        }
        harness.input_mut().events.push(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        harness.run_steps(3);
        let at = out.borrow().expect("it is on the board");
        assert!(at.distance(to) < 2.0, "centred where it was let go: {at:?}");
    }

    /// The minimap, wherever it was put, never ends up under a panel or the pills at the top.
    #[test]
    fn the_minimap_keeps_clear_of_panels_and_pills() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Minimappa", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let set: std::rc::Rc<std::cell::Cell<Option<(bool, bool, [f32; 2])>>> = Default::default();
        let next = set.clone();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs { minimap: true, ..Prefs::default() }, None).unwrap());
                return;
            }
            let b = screen.as_mut().unwrap();
            if let Some((left, right, at)) = next.take() {
                (b.editor.prefs.left_panel, b.editor.prefs.right_panel, b.editor.prefs.minimap_at) = (left, right, Some(at));
            }
            b.ui(ui, &mut toasts, &mut dialogs);
        });
        let mut wrong = Vec::new();
        for (left, right) in [(false, false), (true, false), (false, true), (true, true)] {
            for at in [[0.0, 0.0], [0.0, 0.5], [0.0, 1.0], [1.0, 0.0], [1.0, 0.5], [0.5, 0.0]] {
                set.set(Some((left, right, at)));
                // Settled (the picture of the map is drawn on another thread).
                for _ in 0..90 {
                    harness.step();
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                let rect = |k: &str| harness.ctx.data(|d| d.get_temp::<egui::Rect>(Id::new(k)));
                let Some(map) = rect("minimap-rect") else {
                    wrong.push(format!("{left} {right} {at:?}: no minimap"));
                    continue;
                };
                let mut over = vec![("left pill", rect("left-pill-rect")), ("right pill", rect("right-pill-rect"))];
                for (name, area) in [("left panel", "left-panel"), ("right panel", "right-panel")] {
                    over.push((name, harness.ctx.memory(|m| m.area_rect(Id::new(area))).filter(|_| if name == "left panel" { left } else { right })));
                }
                for (name, r) in over {
                    if let Some(r) = r
                        && r.intersects(map)
                    {
                        wrong.push(format!("panels {left}/{right}, minimap at {at:?}: under the {name} ({map:?} / {r:?})"));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// The panels close and come back at the first click, however soon one follows the other
    /// (from the second frame of what appears: in the first one egui only measures it), with the
    /// mouse and with a pen's tap (a touch, with the pointer it moves). The pills that bring the
    /// panels back used to ride along with the panel going away: clicked as they appeared, they
    /// had already slid off.
    #[test]
    fn panels_close_and_open_at_the_first_click() {
        use egui_kittest::kittest::Queryable;
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Pannelli", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let prefs = Prefs { left_panel: true, right_panel: true, ..Prefs::default() };
                screen = Some(board::BoardScreen::open(&ctx, &store, &id, prefs, None).unwrap());
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(5);
        let shown = |h: &egui_kittest::Harness| h.ctx.data(|d| d.get_temp::<(bool, bool)>(Id::new("panels-shown"))).unwrap();
        // Where a button is now: of those with this name, the one farthest left (or right).
        let find = |h: &egui_kittest::Harness, label: &str, left: bool| {
            let mut rects: Vec<egui::Rect> = h.get_all_by_label(label).map(|n| n.rect()).collect();
            rects.sort_by(|a, b| a.center().x.total_cmp(&b.center().x));
            (if left { rects.first() } else { rects.last() }).unwrap_or_else(|| panic!("no {label}")).center()
        };
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        let touch = |pos, phase| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(7), phase, pos, force: Some(0.5) };
        let mut missed = Vec::new();
        for way in ["mouse", "pen", "quick pen"] {
            for wait in [2, 4, 8, 30] {
                for left in [true, false] {
                    for (label, open) in [("Nascondi il pannello", false), (if left { "Mostra il pannello dei livelli" } else { "Mostra il pannello" }, true)] {
                        let p = find(&harness, label, left);
                        match way {
                            // Over it, pressed, let go: a frame each.
                            "mouse" => {
                                harness.input_mut().events.push(egui::Event::PointerMoved(p));
                                harness.step();
                                harness.input_mut().events.push(button(p, true));
                                harness.step();
                                harness.input_mut().events.push(button(p, false));
                            }
                            // As the window hands a pen's tap over: the touch, then the pointer
                            // it moves and presses; let go a frame later.
                            "pen" => {
                                harness.input_mut().events.extend([touch(p, egui::TouchPhase::Start), egui::Event::PointerMoved(p), button(p, true)]);
                                harness.step();
                                harness.input_mut().events.extend([touch(p, egui::TouchPhase::End), button(p, false), egui::Event::PointerGone]);
                            }
                            // Down and up between two frames.
                            _ => harness.input_mut().events.extend([touch(p, egui::TouchPhase::Start), egui::Event::PointerMoved(p), button(p, true), touch(p, egui::TouchPhase::End), button(p, false), egui::Event::PointerGone]),
                        }
                        // The frame that takes the release, then the waiting ones (the first of
                        // which shows what the click did).
                        harness.step();
                        harness.run_steps(wait);
                        let now = shown(&harness);
                        if (if left { now.0 } else { now.1 }) != open {
                            missed.push(format!("{way}, {wait} frames after the last: {label} ({})", if left { "left" } else { "right" }));
                            // As someone would: wait, and click again.
                            harness.run_steps(30);
                            harness.get_by_label(label).click();
                            harness.run_steps(30);
                        }
                    }
                }
            }
        }
        assert!(missed.is_empty(), "missed clicks: {missed:#?}");
    }

    /// A comment written with the keyboard: the pin goes down, its box takes the typing, Enter
    /// sends it.
    #[test]
    fn a_comment_can_be_written() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Commenti", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let out = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let seen = out.clone();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), None).unwrap();
                b.editor.tool = crate::editor::Tool::Comment;
                screen = Some(b);
                return;
            }
            let b = screen.as_mut().unwrap();
            b.ui(ui, &mut toasts, &mut dialogs);
            *seen.borrow_mut() = b.editor.board.all().iter().filter_map(|e| match &e.kind {
                crate::model::Kind::Comment { thread } => Some(thread.iter().map(|m| m.text.clone()).collect::<Vec<_>>().join("|")),
                _ => None,
            }).collect();
        });
        harness.run_steps(5);
        let at = egui::pos2(700.0, 400.0);
        harness.input_mut().events.push(egui::Event::PointerMoved(at));
        harness.step();
        harness.input_mut().events.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        harness.step();
        harness.input_mut().events.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        harness.run_steps(4);
        assert_eq!(out.borrow().len(), 1, "a pin on the board");
        harness.input_mut().events.push(egui::Event::Text("Ciao a tutti".into()));
        harness.step();
        harness.input_mut().events.push(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        harness.run_steps(3);
        assert_eq!(out.borrow().as_slice(), ["Ciao a tutti"], "the message is in the thread");
    }

    /// Alt and a 2D drag with the pen: horizontally (X) it adjusts thickness, vertically (Y)
    /// it adjusts opacity; Alt and a click samples colour under the pointer. [ and ] step the size.
    #[test]
    fn alt_and_a_drag_set_the_pen_size_and_opacity() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Penna", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let out: std::rc::Rc<std::cell::Cell<(f64, f64, usize)>> = Default::default();
        let seen = out.clone();
        let color_out: std::rc::Rc<std::cell::RefCell<String>> = Default::default();
        let seen_color = color_out.clone();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs { left_panel: false, right_panel: false, ..Prefs::default() }, None).unwrap();
                b.editor.tool = crate::editor::Tool::Pen;
                b.editor.prefs.pens[0] = crate::prefs::Pen { size: 4.0, opacity: 0.5, color: "#1E1E1E".into(), ..Default::default() };
                let mut target = crate::model::El::new(crate::model::Kind::Shape(crate::model::Shape {
                    fill: "#E03131".into(),
                    stroke: "transparent".into(),
                    stroke_width: 0.0,
                    ..Default::default()
                }));
                target.x = 200.0;
                target.y = 100.0;
                target.w = 600.0;
                target.h = 600.0;
                b.editor.board.put(vec![target]);
                screen = Some(b);
                return;
            }
            let b = screen.as_mut().unwrap();
            b.ui(ui, &mut toasts, &mut dialogs);
            let p = &b.editor.prefs.pens[b.editor.pen];
            seen.set((p.size, p.opacity, b.editor.board.len()));
            *seen_color.borrow_mut() = p.color.clone();
        });
        harness.run_steps(5);
        let alt = egui::Modifiers { alt: true, ..Default::default() };
        let drag_2d = |h: &mut egui_kittest::Harness, button: egui::PointerButton, dx: f32, dy: f32| {
            let a = egui::pos2(500.0, 400.0);
            h.input_mut().events.push(egui::Event::PointerMoved(a));
            h.step();
            h.input_mut().events.push(egui::Event::PointerButton { pos: a, button, pressed: true, modifiers: alt });
            h.step();
            for k in 1..=6 {
                h.input_mut().events.push(egui::Event::PointerMoved(a + egui::vec2(dx * k as f32 / 6.0, dy * k as f32 / 6.0)));
                h.step();
            }
            h.input_mut().events.push(egui::Event::PointerButton { pos: a + egui::vec2(dx, dy), button, pressed: false, modifiers: alt });
            h.run_steps(2);
        };
        // 1. Horizontal drag (dx = 120.0, dy = 0.0) doubles size from 4.0 to ~8.0 without altering opacity
        drag_2d(&mut harness, egui::PointerButton::Primary, 120.0, 0.0);
        let (size, opacity, n) = out.get();
        assert!((size - 8.0).abs() < 0.6, "twice as thick: {size}");
        assert_eq!((opacity, n), (0.5, 1), "opacity unchanged, nothing drawn (sticky remains)");

        // 2. Vertical drag (dx = 0.0, dy = -150.0) increases opacity from 0.5 to ~1.0 without altering size
        drag_2d(&mut harness, egui::PointerButton::Primary, 0.0, -150.0);
        let (size, opacity, n) = out.get();
        assert!((size - 8.0).abs() < 0.6, "size unchanged: {size}");
        assert!((opacity - 1.0).abs() < 0.01 && n == 1, "fully opaque: {opacity}");
        assert!(!egui::Popup::is_any_open(&harness.ctx), "no menu");

        // 3. Alt + click (no drag displacement) samples the color under the cursor
        let click = |h: &mut egui_kittest::Harness, button: egui::PointerButton| {
            let a = egui::pos2(500.0, 400.0);
            h.input_mut().events.push(egui::Event::PointerMoved(a));
            h.step();
            h.input_mut().events.push(egui::Event::PointerButton { pos: a, button, pressed: true, modifiers: alt });
            h.step();
            h.input_mut().events.push(egui::Event::PointerButton { pos: a, button, pressed: false, modifiers: alt });
            h.run_steps(2);
        };
        click(&mut harness, egui::PointerButton::Primary);
        assert_eq!(color_out.borrow().to_uppercase(), "#E03131", "Alt+click samples color under pointer");
        assert_eq!(out.get().2, 1, "no ink drawn by click");

        // 4. [ steps the size thinner
        harness.input_mut().events.push(egui::Event::Key { key: egui::Key::OpenBracket, physical_key: Some(egui::Key::OpenBracket), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        harness.run_steps(2);
        assert!(out.get().0 < 7.0, "a step thinner: {}", out.get().0);
    }

    /// In the timer's panel, the chime and the piece picked are the ones that stay: each list
    /// opens over the panel without closing it, and a choice is kept.
    #[test]
    fn the_timer_chime_and_the_music_can_be_chosen() {
        use egui_kittest::kittest::Queryable;
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Riunione", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        type Seen = (crate::prefs::TimerSound, Option<String>);
        let out: std::rc::Rc<std::cell::RefCell<Option<Seen>>> = Default::default();
        let seen = out.clone();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(false, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                screen = Some(board::BoardScreen::open(&ctx, &store, &id, Prefs { music_volume: 0.0, timer_volume: 0.0, right_panel: false, ..Prefs::default() }, None).unwrap());
                return;
            }
            let b = screen.as_mut().unwrap();
            b.ui(ui, &mut toasts, &mut dialogs);
            let music = b.editor.board.timer_field("music").and_then(|v| v.as_str().map(str::to_string));
            *seen.borrow_mut() = Some((b.editor.prefs.timer_chime, music));
        });
        harness.run_steps(5);
        harness.get_by_label_contains("Timer:").click();
        harness.run_steps(10);
        // (The sounds are side by side in the panel.)
        harness.run_steps(10);
        harness.get_by_label("Carillon").click();
        harness.run_steps(10);
        assert_eq!(out.borrow().as_ref().unwrap().0, crate::prefs::TimerSound::Carillon, "the chime picked");
        assert!(harness.query_by_label_contains("Brano:").is_some(), "the panel is still open");
        // A piece, then play it for everyone; then another while it plays.
        harness.get_by_label_contains("Brano:").click();
        harness.run_steps(10);
        harness.get_by_label("Synthwave").click();
        harness.run_steps(10);
        harness.get_by_label("Fai partire la musica per tutti").click();
        harness.run_steps(10);
        assert_eq!(out.borrow().as_ref().unwrap().1.as_deref(), Some("synthwave"), "the piece picked plays");
        harness.get_by_label_contains("Brano:").click();
        harness.run_steps(10);
        harness.get_by_label("Lo-fi hip hop").click();
        harness.run_steps(10);
        assert_eq!(out.borrow().as_ref().unwrap().1.as_deref(), Some("lofi"), "changed while playing");
    }

    /// The timer and music panel, open, while a timer runs and music plays (TRATTO_PAUSED=1 for
    /// a paused timer, TRATTO_IDLE=1 for none): `TRATTO_SHOT=out.png cargo test timer_picture`.
    #[test]
    fn timer_picture() {
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Riunione", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 760.0)).with_pixels_per_point(ppp()).wgpu().build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(std::env::var("TRATTO_DARK").is_ok(), crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), text_scale());
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs { music_volume: 0.0, ..Prefs::default() }, Some("flow".into())).unwrap();
                let now = crate::platform::shared_now();
                let paused = std::env::var_os("TRATTO_PAUSED").is_some();
                let idle = std::env::var_os("TRATTO_IDLE").is_some();
                b.editor.board.transact_plain(|txn, m| {
                    use yrs::Map;
                    if !idle {
                        m.timer.insert(txn, "total", 300_000.0);
                        m.timer.insert(txn, "left", 192_000.0);
                    }
                    if !paused && !idle {
                        m.timer.insert(txn, "end", now + 192_000.0);
                    }
                    m.timer.insert(txn, "music", "lofi");
                    m.timer.insert(txn, "musicAt", now);
                });
                screen = Some(b);
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(10);
        use egui_kittest::kittest::Queryable;
        harness.get_by_label_contains("Timer:").click();
        harness.run_steps(30);
        // TRATTO_TYPE=34: those digits typed on the time.
        if let Ok(d) = std::env::var("TRATTO_TYPE") {
            harness.ctx.memory_mut(|m| m.request_focus(egui::Id::new("timer-field")));
            harness.run_steps(2);
            for c in d.chars() {
                harness.event(egui::Event::Text(c.to_string()));
                harness.run_steps(1);
            }
            harness.run_steps(2);
        }
        // TRATTO_TRACKS=1: the list of the pieces open.
        if std::env::var_os("TRATTO_TRACKS").is_some() {
            harness.get_by_label_contains("Brano:").click();
            harness.run_steps(20);
        }
        harness.render().unwrap().save(path).unwrap();
    }

    /// Situations to look at: `TRATTO_SHOT=out.png TRATTO_SCENE=table cargo test scene_picture`
    /// (table, menu, sticky, hover, dot, dark…).
    #[test]
    fn scene_picture() {
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let scene = std::env::var("TRATTO_SCENE").unwrap_or_default();
        let dark = std::env::var("TRATTO_DARK").is_ok();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let template = if scene == "dot" || scene == "table" { None } else { Some(std::env::var("TRATTO_TEMPLATE").unwrap_or_else(|_| "flow".into())) };
        let id = store.create("Prova", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut frame = 0;
        // TRATTO_SIZE=800x540 a smaller window, TRATTO_UI=1.5 the interface zoomed in.
        let size = std::env::var("TRATTO_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some(egui::vec2(w.parse().ok()?, h.parse().ok()?)))).unwrap_or(egui::vec2(1440.0, 900.0));
        let zoom: Option<f32> = std::env::var("TRATTO_UI").ok().and_then(|v| v.parse().ok());
        let mut harness = egui_kittest::Harness::builder().with_size(size).with_pixels_per_point(ppp()).wgpu().build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            frame += 1;
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(dark, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), text_scale());
                let mut prefs = Prefs::default();
                if let Some(z) = zoom {
                    prefs.ui_scale = z;
                    ctx.set_zoom_factor(z);
                }
                let mut b = board::BoardScreen::open(&ctx, &store, &id, prefs, template.clone()).unwrap();
                b.editor.tool = crate::editor::Tool::Select;
                if let Ok(bg) = std::env::var("TRATTO_BG") {
                    let meta = crate::model::BoardMeta { background: bg, ..b.editor.board.meta() };
                    b.editor.board.set_meta(&meta);
                }
                let ed = &mut b.editor;
                match scene.as_str() {
                    "table" => {
                        ed.prefs.table_size = [4, 3];
                        ed.insert_table(Some(crate::geom::pt(0.0, 0.0)));
                        let tid = ed.selection[0].clone();
                        ed.board.update(&[tid.clone()], |el| {
                            if let crate::model::Kind::Table(t) = &mut el.kind {
                                for (i, s) in ["Attività", "Chi", "Quando", "Bozza", "Anna", "Lunedì", "Revisione", "Marco", "Giovedì"].iter().enumerate() {
                                    t.cells[i / 3][i % 3].text = s.to_string();
                                }
                                t.cells[2][2].fill = Some("#C9F2C7".into());
                            }
                            crate::text::fit_text(el);
                        });
                        ed.editing = Some(tid.clone());
                        ed.editing_cell = Some((1, 1));
                        b.ui.open_folders.insert(format!("el:{tid}"));
                    }
                    "sticky" => {
                        let mut s = crate::model::El::new(crate::model::Kind::Sticky(crate::model::Sticky { text: "Idea da sviluppare".into(), bold: true, radius: Some(18.0), ..Default::default() }));
                        (s.x, s.y, s.w, s.h) = (-120.0, -420.0, 220.0, 220.0);
                        s.shadow = Some(crate::model::Shade { opacity: 0.35, blur: 30.0, y: 14.0 });
                        let sid = s.id.clone();
                        ed.board.put([s]);
                        ed.selection = vec![sid];
                    }
                    "menu" => {
                        let first = ed.board.all().iter().find(|e| e.shape().is_some()).unwrap().id.clone();
                        ed.selection = vec![first];
                        b.ui.menu = Some(egui::pos2(520.0, 300.0));
                    }
                    "stamp" => {
                        for (i, s) in crate::assets::STAMPS.iter().take(8).enumerate() {
                            let mut e = crate::model::El::new(crate::model::Kind::Stamp { emoji: s.emoji.into() });
                            (e.x, e.y, e.w, e.h) = (-300.0 + i as f64 * 90.0, -380.0, 72.0, 72.0);
                            ed.board.put([e]);
                        }
                        ed.tool = crate::editor::Tool::Stamp;
                    }
                    "hover" => {
                        let first = ed.board.all().iter().find(|e| e.shape().is_some()).unwrap().id.clone();
                        ed.hover_adds = Some(first);
                    }
                    "zen" => ed.prefs.zen = true,
                    "emote" => {
                        let now = crate::platform::now_ms();
                        ed.emote_hold = Some((now - 2000.0, egui::pos2(900.0, 300.0)));
                        for (i, e) in crate::editor::EMOTES.iter().enumerate().take(3) {
                            ed.floaters.push((e.to_string(), -300.0 + i as f64 * 60.0, 150.0, now - 300.0 - i as f64 * 250.0));
                        }
                    }
                    "menu-main" => (ed.prefs.left_panel, ed.prefs.right_panel, ed.prefs.minimap) = (false, false, false),
                    "dot" => {
                        let mut d = crate::model::El::new(crate::model::Kind::Ink(crate::model::Ink { colors: Vec::new(), points: vec![0.0, 0.0, 0.5, 1.0, 1.0, 0.5], color: "#1E1E1E".into(), size: 6.0, brush: None }));
                        (d.w, d.h) = (1.0, 1.0);
                        ed.board.put([d]);
                    }
                    _ => {}
                }
                screen = Some(b);
                return;
            }
            let b = screen.as_mut().unwrap();
            // The share dialog: its form first, then (with a local share) the live link and people.
            if scene == "share" {
                b.ui.share = true;
                if frame == 20 {
                    b.share = Some(crate::share::Share::start(b.store().clone(), &b.id, &b.title, b.editor.board.doc.clone(), crate::share::Access::Edit, false, None, std::sync::Arc::new(|| {})).unwrap());
                }
            }
            // Scrolled down far from the dot: the minimap must keep the view marker inside.
            if scene == "dot" && frame == 20 {
                let c = b.editor.cam;
                b.editor.set_cam(crate::geom::Camera { y: c.y - 2600.0, ..c });
            }
            b.ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(60);
        if std::env::var("TRATTO_SCENE").is_ok_and(|s| s == "menu-main") {
            use egui_kittest::kittest::Queryable;
            harness.get_by_label("Menu principale").click();
            harness.run_steps(10);
        }
        harness.render().unwrap().save(path).unwrap();
    }
}
