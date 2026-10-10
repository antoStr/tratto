//! Sharing a board (desktop): a local server reached only through a Cloudflare quick tunnel,
//! a lobby where the host admits each guest, live sync of the document and of presence, and
//! the share dialog. The PC only connects out to Cloudflare: no router port is opened and the
//! guests never see its IP, nor each other's.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use yrs::Doc;

use crate::net::{self, CLOSE_ENDED, CLOSE_RECONNECT, CLOSE_REMOVED, DOC, HELLO, PRESENCE};
use crate::store::{Store, new_id, sniff_image};

/// People who can be in a shared board at once, besides the host.
pub const MAX_GUESTS: usize = 30;
const MAX_PENDING: usize = 30;
const MAX_GUEST_UPLOAD: usize = 300 * 1024 * 1024;
const MAX_GUEST_MESSAGE: usize = 4 * 1024 * 1024;
const MAX_IMAGE: usize = 15 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Edit,
    View,
}

impl Access {
    fn name(self) -> &'static str {
        match self {
            Access::Edit => "edit",
            Access::View => "view",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TicketStatus {
    Pending,
    Approved,
    Denied,
}

struct Ticket {
    name: String,
    status: TicketStatus,
    /// Last time the guest checked in while waiting; abandoned requests drop out of the list.
    seen: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Starting,
    Live,
    Error(String),
}

struct Peer {
    ticket: String,
    state: Value,
}

struct Inner {
    board_id: String,
    title: String,
    code: String,
    access: Access,
    auto_admit: bool,
    status: Status,
    url: Option<String>,
    tickets: HashMap<String, Ticket>,
    upload_bytes: usize,
    peers: HashMap<u64, Peer>,
    next_id: u64,
    host_state: Value,
    /// Failed guest logins per IP: 20 tries per 10 minutes.
    fails: HashMap<String, (u32, Instant)>,
}

#[derive(Clone)]
enum Out {
    Msg(Bytes),
    Close(u16),
}

type Senders = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Out>>>>;

struct Shared {
    inner: Mutex<Inner>,
    senders: Senders,
    doc: Doc,
    store: Store,
    notify: Arc<dyn Fn() + Send + Sync>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("tratto-share").build().expect("tokio runtime"))
}

impl Shared {
    fn send_all(&self, msg: Out, except: Option<u64>) {
        for (id, tx) in lock(&self.senders).iter() {
            if Some(*id) != except {
                let _ = tx.send(msg.clone());
            }
        }
    }
    fn close_where(&self, code: u16, pick: impl Fn(&Peer) -> bool) {
        let ids: Vec<u64> = lock(&self.inner).peers.iter().filter(|(_, p)| pick(p)).map(|(id, _)| *id).collect();
        let senders = lock(&self.senders);
        for id in ids {
            if let Some(tx) = senders.get(&id) {
                let _ = tx.send(Out::Close(code));
            }
        }
    }
    fn presence_msg(id: u64, state: &Value) -> Out {
        Out::Msg(net::frame(PRESENCE, json!({ "id": id, "state": state }).to_string().as_bytes()).into())
    }
}

/// The people let in, and those waiting.
pub struct View {
    pub status: Status,
    pub url: Option<String>,
    pub access: Access,
    pub auto_admit: bool,
    pub requests: Vec<(String, String)>,
    pub guests: Vec<(String, String, bool)>,
}

/// A board being shared. Dropping it ends the sharing.
pub struct Share {
    shared: Arc<Shared>,
    shutdown: Option<oneshot::Sender<()>>,
    tunnel: Arc<Mutex<Option<std::process::Child>>>,
    pub port: u16,
}

/// Where the tunnel program is: next to Tratto, or in bin/ while developing. Installed it is
/// called tratto-cloudflared, so the .deb does not clash with Cloudflare's own /usr/bin/cloudflared.
pub fn cloudflared() -> PathBuf {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(format!("tratto-cloudflared{ext}"))));
    match beside {
        Some(p) if p.exists() => p,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin").join(format!("cloudflared{ext}")),
    }
}

