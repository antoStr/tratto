//! A guest in the browser: opens a board shared from someone's Tratto through its link
//! (`https://….trycloudflare.com/#code`), asks to enter, then works on it live. Same board,
//! tools and panels as the desktop app; the board itself stays on the host's PC.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui::{Align2, Id, RichText};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use yrs::Doc;

use crate::app::board::BoardScreen;
use crate::app::{Toasts, dialogs::Dialogs};
use crate::doc::{Board, REMOTE, apply_update, encode_state};
use crate::editor::{Editor, Tool};
use crate::geom::Pt;
use crate::model::{El, Kind};
use crate::net::{self, CLOSE_ENDED, CLOSE_RECONNECT, CLOSE_REMOVED, DOC, HELLO, PRESENCE};
use crate::paint::{Fetch, Images, Painter};
use crate::prefs::Prefs;
use crate::ui::{self, Kind as Btn, Theme};

fn now() -> f64 {
    crate::platform::now_ms()
}

fn window() -> web_sys::Window {
    web_sys::window().expect("window")
}

fn local(key: &str) -> Option<String> {
    window().local_storage().ok().flatten()?.get_item(key).ok().flatten()
}

fn set_local(key: &str, v: &str) {
    if let Some(s) = window().local_storage().ok().flatten() {
        let _ = s.set_item(key, v);
    }
}

/// The ticket survives a page reload (same tab), so a guest let in doesn't have to ask again.
fn session(key: &str) -> Option<String> {
    window().session_storage().ok().flatten()?.get_item(key).ok().flatten()
}

fn set_session(key: &str, v: Option<&str>) {
    if let Some(s) = window().session_storage().ok().flatten() {
        let _ = match v {
            Some(v) => s.set_item(key, v),
            None => s.remove_item(key),
        };
    }
}

const COLORS: [&str; 8] = ["#9747FF", "#F24822", "#14AE5C", "#FFA629", "#E84393", "#00B5CE", "#7B61FF", "#FF7262"];

/// A guest's colour, the same for the same ticket (as in the first Tratto).
pub fn color_for(token: &str) -> &'static str {
    let seed = token.rsplit('.').next().unwrap_or(token);
    let h = seed.bytes().fold(7u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
    COLORS[h as usize % COLORS.len()]
}

/// Starts the app on the page's canvas.
pub fn start() {
    let canvas = window().document().and_then(|d| d.get_element_by_id("tratto")).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()).expect("canvas");
    wasm_bindgen_futures::spawn_local(async move {
        let started = eframe::WebRunner::new().start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(GuestApp::new(cc))))).await;
        if let Some(el) = window().document().and_then(|d| d.get_element_by_id("loading")) {
            match started {
                Ok(()) => el.remove(),
                Err(_) => el.set_text_content(Some("Questo browser non riesce a disegnare la lavagna (serve WebGL 2). Prova con Chrome, Edge, Firefox o Safari aggiornati.")),
            }
        }
    });
}

