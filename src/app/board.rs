//! An open board on this PC: the tab bar, the panels, the board itself with the text being
//! typed and the context menu, and saving (document, thumbnail, camera) as it changes.

use std::collections::HashSet;
use std::sync::Arc;

use egui::{Color32, FontFamily, FontId, Id, Pos2, Rect, Sense, Stroke, pos2, vec2};

use super::{Action, Toasts, dialogs::Dialogs};
use crate::doc::{Board, REMOTE, apply_update, encode_state};
use crate::editor::{Editor, Request, Tool};
use crate::geom::{Camera, frame_box, union, pt};
use crate::model::{El, Kind};
use crate::paint::{Fetch, Frame, Images, Painter};
use crate::prefs::Prefs;
use crate::prims::ImageKey;
#[cfg(not(target_arch = "wasm32"))]
use crate::store::Store;
use crate::ui::{self, Theme};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LeftTab {
    Layers,
    Templates,
}

/// State of the interface around the board.
pub struct BoardUi {
    pub left_tab: LeftTab,
    pub open_folders: HashSet<String>,
    pub renaming: Option<String>,
    pub rename_buf: String,
    pub menu: Option<Pos2>,
    pub export: Option<bool>,
    /// What the export dialog was opened for, kept while it fades away.
    pub export_sel: bool,
    pub share: bool,
    pub shortcuts: bool,
    pub title_edit: Option<String>,
    pub comment_draft: String,
    pub chat: Option<String>,
    pub chat_clear: Option<f64>,
    pub minimap: super::toolbar::Minimap,
    /// Text being typed into the element `editing` (kept here while the box is open).
    pub text: String,
    pub text_for: Option<String>,
    /// That text as the board last had it: when the board's changes, someone else typed there.
    pub text_seen: String,
    /// A text box just opened under the pointer: it keeps the keys while the button that made
    /// it is still down (letting go over the board would take them away).
    pub text_grab: bool,
    pub scroll_to: Option<String>,
    /// In a narrow window, a panel opened from its pill anyway.
    pub peek_left: bool,
    pub peek_right: bool,
    /// Zen mode as last seen (to go full screen when it starts), and whether it made the window
    /// full screen (so leaving it gives the window back as it was).
    pub zen_seen: bool,
    pub zen_fullscreen: bool,
    /// Last time the pointer came near the toolbar's edge in zen mode (it shows for a while).
    pub zen_near: f64,
    /// The board's size on screen last frame (None before the first).
    pub seen_size: Option<(f64, f64)>,
}

impl Default for BoardUi {
    fn default() -> Self {
        BoardUi {
            left_tab: LeftTab::Layers,
            open_folders: HashSet::new(),
            renaming: None,
            rename_buf: String::new(),
            menu: None,
            export: None,
            export_sel: false,
            share: false,
            shortcuts: false,
            title_edit: None,
            comment_draft: String::new(),
            chat: None,
            chat_clear: None,
            minimap: Default::default(),
            text: String::new(),
            text_for: None,
            text_seen: String::new(),
            text_grab: false,
            scroll_to: None,
            peek_left: false,
            peek_right: false,
            zen_seen: false,
            zen_fullscreen: false,
            zen_near: 0.0,
            seen_size: None,
        }
    }
}

pub struct BoardScreen {
    pub editor: Editor,
    /// This PC's board (not a guest's view of someone else's).
    pub host: bool,
    pub id: String,
    pub title: String,
    pub ui: BoardUi,
    need_fit: bool,
    saved_version: u64,
    dirty_since: Option<f64>,
    last_change: f64,
    thumb_due: Option<f64>,
    seen_version: u64,
    pub(crate) last_presence: f64,
    #[cfg(not(target_arch = "wasm32"))]
    store: Store,
    /// Writes the board to disk away from the interface (a big board takes a tenth of a
    /// second, enough to make typing stutter); the newest state wins when several wait.
    #[cfg(not(target_arch = "wasm32"))]
    writer: Option<(std::sync::mpsc::Sender<Vec<u8>>, std::thread::JoinHandle<()>)>,
    /// The board shared with guests, while it is.
    #[cfg(not(target_arch = "wasm32"))]
    pub share: Option<crate::share::Share>,
    /// A guest's connection to the host.
    #[cfg(target_arch = "wasm32")]
    pub guest: crate::guest::Link,
}

fn now() -> f64 {
    crate::platform::now_ms()
}

/// Images of a board read from the database, off the interface thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn store_fetch(store: &Store, board: &str) -> Fetch {
    let (store, board) = (store.clone(), board.to_string());
    Arc::new(move |file, done| {
        let (s, b) = (store.clone(), board.clone());
        std::thread::spawn(move || done(s.file(&b, &file).ok().flatten().map(|f| f.1)));
    })
}

impl BoardScreen {
    /// Around a board ready to show; `need_fit` frames the content on the first frame.
    pub(crate) fn assemble(editor: Editor, id: &str, title: String, need_fit: bool, #[cfg(not(target_arch = "wasm32"))] store: Store, #[cfg(target_arch = "wasm32")] guest: crate::guest::Link) -> BoardScreen {
        let version = editor.board.version;
        let mut b = BoardScreen {
            editor,
            host: cfg!(not(target_arch = "wasm32")),
            id: id.to_string(),
            title,
            ui: BoardUi::default(),
            need_fit,
            saved_version: version,
            dirty_since: None,
            last_change: 0.0,
            thumb_due: None,
            seen_version: version,
            last_presence: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            store,
            #[cfg(not(target_arch = "wasm32"))]
            writer: None,
            #[cfg(not(target_arch = "wasm32"))]
            share: None,
            #[cfg(target_arch = "wasm32")]
            guest,
        };
        b.update_identity();
        b
    }