impl Share {
    /// Starts sharing `doc`. With `tunnel` None the board is only reachable on this PC
    /// (http://127.0.0.1:port, for tests).
    pub fn start(store: Store, board_id: &str, title: &str, doc: Doc, access: Access, auto_admit: bool, tunnel: Option<PathBuf>, notify: Arc<dyn Fn() + Send + Sync>) -> Result<Share, String> {
        let senders: Senders = Default::default();
        let inner = Inner {
            board_id: board_id.into(),
            title: title.into(),
            code: new_id(16),
            access,
            auto_admit,
            status: Status::Starting,
            url: None,
            tickets: HashMap::new(),
            upload_bytes: 0,
            peers: HashMap::new(),
            next_id: 1,
            host_state: Value::Null,
            fails: HashMap::new(),
        };
        let shared = Arc::new(Shared { inner: Mutex::new(inner), senders: senders.clone(), doc: doc.clone(), store, notify });
        // Every change to the document goes to everyone but the guest it came from.
        doc.observe_update_v1("share", move |txn, e| {
            let from = txn.origin().map(|o| o.as_ref().to_vec());
            let msg = Out::Msg(net::frame(DOC, &e.update).into());
            for (id, tx) in lock(&senders).iter() {
                if from.as_deref() != Some(format!("guest:{id}").as_bytes()) {
                    let _ = tx.send(msg.clone());
                }
            }
        })
        .map_err(|_| "Non riesco a preparare la condivisione.".to_string())?;
        let rt = runtime();
        let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).map_err(|e| format!("Non riesco ad avviare la condivisione: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (tx, rx) = oneshot::channel::<()>();
        let app = router(shared.clone());
        rt.spawn(async move {
            let _ = axum::serve(listener, app.into_make_service()).with_graceful_shutdown(async move {
                let _ = rx.await;
            }).await;
        });
        let tunnel_slot: Arc<Mutex<Option<std::process::Child>>> = Default::default();
        match tunnel {
            None => {
                let mut i = lock(&shared.inner);
                i.status = Status::Live;
                i.url = Some(format!("http://127.0.0.1:{port}"));
            }
            Some(bin) => open_tunnel(shared.clone(), bin, port, tunnel_slot.clone()),
        }
        Ok(Share { shared, shutdown: Some(tx), tunnel: tunnel_slot, port })
    }

    pub fn view(&self) -> View {
        let i = lock(&self.shared.inner);
        let fresh = |t: &Ticket| t.seen.elapsed() < Duration::from_secs(15);
        let online: Vec<&str> = i.peers.values().map(|p| p.ticket.as_str()).collect();
        View {
            status: i.status.clone(),
            url: i.url.as_ref().map(|u| if u.starts_with("https://") { format!("{u}/#{}", i.code) } else { format!("{u}/#{}", i.code) }),
            access: i.access,
            auto_admit: i.auto_admit,
            requests: i.tickets.iter().filter(|(_, t)| t.status == TicketStatus::Pending && fresh(t)).map(|(k, t)| (k.clone(), t.name.clone())).collect(),
            guests: i.tickets.iter().filter(|(_, t)| t.status == TicketStatus::Approved).map(|(k, t)| (k.clone(), t.name.clone(), online.contains(&k.as_str()))).collect(),
        }
    }

    /// Who can edit; everyone reconnects and picks up the new permission.
    pub fn set_access(&self, a: Access) {
        lock(&self.shared.inner).access = a;
        self.shared.send_all(Out::Close(CLOSE_RECONNECT), None);
    }

    pub fn set_auto_admit(&self, v: bool) {
        lock(&self.shared.inner).auto_admit = v;
    }

    /// A new link: everyone, admitted or waiting, has to ask again.
    pub fn rotate(&self) {
        {
            let mut i = lock(&self.shared.inner);
            i.code = new_id(16);
            i.tickets.clear();
        }
        self.shared.send_all(Out::Close(CLOSE_REMOVED), None);
    }

    pub fn admit(&self, ticket: &str, allow: bool) {
        if let Some(t) = lock(&self.shared.inner).tickets.get_mut(ticket)
            && t.status == TicketStatus::Pending
        {
            t.status = if allow { TicketStatus::Approved } else { TicketStatus::Denied };
        }
    }

    /// Takes a person's access away.
    pub fn kick(&self, ticket: &str) {
        if let Some(t) = lock(&self.shared.inner).tickets.get_mut(ticket) {
            t.status = TicketStatus::Denied;
        }
        self.shared.close_where(CLOSE_REMOVED, |p| p.ticket == ticket);
    }

    pub fn set_host_presence(&self, state: Value) {
        lock(&self.shared.inner).host_state = state.clone();
        self.shared.send_all(Shared::presence_msg(0, &state), None);
    }

    /// The guests' presence states, by id.
    pub fn peers(&self) -> Vec<(u64, Value)> {
        let mut v: Vec<(u64, Value)> = lock(&self.shared.inner).peers.iter().filter(|(_, p)| !p.state.is_null()).map(|(id, p)| (*id, p.state.clone())).collect();
        v.sort_by_key(|p| p.0);
        v
    }

    pub fn board_id(&self) -> String {
        lock(&self.shared.inner).board_id.clone()
    }

    pub fn set_title(&self, title: &str) {
        lock(&self.shared.inner).title = title.into();
    }

    fn stop(&mut self) {
        self.shared.send_all(Out::Close(CLOSE_ENDED), None);
        // The goodbyes go out before the tunnel closes (each guest answers within a second).
        let until = Instant::now() + Duration::from_millis(1500);
        while !lock(&self.shared.senders).is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(mut c) = lock(&self.tunnel).take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.shared.doc.unobserve_update_v1("share");
        lock(&self.shared.inner).status = Status::Error("Condivisione terminata.".into());
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Opens a Cloudflare quick tunnel to the local port; the link appears when it is ready.
fn open_tunnel(shared: Arc<Shared>, bin: PathBuf, port: u16, slot: Arc<Mutex<Option<std::process::Child>>>) {
    let fail = |shared: &Shared, msg: String| {
        lock(&shared.inner).status = Status::Error(msg);
        shared.send_all(Out::Close(CLOSE_ENDED), None);
        (shared.notify)();
    };
    if !bin.exists() {
        return fail(&shared, "Manca il componente di condivisione (cloudflared). Reinstalla Tratto.".into());
    }
    let mut cmd = std::process::Command::new(&bin);
    cmd.args(["tunnel", "--no-autoupdate", "--url", &format!("http://127.0.0.1:{port}")]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return fail(&shared, format!("Impossibile avviare la condivisione: {e}")),
    };
    let stderr = child.stderr.take().unwrap();
    *lock(&slot) = Some(child);
    let started = Instant::now();
    {
        let (shared, slot) = (shared.clone(), slot.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(45));
            if lock(&shared.inner).status == Status::Starting {
                if let Some(mut c) = lock(&slot).take() {
                    let _ = c.kill();
                }
                lock(&shared.inner).status = Status::Error("Cloudflare non risponde da 45 secondi. Controlla la connessione a internet.".into());
                (shared.notify)();
            }
        });
    }
    std::thread::spawn(move || {
        let mut url: Option<String> = None;
        let mut last_err = String::new();
        for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
            if url.is_none()
                && let Some(start) = line.find("https://")
            {
                let rest = &line[start..];
                let end = rest.find(|c: char| c.is_whitespace() || c == '|').unwrap_or(rest.len());
                let candidate = rest[..end].trim_end_matches('/');
                if candidate.ends_with(".trycloudflare.com") {
                    url = Some(candidate.to_string());
                }
            }
            if line.contains("ERR") {
                last_err = line.clone();
            }
            if line.contains("Registered tunnel connection")
                && let Some(u) = &url
            {
                let mut i = lock(&shared.inner);
                if i.status == Status::Starting {
                    i.status = Status::Live;
                    i.url = Some(u.clone());
                    drop(i);
                    (shared.notify)();
                }
            }
        }
        // The program ended.
        let was_live = lock(&shared.inner).status == Status::Live;
        if lock(&slot).is_some() {
            let msg = if was_live { "La connessione con Cloudflare si è interrotta. Riavvia la condivisione.".to_string() } else { format!("La condivisione si è chiusa. {}", last_err.chars().take(200).collect::<String>()) };
            if !matches!(lock(&shared.inner).status, Status::Error(_)) {
                fail(&shared, msg);
            }
        }
        let _ = started;
    });
}