/// Saves a file the guest exported.
pub fn download(name: &str, bytes: &[u8]) {
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence(&parts) else { return };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
    if let Some(a) = window().document().and_then(|d| d.create_element("a").ok()).and_then(|a| a.dyn_into::<web_sys::HtmlAnchorElement>().ok()) {
        a.set_href(&url);
        a.set_download(name);
        a.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/* ---------------- talking to the host ---------------- */

type Reply = Arc<Mutex<Option<(u16, Value)>>>;

/// A call to the host; the answer (status, JSON) lands in the returned slot.
fn api(ctx: &egui::Context, method: ehttp::Method, path: &str, token: &str, body: Option<Vec<u8>>) -> Reply {
    let slot: Reply = Default::default();
    let mut req = ehttp::Request::new(method, path, &[("Accept", "application/json"), ("Content-Type", "application/json")]);
    req.headers.insert("Authorization", format!("Bearer {token}"));
    req.body = body.unwrap_or_default();
    let (s, c) = (slot.clone(), ctx.clone());
    ehttp::fetch(req, move |r| {
        *s.lock().unwrap() = Some(match r {
            Ok(r) => (r.status, serde_json::from_slice(&r.bytes).unwrap_or(Value::Null)),
            Err(_) => (0, Value::Null),
        });
        c.request_repaint();
    });
    slot
}

enum Ev {
    Open,
    Msg(Vec<u8>),
    Closed(u16),
}

type Uploaded = Arc<Mutex<Vec<Result<(String, egui::ColorImage, Pt), String>>>>;

/// A guest's live connection: the document and presence both ways over a WebSocket, images
/// over HTTP. Reconnects by itself after a network drop.
pub struct Link {
    /// `code.ticket`: what the host checks on every request.
    pub token: String,
    board: String,
    doc: Doc,
    ws: Option<web_sys::WebSocket>,
    generation: u32,
    inbox: Rc<RefCell<VecDeque<(u32, Ev)>>>,
    /// Our changes, waiting to go to the host.
    outbox: Arc<Mutex<Vec<Vec<u8>>>>,
    /// The board arrived on this connection: changes and presence can flow.
    pub synced: bool,
    fitted: bool,
    pub ended: Option<String>,
    pub errors: Vec<String>,
    offline_since: Option<f64>,
    retry_at: f64,
    attempts: u32,
    uploads: Uploaded,
    ctx: egui::Context,
}

impl Link {
    fn new(ctx: &egui::Context, doc: Doc, token: String, board: String) -> Link {
        let outbox: Arc<Mutex<Vec<Vec<u8>>>> = Default::default();
        let (out, c) = (outbox.clone(), ctx.clone());
        let _ = doc.observe_update_v1("guest", move |txn, e| {
            if !txn.origin().is_some_and(|o| o.as_ref() == REMOTE.as_bytes()) {
                out.lock().unwrap().push(e.update.clone());
                c.request_repaint();
            }
        });
        Link { token, board, doc, ws: None, generation: 0, inbox: Default::default(), outbox, synced: false, fitted: false, ended: None, errors: Vec::new(), offline_since: None, retry_at: 0.0, attempts: 0, uploads: Default::default(), ctx: ctx.clone() }
    }

    fn connect(&mut self) {
        self.generation += 1;
        let location = window().location();
        let proto = if location.protocol().ok().as_deref() == Some("https:") { "wss:" } else { "ws:" };
        let Ok(ws) = web_sys::WebSocket::new(&format!("{proto}//{}/collab", location.host().unwrap_or_default())) else {
            self.retry_at = now() + 2000.0;
            return;
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        let push = {
            let (inbox, ctx, generation) = (self.inbox.clone(), self.ctx.clone(), self.generation);
            move |ev: Ev| {
                inbox.borrow_mut().push_back((generation, ev));
                ctx.request_repaint();
            }
        };
        let p = push.clone();
        let on_open = Closure::<dyn FnMut()>::new(move || p(Ev::Open));
        let p = push.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Ok(buf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                p(Ev::Msg(js_sys::Uint8Array::new(&buf).to_vec()));
            }
        });
        let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |e: web_sys::CloseEvent| push(Ev::Closed(e.code())));
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        // ponytail: three small closures leak per (re)connection; a session only has a handful.
        on_open.forget();
        on_message.forget();
        on_close.forget();
        self.ws = Some(ws);
    }

    fn send(&self, kind: u8, body: &[u8]) {
        if let Some(ws) = &self.ws {
            let _ = ws.send_with_u8_array(&net::frame(kind, body));
        }
    }

    /// Pictures of the board, from the host.
    fn fetch(&self) -> Fetch {
        let (board, token) = (self.board.clone(), self.token.clone());
        Arc::new(move |file, done| {
            let mut req = ehttp::Request::get(format!("/api/boards/{board}/files/{file}"));
            req.headers.insert("Authorization", format!("Bearer {token}"));
            ehttp::fetch(req, move |r| done(r.ok().filter(|r| r.ok).map(|r| r.bytes)));
        })
    }

    fn upload_sink(&self) -> UploadSink {
        UploadSink { board: self.board.clone(), token: self.token.clone(), done: self.uploads.clone() }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.doc.unobserve_update_v1("guest");
        if let Some(ws) = self.ws.take() {
            let _ = ws.close();
        }
    }
}

