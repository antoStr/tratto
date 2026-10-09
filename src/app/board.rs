//! An open board on this PC: the tab bar, the panels, the board itself with the text being
//! typed and the context menu, and saving (document, thumbnail, camera) as it changes.

use std::collections::HashSet;
use std::sync::Arc;

use egui::{Color32, FontFamily, FontId, Id, Pos2, Rect, Sense, Stroke, vec2};

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
    pub scroll_to: Option<String>,
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
            share: false,
            shortcuts: false,
            title_edit: None,
            comment_draft: String::new(),
            chat: None,
            chat_clear: None,
            minimap: Default::default(),
            text: String::new(),
            text_for: None,
            scroll_to: None,
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
        let color = "#0D99FF";
        #[cfg(target_arch = "wasm32")]
        let color = crate::guest::color_for(&self.guest.token);
        ed.presence.set("user", serde_json::json!({ "name": if name.is_empty() { fallback } else { &name }, "color": color }));
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
            apply_update(&doc, &state, REMOTE).map_err(|_| "Questa lavagna è danneggiata e non si può aprire.".to_string())?;
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
                share.set_host_presence(serde_json::Value::Object(self.editor.presence.local.clone()));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(wait as u64 + 1));
            }
        }
    }

    /// Saves everything; called when leaving the board or closing the app.
    pub fn close(&mut self) {
        self.stop_share();
        self.editor.stop_editing();
        self.editor.flush_preview();
        self.save();
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
        if let Err(e) = self.store.save_ydoc(&self.id, &state) {
            eprintln!("save failed: {e}");
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
                toasts.error("Questa immagine non si può aprire. Usa PNG, JPG, GIF o WebP.");
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

        // Desktop tab bar, dark in both themes like Figma's (not in a guest's browser).
        #[cfg(not(target_arch = "wasm32"))]
        egui::Panel::top(Id::new("tabs")).exact_size(40.0).frame(egui::Frame::new().fill(t.chrome)).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                let home = ui.allocate_response(vec2(40.0, 40.0), Sense::click());
                if home.hovered() {
                    ui.painter().rect_filled(home.rect, 0.0, t.chrome2);
                }
                ui::icon(ui, "home", home.rect.center(), 16.0, t.chrome_text);
                if ui::tip(home, "Lavagne", None).clicked() {
                    action = Some(Action::Home);
                }
                let (tab, _) = ui.allocate_exact_size(vec2(200.0, 40.0), Sense::hover());
                ui.painter().rect_filled(tab, 0.0, t.chrome2);
                ui.painter().with_clip_rect(tab).text(tab.left_center() + vec2(32.0, 0.0), egui::Align2::LEFT_CENTER, &self.title, FontId::proportional(11.0), t.chrome_text);
                let logo = Rect::from_center_size(tab.left_center() + vec2(16.0, 0.0), vec2(14.0, 14.0));
                ui::icons::logo(ui, logo);
            });
        });

        let prefs = &self.editor.prefs;
        let show_ui = !prefs.focus;
        if show_ui && prefs.left_panel {
            egui::Panel::left(Id::new("left")).exact_size(240.0).resizable(false).frame(egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.border))).show(ui, |ui| {
                if let Some(a) = super::panels::left(ui, self, toasts, dialogs) {
                    action = Some(a);
                }
            });
        }
        if show_ui && self.editor.prefs.right_panel {
            egui::Panel::right(Id::new("right")).exact_size(240.0).resizable(false).frame(egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.border))).show(ui, |ui| {
                super::panels::right(ui, self, toasts);
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
        let resp = ui.interact(rect, Id::new("board"), Sense::click_and_drag());
        let ed = &mut self.editor;
        ed.size = (rect.width() as f64, rect.height() as f64);
        ed.origin = rect.min;
        if self.need_fit && rect.width() > 10.0 {
            self.need_fit = false;
            if ed.board.len() > 0 { ed.fit() } else { ed.set_cam(Camera { x: rect.width() as f64 / 2.0, y: rect.height() as f64 / 2.0, z: 1.0 }) }
        }
        let any_dialog = self.ui.export.is_some() || self.ui.share || self.ui.shortcuts || dialogs.settings || self.ui.menu.is_some();
        let keys = !ctx.egui_wants_keyboard_input() && !any_dialog;
        let hovered = resp.hovered() && !any_dialog;
        let dbl = if resp.double_clicked() { resp.interact_pointer_pos() } else { None };
        ed.handle_input(&ctx, hovered, keys, dbl);

        // The board.
        let painter = ui.painter_at(rect);
        let mut shapes = Vec::new();
        let ppp = ctx.pixels_per_point();
        crate::paint::background(&mut shapes, rect, &ed.cam, &ed.board.meta(), ppp);
        let els = ed.paint_list();
        let frame = Frame { cam: ed.cam, origin: rect.min, view: ed.view(), pixels_per_point: ppp, editing: ed.editing.as_deref() };
        let editing = ed.editing.clone();
        let frame = Frame { editing: editing.as_deref(), ..frame };
        let on_screen = ed.painter.paint(&ctx, &mut shapes, &els, &frame);
        let any_visible = els.iter().any(|e| !e.hidden);
        ed.lost = any_visible && !on_screen;
        painter.extend(shapes);
        ed.paint_overlay(&painter, t.dark);
        if ed.gesture.is_some() || !ed.laser.is_empty() || ed.presence.peers.iter().any(|(_, s)| s["laser"].is_array()) {
            ctx.request_repaint();
        }

        self.text_editor(&ctx, rect, t);
        super::live::comment_thread(&ctx, &mut self.editor, &mut self.ui);
        super::live::cursor_chat(&ctx, &mut self.editor, &mut self.ui);
        let mut action = None;
        super::toolbar::toolbar(&ctx, rect, &mut self.editor, &mut self.ui);
        super::live::top_bars(&ctx, rect, &mut self.editor);
        if !self.editor.prefs.focus {
            super::toolbar::view_controls(&ctx, rect, &mut self.editor);
            if self.editor.prefs.minimap {
                super::toolbar::minimap(&ctx, rect, &mut self.editor, &mut self.ui.minimap);
            }
        }
        if self.editor.prefs.focus || !self.editor.prefs.left_panel {
            if let Some(a) = super::panels::focus_pill(&ctx, rect, self, dialogs) {
                action = Some(a);
            }
        }
        if !self.editor.prefs.focus && !self.editor.prefs.right_panel {
            super::panels::right_pill(&ctx, rect, self);
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
        if let Some(sel) = self.ui.export {
            let mut open = true;
            super::dialogs::export(&ctx, &mut open, sel, self, toasts);
            if !open {
                self.ui.export = None;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.ui.share {
            let mut open = true;
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
        let Some(id) = ed.editing.clone() else {
            self.ui.text_for = None;
            return;
        };
        let Some(el) = ed.board.get(&id).cloned() else {
            ed.editing = None;
            return;
        };
        if self.ui.text_for.as_deref() != Some(id.as_str()) {
            self.ui.text_for = Some(id.clone());
            self.ui.text = match &el.kind {
                Kind::Text(x) => x.text.clone(),
                Kind::Sticky(s) => s.text.clone(),
                Kind::Shape(s) => s.text.clone().unwrap_or_default(),
                Kind::Section { .. } => el.name.clone().unwrap_or_default(),
                _ => String::new(),
            };
            ed.board.stop_capturing();
            ctx.memory_mut(|m| m.request_focus(Id::new("board-text")));
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
                (r, FontId::new(size as f32 * z, family(crate::text::font(s.font, false, false).face)), if crate::model::is_dark(&s.color) { Color32::WHITE } else { crate::model::DARK }, s.align, false, true)
            }
            Kind::Shape(s) => {
                let st = crate::text::shape_text_layout(&el, s);
                let h = (st.layout.height as f32).max(st.size as f32 * 1.3) * z;
                let r = Rect::from_min_size(p + vec2(st.left as f32 * z, st.top as f32 * z), vec2(st.width as f32 * z, h));
                (r, FontId::new(st.size as f32 * z, family(crate::text::font(s.font.unwrap_or_default(), false, false).face)), crate::text::shape_text_color(s), crate::model::Align::Center, false, true)
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
        egui::Area::new(Id::new("board-text-area")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
            ui.set_min_size(rect.size());
            let edit = if single { egui::TextEdit::singleline(&mut self.ui.text) } else { egui::TextEdit::multiline(&mut self.ui.text) };
            let mut edit = edit.id(Id::new("board-text")).font(font.clone()).text_color(color).frame(egui::Frame::NONE).horizontal_align(halign).desired_width(rect.width()).margin(egui::Margin::ZERO).lock_focus(true);
            if single {
                edit = edit.background_color(t.bg).frame(egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.brand)).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 2)));
            }
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut job = egui::text::LayoutJob::simple(text.as_str().to_string(), font.clone(), color, if wrap { wrap_width } else { f32::INFINITY });
                job.halign = halign;
                job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let resp = ui.add(edit.layouter(&mut layouter));
            if resp.changed() {
                changed = true;
            }
            let (esc, ctrl_enter, enter) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.modifiers.command && i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Enter)));
            if esc || ctrl_enter || (single && enter) {
                finish = true;
            }
        });
        if changed {
            let text = self.ui.text.clone();
            self.editor.set_text(&id, text);
        }
        if finish {
            self.editor.stop_editing();
            self.ui.text_for = None;
        } else if let Some(el) = self.editor.board.get(&id).cloned()
            && !matches!(el.kind, Kind::Section { .. })
        {
            super::panels::text_tools(ctx, stage, &mut self.editor, &el);
        }
    }

    /* ---------- context menu ---------- */

    fn context_menu(&mut self, ctx: &egui::Context) {
        let Some(at) = self.ui.menu else { return };
        let t = ui::theme(ctx);
        let mut close = false;
        let resp = egui::Area::new(Id::new("board-menu")).fixed_pos(at).order(egui::Order::Foreground).constrain(true).show(ctx, |ui| {
            ui::menu_frame(&t).show(ui, |ui| {
                ui.set_min_width(220.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                close = super::panels::selection_menu(ui, &mut self.editor, &mut self.ui);
            });
        });
        let clicked_out = ctx.input(|i| i.pointer.any_pressed()) && !resp.response.contains_pointer() && resp.response.ctx.input(|i| i.pointer.press_origin()).is_some_and(|p| !resp.response.rect.contains(p));
        if close || clicked_out {
            self.ui.menu = None;
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