/* ---------------- the server ---------------- */

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/api/guest/session", get(session))
        .route("/api/guest/join", post(join))
        .route("/api/guest/ticket", get(ticket))
        .route("/api/boards/{board}/files/{file}", get(file))
        .route("/api/boards/{board}/files", post(upload).layer(DefaultBodyLimit::max(MAX_IMAGE)))
        .route("/collab", get(collab))
        .route("/", get(|h: HeaderMap| async move { asset("index.html", &h) }))
        .route("/{*path}", get(|Path(p): Path<String>, h: HeaderMap| async move { asset(&p, &h) }))
        .layer(axum::middleware::from_fn(security_headers))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(shared)
}

async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    for (k, v) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("permissions-policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
        ("strict-transport-security", "max-age=31536000"),
    ] {
        h.insert(k, HeaderValue::from_static(v));
    }
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; font-src 'self' data:; connect-src 'self' wss: ws://127.0.0.1:*; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"),
        );
    }
    res
}

/// The guest page: the same app built for the browser (web/), bundled into this program.
fn asset(path: &str, h: &HeaderMap) -> Response {
    let (body, mime): (&'static [u8], &str) = match path {
        "index.html" => (include_bytes!("../web/index.html"), "text/html; charset=utf-8"),
        "main.js" => (include_bytes!("../web/main.js"), "text/javascript"),
        "pkg/tratto.js" => (include_bytes!("../web/pkg/tratto.js"), "text/javascript"),
        "pkg/tratto_bg.wasm" => (include_bytes!("../web/pkg/tratto_bg.wasm"), "application/wasm"),
        "icon.svg" => (crate::assets::LOGO, "image/svg+xml"),
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Risorsa non trovata" }))).into_response(),
    };
    // Big files go compressed: the app shrinks to about a third, a faster start for guests.
    let gzip = h.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("gzip"));
    if gzip && body.len() > 64 * 1024 {
        static GZ: std::sync::LazyLock<Mutex<HashMap<&'static str, Bytes>>> = std::sync::LazyLock::new(Default::default);
        let key: &'static str = match path {
            "pkg/tratto.js" => "js",
            _ => "wasm",
        };
        let packed = lock(&GZ)
            .entry(key)
            .or_insert_with(|| {
                use std::io::Write;
                let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                let _ = e.write_all(body);
                e.finish().unwrap_or_default().into()
            })
            .clone();
        return ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache"), (header::CONTENT_ENCODING, "gzip"), (header::VARY, "accept-encoding")], packed).into_response();
    }
    ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(json!({ "error": msg }))).into_response()
}