impl BoardScreen {
    /// Messages from the host, our changes and presence to it, reconnection.
    pub(crate) fn sync(&mut self, _ctx: &egui::Context) {
        let events: Vec<(u32, Ev)> = self.guest.inbox.borrow_mut().drain(..).collect();
        for (generation, ev) in events {
            if generation != self.guest.generation {
                continue;
            }
            match ev {
                Ev::Open => {
                    if let Some(ws) = &self.guest.ws {
                        let _ = ws.send_with_str(&self.guest.token);
                    }
                }
                Ev::Msg(m) if !m.is_empty() => match m[0] {
                    HELLO => {
                        let v: Value = serde_json::from_slice(&m[1..]).unwrap_or_default();
                        let view = v["access"] == "view";
                        let ed = &mut self.editor;
                        ed.presence.client = v["client"].as_u64().unwrap_or(0);
                        if view && !ed.read_only {
                            ed.stop_editing();
                            ed.selection.clear();
                            ed.tool = Tool::Select;
                        }
                        ed.read_only = view;
                        ed.presence.dirty = true;
                    }
                    DOC => {
                        if apply_update(&self.guest.doc, &m[1..], REMOTE).is_err() || self.guest.synced {
                            continue;
                        }
                        // First message: the whole board. Ours goes back too, with what was drawn
                        // while offline; repeats are harmless.
                        self.guest.synced = true;
                        self.guest.offline_since = None;
                        self.guest.attempts = 0;
                        self.guest.outbox.lock().unwrap().clear();
                        self.guest.send(DOC, &encode_state(&self.guest.doc));
                        self.editor.board.refresh();
                        if !std::mem::replace(&mut self.guest.fitted, true) && self.editor.board.len() > 0 {
                            self.editor.fit();
                        }
                    }
                    PRESENCE => {
                        let v: Value = serde_json::from_slice(&m[1..]).unwrap_or_default();
                        let Some(id) = v["id"].as_u64() else { continue };
                        let peers = &mut self.editor.presence.peers;
                        peers.retain(|(p, _)| *p != id);
                        if v["state"].is_object() {
                            peers.push((id, v["state"].clone()));
                            peers.sort_by_key(|p| p.0);
                        }
                    }
                    _ => {}
                },
                Ev::Msg(_) => {}
                Ev::Closed(code) => {
                    self.guest.ws = None;
                    self.guest.synced = false;
                    self.editor.presence.peers.clear();
                    self.editor.following = None;
                    match code {
                        CLOSE_ENDED => self.guest.ended = Some("La condivisione è terminata: il proprietario l'ha chiusa.".into()),
                        CLOSE_REMOVED => self.guest.ended = Some("Non hai più accesso a questa lavagna.".into()),
                        CLOSE_RECONNECT => self.guest.retry_at = now(),
                        _ => {
                            let since = *self.guest.offline_since.get_or_insert(now());
                            if now() - since > 20_000.0 {
                                self.guest.ended = Some("Connessione persa. Forse il proprietario ha chiuso Tratto o è offline.".into());
                            }
                            self.guest.attempts += 1;
                            self.guest.retry_at = now() + (500.0 * 2f64.powi(self.guest.attempts as i32)).min(5000.0);
                        }
                    }
                }
            }
        }
        let link = &mut self.guest;
        if link.ended.is_some() {
            return;
        }
        if link.ws.is_none() {
            if now() >= link.retry_at {
                link.connect();
            } else {
                link.ctx.request_repaint_after(std::time::Duration::from_millis((link.retry_at - now()) as u64 + 1));
            }
            return;
        }
        if !link.synced {
            return;
        }
        for update in std::mem::take(&mut *link.outbox.lock().unwrap()) {
            link.send(DOC, &update);
        }
        // Presence at most every 50 ms (slower with many people).
        let ed = &mut self.editor;
        if ed.presence.dirty {
            let wait = self.last_presence + net::presence_interval(ed.presence.peers.len() + 1) - now();
            if wait <= 0.0 {
                ed.presence.dirty = false;
                self.last_presence = now();
                link.send(PRESENCE, Value::Object(ed.presence.local.clone()).to_string().as_bytes());
            } else {
                link.ctx.request_repaint_after(std::time::Duration::from_millis(wait as u64 + 1));
            }
        }
        // Pictures uploaded: on the board where they were dropped.
        let done: Vec<_> = std::mem::take(&mut *link.uploads.lock().unwrap());
        let ctx = link.ctx.clone();
        let mut els = Vec::new();
        let mut first_at = None;
        for r in done {
            match r {
                Ok((file_id, img, at)) => {
                    let z = ed.cam.z;
                    let (w, h) = (img.size[0] as f64, img.size[1] as f64);
                    ed.painter.images.prime(&ctx, &file_id, img);
                    let k = (640.0 / w.max(h)).min(1.0) / z;
                    let offset = els.len() as f64 * 24.0 / z;
                    let mut el = El::new(Kind::Image { file_id });
                    (el.x, el.y, el.w, el.h) = (at.x - w * k / 2.0 + offset, at.y - h * k / 2.0 + offset, w * k, h * k);
                    els.push(el);
                    first_at.get_or_insert(at);
                }
                Err(e) => link.errors.push(e),
            }
        }
        if !els.is_empty() {
            ed.insert(els, first_at, false, false);
        }
    }

