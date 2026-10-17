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

/// Short messages at the bottom of the window, dark like Figma's.
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
        prefs::Theme::System => ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark),
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
        ui::install_fonts(&cc.egui_ctx);
        // A new version shows up on the home screen (and in Settings ▸ Updates).
        if std::env::var_os("TRATTO_BENCH").is_none() {
            crate::updates::check(&cc.egui_ctx);
        }
        let prefs = store.setting("prefs").map(|s| Prefs::from_json(&s)).unwrap_or_default();
        let saved_prefs = prefs.to_json();
        let home = home::Home::new(&store);
        let bench = std::env::var_os("TRATTO_BENCH").map(|_| Bench { open: std::env::var("TRATTO_OPEN").ok(), pan: std::env::var_os("TRATTO_PAN").is_some(), ..Default::default() });
        App { prefs, saved_prefs, store, screen: Screen::Home(home), toasts: Toasts::default(), dialogs: dialogs::Dialogs::default(), applied: None, last_save: 0.0, bench }
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
    }

    fn on_exit(&mut self) {
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
        harness.render().unwrap().save(path).unwrap();
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
                    let pts: Vec<f32> = (0..150).flat_map(|k| [(k as f32 * 3.0), ((k as f32 * 0.2 + i as f32).sin() * 30.0), -1.0]).collect();
                    let mut s = El::new(Kind::Ink(Ink { points: pts, color: "#1E1E1E".into(), size: 4.0 }));
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
        let mut measure = |name: &str, harness: &mut egui_kittest::Harness<'_>, n: usize| {
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
        // TRATTO_SIZE=1300x800 for a smaller window, TRATTO_MINIMAP=br for the minimap put down
        // in the bottom right corner.
        let size = std::env::var("TRATTO_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some(egui::vec2(w.parse().ok()?, h.parse().ok()?)))).unwrap_or(egui::vec2(1440.0, 900.0));
        let mut prefs = Prefs::default();
        if std::env::var("TRATTO_MINIMAP").is_ok_and(|v| v == "br") {
            prefs.minimap_at = Some([1.0, 1.0]);
        }
        // TRATTO_CLEAN=1: both side panels closed and no minimap (FigJam's layout).
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
                b.editor.selection = vec![b.editor.board.all().iter().find(|e| e.shape().is_some()).unwrap().id.clone()];
                b.editor.tool = if std::env::var_os("TRATTO_SELECT").is_some() { crate::editor::Tool::Select } else { crate::editor::Tool::Pen };
                b.editor.prefs.focus = std::env::var_os("TRATTO_FOCUS").is_some();
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

    /// The timer and music panel, open, while a timer runs and music plays (TRATTO_PAUSED=1 for
    /// a paused timer): `TRATTO_SHOT=out.png cargo test timer_picture`.
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
        harness.get_by_label_contains(if std::env::var_os("TRATTO_IDLE").is_some() { "Timer e musica" } else { "Timer:" }).click();
        harness.run_steps(30);
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
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_pixels_per_point(ppp()).wgpu().build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            frame += 1;
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(dark, crate::model::parse_color(crate::prefs::DEFAULT_ACCENT).unwrap(), false), text_scale());
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), template.clone()).unwrap();
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
                        let mut d = crate::model::El::new(crate::model::Kind::Ink(crate::model::Ink { points: vec![0.0, 0.0, 0.5, 1.0, 1.0, 0.5], color: "#1E1E1E".into(), size: 6.0 }));
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