fn client_ip(h: &HeaderMap) -> String {
    // Cloudflare sets this at its edge; through the tunnel the socket is always local.
    h.get("cf-connecting-ip").and_then(|v| v.to_str().ok()).unwrap_or("?").to_string()
}

/// Compares share codes in constant time, so their content can't be guessed from timings.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn bearer(h: &HeaderMap) -> String {
    h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("").trim().to_string()
}

fn blocked(i: &Inner, ip: &str) -> bool {
    i.fails.get(ip).is_some_and(|(n, until)| *until > Instant::now() && *n >= 20)
}

fn fail_login(i: &mut Inner, ip: &str) {
    if i.fails.len() > 10_000 {
        i.fails.clear();
    }
    let now = Instant::now();
    let e = i.fails.entry(ip.to_string()).or_insert((0, now + Duration::from_secs(600)));
    if e.1 < now {
        *e = (0, now + Duration::from_secs(600));
    }
    e.0 += 1;
}

/// Checks the share code of a request; Err is the reply to send.
fn guest_auth(s: &Shared, h: &HeaderMap) -> Result<(), Response> {
    let ip = client_ip(h);
    let mut i = lock(&s.inner);
    if blocked(&i, &ip) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "Troppi tentativi. Riprova tra qualche minuto."));
    }
    let token = bearer(h);
    let code = token.split('.').next().unwrap_or("");
    if i.status != Status::Live || !same(code, &i.code) {
        fail_login(&mut i, &ip);
        return Err(err(StatusCode::UNAUTHORIZED, "Questo link non è valido oppure la sessione è terminata."));
    }
    Ok(())
}

/// The ticket of a `code.ticket` token, if the host admitted that guest.
fn admitted(i: &Inner, token: &str) -> Option<String> {
    let id = token.split('.').nth(1)?;
    (i.tickets.get(id)?.status == TicketStatus::Approved).then(|| id.to_string())
}

async fn session(State(s): State<Arc<Shared>>, h: HeaderMap) -> Response {
    if let Err(r) = guest_auth(&s, &h) {
        return r;
    }
    let i = lock(&s.inner);
    (StatusCode::OK, [(header::CACHE_CONTROL, "no-store")], Json(json!({ "boardId": i.board_id, "title": i.title, "access": i.access.name() }))).into_response()
}