    /// Nothing to save on a guest's side: the board lives on the host's PC.
    pub(crate) fn persist(&mut self, _ctx: &egui::Context) {}

    /// Connected, and nothing waiting to be sent.
    pub fn is_saved(&self) -> bool {
        self.guest.synced && self.guest.outbox.lock().unwrap().is_empty()
    }

    pub fn sharing(&self) -> bool {
        false
    }

    pub fn set_title(&mut self, _title: &str, _toasts: &mut Toasts) {}

    pub fn insert_image_dialog(&mut self, ctx: &egui::Context, _toasts: &mut Toasts) {
        if self.editor.read_only {
            return;
        }
        let at = self.editor.viewport_center();
        let (sink, c) = (self.guest.upload_sink(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let Some(files) = rfd::AsyncFileDialog::new().add_filter("Immagini", &["png", "jpg", "jpeg", "gif", "webp"]).pick_files().await else { return };
            for f in files.into_iter().take(20) {
                sink.upload(&c, f.read().await, at);
            }
        });
    }

    pub fn add_images(&mut self, ctx: &egui::Context, files: Vec<Vec<u8>>, at: Pt, _toasts: &mut Toasts) {
        let sink = self.guest.upload_sink();
        for bytes in files.into_iter().take(20) {
            sink.upload(ctx, bytes, at);
        }
    }

    pub(crate) fn add_dropped(&mut self, ctx: &egui::Context, dropped: Vec<egui::DroppedFileHandle>, at: Pt, _toasts: &mut Toasts) {
        let (sink, c) = (self.guest.upload_sink(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            for f in dropped.iter().take(20) {
                if let Ok(bytes) = f.bytes_async().await {
                    sink.upload(&c, bytes, at);
                }
            }
        });
    }

    pub(crate) fn paste(&mut self, _ctx: &egui::Context, text: &str, _toasts: &mut Toasts) {
        self.editor.paste_text(text);
    }
}

/// Where uploads go and come back; cheap to move into async tasks.
#[derive(Clone)]
struct UploadSink {
    board: String,
    token: String,
    done: Uploaded,
}

