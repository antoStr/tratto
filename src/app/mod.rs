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
            let resp = egui::Area::new(id).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -88.0 - lift + (1.0 - k.min(1.0)) * 16.0]).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
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

    /// The whole board screen as a picture: `TRATTO_SHOT=out.png cargo test app::picture -- --nocapture`.
    #[test]
    fn board_screen_picture() {
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let dark = std::env::var("TRATTO_DARK").is_ok();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store.create("Diagramma di flusso", None).unwrap();
        let mut screen: Option<board::BoardScreen> = None;
        let (mut toasts, mut dialogs) = (Toasts::default(), dialogs::Dialogs::default());
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).wgpu().build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, Theme::new(dark, egui::Color32::from_rgb(0x0D, 0x99, 0xFF), false), 1.0);
                let mut b = board::BoardScreen::open(&ctx, &store, &id, Prefs::default(), Some("flow".into())).unwrap();
                b.editor.selection = vec![b.editor.board.all().iter().find(|e| e.shape().is_some()).unwrap().id.clone()];
                b.editor.tool = if std::env::var_os("TRATTO_SELECT").is_some() { crate::editor::Tool::Select } else { crate::editor::Tool::Pen };
                screen = Some(b);
                return;
            }
            screen.as_mut().unwrap().ui(ui, &mut toasts, &mut dialogs);
        });
        harness.run_steps(40);
        harness.render().unwrap().save(path).unwrap();
    }
}