    /// The name the others see, after it changed in the share dialog.
    pub fn update_identity(&mut self) {
        let ed = &mut self.editor;
        let name: String = ed.prefs.name.trim().chars().take(40).collect();
        let fallback = if self.host { "Proprietario" } else { "Ospite" };
        #[cfg(not(target_arch = "wasm32"))]
        let auto = "#0D99FF";
        #[cfg(target_arch = "wasm32")]
        let auto = crate::guest::color_for(&self.guest.token);
        let color = ed.prefs.own_color().unwrap_or(auto).to_string();
        ed.presence.set("user", serde_json::json!({ "name": if name.is_empty() { fallback } else { &name }, "color": color }));
        ed.presence.set("laserColor", serde_json::json!(ed.prefs.laser_color));
        ed.name = (!name.is_empty()).then_some(name);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl BoardScreen {
    pub fn open(ctx: &egui::Context, store: &Store, id: &str, prefs: Prefs, template: Option<String>) -> Result<BoardScreen, String> {
        let row = store.board(id)?.ok_or("Questa lavagna non esiste più.")?;
        let doc = yrs::Doc::new();
        if let Some(state) = store.ydoc(id)?
            && !state.is_empty()
        {
            apply_update(&doc, &state, REMOTE).map_err(|_| "Lavagna danneggiata: non si può aprire.".to_string())?;
        }
        let c = ctx.clone();
        let board = Board::new(doc, Arc::new(move || c.request_repaint()));
        let painter = Painter::new(Images::new(Some(store_fetch(store, id))));
        let voter = match store.setting("voter") {
            Some(v) => v,
            None => {
                let v: String = crate::model::uid().chars().take(12).collect();
                let _ = store.set_setting("voter", &v);
                v
            }
        };
        let mut editor = Editor::new(board, id.to_string(), painter, prefs, voter);
        editor.board.reroute_all();
        let mut had_camera = false;
        if let Some(cam) = store.setting(&format!("cam:{id}")).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            && let (Some(x), Some(y), Some(z)) = (cam["x"].as_f64(), cam["y"].as_f64(), cam["z"].as_f64())
            && x.is_finite()
            && y.is_finite()
            && z.is_finite()
        {
            editor.cam = Camera { x, y, z: z.clamp(crate::editor::MIN_ZOOM, crate::editor::MAX_ZOOM) };
            had_camera = true;
        }
        // A brand-new board gets the background chosen in Settings (not an undoable edit).
        if editor.board.len() == 0 && !editor.board.has_meta() {
            let meta = editor.prefs.new_board.clone();
            editor.board.transact_plain(|txn, b| {
                use yrs::Map;
                for (k, v) in serde_json::to_value(&meta).unwrap().as_object().unwrap() {
                    b.meta.insert(txn, k.as_str(), crate::doc::json_any(v));
                }
            });
        }
        if let Some(tpl) = template.as_deref().and_then(|t| crate::templates::TEMPLATES.iter().find(|x| x.id == t))
            && editor.board.len() == 0
        {
            editor.insert((tpl.build)(), Some(pt(0.0, 0.0)), false, false);
            editor.selection.clear();
            had_camera = false;
        }
        Ok(BoardScreen::assemble(editor, id, row.title, !had_camera, store.clone()))
    }

    pub fn start_share(&mut self, ctx: &egui::Context, access: crate::share::Access, auto_admit: bool, toasts: &mut Toasts) {
        self.share = None;
        let c = ctx.clone();
        let tunnel = Some(crate::share::cloudflared());
        match crate::share::Share::start(self.store.clone(), &self.id, &self.title, self.editor.board.doc.clone(), access, auto_admit, tunnel, Arc::new(move || c.request_repaint())) {
            Ok(s) => {
                self.share = Some(s);
                self.editor.presence.dirty = true;
            }
            Err(e) => toasts.error(e),
        }
    }

    pub fn stop_share(&mut self) {
        self.share = None;
        self.editor.presence.peers.clear();
        self.editor.following = None;
    }

    /// Presence both ways with the guests: ours at most every 50 ms (slower with many people).
    fn sync(&mut self, ctx: &egui::Context) {
        if !self.ui.share {
            crate::share::requests(ctx, self);
        }
        let Some(share) = &self.share else { return };
        let peers = share.peers();
        let interval = crate::net::presence_interval(peers.len() + 1);
        self.editor.presence.peers = peers;
        if self.editor.presence.dirty {
            let wait = self.last_presence + interval - now();
            if wait <= 0.0 {
                self.editor.presence.dirty = false;
                self.last_presence = now();
                share.set_host_presence(self.editor.presence_message());
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(wait as u64 + 1));
            }
        }
    }

    /// Saves everything; called when leaving the board or closing the app.
    pub fn close(&mut self) {
        crate::audio::music(None, 0.0);
        self.stop_share();
        self.editor.stop_editing();
        self.editor.flush_preview();
        self.save();
        self.finish_writing();
        self.save_camera();
        if self.thumb_due.is_some() {
            self.thumbnail(true);
        }
    }

    fn save(&mut self) {
        self.dirty_since = None;
        if self.saved_version == self.editor.board.version {
            return;
        }
        self.saved_version = self.editor.board.version;
        let state = encode_state(&self.editor.board.doc);
        let (store, id) = (self.store.clone(), self.id.clone());
        let (tx, _) = self.writer.get_or_insert_with(|| {
            let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
            let handle = std::thread::Builder::new()
                .name("tratto-save".into())
                .spawn(move || {
                    while let Ok(mut state) = rx.recv() {
                        // Only the newest of the states waiting matters.
                        while let Ok(newer) = rx.try_recv() {
                            state = newer;
                        }
                        if let Err(e) = store.save_ydoc(&id, &state) {
                            eprintln!("save failed: {e}");
                        }
                    }
                })
                .expect("save thread");
            (tx, handle)
        });
        let _ = tx.send(state);
    }

    /// Waits for the board to be on disk (leaving it, closing the app).
    fn finish_writing(&mut self) {
        if let Some((tx, handle)) = self.writer.take() {
            drop(tx);
            let _ = handle.join();
        }
    }

    fn save_camera(&mut self) {
        if std::mem::take(&mut self.editor.cam_dirty) {
            let c = self.editor.cam;
            let _ = self.store.set_setting(&format!("cam:{}", self.id), &serde_json::json!({ "x": c.x, "y": c.y, "z": c.z }).to_string());
        }
    }

    /// The picture in the board list, drawn off the interface thread.
    fn thumbnail(&mut self, wait: bool) {
        self.thumb_due = None;
        let els: Vec<Arc<El>> = self.editor.board.paint_order().iter().filter(|e| !e.hidden && !e.is_comment()).cloned().collect();
        let meta = self.editor.board.meta();
        let (store, id) = (self.store.clone(), self.id.clone());
        let job = move || {
            if let Some(png) = crate::export::thumbnail(&els, &meta, &crate::export::store_images(&store, &id)) {
                let _ = store.set_thumbnail(&id, &png);
            }
        };
        if wait { job() } else { drop(std::thread::spawn(job)) }
    }

    /// Saving: shortly after changes stop (at most every 4 s while they keep coming).
    fn persist(&mut self, ctx: &egui::Context) {
        let v = self.editor.board.version;
        let t_now = now();
        if v != self.seen_version {
            self.seen_version = v;
            self.last_change = t_now;
            self.dirty_since.get_or_insert(t_now);
            self.thumb_due = Some(t_now + 3000.0);
        }
        if let Some(since) = self.dirty_since {
            if t_now - self.last_change > 800.0 || t_now - since > 4000.0 {
                self.save();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
        if let Some(due) = self.thumb_due {
            if t_now >= due {
                self.thumbnail(false);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis((due - t_now) as u64 + 10));
            }
        }
        if self.editor.cam_dirty && self.editor.gesture.is_none() {
            self.save_camera();
        }
    }

    /* ---------- images ---------- */

    pub fn insert_image_dialog(&mut self, ctx: &egui::Context, toasts: &mut Toasts) {
        if self.editor.read_only {
            return;
        }
        let Some(paths) = rfd::FileDialog::new().set_title("Inserisci immagini").add_filter("Immagini", &["png", "jpg", "jpeg", "gif", "webp"]).pick_files() else { return };
        let files: Vec<Vec<u8>> = paths.iter().take(20).filter_map(|p| std::fs::read(p).ok()).collect();
        let at = self.editor.viewport_center();
        self.add_images(ctx, files, at, toasts);
    }

    /// Pictures put on the board: scaled down and re-encoded, stored, centred on `at`.
    pub fn add_images(&mut self, ctx: &egui::Context, files: Vec<Vec<u8>>, at: crate::geom::Pt, toasts: &mut Toasts) {
        let z = self.editor.cam.z;
        let mut els = Vec::new();
        for bytes in files.into_iter().take(20) {
            let Some((data, img)) = crate::export::prepare_image(&bytes) else {
                toasts.error("Immagine non supportata (PNG, JPG, GIF o WebP).");
                continue;
            };
            match self.store.save_file(&self.id, &data) {
                Ok(file_id) => {
                    let (w, h) = (img.size[0] as f64, img.size[1] as f64);
                    self.editor.painter.images.prime(ctx, &file_id, img);
                    let k = (640.0 / w.max(h)).min(1.0) / z;
                    let offset = els.len() as f64 * 24.0 / z;
                    let mut el = El::new(Kind::Image { file_id });
                    (el.x, el.y, el.w, el.h) = (at.x - w * k / 2.0 + offset, at.y - h * k / 2.0 + offset, w * k, h * k);
                    els.push(el);
                }
                Err(e) => toasts.error(e),
            }
        }
        if !els.is_empty() {
            self.editor.insert(els, Some(at), false, false);
        }
    }

    fn add_dropped(&mut self, ctx: &egui::Context, dropped: Vec<egui::DroppedFileHandle>, at: crate::geom::Pt, toasts: &mut Toasts) {
        let files: Vec<Vec<u8>> = dropped.iter().filter_map(|f| f.bytes().ok()).collect();
        self.add_images(ctx, files, at, toasts);
    }

    fn paste(&mut self, ctx: &egui::Context, text: &str, toasts: &mut Toasts) {
        // A picture on the clipboard wins over text, as in the first Tratto.
        if let Ok(mut cb) = arboard::Clipboard::new()
            && let Ok(img) = cb.get_image()
        {
            let mut png = Vec::new();
            if let Some(buf) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
                && image::DynamicImage::ImageRgba8(buf).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_ok()
            {
                let at = self.editor.viewport_center();
                self.add_images(ctx, vec![png], at, toasts);
                return;
            }
        }
        self.editor.paste_text(text);
    }

    pub fn is_saved(&self) -> bool {
        self.dirty_since.is_none()
    }

    /// Whether this board is being shared right now.
    pub fn sharing(&self) -> bool {
        self.share.as_ref().is_some_and(|s| s.view().status == crate::share::Status::Live)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn set_title(&mut self, title: &str, toasts: &mut Toasts) {
        match self.store.rename(&self.id, title) {
            Ok(()) => {
                self.title = crate::store::clean_title(title);
                if let Some(s) = &self.share {
                    s.set_title(&self.title);
                }
            }
            Err(e) => toasts.error(e),
        }
    }
}

impl BoardScreen {
    pub fn ui(&mut self, ui: &mut egui::Ui, toasts: &mut Toasts, dialogs: &mut Dialogs) -> Option<Action> {
        let ctx = ui.ctx().clone();
        let t = ui::theme(&ctx);
        let mut action = None;
        self.editor.board.refresh();
        // Name and colours as chosen (in the profile, the share dialog or the settings).
        self.update_identity();

        // Desktop tab bar, dark in both themes like Figma's (not in a guest's browser): the way
        // home, the settings, and the board's name. Zen mode hides it with everything else.
        #[cfg(not(target_arch = "wasm32"))]
        if !self.editor.prefs.zen {
            egui::Panel::top(Id::new("tabs")).exact_size(40.0).frame(egui::Frame::new().fill(t.chrome)).show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(8.0);
                    let chrome_button = |ui: &mut egui::Ui, name: &str, label: &str, kbd: Option<&str>| {
                        let r = ui.allocate_response(vec2(40.0, 40.0), Sense::click());
                        ui::hover_fill(ui, r.id, r.rect, r.hovered(), 0.0, t.chrome2);
                        ui::icon(ui, name, r.rect.center(), 16.0, t.chrome_text);
                        ui::tip(r, label, kbd)
                    };
                    if chrome_button(ui, "home", "Lavagne", None).clicked() {
                        action = Some(Action::Home);
                    }
                    if chrome_button(ui, "settings", "Impostazioni", Some("Ctrl+,")).clicked() {
                        dialogs.settings = true;
                    }
                    ui.add_space(4.0);
                    let g = ui.painter().layout_no_wrap(self.title.clone(), FontId::proportional(ui::px(11.0)), t.chrome_text);
                    let (tab, _) = ui.allocate_exact_size(vec2((g.size().x + 32.0).clamp(120.0, 280.0), 40.0), Sense::hover());
                    ui.painter().rect_filled(tab, 0.0, t.chrome2);
                    ui.painter().with_clip_rect((tab.shrink2(vec2(16.0, 0.0))).intersect(ui.clip_rect())).galley(pos2(tab.min.x + 16.0, tab.center().y - g.size().y / 2.0), g, t.chrome_text);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        // Zen mode, with its name: an icon alone was easy to miss.
                        let g = ui.painter().layout_no_wrap("Zen".into(), ui::medium(ui::px(11.0)), t.chrome_text);
                        let r = ui.allocate_response(vec2(g.size().x + 40.0, 40.0), Sense::click());
                        let pill = Rect::from_center_size(r.rect.center(), vec2(r.rect.width() - 8.0, 28.0));
                        ui::hover_fill(ui, r.id, pill, r.hovered(), 8.0, t.chrome2);
                        ui::icon(ui, "scan", pos2(pill.min.x + 14.0, pill.center().y), 15.0, t.chrome_text);
                        ui.painter().galley(pos2(pill.min.x + 27.0, pill.center().y - g.size().y / 2.0), g, t.chrome_text);
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Modalità zen"));
                        if ui::tip(r, "Modalità zen", Some("Alt+Z")).clicked() {
                            self.editor.toggle_zen();
                        }
                    });
                });
            });
        }

        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            if let Some(a) = self.stage(ui, &t, toasts, dialogs) {
                action = Some(a);
            }
        });

        self.persist(&ctx);
        self.sync(&ctx);
        action
    }

    /// The board, with everything floating over it.
    fn stage(&mut self, ui: &mut egui::Ui, t: &Theme, toasts: &mut Toasts, dialogs: &mut Dialogs) -> Option<Action> {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        let mut action_from_panel = None;
        // Figma UI3: the panels are cards floating over the board, which runs underneath them.
        // Zen mode takes the whole screen on a computer, and gives the window back after.
        #[cfg(not(target_arch = "wasm32"))]
        if self.editor.prefs.zen != self.ui.zen_seen {
            self.ui.zen_seen = self.editor.prefs.zen;
            let full = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
            if self.editor.prefs.zen && !full {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                self.ui.zen_fullscreen = true;
            } else if !self.editor.prefs.zen && std::mem::take(&mut self.ui.zen_fullscreen) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
        }
        let prefs = &self.editor.prefs;
        let zen = prefs.zen;
        // A window too narrow for both panels and the board: the right one waits in its pill,
        // and in a very narrow one the left one too.
        let narrow = rect.width() < 1000.0;
        let tiny = rect.width() < 720.0;
        let is_mobile = rect.width() < 640.0;
        let (show_left, show_right) = (!zen && !prefs.focus && prefs.left_panel && (!tiny || self.ui.peek_left), !zen && !prefs.focus && prefs.right_panel && (!narrow || self.ui.peek_right));
        ctx.data_mut(|d| d.insert_temp(Id::new("panels-shown"), (show_left, show_right)));
        let kl = ui::motion::presence(&ctx, Id::new("left-panel"), show_left);
        let kr = ui::motion::presence(&ctx, Id::new("right-panel"), show_right);
        const GAP: f32 = 8.0;
        // Wider with bigger text, so names and fields keep their room.
        let panel: f32 = if is_mobile { (rect.width() - GAP * 2.0).min(320.0) } else { (240.0 * (1.0 + (ui::text_scale() - 1.0) * 0.6)).round() };
        self.editor.safe = [if show_left && !is_mobile { (panel + GAP * 2.0) as f64 } else { 0.0 }, 0.0, if show_right && !is_mobile { (panel + GAP * 2.0) as f64 } else { 0.0 }, 0.0];
        let left_rect = Rect::from_min_size(rect.min + vec2(GAP, GAP), vec2(panel, rect.height() - GAP * 2.0));
        let right_rect = Rect::from_min_size(pos2(rect.max.x - GAP - panel, rect.min.y + GAP), vec2(panel, rect.height() - GAP * 2.0));
        panel_card(&ctx, "left-panel", left_rect, kl, -1.0, t, |ui| {
            if let Some(a) = super::panels::left(ui, self, toasts, dialogs) {
                action_from_panel = Some(a);
            }
        });
        panel_card(&ctx, "right-panel", right_rect, kr, 1.0, t, |ui| super::panels::right(ui, self, toasts));
        // What floats over the board centres on the part the panels leave free.
        let stage = rect;
        let rect = if is_mobile {
            stage
        } else {
            Rect::from_min_max(rect.min + vec2((panel + GAP) * kl.clamp(0.0, 1.0), 0.0), rect.max - vec2((panel + GAP) * kr.clamp(0.0, 1.0), 0.0))
        };

        let resp = ui.interact(stage, Id::new("board"), Sense::click_and_drag());
        let ed = &mut self.editor;
        // The window resized (a phone turned, a window restored): what was in the middle stays in
        // the middle, instead of sliding off to a corner.
        let size = (stage.width() as f64, stage.height() as f64);
        if let Some(was) = self.ui.seen_size
            && was != size
            && size.0 > 10.0
        {
            let (dx, dy) = ((size.0 - was.0) / 2.0, (size.1 - was.1) / 2.0);
            let c = ed.cam;
            ed.set_cam(Camera { x: c.x + dx, y: c.y + dy, z: c.z });
        }
        self.ui.seen_size = Some(size);
        ed.size = size;
        ed.origin = stage.min;
        if self.need_fit && stage.width() > 10.0 {
            self.need_fit = false;
            ed.fit_now();
        }
        let any_dialog = self.ui.export.is_some() || self.ui.share || self.ui.shortcuts || dialogs.settings || self.ui.menu.is_some();
        let keys = !ctx.egui_wants_keyboard_input() && !any_dialog;
        let hovered = resp.hovered() && !any_dialog;
        let dbl = if resp.double_clicked() { resp.interact_pointer_pos() } else { None };
        ed.handle_input(&ctx, hovered, keys, dbl);
        if ed.tick_camera(crate::platform::now_ms()) {
            ctx.request_repaint();
        }

        // The board.
        let painter = ui.painter_at(stage);
        let mut shapes = Vec::new();
        let ppp = ctx.pixels_per_point();
        crate::paint::background(&mut shapes, stage, &ed.cam, &ed.board.meta(), ppp);
        let els = ed.paint_list();
        // What lies entirely under a panel is not drawn: the panels are opaque.
        let seen = rect.expand(GAP);
        let (a, b) = (ed.cam.to_world((seen.min.x - stage.min.x) as f64, (seen.min.y - stage.min.y) as f64), ed.cam.to_world((seen.max.x - stage.min.x) as f64, (seen.max.y - stage.min.y) as f64));
        let view = crate::geom::BBox { x: a.x, y: a.y, w: b.x - a.x, h: b.y - a.y };
        let glides = ed.board.glide_offsets();
        if !glides.is_empty() {
            ctx.request_repaint();
        }
        let editing = ed.editing.clone();
        let frame = Frame { cam: ed.cam, origin: stage.min, view, pixels_per_point: ppp, editing: editing.as_deref(), editing_cell: ed.editing_cell, glides: (!glides.is_empty()).then_some(&glides), dark_board: crate::model::is_dark(&ed.board.meta().background) };
        let on_screen = ed.painter.paint(&ctx, &mut shapes, &els, &frame);
        let any_visible = els.iter().any(|e| !e.hidden);
        ed.lost = any_visible && !on_screen;
        painter.extend(shapes);
        ed.paint_overlay(&painter, t.dark);
        if ed.gesture.is_some() || !ed.laser.is_empty() || ed.presence.peers.iter().any(|(_, s)| s["laser"].is_array()) {
            ctx.request_repaint();
        }

        self.text_editor(&ctx, stage, t);
        super::live::comment_thread(&ctx, &mut self.editor, &mut self.ui);
        super::live::cursor_chat(&ctx, &mut self.editor, &mut self.ui);
        let mut action = action_from_panel;
        // Zen mode: nothing over the board but what is being done on it, and a way out.
        let zen = self.editor.prefs.zen;
        // In zen mode the toolbar steps aside: it shows when the pointer (or a finger) comes near
        // its edge, stays a moment, and slides away while something is being drawn.
        let dock = if zen {
            let top = self.editor.prefs.toolbar_pos == crate::prefs::ToolbarPos::Top;
            let near = ctx.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos())).is_some_and(|p| if top { p.y < stage.min.y + 140.0 } else { p.y > stage.max.y - 140.0 });
            if near || egui::Popup::is_any_open(&ctx) {
                self.ui.zen_near = crate::platform::now_ms();
            }
            let recent = crate::platform::now_ms() - self.ui.zen_near < 2500.0;
            if recent {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            }
            ui::motion::presence(&ctx, Id::new("zen-dock"), recent && self.editor.gesture.is_none())
        } else {
            1.0
        };
        ctx.data_mut(|d| d.insert_temp(Id::new("toolbar-dock"), dock));
        if zen {
            if dock > 0.01 {
                super::toolbar::toolbar(&ctx, rect, &mut self.editor, &mut self.ui);
            } else {
                ctx.data_mut(|d| d.remove::<Rect>(Id::new("bottom-bars")));
            }
        } else {
            let (left_shown, right_shown) = ctx.data(|d| d.get_temp::<(bool, bool)>(Id::new("panels-shown"))).unwrap_or((true, true));
            // On a phone a panel takes the whole screen: the bars and pills wait until it is closed
            // (drawn over it, they covered its fields).
            let phone_panel = ui::narrow(&ctx) && (left_shown || right_shown);
            ctx.data_mut(|d| d.remove::<Rect>(Id::new("ctx-bar-rect")));
            if phone_panel {
                ctx.data_mut(|d| d.remove::<Rect>(Id::new("bottom-bars")));
            } else {
                super::context_bar::context_bar(&ctx, rect, &mut self.editor);
                super::toolbar::toolbar(&ctx, rect, &mut self.editor, &mut self.ui);
            }
            let is_mobile = stage.width() < 600.0;
            // Where the minimap is this frame, for what has to keep clear of it (none: not shown).
            ctx.data_mut(|d| {
                for k in ["minimap-rect", "left-pill-rect", "right-pill-rect"] {
                    d.remove::<Rect>(Id::new(k));
                }
            });
            if !self.editor.prefs.focus && !is_mobile {
                super::toolbar::view_controls(&ctx, rect, &mut self.editor);
                if self.editor.prefs.minimap {
                    super::toolbar::minimap(&ctx, rect, &mut self.editor, &mut self.ui.minimap);
                }
            }
            if !left_shown && !phone_panel {
                if let Some(a) = super::panels::focus_pill(&ctx, rect, self, dialogs) {
                    action = Some(a);
                }
            }
            if !right_shown && !self.editor.prefs.focus && !phone_panel {
                super::panels::right_pill(&ctx, rect, self);
            }
        }
        if !zen {
            super::toolbar::shapes_panel(&ctx, rect, &mut self.editor);
        }
        ctx.data_mut(|d| d.insert_temp(Id::new("board-host"), self.host));
        super::live::vote_results(&ctx, rect, &mut self.editor);
        super::live::top_bars(&ctx, rect, &mut self.editor);
        super::live::sound(&ctx, &self.editor);
        // The others see what this person has selected, and where they are typing.
        let ed = &mut self.editor;
        let sel: Vec<&String> = ed.selection.iter().take(200).collect();
        let sel = serde_json::json!(sel);
        ed.presence.set("sel", sel);
        let typing = ed.editing.as_ref().map(|id| serde_json::json!({ "id": id, "cell": ed.editing_cell.map(|(r, c)| [r, c]) }));
        ed.presence.set("editing", typing.unwrap_or_default());
        super::toolbar::insert_drag(&ctx, stage, &mut self.editor);
        if zen {
            super::panels::zen_pill(&ctx, rect, &mut self.editor);
        }

        for req in std::mem::take(&mut self.editor.requests) {
            match req {
                Request::Menu(p) => self.ui.menu = Some(p),
                Request::Export { selection } => self.ui.export = Some(selection),
                Request::InsertImage => self.insert_image_dialog(&ctx, toasts),
                Request::Chat => self.ui.chat = Some(String::new()),
                Request::FolderCreated(id) => {
                    self.ui.open_folders.insert(id.clone());
                    self.ui.rename_buf = self.editor.board.group_name(&id);
                    self.ui.renaming = Some(format!("folder:{id}"));
                }
                Request::Paste(text) => self.paste(&ctx, &text, toasts),
                Request::Shortcuts => self.ui.shortcuts = true,
                Request::Templates => {
                    // Shown even where the panel only shows on request (a phone, a narrow window).
                    self.ui.left_tab = super::board::LeftTab::Templates;
                    self.ui.peek_left = true;
                    if ui::narrow(&ctx) {
                        self.ui.peek_right = false;
                    }
                }
                Request::OpenLink(url) => {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        ctx.open_url(egui::OpenUrl::new_tab(url));
                    }
                }
                Request::Notice(text) => toasts.info(text),
            }
        }
        // Files dropped on the window: pictures go on the board where they land.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() && !self.editor.read_only {
            let at = ctx.input(|i| i.pointer.hover_pos()).map(|p| self.editor.to_world(self.editor.screen_pt(p))).unwrap_or_else(|| self.editor.viewport_center());
            self.add_dropped(&ctx, dropped, at, toasts);
        }
        self.context_menu(&ctx);
        super::dialogs::shortcuts(&ctx, &mut self.ui.shortcuts);
        // Dialogs are called while closed too, so they fade away instead of vanishing.
        {
            let mut open = self.ui.export.is_some();
            if let Some(sel) = self.ui.export {
                self.ui.export_sel = sel;
            }
            let sel = self.ui.export_sel;
            super::dialogs::export(&ctx, &mut open, sel, self, toasts);
            if !open {
                self.ui.export = None;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut open = self.ui.share;
            super::dialogs::share(&ctx, &mut open, self, toasts);
            self.ui.share = open;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.ui.menu.is_some() {
            self.ui.menu = None;
        }
        if self.ui.export.is_none() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::E)) {
            self.ui.export = Some(false);
        }
        action
    }

    /* ---------- typing on the board ---------- */

    fn text_editor(&mut self, ctx: &egui::Context, stage: Rect, t: &Theme) {
        let ed = &mut self.editor;
        // On a phone an open panel covers the board: the writing there ends (its box and its
        // tools would stay on top of the panel).
        let (left, right) = ctx.data(|d| d.get_temp::<(bool, bool)>(Id::new("panels-shown"))).unwrap_or_default();
        if ui::narrow(ctx) && (left || right) && ed.editing.is_some() {
            ed.stop_editing();
        }
        let Some(id) = ed.editing.clone() else {
            self.ui.text_for = None;
            return;
        };
        let Some(el) = ed.board.get(&id).cloned() else {
            ed.editing = None;
            return;
        };
        // A table reloads the text box for each cell.
        if let Some(tb) = el.table() {
            let (r, c) = ed.editing_cell.unwrap_or((0, 0));
            ed.editing_cell = Some((r.min(tb.rows.len() - 1), c.min(tb.cols.len() - 1)));
        }
        let key = format!("{id}/{:?}", ed.editing_cell);
        let written = written_text(&el, ed.editing_cell);
        if self.ui.text_for.as_deref() != Some(key.as_str()) {
            // Someone else is already typing right here: one at a time, or each would undo the other.
            let cell = ed.editing_cell.map(|(r, c)| serde_json::json!([r, c])).unwrap_or_default();
            if let Some(who) = ed.presence.peers.iter().find(|(_, s)| s["editing"]["id"].as_str() == Some(id.as_str()) && s["editing"]["cell"] == cell).map(|(_, s)| crate::editor::peer_name(s)) {
                ed.editing = None;
                ed.editing_cell = None;
                self.ui.text_for = None;
                ed.requests.push(crate::editor::Request::Notice(format!("{who} sta già scrivendo qui")));
                return;
            }
            self.ui.text_for = Some(key);
            self.ui.text = written.clone();
            self.ui.text_seen = written;
            ed.board.stop_capturing();
            ctx.memory_mut(|m| m.request_focus(Id::new("board-text")));
            self.ui.text_grab = true;
        } else if written != self.ui.text_seen {
            // Someone else typed here meanwhile (both started at once): their text, with the
            // cursor kept by the same letters.
            let id = Id::new("board-text");
            if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
                if let Some(r) = state.cursor.char_range() {
                    let at = |c: egui::text::CCursor| egui::text::CCursor::new(crate::text::shift_cursor(&self.ui.text, &written, c.index.0));
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(at(r.secondary), at(r.primary))));
                }
                state.store(ctx, id);
            }
            self.ui.text = written.clone();
            self.ui.text_seen = written;
        }
        let z = ed.cam.z as f32;
        let p = ed.to_screen(el.x, el.y) + stage.min.to_vec2();
        let family = |face: usize| FontFamily::Name(crate::text::FILES[face].0.into());
        // Where the text goes, its font, colour and alignment.
        let (rect, font, color, align, single, wrap) = match &el.kind {
            Kind::Text(x) => {
                let f = crate::text::text_font(x);
                let w = if x.fixed_width { el.w as f32 * z } else { (el.w as f32 * z).max(8.0) + x.font_size as f32 * z };
                (Rect::from_min_size(p, vec2(w, (el.h as f32 * z).max(x.font_size as f32 * z * 1.3))), FontId::new(x.font_size as f32 * z, family(f.face)), crate::model::color_or(&x.color, crate::model::DARK), x.align, false, x.fixed_width)
            }
            Kind::Sticky(s) => {
                let (layout, size) = crate::text::sticky_layout(&el, s);
                let pad = crate::text::STICKY_PAD as f32 * z;
                let band = crate::text::author_band(&el, s) as f32 * z;
                let h = (layout.height as f32).max(size as f32 * 1.3) * z;
                let inner = Rect::from_min_max(p + vec2(pad, pad), p + vec2(el.w as f32 * z - pad, el.h as f32 * z - pad - band));
                let r = Rect::from_center_size(inner.center(), vec2(inner.width(), h.min(inner.height().max(h))));
                (r, FontId::new(size as f32 * z, family(crate::text::sticky_font(s).face)), crate::text::sticky_text_color(s), s.align, false, true)
            }
            Kind::Shape(s) => {
                let st = crate::text::shape_text_layout(&el, s);
                let h = (st.layout.height as f32).max(st.size as f32 * 1.3) * z;
                let r = Rect::from_min_size(p + vec2(st.left as f32 * z, st.top as f32 * z), vec2(st.width as f32 * z, h));
                (r, FontId::new(st.size as f32 * z, family(crate::text::shape_font(s).face)), crate::text::shape_text_color(s), s.align.unwrap_or(crate::model::Align::Center), false, true)
            }
            Kind::Table(tb) => {
                let (r, c) = ed.editing_cell.unwrap_or((0, 0));
                let (x, y, cw, _) = crate::table::cell_box(tb, r, c);
                let pad = crate::model::CELL_PAD;
                let f = crate::table::cell_font(tb, r);
                let lines = crate::table::cell_lines(tb, r, c).len().max(1) as f32;
                let size = tb.font_size as f32;
                let rect = Rect::from_min_size(p + vec2((x + pad) as f32 * z, (y + pad) as f32 * z), vec2(((cw - pad * 2.0) as f32 * z).max(8.0), lines * size * 1.3 * z));
                (rect, FontId::new(size * z, family(f.face)), crate::table::cell_color(tb, r, c), tb.align.unwrap_or(crate::model::Align::Left), false, true)
            }
            Kind::Line(l) => {
                let (mx, my) = crate::geom::route_mid(l);
                let c = ed.to_screen(el.x + mx, el.y + my) + stage.min.to_vec2();
                let size = crate::prims::label_size(l) as f32 * z;
                let w = (size * 10.0).clamp(120.0, 360.0);
                let rect = Rect::from_center_size(c, vec2(w, size * 1.4));
                (rect, FontId::new(size.max(11.0), family(crate::text::font(crate::model::FontKind::Sans, false, false).face)), t.text, crate::model::Align::Center, true, false)
            }
            Kind::Code(c) => {
                let l = crate::code::layout(&el, c);
                let left = l.pad + l.gutter;
                let top = crate::code::code_top(&l, c.font_size, 0);
                let rect = Rect::from_min_size(p + vec2(left as f32 * z, top as f32 * z), vec2(((el.w - left - l.pad) as f32 * z).max(8.0), (l.lines.len() as f64 * l.line_h) as f32 * z));
                let mono = crate::text::font(crate::model::FontKind::Mono, false, false);
                (rect, FontId::new(c.font_size as f32 * z, family(mono.face)), crate::code::color(crate::code::Tok::Plain, c.light), crate::model::Align::Left, false, true)
            }
            Kind::Section { .. } => {
                let tb = crate::text::section_title_box(&el, ed.cam.z);
                let a = ed.to_screen(tb.x, tb.y) + stage.min.to_vec2();
                (Rect::from_min_size(a, vec2((tb.w as f32 * z).max(120.0), (tb.h as f32 * z).max(22.0))), FontId::new(12.0, FontFamily::Name("medium".into())), t.text, crate::model::Align::Left, true, false)
            }
            _ => {
                ed.editing = None;
                return;
            }
        };
        let halign = match align {
            crate::model::Align::Left => egui::Align::Min,
            crate::model::Align::Center => egui::Align::Center,
            crate::model::Align::Right => egui::Align::Max,
        };
        let mut changed = false;
        let mut finish = false;
        let mut tab = None;
        // Code is coloured while it is typed, with the same rows as when it is drawn.
        let code = el.code().map(|c| (c.language.clone(), c.light, (c.font_size * 1.55) as f32 * z));
        let is_table = el.table().is_some();
        // In a shape or a note, Tab goes on with the diagram: a connected node, ready to write in.
        let branches = matches!(el.kind, Kind::Shape(_) | Kind::Sticky(_));
        let mut grow = None;
        egui::Area::new(Id::new("board-text-area")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
            ui.set_min_size(rect.size());
            if branches && !self.editor.read_only {
                grow = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                        Some(true)
                    } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                        Some(false)
                    } else {
                        None
                    }
                });
            }
            if is_table {
                tab = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                        Some(true)
                    } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                        Some(false)
                    } else {
                        None
                    }
                });
            }
            let edit = if single { egui::TextEdit::singleline(&mut self.ui.text) } else { egui::TextEdit::multiline(&mut self.ui.text) };
            let mut edit = edit.id(Id::new("board-text")).font(font.clone()).text_color(color).frame(egui::Frame::NONE).horizontal_align(halign).desired_width(rect.width()).margin(egui::Margin::ZERO).lock_focus(true);
            if single {
                edit = edit.background_color(t.bg).frame(egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.brand)).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 2)));
            }
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                if let Some((lang, light, line_h)) = &code {
                    let mut job = egui::text::LayoutJob::default();
                    let mut in_block = false;
                    let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, line_height: Some(*line_h), ..Default::default() };
                    for (i, line) in text.as_str().split('\n').enumerate() {
                        if i > 0 {
                            job.append("\n", 0.0, fmt(color));
                        }
                        for (tok, piece) in crate::code::tokens(lang, line, &mut in_block) {
                            job.append(piece, 0.0, fmt(crate::code::color(tok, *light)));
                        }
                    }
                    job.wrap.max_width = wrap_width;
                    return ui.fonts_mut(|f| f.layout_job(job));
                }
                let mut job = egui::text::LayoutJob::simple(text.as_str().to_string(), font.clone(), color, if wrap { wrap_width } else { f32::INFINITY });
                job.halign = halign;
                job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let out = edit.layouter(&mut layouter).show(ui);
            if out.response.changed() {
                changed = true;
                // Lists carry on with Enter (and end on an empty item), as in FigJam.
                if !single
                    && !is_table
                    && code.is_none()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(at) = out.cursor_range.map(|r| r.primary.index.0)
                    && let Some((text, cursor)) = crate::text::continue_list(&self.ui.text, at)
                {
                    self.ui.text = text;
                    let mut state = out.state.clone();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor))));
                    state.store(ui.ctx(), Id::new("board-text"));
                }
            }
            let (esc, ctrl_enter, enter) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.modifiers.command && i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Enter)));
            if esc || ctrl_enter || (single && enter) {
                finish = true;
            }
        });
        if self.ui.text_grab {
            ctx.memory_mut(|m| m.request_focus(Id::new("board-text")));
            self.ui.text_grab = ctx.input(|i| i.pointer.any_down());
        }
        if changed {
            let text = self.ui.text.clone();
            self.editor.set_text(&id, text);
            if let Some(el) = self.editor.board.get(&id) {
                self.ui.text_seen = written_text(&el, self.editor.editing_cell);
            }
        }
        if let Some(back) = tab {
            self.editor.next_cell(back);
        }
        if let Some(sibling) = grow {
            self.ui.text_for = None;
            self.editor.grow_diagram(&id, sibling);
            return;
        }
        if finish {
            self.editor.stop_editing();
            self.ui.text_for = None;
            ui::motion::reset(ctx, Id::new("text-tools-anim"));
            ui::motion::reset(ctx, Id::new("table-tools-anim"));
            ui::motion::reset(ctx, Id::new("code-tools-anim"));
        } else if let Some(el) = self.editor.board.get(&id).cloned()
            && !matches!(el.kind, Kind::Section { .. })
        {
            super::panels::text_tools(ctx, stage, &mut self.editor, &el);
        }
    }

    /* ---------- context menu ---------- */

    fn context_menu(&mut self, ctx: &egui::Context) {
        let Some(at) = self.ui.menu else {
            ctx.data_mut(|d| d.remove::<Pos2>(Id::new("board-menu-at")));
            ui::motion::reset(ctx, Id::new("board-menu-anim"));
            return;
        };
        let t = ui::theme(ctx);
        let mut close = false;
        // Measured afresh each time it opens: as wide as its longest row, not a pixel more.
        let fresh = ctx.data(|d| d.get_temp::<Pos2>(Id::new("board-menu-at"))) != Some(at);
        ctx.data_mut(|d| d.insert_temp(Id::new("board-menu-at"), at));
        let k = ui::motion::appear(ctx, Id::new("board-menu-anim"), true);
        let lift = ui::motion::travel(ctx, k) * 4.0;
        let resp = egui::Area::new(Id::new("board-menu")).fixed_pos(at + vec2(0.0, lift)).order(egui::Order::Foreground).constrain(true).sizing_pass(fresh).show(ctx, |ui| {
            ui.set_opacity(k.clamp(0.0, 1.0));
            ui::menu_frame(&t).show(ui, |ui| {
                ui.set_min_width(180.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                close = super::panels::selection_menu(ui, &mut self.editor, &mut self.ui);
            });
        });
        let clicked_out = ctx.input(|i| i.pointer.any_pressed()) && !resp.response.contains_pointer() && resp.response.ctx.input(|i| i.pointer.press_origin()).is_some_and(|p| !resp.response.rect.contains(p));
        if close || clicked_out {
            self.ui.menu = None;
            ui::motion::reset(ctx, Id::new("board-menu-anim"));
        }
    }

    /// Board box of the selection or of everything (for exports and fitting).
    pub fn content(&mut self, only_selection: bool) -> Option<crate::geom::BBox> {
        if only_selection {
            union(self.editor.selected().iter().map(|e| frame_box(e)))
        } else {
            self.editor.content_box()
        }
    }


    pub fn tool(&self) -> Tool {
        self.editor.tool
    }

    pub fn images(&self) -> impl Fn(&ImageKey) -> Option<Arc<tiny_skia::Pixmap>> + '_ {
        |k| self.editor.painter.images.pixmap(k)
    }
}
/// A Figma UI3 panel: a rounded card floating over the board, sliding in from its side
/// (`side` -1 left, 1 right) as `k` goes from 0 to 1.
fn panel_card(ctx: &egui::Context, id: &str, rect: Rect, k: f32, side: f32, t: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    if k <= 0.01 {
        return;
    }
    let rect = rect.translate(vec2(ui::motion::travel(ctx, k) * 20.0 * side, 0.0));
    egui::Area::new(Id::new(id)).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        // The whole card takes the pointer, also where it is empty.
        let (r, _) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
        let radius = egui::CornerRadius::same(ui::RADIUS_LG);
        for s in ui::card_shadows(t) {
            ui.painter().add(s.as_shape(r, radius));
        }
        ui.painter().rect(r, radius, t.bg, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let inner = r.shrink(1.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        // Rectangular clipping: keep the content off the rounded bottom corners.
        child.set_clip_rect(Rect::from_min_max(inner.min, inner.max - vec2(0.0, 6.0)));
        add(&mut child);
    });
}

/// The text of `el` the board-text box edits (a table's: of the cell being edited).
fn written_text(el: &crate::model::El, cell: Option<(usize, usize)>) -> String {
    match &el.kind {
        Kind::Text(x) => x.text.clone(),
        Kind::Sticky(s) => s.text.clone(),
        Kind::Shape(s) => s.text.clone().unwrap_or_default(),
        Kind::Section { .. } => el.name.clone().unwrap_or_default(),
        Kind::Code(c) => c.code.clone(),
        Kind::Line(l) => l.label.clone().unwrap_or_default(),
        Kind::Table(tb) => {
            let (r, c) = cell.unwrap_or((0, 0));
            tb.cells.get(r).and_then(|row| row.get(c)).map(|c| c.text.clone()).unwrap_or_default()
        }
        _ => String::new(),
    }
}