impl UploadSink {
    /// Scales the picture down, sends it to the host, then it goes on the board.
    fn upload(&self, ctx: &egui::Context, bytes: Vec<u8>, at: Pt) {
        let Some((data, img)) = crate::export::prepare_image(&bytes) else {
            self.done.lock().unwrap().push(Err("Questa immagine non si può aprire. Usa PNG, JPG, GIF o WebP.".into()));
            ctx.request_repaint();
            return;
        };
        let mut req = ehttp::Request::new(ehttp::Method::POST, format!("/api/boards/{}/files", self.board), &[("Accept", "application/json"), ("Content-Type", "application/octet-stream")]);
        req.headers.insert("Authorization", format!("Bearer {}", self.token));
        req.body = data;
        let (done, c) = (self.done.clone(), ctx.clone());
        ehttp::fetch(req, move |r| {
            let failed = "Il caricamento dell'immagine non è riuscito.";
            let result = match r {
                Ok(r) if r.ok => serde_json::from_slice::<Value>(&r.bytes).ok().and_then(|v| v["id"].as_str().map(String::from)).map(|id| (id, img, at)).ok_or_else(|| failed.to_string()),
                Ok(r) => Err(serde_json::from_slice::<Value>(&r.bytes).ok().and_then(|v| v["error"].as_str().map(String::from)).unwrap_or_else(|| failed.into())),
                Err(_) => Err(format!("{failed} Controlla la connessione.")),
            };
            done.lock().unwrap().push(result);
            c.request_repaint();
        });
    }
}

/* ---------------- the app ---------------- */

enum Stage {
    Checking,
    Name,
    Waiting,
    In(Box<BoardScreen>),
    Ended(String),
}

struct GuestApp {
    code: String,
    ticket: Option<String>,
    board_id: String,
    title: String,
    access: String,
    stage: Stage,
    pending: Option<Reply>,
    next_poll: f64,
    name: String,
    error: Option<String>,
    prefs: Prefs,
    saved_prefs: String,
    applied: Option<(Theme, f32)>,
    toasts: Toasts,
    dialogs: Dialogs,
    last_save: f64,
}

impl GuestApp {
    fn new(cc: &eframe::CreationContext) -> GuestApp {
        ui::install_fonts(&cc.egui_ctx);
        let prefs = local("tratto-prefs").map(|s| Prefs::from_json(&s)).unwrap_or_default();
        let code = window().location().hash().unwrap_or_default().trim_start_matches('#').to_string();
        let mut app = GuestApp {
            ticket: session(&format!("tratto-ticket:{code}")),
            name: local("tratto-name").unwrap_or_else(|| prefs.name.clone()),
            saved_prefs: prefs.to_json(),
            prefs,
            board_id: String::new(),
            title: String::new(),
            access: "view".into(),
            stage: Stage::Checking,
            pending: None,
            next_poll: 0.0,
            error: None,
            applied: None,
            toasts: Toasts::default(),
            dialogs: Dialogs::default(),
            last_save: 0.0,
            code,
        };
        if net::valid_code(&app.code) {
            app.pending = Some(api(&cc.egui_ctx, ehttp::Method::GET, "/api/guest/session", &app.code, None));
        } else {
            app.stage = Stage::Ended("Questo link non è completo. Chiedi a chi te l'ha mandato di copiarlo di nuovo.".into());
        }
        app
    }

    fn token(&self) -> String {
        format!("{}.{}", self.code, self.ticket.as_deref().unwrap_or(""))
    }