async fn join(State(s): State<Arc<Shared>>, h: HeaderMap, body: Bytes) -> Response {
    if let Err(r) = guest_auth(&s, &h) {
        return r;
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or_default();
    let name: String = v["name"].as_str().unwrap_or("").chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(40).collect();
    if name.is_empty() {
        return err(StatusCode::BAD_REQUEST, "Scrivi il tuo nome per entrare.");
    }
    let mut i = lock(&s.inner);
    let waiting = i.tickets.values().filter(|t| t.status == TicketStatus::Pending && t.seen.elapsed() < Duration::from_secs(15)).count();
    if waiting >= MAX_PENDING || i.tickets.len() >= 500 {
        return err(StatusCode::TOO_MANY_REQUESTS, "Troppe persone in attesa. Riprova tra poco.");
    }
    let online: std::collections::HashSet<&String> = i.peers.values().map(|p| &p.ticket).collect();
    if online.len() >= MAX_GUESTS {
        return err(StatusCode::TOO_MANY_REQUESTS, &format!("La lavagna è al completo: possono esserci al massimo {MAX_GUESTS} persone. Riprova più tardi."));
    }
    let ticket = new_id(16);
    let status = if i.auto_admit { TicketStatus::Approved } else { TicketStatus::Pending };
    i.tickets.insert(ticket.clone(), Ticket { name, status, seen: Instant::now() });
    drop(i);
    (s.notify)();
    Json(json!({ "ticket": ticket, "status": if status == TicketStatus::Approved { "approved" } else { "pending" } })).into_response()
}

async fn ticket(State(s): State<Arc<Shared>>, h: HeaderMap) -> Response {
    if let Err(r) = guest_auth(&s, &h) {
        return r;
    }
    let token = bearer(&h);
    let mut i = lock(&s.inner);
    let status = match token.split('.').nth(1).and_then(|t| i.tickets.get_mut(t)) {
        Some(t) => {
            t.seen = Instant::now();
            match t.status {
                TicketStatus::Pending => "pending",
                TicketStatus::Approved => "approved",
                TicketStatus::Denied => "denied",
            }
        }
        None => "denied",
    };
    (StatusCode::OK, [(header::CACHE_CONTROL, "no-store")], Json(json!({ "status": status }))).into_response()
}

fn board_check(s: &Shared, h: &HeaderMap, board: &str) -> Result<String, Response> {
    guest_auth(s, h)?;
    let i = lock(&s.inner);
    if board != i.board_id {
        return Err(err(StatusCode::FORBIDDEN, "Accesso negato"));
    }
    admitted(&i, &bearer(h)).ok_or_else(|| err(StatusCode::FORBIDDEN, "Non sei ancora stato fatto entrare in questa lavagna."))
}

async fn file(State(s): State<Arc<Shared>>, h: HeaderMap, Path((board, file)): Path<(String, String)>) -> Response {
    if let Err(r) = board_check(&s, &h, &board) {
        return r;
    }
    match s.store.file(&board, &file) {
        Ok(Some((mime, data))) => ([(header::CONTENT_TYPE, mime.as_str()), (header::CACHE_CONTROL, "private, max-age=31536000, immutable"), (header::CONTENT_SECURITY_POLICY, "default-src 'none'")], data).into_response(),
        _ => err(StatusCode::NOT_FOUND, "Immagine non trovata"),
    }
}

async fn upload(State(s): State<Arc<Shared>>, h: HeaderMap, Path(board): Path<String>, body: Bytes) -> Response {
    if let Err(r) = board_check(&s, &h, &board) {
        return r;
    }
    {
        let i = lock(&s.inner);
        if i.access != Access::Edit {
            return err(StatusCode::FORBIDDEN, "Puoi solo guardare questa lavagna");
        }
        if i.upload_bytes + body.len() > MAX_GUEST_UPLOAD {
            return err(StatusCode::TOO_MANY_REQUESTS, "Limite di immagini raggiunto per questa sessione");
        }
    }
    if body.is_empty() || sniff_image(&body).is_none() {
        return err(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Formato non supportato: usa PNG, JPG, GIF o WebP");
    }
    match s.store.save_file(&board, &body) {
        Ok(id) => {
            lock(&s.inner).upload_bytes += body.len();
            Json(json!({ "id": id })).into_response()
        }
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn collab(State(s): State<Arc<Shared>>, h: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let ip = client_ip(&h);
    {
        let i = lock(&s.inner);
        let origin = h.get(header::ORIGIN).and_then(|v| v.to_str().ok()).unwrap_or("");
        if i.status != Status::Live || i.url.as_deref() != Some(origin) {
            return StatusCode::FORBIDDEN.into_response();
        }
        if blocked(&i, &ip) {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }
    // A few spare sockets for guests who are reconnecting while their old socket closes.
    if lock(&s.senders).len() >= MAX_GUESTS + 5 {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ws.max_message_size(32 << 20).on_upgrade(move |socket| conn(socket, s, ip))
}

async fn close(mut tx: impl SinkExt<Message> + Unpin, code: u16) {
    let reason = match code {
        CLOSE_REMOVED => "removed",
        CLOSE_RECONNECT => "reconnect",
        _ => "session-ended",
    };
    let _ = tx.send(Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
}

/// One guest's connection: token first, then the document and presence both ways.
async fn conn(socket: WebSocket, s: Arc<Shared>, ip: String) {
    let (mut tx, mut rx) = socket.split();
    let token = match tokio::time::timeout(Duration::from_secs(10), rx.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => t.to_string(),
        _ => return,
    };
    let joined = {
        let mut i = lock(&s.inner);
        let code = token.split('.').next().unwrap_or("");
        if !same(code, &i.code) {
            fail_login(&mut i, &ip);
            None
        } else if let Some(ticket) = admitted(&i, &token) {
            let id = i.next_id;
            i.next_id += 1;
            i.peers.insert(id, Peer { ticket, state: Value::Null });
            Some((id, i.access))
        } else {
            None
        }
    };
    let Some((id, access)) = joined else {
        return close(tx, CLOSE_REMOVED).await;
    };
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Out>();
    // Listening before the document is read, so no change falls in between (repeats are harmless).
    lock(&s.senders).insert(id, out_tx);
    // Welcome, the whole document, and who is here.
    let hello = net::frame(HELLO, json!({ "client": id, "access": access.name() }).to_string().as_bytes());
    let state = crate::doc::encode_state(&s.doc);
    let mut first = vec![hello, net::frame(DOC, &state)];
    {
        let i = lock(&s.inner);
        first.push(net::frame(PRESENCE, json!({ "id": 0, "state": i.host_state }).to_string().as_bytes()));
        for (pid, p) in &i.peers {
            if *pid != id && !p.state.is_null() {
                first.push(net::frame(PRESENCE, json!({ "id": pid, "state": p.state }).to_string().as_bytes()));
            }
        }
    }
    (s.notify)();
    for m in first {
        if tx.send(Message::Binary(m.into())).await.is_err() {
            break;
        }
    }
    let origin = format!("guest:{id}");
    loop {
        tokio::select! {
            out = out_rx.recv() => match out {
                Some(Out::Msg(m)) => {
                    if tx.send(Message::Binary(m)).await.is_err() {
                        break;
                    }
                }
                Some(Out::Close(code)) => {
                    close(&mut tx, code).await;
                    // Wait for the guest's reply, so the reason surely reached it.
                    let _ = tokio::time::timeout(Duration::from_secs(1), async {
                        while let Some(Ok(m)) = rx.next().await {
                            if matches!(m, Message::Close(_)) {
                                break;
                            }
                        }
                    })
                    .await;
                    break;
                }
                None => break,
            },
            msg = rx.next() => match msg {
                Some(Ok(Message::Binary(b))) if !b.is_empty() => match b[0] {
                    DOC => {
                        // Read-only guests can't change the board, whatever they send.
                        if access != Access::Edit {
                            continue;
                        }
                        if b.len() > MAX_GUEST_MESSAGE {
                            close(&mut tx, CLOSE_RECONNECT).await;
                            break;
                        }
                        let doc = s.doc.clone();
                        let origin = origin.clone();
                        let update = b.slice(1..);
                        let _ = tokio::task::spawn_blocking(move || crate::doc::apply_update(&doc, &update, &origin)).await;
                        (s.notify)();
                    }
                    PRESENCE if b.len() < 256 * 1024 => {
                        let Ok(state) = serde_json::from_slice::<Value>(&b[1..]) else { continue };
                        if !state.is_object() && !state.is_null() {
                            continue;
                        }
                        if let Some(p) = lock(&s.inner).peers.get_mut(&id) {
                            p.state = state.clone();
                        }
                        s.send_all(Shared::presence_msg(id, &state), Some(id));
                        (s.notify)();
                    }
                    _ => {}
                },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
    lock(&s.senders).remove(&id);
    lock(&s.inner).peers.remove(&id);
    s.send_all(Shared::presence_msg(id, &Value::Null), None);
    (s.notify)();
}

/* ---------------- the share dialog ---------------- */

use crate::app::Toasts;
use crate::app::board::BoardScreen;
use crate::ui::{self, Kind as Btn};
use egui::RichText;

fn secure_note(ui: &mut egui::Ui) {
    ui::hint(ui, "Il collegamento passa da un tunnel cifrato di Cloudflare: il tuo indirizzo IP resta nascosto e non devi aprire porte sul router. Quando termini la condivisione o chiudi Tratto, il link smette di funzionare.");
}

pub fn dialog(ui: &mut egui::Ui, b: &mut BoardScreen, toasts: &mut Toasts) {
    let t = ui::theme(ui.ctx());
    let view = b.share.as_ref().map(|s| s.view());
    let id = egui::Id::new("share-form");
    let (mut access, mut auto): (Access, bool) = ui.data(|d| d.get_temp(id)).unwrap_or((Access::Edit, false));
    let mut stop = false;
    match view {
        None => {
            ui::hint(ui, "Invita qualcuno a lavorare con te su questa lavagna. Riceverà un link da aprire nel browser (Windows, Mac, Linux, tablet), senza installare niente. Possono esserci fino a 30 persone insieme.");
            ui.label(RichText::new("Il tuo nome").font(ui::medium(11.0)).color(t.text));
            let before = b.editor.prefs.name.clone();
            ui.add(egui::TextEdit::singleline(&mut b.editor.prefs.name).hint_text("Come ti vedranno gli altri").char_limit(40).desired_width(f32::INFINITY));
            if b.editor.prefs.name != before {
                b.update_identity();
            }
            ui.label(RichText::new("Chi ha il link").font(ui::medium(11.0)).color(t.text));
            ui::segmented(ui, &mut access, &[(Access::Edit, "Può modificare"), (Access::View, "Può solo guardare")], ui.available_width());
            ui::switch(ui, &mut auto, "Fai entrare senza chiedermelo");
            ui.add_space(4.0);
            if ui::button(ui, "Crea link di invito", Btn::Primary, Some("link-2"), true, true).clicked() {
                b.start_share(ui.ctx(), access, auto, toasts);
            }
            secure_note(ui);
        }
        Some(v) => match &v.status {
            Status::Starting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Apro un collegamento sicuro…").color(t.text));
                });
                ui.ctx().request_repaint_after(Duration::from_millis(250));
                secure_note(ui);
            }
            Status::Error(msg) => {
                ui.label(RichText::new(msg).color(t.danger_text));
                ui.horizontal(|ui| {
                    if ui::button(ui, "Riprova", Btn::Primary, Some("refresh-cw"), false, true).clicked() {
                        b.start_share(ui.ctx(), v.access, v.auto_admit, toasts);
                    }
                    if ui::button(ui, "Chiudi", Btn::Secondary, None, false, true).clicked() {
                        stop = true;
                    }
                });
            }
            Status::Live => {
                let share = b.share.as_ref().unwrap();
                let url = v.url.clone().unwrap_or_default();
                ui.label(RichText::new("Link di invito").font(ui::medium(11.0)).color(t.text));
                ui.horizontal(|ui| {
                    let mut shown = url.clone();
                    ui.add(egui::TextEdit::singleline(&mut shown).desired_width(ui.available_width() - 110.0).interactive(true));
                    if ui::button(ui, "Copia link", Btn::Primary, Some("copy"), false, true).clicked() {
                        ui.ctx().copy_text(url.clone());
                        toasts.info("Link copiato: mandalo a chi vuoi.");
                    }
                });
                ui.label(RichText::new("Chi ha il link").font(ui::medium(11.0)).color(t.text));
                let mut a = v.access;
                if ui::segmented(ui, &mut a, &[(Access::Edit, "Può modificare"), (Access::View, "Può solo guardare")], ui.available_width()) {
                    share.set_access(a);
                }
                let mut auto = v.auto_admit;
                if ui::switch(ui, &mut auto, "Fai entrare senza chiedermelo") {
                    share.set_auto_admit(auto);
                }
                for (ticket, name) in &v.requests {
                    ui.horizontal(|ui| {
                        ui::avatar(ui, name, egui::Color32::from_rgb(0x97, 0x47, 0xFF), 24.0);
                        ui.label(RichText::new(format!("{name} vuole entrare")).color(t.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui::button(ui, "Fai entrare", Btn::Primary, None, false, true).clicked() {
                                share.admit(ticket, true);
                            }
                            if ui::button(ui, "Rifiuta", Btn::Secondary, None, false, true).clicked() {
                                share.admit(ticket, false);
                            }
                        });
                    });
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let me = if b.editor.prefs.name.trim().is_empty() { "Tu".to_string() } else { b.editor.prefs.name.trim().to_string() };
                    ui::avatar(ui, &me, egui::Color32::from_rgb(0x0D, 0x99, 0xFF), 24.0);
                    ui.label(RichText::new(format!("{me} (tu)")).color(t.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new("Proprietario").color(t.text2)));
                });
                for (ticket, name, online) in &v.guests {
                    ui.horizontal(|ui| {
                        ui::avatar(ui, name, egui::Color32::from_rgb(0x97, 0x47, 0xFF), 24.0);
                        ui.label(RichText::new(name).color(t.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui::button(ui, "Rimuovi", Btn::DangerText, None, false, true).clicked() {
                                share.kick(ticket);
                            }
                            let role = if !online { "Non collegato" } else if v.access == Access::Edit { "Può modificare" } else { "Guarda" };
                            ui.label(RichText::new(role).color(t.text2));
                        });
                    });
                }
                secure_note(ui);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui::button(ui, "Nuovo link", Btn::Secondary, Some("refresh-cw"), false, true).clicked() {
                        share.rotate();
                        toasts.info("Nuovo link creato. Quello vecchio non funziona più.");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui::button(ui, "Termina condivisione", Btn::DangerText, None, false, true).clicked() {
                            stop = true;
                            toasts.info("Condivisione terminata. Il link non funziona più.");
                        }
                    });
                });
            }
        },
    }
    ui.data_mut(|d| d.insert_temp(id, (access, auto)));
    if stop {
        b.stop_share();
    }
}

/// Join requests over the board while the share dialog is closed.
pub fn requests(ctx: &egui::Context, b: &mut BoardScreen) {
    let Some(share) = &b.share else { return };
    let v = share.view();
    if v.requests.is_empty() {
        return;
    }
    ctx.request_repaint_after(Duration::from_millis(1500));
    let t = ui::theme(ctx);
    egui::Area::new(egui::Id::new("knock")).anchor(egui::Align2::RIGHT_BOTTOM, [-80.0, -96.0]).order(egui::Order::Foreground).show(ctx, |ui| {
        for (ticket, name) in v.requests.iter().take(3) {
            ui::menu_frame(&t).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{name} vuole entrare nella lavagna")).color(t.menu_text));
                    if ui.add(egui::Button::new(RichText::new("Rifiuta").color(t.menu_text2)).frame(false)).clicked() {
                        share.admit(ticket, false);
                    }
                    if ui.add(egui::Button::new(RichText::new("Fai entrare").color(t.menu_text)).frame(false)).clicked() {
                        share.admit(ticket, true);
                    }
                });
            });
            ui.add_space(6.0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A guest asks to enter, is let in, connects, edits; a view-only guest can't.
    #[test]
    fn lobby_and_sync() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let board = store.create("Prova", None).unwrap();
        let doc = Doc::new();
        let share = Share::start(store, &board, "Prova", doc.clone(), Access::Edit, false, None, Arc::new(|| {})).unwrap();
        let base = format!("http://127.0.0.1:{}", share.port);
        let code = share.view().url.unwrap().split('#').nth(1).unwrap().to_string();
        let rt = runtime();
        rt.block_on(async {
            let client = |path: &str, token: &str| {
                let url = format!("{base}{path}");
                let token = token.to_string();
                async move { reqwest_lite(&url, &token, None).await }
            };
            // Wrong code: refused.
            assert_eq!(client("/api/guest/session", "x".repeat(22).as_str()).await.0, 401);
            let (st, body) = client("/api/guest/session", &code).await;
            assert_eq!(st, 200, "{body}");
            assert!(body.contains("\"access\":\"edit\""));
            let (st, body) = reqwest_lite(&format!("{base}/api/guest/join"), &code, Some(r#"{"name":"Ospite"}"#)).await;
            assert_eq!(st, 200);
            let v: Value = serde_json::from_str(&body).unwrap();
            let ticket = v["ticket"].as_str().unwrap().to_string();
            assert_eq!(v["status"], "pending");
            assert_eq!(share.view().requests.len(), 1);
            share.admit(&ticket, true);
            let (_, body) = client("/api/guest/ticket", &format!("{code}.{ticket}")).await;
            assert!(body.contains("approved"));
            // The guest page is served.
            assert_eq!(client("/", "").await.0, 200);
        });
    }

    /// Shares a sample board for a browser test, letting everyone in after 2 s, and reports what
    /// the guests do: `TRATTO_SERVE=120 [TRATTO_TUNNEL=1] cargo test serve_for_browser -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn serve_for_browser() {
        let secs: u64 = std::env::var("TRATTO_SERVE").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let board = store.create("Prova condivisione", None).unwrap();
        let doc = Doc::new();
        let mut b = crate::doc::Board::new(doc.clone(), Arc::new(|| {}));
        b.transact(|e| {
            for el in (crate::templates::TEMPLATES[0].build)() {
                e.put(&el);
            }
        });
        let tunnel = std::env::var("TRATTO_TUNNEL").is_ok().then(cloudflared);
        let share = Share::start(store, &board, "Prova condivisione", doc.clone(), Access::Edit, false, tunnel, Arc::new(|| {})).unwrap();
        let start = Instant::now();
        let mut printed = false;
        let mut seen = 0;
        while start.elapsed() < Duration::from_secs(secs) {
            std::thread::sleep(Duration::from_millis(500));
            let v = share.view();
            if !printed && v.status == Status::Live {
                println!("LINK {}", v.url.clone().unwrap());
                printed = true;
            }
            if let Status::Error(e) = &v.status {
                panic!("{e}");
            }
            for (t, name) in &v.requests {
                println!("REQUEST {name}");
                std::thread::sleep(Duration::from_secs(2));
                share.admit(t, true);
            }
            // Host actions from a file: view | edit | kick NAME | rotate | stop.
            if let Ok(path) = std::env::var("TRATTO_CMD")
                && let Ok(cmd) = std::fs::read_to_string(&path)
                && !cmd.trim().is_empty()
            {
                let _ = std::fs::write(&path, "");
                let cmd = cmd.trim();
                println!("CMD {cmd}");
                match cmd.split_once(' ').unwrap_or((cmd, "")) {
                    ("view", _) => share.set_access(Access::View),
                    ("edit", _) => share.set_access(Access::Edit),
                    ("rotate", _) => share.rotate(),
                    ("kick", who) => {
                        if let Some(g) = v.guests.iter().find(|g| g.1 == who) {
                            share.kick(&g.0);
                        }
                    }
                    ("stop", _) => {
                        drop(share);
                        std::thread::sleep(Duration::from_secs(2));
                        return;
                    }
                    _ => {}
                }
            }
            b.refresh();
            if b.len() != seen {
                println!("ELEMENTS {} (guests online: {})", b.len(), v.guests.iter().filter(|g| g.2).count());
                seen = b.len();
            }
            let peers = share.peers();
            if !peers.is_empty() && start.elapsed().as_millis() % 5000 < 500 {
                println!("PEERS {}", peers.iter().map(|(id, s)| format!("{id}:{}", s["user"]["name"])).collect::<Vec<_>>().join(", "));
            }
        }
    }

    /// Minimal HTTP client for the test (no extra dependency): GET, or POST with a JSON body.
    async fn reqwest_lite(url: &str, token: &str, body: Option<&str>) -> (u16, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_at(rest.find('/').unwrap());
        let mut s = tokio::net::TcpStream::connect(host).await.unwrap();
        let method = if body.is_some() { "POST" } else { "GET" };
        let body = body.unwrap_or("");
        let req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.unwrap();
        let text = String::from_utf8_lossy(&out).to_string();
        let status = text[9..12].parse().unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }
}