    fn enter(&mut self, ctx: &egui::Context) {
        let mut id = [0u8; 4];
        crate::platform::random(&mut id);
        let doc = Doc::with_client_id(u32::from_le_bytes(id) as u64);
        let c = ctx.clone();
        let board = Board::new(doc.clone(), Arc::new(move || c.request_repaint()));
        let link = Link::new(ctx, doc, self.token(), self.board_id.clone());
        let painter = Painter::new(Images::new(Some(link.fetch())));
        let voter = local("tratto-voter").unwrap_or_else(|| {
            let v: String = crate::model::uid().chars().take(12).collect();
            set_local("tratto-voter", &v);
            v
        });
        let mut prefs = self.prefs.clone();
        prefs.name = self.name.trim().chars().take(40).collect();
        let mut editor = Editor::new(board, self.board_id.clone(), painter, prefs, voter);
        editor.read_only = self.access == "view";
        editor.tool = if editor.read_only { Tool::Select } else { Tool::Pen };
        self.stage = Stage::In(Box::new(BoardScreen::assemble(editor, &self.board_id, self.title.clone(), true, link)));
    }

    /// The host's answers, and asking again while waiting to be let in.
    fn poll(&mut self, ctx: &egui::Context) {
        let reply = self.pending.as_ref().and_then(|s| s.lock().unwrap().take());
        let Some((status, v)) = reply else {
            if matches!(self.stage, Stage::Waiting) {
                if self.pending.is_none() && now() >= self.next_poll {
                    self.pending = Some(api(ctx, ehttp::Method::GET, "/api/guest/ticket", &self.token(), None));
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            return;
        };
        self.pending = None;
        let ticket_key = format!("tratto-ticket:{}", self.code);
        match self.stage {
            Stage::Checking if status == 200 => {
                self.board_id = v["boardId"].as_str().unwrap_or("").to_string();
                self.title = v["title"].as_str().unwrap_or("Lavagna").to_string();
                self.access = v["access"].as_str().unwrap_or("view").to_string();
                if let Some(d) = window().document() {
                    d.set_title(&format!("{} – Tratto", self.title));
                }
                self.stage = if self.ticket.is_some() { Stage::Waiting } else { Stage::Name };
            }
            Stage::Name if status == 200 => {
                self.ticket = v["ticket"].as_str().map(String::from);
                set_session(&ticket_key, self.ticket.as_deref());
                set_local("tratto-name", self.name.trim());
                if v["status"] == "approved" {
                    self.enter(ctx);
                } else {
                    self.stage = Stage::Waiting;
                    self.next_poll = now() + 1500.0;
                }
            }
            Stage::Name if status == 400 || status == 429 => self.error = v["error"].as_str().map(String::from),
            Stage::Waiting if status == 200 => match v["status"].as_str() {
                Some("approved") => self.enter(ctx),
                Some("pending") => self.next_poll = now() + 1500.0,
                _ => {
                    set_session(&ticket_key, None);
                    self.ticket = None;
                    self.stage = Stage::Ended("Il proprietario non ti ha fatto entrare.".into());
                }
            },
            _ => {
                self.stage = Stage::Ended(match status {
                    0 => "Non riesco a raggiungere la lavagna. Controlla la connessione; forse il proprietario ha chiuso Tratto.".into(),
                    401 => "Questo link non è valido oppure la condivisione è terminata.".into(),
                    _ => v["error"].as_str().unwrap_or("Qualcosa è andato storto. Riprova tra poco.").to_string(),
                })
            }
        }
    }

    fn prefs(&self) -> &Prefs {
        match &self.stage {
            Stage::In(b) => &b.editor.prefs,
            _ => &self.prefs,
        }
    }

    fn save_prefs(&mut self) {
        if now() - self.last_save < 1000.0 {
            return;
        }
        self.last_save = now();
        let json = self.prefs().to_json();
        if json != self.saved_prefs {
            set_local("tratto-prefs", &json);
            self.saved_prefs = json;
        }
    }
}

/// A card in the middle of the page, for the steps before the board.
fn card(ui: &mut egui::Ui, t: &Theme, body: impl FnOnce(&mut egui::Ui)) {
    let ctx = ui.ctx().clone();
    ui.painter().rect_filled(ui.max_rect(), 0.0, t.canvas);
    egui::Area::new(Id::new("join")).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
        let width = (ui.ctx().content_rect().width() - 32.0).min(400.0);
        egui::Frame::new().fill(t.bg).stroke(egui::Stroke::new(1.0, t.border)).corner_radius(ui::RADIUS_LG).inner_margin(egui::Margin::same(24)).show(ui, |ui| {
            ui.set_width(width - 48.0);
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.horizontal(|ui| {
                ui::logo(ui, 24.0);
                ui.label(RichText::new("Tratto").font(ui::medium(14.0)).color(t.text));
            });
            body(ui);
        });
    });
}

impl eframe::App for GuestApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let prefs = self.prefs().clone();
        crate::app::apply_theme(&ctx, &prefs, &mut self.applied);
        let t = ui::theme(&ctx);
        self.poll(&ctx);
        let mut ended = None;
        match &mut self.stage {
            Stage::In(b) => {
                let _ = b.ui(ui, &mut self.toasts, &mut self.dialogs);
                for e in b.guest.errors.drain(..) {
                    self.toasts.error(e);
                }
                crate::app::dialogs::settings(&ctx, &mut self.dialogs.settings, &mut b.editor.prefs, false);
                if let Some(msg) = &b.guest.ended {
                    self.prefs = b.editor.prefs.clone();
                    ended = Some(msg.clone());
                } else if !b.guest.synced {
                    egui::Area::new(Id::new("connecting")).anchor(Align2::CENTER_TOP, [0.0, 56.0]).order(egui::Order::Foreground).interactable(false).show(&ctx, |ui| {
                        ui::menu_frame(&t).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Mi collego alla lavagna…").color(t.menu_text));
                            });
                        });
                    });
                }
            }
            Stage::Checking => card(ui, &t, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Apro la lavagna…").color(t.text));
                });
            }),
            Stage::Name => {
                let mut ask = false;
                let (title, view, busy) = (self.title.clone(), self.access == "view", self.pending.is_some());
                let (name, error) = (&mut self.name, &self.error);
                card(ui, &t, |ui| {
                    ui.label(RichText::new(format!("Ti hanno invitato su «{title}»")).font(ui::medium(16.0)).color(t.text));
                    ui::hint(ui, if view { "Potrai guardare la lavagna dal vivo. Chi ti ha invitato deve farti entrare." } else { "Potrai disegnare e scrivere insieme agli altri. Chi ti ha invitato deve farti entrare." });
                    ui.label(RichText::new("Il tuo nome").font(ui::medium(11.0)).color(t.text));
                    let edit = ui.add(egui::TextEdit::singleline(name).hint_text("Come ti vedranno gli altri").char_limit(40).desired_width(f32::INFINITY));
                    if let Some(e) = error {
                        ui.label(RichText::new(e).color(t.danger_text));
                    }
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let ready = !name.trim().is_empty() && !busy;
                    ask = (ui::button(ui, "Chiedi di entrare", Btn::Primary, Some("arrow-up-right"), true, ready).clicked() || enter) && ready;
                });
                if ask {
                    self.error = None;
                    let body = json!({ "name": self.name.trim() }).to_string().into_bytes();
                    self.pending = Some(api(&ctx, ehttp::Method::POST, "/api/guest/join", &self.code, Some(body)));
                }
            }
            Stage::Waiting => card(ui, &t, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Aspetto che ti facciano entrare…").font(ui::medium(14.0)).color(t.text));
                });
                ui::hint(ui, "Chi ti ha invitato vede la tua richiesta su Tratto. Tieni aperta questa pagina.");
            }),
            Stage::Ended(msg) => {
                let msg = msg.clone();
                card(ui, &t, |ui| {
                    ui.label(RichText::new(msg).color(t.text));
                    if ui::button(ui, "Riprova", Btn::Secondary, Some("refresh-cw"), false, true).clicked() {
                        let _ = window().location().reload();
                    }
                });
            }
        }
        if let Some(msg) = ended {
            self.stage = Stage::Ended(msg);
        }
        self.toasts.show(&ctx);
        self.save_prefs();
    }
}
