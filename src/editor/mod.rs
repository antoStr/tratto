//! The board editor: tools, selection, gestures and commands (a port of the controller of the
//! first Tratto). The UI around the board (panels, toolbar, dialogs) reads and changes this state.

pub mod input;
pub mod overlay;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::{Color32, Pos2, pos2};
use serde_json::{Value, json};

use crate::doc::Board;
use crate::geom::*;
use crate::ink::{self, OneEuro, Recognized, Ruler, RulerSnap};
use crate::model::*;
use crate::paint::Painter;
use crate::prefs::{Prefs, ShapeTool};
use crate::text::{self, LINE_HEIGHT};

pub const MIN_ZOOM: f64 = 0.04;
pub const MAX_ZOOM: f64 = 32.0;
pub const CLIP_PREFIX: &str = "tratto-clipboard:";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Hand,
    Pen,
    Highlighter,
    Eraser,
    Lasso,
    Shape,
    Line,
    Arrow,
    Text,
    Sticky,
    Stamp,
    Laser,
    Section,
    Tape,
    Comment,
    Table,
}

/// Selection handles. `Add*` are FigJam's "+" beside a shape or sticky: click for a connected
/// copy, drag for a connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Rot,
    P0,
    P1,
    AddN,
    AddE,
    AddS,
    AddW,
    /// A table's "+" under its last row and after its last column.
    AddRow,
    AddCol,
}

impl Handle {
    pub fn is_add(self) -> bool {
        matches!(self, Handle::AddN | Handle::AddE | Handle::AddS | Handle::AddW)
    }
    fn corner(self) -> bool {
        matches!(self, Handle::Nw | Handle::Ne | Handle::Se | Handle::Sw)
    }
    fn n(self) -> bool {
        matches!(self, Handle::Nw | Handle::N | Handle::Ne)
    }
    fn s(self) -> bool {
        matches!(self, Handle::Sw | Handle::S | Handle::Se)
    }
    fn w(self) -> bool {
        matches!(self, Handle::Nw | Handle::W | Handle::Sw)
    }
    fn e(self) -> bool {
        matches!(self, Handle::Ne | Handle::E | Handle::Se)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Pen,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignKind {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Front,
    Back,
    Forward,
    Backward,
}

/// What this person shows the others (cursor, selection, stroke being drawn, laser, view,
/// name) and what the others show: the Yjs "awareness" states of the first Tratto.
#[derive(Default)]
pub struct Presence {
    pub client: u64,
    pub local: serde_json::Map<String, Value>,
    pub peers: Vec<(u64, Value)>,
    pub dirty: bool,
    /// The stroke being drawn goes out in pieces: which stroke, and how many of its points the
    /// others already have.
    live_sid: u64,
    live_sent: usize,
}

impl Presence {
    pub fn set(&mut self, k: &str, v: Value) {
        if self.local.get(k) != Some(&v) {
            self.local.insert(k.into(), v);
            self.dirty = true;
        }
    }
    /// The stroke being drawn is finished (or dropped): the others stop showing it.
    pub fn end_live(&mut self) {
        self.local.insert("live".into(), Value::Null);
        self.dirty = true;
    }
    pub fn peer(&self, id: u64) -> Option<&Value> {
        self.peers.iter().find(|(c, _)| *c == id).map(|(_, v)| v)
    }
}

pub fn peer_name(s: &Value) -> String {
    match s["user"]["name"].as_str().map(str::trim) {
        Some(n) if !n.is_empty() => n.chars().take(32).collect(),
        _ => "Ospite".into(),
    }
}

pub fn peer_color(s: &Value) -> Color32 {
    s["user"]["color"].as_str().filter(|c| c.len() == 7).and_then(parse_color).unwrap_or(Color32::from_rgb(0x97, 0x47, 0xFF))
}

/// Things the board asks of the UI around it.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Show the templates in the left panel.
    Templates,
    /// Context menu at a screen position.
    Menu(Pos2),
    Export { selection: bool },
    InsertImage,
    /// Cursor chat (key /).
    Chat,
    /// A new folder: the layers panel opens it with its name ready to type.
    FolderCreated(String),
    /// Ctrl+V with this text: the app pastes a picture instead if the clipboard holds one.
    Paste(String),
    Shortcuts,
    /// Open a web address (http or https only), in the browser.
    OpenLink(String),
    /// A short message for the person (a toast).
    Notice(String),
    /// Show the right panel, with the settings of the tool in hand.
    ToolSettings,
}

pub(crate) struct Draw {
    hl: bool,
    /// The pen's brush (None for the highlighter).
    pub(crate) brush: Option<Brush>,
    color: String,
    size: f64,
    opacity: f64,
    /// Board coordinates, flat [x, y, pressure, …].
    pts: Vec<f64>,
    snap: Option<RulerSnap>,
    /// Tremor smoothing for pens and fingers; a mouse goes straight (it doesn't tremble, and
    /// smoothing it only rounds off the small loops of handwriting).
    filter: Option<OneEuro>,
    /// Smoothed pen pressure; None until the first sample.
    pressure: Option<f64>,
    use_pressure: bool,
    /// First raw force seen on this stroke, and whether hardware force actually varied from 0.5.
    first_force: Option<f32>,
    force_varied: bool,

    /// When the pointer last moved significantly (ms), for QuickShape hold detection.
    pub(crate) last_move_ms: f64,
    /// True once holding still at the end of the stroke has snapped it into a geometric shape.
    pub(crate) quick_shape_snapped: bool,
    /// Board centre used for symmetry axes during this stroke.
    pub(crate) sym_center: Pt,
    /// The pressure is raised to this power (Settings ▸ Penna ▸ Sensibilità).
    gamma: f64,
    /// The brush pen without pressure: how fast it moves (screen points per ms, smoothed) and
    /// where and when it was last.
    speed: Option<f64>,
    last_raw: Option<(Pos2, f64)>,
    /// Last raw position, added at release so the line ends exactly where the pen lifted.
    raw: Option<Pos2>,
    /// When the pen touched down (ms).
    down_ms: Option<f64>,
    pointer: u64,
    /// Tells this stroke's pieces apart from the previous one's, for the others.
    sid: u64,
    /// The stroke on screen a piece at a time: what each piece is and its meshes, kept while the
    /// piece stays the same (only the last one changes as the stroke grows) and the view doesn't
    /// move. The shape is still worked out whole at each move (quick: a few milliseconds for
    /// thousands of points); only turning it into triangles, the slow part, is spared.
    pub(crate) pieces: Vec<(Option<crate::prims::PieceKey>, Vec<egui::Shape>)>,
    /// The view the pieces were made for, and how many points they show.
    pub(crate) pieces_cam: (i64, i64, i64, i64, i64),
    pub(crate) pieces_pts: usize,
    /// The sfumino: the colour each point carries (one a point, 0xRRGGBB), and what it picks
    /// them up from.
    pub(crate) colors: Vec<u32>,
    smear: Option<Box<Smear>>,
}

/// What the sfumino smears: the board as it was when the stroke began, in a picture, and the
/// colour it drags along.
struct Smear {
    pic: tiny_skia::Pixmap,
    /// The board point at the picture's top left corner, and its pixels a board unit.
    at: Pt,
    k: f64,
    carried: Option<[f64; 3]>,
    last: Option<Pt>,
}

impl Smear {
    /// The board as it is on screen (and a little around), at up to 640 pixels across: enough
    /// for colours, quick to draw (a few milliseconds when the stroke starts).
    fn of(ed: &mut Editor, margin: f64) -> Option<Smear> {
        let v = ed.view();
        let view = BBox { x: v.x - margin, y: v.y - margin, w: v.w + margin * 2.0, h: v.h + margin * 2.0 };
        let k = (640.0 / view.w.max(view.h)).min(ed.cam.z);
        let (w, h) = ((view.w * k).ceil().max(1.0) as u32, (view.h * k).ceil().max(1.0) as u32);
        let mut pic = tiny_skia::Pixmap::new(w, h)?;
        let meta = ed.board.meta();
        let bg = color_or(&meta.background, Color32::from_gray(0xF5));
        let [r, g, b, a] = bg.to_srgba_unmultiplied();
        pic.fill(tiny_skia::Color::from_rgba8(r, g, b, a));
        let env = crate::prims::Env { dark_board: is_dark(&meta.background), pixel: Some(1.0 / k), ..Default::default() };
        let t = tiny_skia::Transform::from_row(k as f32, 0.0, 0.0, k as f32, (-view.x * k) as f32, (-view.y * k) as f32);
        let order = ed.board.paint_order().to_vec();
        let images = &ed.painter.images;
        let fetch = |key: &crate::prims::ImageKey| images.pixmap(key);
        for el in order.iter().filter(|e| !e.hidden && intersects(&aabb(e), &view)) {
            crate::raster::draw_element(&mut pic, el, &env, t, &fetch);
        }
        Some(Smear { pic, at: pt(view.x, view.y), k, carried: None, last: None })
    }

    /// The colour under a board point, averaged over a little disc of radius `r` (board units),
    /// in OKLab.
    fn sample(&self, p: Pt, r: f64) -> [f64; 3] {
        let mut sum = [0.0; 3];
        let taps = [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];
        for (dx, dy) in taps {
            let x = ((p.x + dx * r * 0.5 - self.at.x) * self.k).floor().clamp(0.0, self.pic.width() as f64 - 1.0) as u32;
            let y = ((p.y + dy * r * 0.5 - self.at.y) * self.k).floor().clamp(0.0, self.pic.height() as f64 - 1.0) as u32;
            if let Some(c) = self.pic.pixel(x, y) {
                let c = c.demultiply();
                let lab = ink::to_oklab(((c.red() as u32) << 16) | ((c.green() as u32) << 8) | c.blue() as u32);
                for i in 0..3 {
                    sum[i] += lab[i];
                }
            }
        }
        sum.map(|v| v / taps.len() as f64)
    }

    /// The colour the sfumino carries at the next point: what it had, with some of what is under
    /// it there (more the farther it went: it drags colours about a width), mixed as the eye
    /// sees colours (OKLab).
    fn carry(&mut self, p: Pt, size: f64) -> u32 {
        let under = self.sample(p, size / 2.0);
        let c = match (self.carried, self.last) {
            (Some(c), Some(l)) => {
                let k = 1.0 - (-(p.x - l.x).hypot(p.y - l.y) / (size * 1.1).max(1e-6)).exp();
                ink::mix_lab(c, under, k)
            }
            _ => under,
        };
        (self.carried, self.last) = (Some(c), Some(p));
        ink::from_oklab(c)
    }
}

pub(crate) struct Touched {
    orig: El,
    strokes: Vec<Vec<f32>>,
    last_seg: i64,
}

pub(crate) enum Gesture {
    Pan { s0: Pos2, cam: Camera },
    Pinch { ids: [u64; 2], d0: f64, c0: Pos2, cam: Camera, ruler: Option<[f64; 4]> },
    Draw(Box<Draw>),
    Erase { last: Pt, seg: i64, touched: HashMap<String, Touched> },
    /// `tap`: started with the pen's barrel button, which opens the menu when not dragged.
    Lasso { poly: Vec<f64>, tap: Option<Pos2> },
    Marquee { start: Pt, cur: Pt, base: Vec<String> },
    /// `edit`: released without moving, start writing in this element (in this cell of a table).
    Move { start: Pt, orig: Vec<El>, bbox: BBox, cands: Vec<BBox>, moved: bool, s0: Pos2, edit: Option<(String, Option<(usize, usize)>)> },
    /// Dragging the line after column `col` of a table.
    ColWidth { col: usize, orig: El, start: Pt },
    Resize { handle: Handle, start: Pt, orig: Vec<El>, frame: BBox, rotation: f64 },
    Rotate { c: Pt, a0: f64, orig: Vec<El> },
    Endpoint { which: usize, orig: El },
    /// `spawn`: dragged from a "+": released on empty board, a copy of it is made there and joined.
    Create { el: El, start: Pt, moved: bool, spawn: Option<El> },
    Laser,
    Ruler { s0: Pos2, x: f64, y: f64 },
    RulerRotate,
    /// Alt and a drag with a pen, the highlighter or the eraser: its size (left button) or its
    /// opacity (right button) follow the pointer. Alt+click without a drag picks a colour.
    BrushAdjust { s0: Pos2, size0: f64, opacity0: f64, opacity: bool, moved: bool },
}

pub struct Editor {
    pub board: Board,
    pub board_id: String,
    pub painter: Painter,
    pub presence: Presence,
    pub prefs: Prefs,
    pub cam: Camera,
    pub tool: Tool,
    /// Which of the pens is in use.
    pub pen: usize,
    pub selection: Vec<String>,
    /// Element whose text is being typed.
    pub editing: Option<String>,
    /// For a table: the cell being typed in.
    pub editing_cell: Option<(usize, usize)>,
    /// Comment whose thread is open.
    pub comment: Option<String>,
    pub read_only: bool,
    /// Person (presence client id) whose view we follow.
    pub following: Option<u64>,
    pub ruler: Ruler,
    /// The board has visible content but none of it is on screen.
    pub lost: bool,
    /// Canvas size and top-left corner on screen, in points.
    pub size: (f64, f64),
    pub origin: Pos2,
    /// Canvas edges covered by floating panels (left, top, right, bottom): fitting and centring
    /// use the part of the board that can be seen.
    pub safe: [f64; 4],
    /// A camera move in progress: from, to, start (ms).
    fly: Option<(Camera, Camera, f64)>,
    /// This device's voter id.
    pub voter: String,
    /// Name to sign stickies with, if any.
    pub name: Option<String>,
    pub requests: Vec<Request>,
    pub(crate) gesture: Option<Gesture>,
    /// Elements changed by a gesture in progress, shown at once and written to the board a few
    /// times a second.
    pub(crate) preview: HashMap<String, Arc<El>>,
    pub(crate) hover: Option<String>,
    /// The pointer is over something clickable in a widget.
    pub(crate) hot: bool,
    pub(crate) hover_handle: Option<Handle>,
    /// The pointer is on a column line of the selected table.
    pub(crate) hover_col: bool,
    /// Shape or note under the pointer whose "+" show without selecting it, as in FigJam.
    pub(crate) hover_adds: Option<String>,
    pub(crate) space: bool,
    pub(crate) eraser_at: Option<Pos2>,
    pub(crate) guides: Vec<[f64; 4]>,
    pub(crate) laser: Vec<(f64, f64, f64)>,
    pub(crate) clipboard: Vec<El>,
    paste_count: u32,
    pub(crate) last_pen: f64,
    pub(crate) touches: HashMap<u64, Pos2>,
    /// Pen or finger held still: the menu opens after a while.
    pub(crate) press: Option<(Pos2, f64)>,
    flush_due: Option<f64>,
    pub(crate) pending_add: Option<Handle>,
    pub(crate) laser_clear: Option<f64>,
    /// The camera changed since the app last saved it.
    pub cam_dirty: bool,
    pub(crate) input: input::InputState,
    /// E held down: when, and where the emote wheel opens (screen).
    pub(crate) emote_hold: Option<(f64, Pos2)>,
    /// Mouse held on an emoji of the wheel: when the last of the stream went out.
    pub(crate) emote_spray: Option<f64>,
    /// Some were sent with clicks: letting go of E sends no more.
    pub(crate) emote_sprayed: bool,
    /// The eyedropper is armed: the next press on the board takes the colour there for the pen.
    pub eyedropper: bool,
    /// When the size was last set from the keyboard: its preview shows for a moment.
    pub(crate) flash_size: Option<f64>,
    /// Emotes floating up, everyone's: emoji, board point, when it started (ms, this clock).
    pub(crate) floaters: Vec<(String, f64, f64, f64)>,
    emote_n: u64,
    /// The last emote seen from each person, so each one floats once.
    pub(crate) seen_emotes: HashMap<u64, u64>,
}

/// FigJam's emotes: a quick feeling, floating up from the cursor for everyone, then gone.
pub const EMOTES: [&str; 8] = ["❤️", "👍", "😂", "🎉", "👏", "🔥", "😮", "💯"];

/// How long the E key is held before the emote wheel opens (a quick press picks the eraser).
pub const EMOTE_HOLD_MS: f64 = 250.0;

/// Rounds to 1/100.
fn r2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

/// Stroke coordinates rounded to 1/100 of the pen width, never coarser than 0.01.
fn stroke_round(size: f64) -> impl Fn(f64) -> f32 {
    let q = 100.0 / size.min(1.0);
    move |n| ((n * q).round() / q) as f32
}

fn now() -> f64 {
    crate::platform::now_ms()
}

pub fn editable(el: &El) -> bool {
    !el.locked && !el.hidden
}

/// What can receive votes: sticky notes, shapes, text, stamps, code, images; not lines, strokes, comments, tables or sections.
fn votable(el: &El) -> bool {
    !matches!(el.kind, Kind::Line(_) | Kind::Ink(_) | Kind::Highlighter(_) | Kind::Comment { .. } | Kind::Table(_) | Kind::Section { .. })
}

/// Elements with text to type into: text, stickies, shapes, section titles.
pub fn writable(el: &El) -> bool {
    !el.locked && (matches!(el.kind, Kind::Text(_) | Kind::Sticky(_) | Kind::Shape(_) | Kind::Section { .. } | Kind::Table(_) | Kind::Code(_)) || el.line().is_some_and(|l| !l.tape))
}

impl Editor {
    pub fn new(board: Board, board_id: String, painter: Painter, prefs: Prefs, voter: String) -> Editor {
        Editor {
            board,
            board_id,
            painter,
            presence: Presence::default(),
            prefs,
            cam: Camera { x: 0.0, y: 0.0, z: 1.0 },
            tool: Tool::Pen,
            pen: 0,
            selection: Vec::new(),
            editing: None,
            editing_cell: None,
            comment: None,
            read_only: false,
            following: None,
            ruler: Ruler { visible: false, x: 600.0, y: 400.0, angle: 0.0 },
            lost: false,
            size: (800.0, 600.0),
            origin: pos2(0.0, 0.0),
            safe: [0.0; 4],
            fly: None,
            voter,
            name: None,
            requests: Vec::new(),
            gesture: None,
            preview: HashMap::new(),
            hover: None,
            hot: false,
            hover_handle: None,
            hover_col: false,
            hover_adds: None,
            space: false,
            eraser_at: None,
            guides: Vec::new(),
            laser: Vec::new(),
            clipboard: Vec::new(),
            paste_count: 0,
            last_pen: 0.0,
            touches: HashMap::new(),
            press: None,
            flush_due: None,
            pending_add: None,
            laser_clear: None,
            cam_dirty: false,
            input: Default::default(),
            emote_hold: None,
            emote_spray: None,
            emote_sprayed: false,
            eyedropper: false,
            flash_size: None,
            floaters: Vec::new(),
            emote_n: 0,
            seen_emotes: HashMap::new(),
        }
    }

    /* ---------- sorting notes ---------- */

    /// FigJam's "Sort": the selected notes in columns, one per colour (or per author), from
    /// where the selection starts; within a column in their reading order.
    pub fn sort_stickies(&mut self, by_author: bool) {
        let mut notes: Vec<Arc<El>> = self.selected().into_iter().filter(|e| e.sticky().is_some() && editable(e)).collect();
        if notes.len() < 2 {
            return;
        }
        let Some(b) = union(notes.iter().map(|e| frame_box(e))) else { return };
        notes.sort_by(|a, c| (a.y / 50.0).round().total_cmp(&(c.y / 50.0).round()).then(a.x.total_cmp(&c.x)));
        let key = |e: &El| {
            let s = e.sticky().unwrap();
            if by_author { s.author.clone().unwrap_or_default() } else { s.color.to_uppercase() }
        };
        let mut groups: Vec<(String, Vec<Arc<El>>)> = Vec::new();
        for n in notes {
            let k = key(&n);
            match groups.iter_mut().find(|g| g.0 == k) {
                Some(g) => g.1.push(n),
                None => groups.push((k, vec![n])),
            }
        }
        let (w, h) = groups.iter().flat_map(|g| &g.1).fold((0.0f64, 0.0f64), |(w, h), e| (w.max(e.w), h.max(e.h)));
        let gap = 16.0;
        let mut out = Vec::new();
        let mut col = 0.0;
        for (_, group) in &groups {
            // Long groups wrap into several columns of at most six.
            for (i, e) in group.iter().enumerate() {
                let (c, r) = (i / 6, i % 6);
                out.push(El { x: b.x + (col + c as f64) * (w + gap), y: b.y + r as f64 * (h + gap), ..(**e).clone() });
            }
            col += group.len().div_ceil(6) as f64 + 0.5;
        }
        self.board.stop_capturing();
        self.board.put(out);
    }

    /* ---------- emotes ---------- */

    /// Sends an emote from a board point: it floats here and for everyone else.
    pub fn send_emote(&mut self, emoji: &str, at: Pt) {
        self.emote_n += 1;
        self.presence.set("emote", json!({ "e": emoji, "x": r2(at.x), "y": r2(at.y), "n": self.emote_n }));
        self.floaters.push((emoji.to_string(), at.x, at.y, now()));
    }

    /// The emote the wheel points at, for a pointer at `p` (screen).
    pub fn emote_under(&self, p: Pos2) -> Option<&'static str> {
        let (_, c) = self.emote_hold?;
        let d = p - c;
        if d.length() < 18.0 || d.length() > 110.0 {
            return None;
        }
        // Eight slices, the first straight up, going round clockwise.
        let a = (d.x.atan2(-d.y) / std::f32::consts::TAU).rem_euclid(1.0);
        Some(EMOTES[((a * 8.0 + 0.5) as usize) % 8])
    }

    /// Others' emotes, new since last time: they start floating.
    pub(crate) fn catch_emotes(&mut self) {
        for (id, s) in &self.presence.peers {
            let e = &s["emote"];
            let (Some(n), Some(emoji), Some(x), Some(y)) = (e["n"].as_u64(), e["e"].as_str(), e["x"].as_f64(), e["y"].as_f64()) else { continue };
            let seen = self.seen_emotes.insert(*id, n);
            // The first state seen of someone may hold an old emote: only changes float.
            if seen.is_some_and(|old| old != n) && EMOTES.contains(&emoji) && x.is_finite() && y.is_finite() {
                self.floaters.push((emoji.to_string(), x, y, now()));
            }
        }
        if self.floaters.len() > 60 {
            self.floaters.drain(..self.floaters.len() - 60);
        }
    }

    /* ---------- reading ---------- */

    /// Visible part of the board.
    pub fn view(&self) -> BBox {
        let a = self.cam.to_world(0.0, 0.0);
        BBox { x: a.x, y: a.y, w: self.size.0 / self.cam.z, h: self.size.1 / self.cam.z }
    }

    pub fn current(&self, id: &str) -> Option<Arc<El>> {
        self.preview.get(id).or_else(|| self.board.get(id)).cloned()
    }

    pub fn selected(&self) -> Vec<Arc<El>> {
        self.selection.iter().filter_map(|id| self.current(id)).collect()
    }

    /// Elements as they are drawn, gestures in progress included.
    pub fn paint_list(&mut self) -> Vec<Arc<El>> {
        let order = self.board.paint_order();
        let mut out: Vec<Arc<El>> = if self.preview.is_empty() { order.to_vec() } else { order.iter().map(|el| self.preview.get(&el.id).cloned().unwrap_or_else(|| el.clone())).collect() };
        // A section being drawn goes behind everything, anything else on top. Pressed on a "+",
        // nothing until it is dragged: a click adds a note or a shape, not this connector.
        if let Some(Gesture::Create { el, moved, spawn, .. }) = &self.gesture
            && (*moved || spawn.is_none())
        {
            let el = Arc::new(el.clone());
            if el.is_section() { out.insert(0, el) } else { out.push(el) }
        }
        out
    }

    pub fn screen_pt(&self, p: Pos2) -> Pos2 {
        pos2(p.x - self.origin.x, p.y - self.origin.y)
    }

    pub fn to_world(&self, s: Pos2) -> Pt {
        self.cam.to_world(s.x as f64, s.y as f64)
    }

    pub fn to_screen(&self, x: f64, y: f64) -> Pos2 {
        let p = self.cam.to_screen(x, y);
        pos2(p.x as f32, p.y as f32)
    }

    /// The topmost element under a board point; `tol_px` is the reach on screen.
    pub fn hit_element(&mut self, p: Pt, tol_px: f64) -> Option<Arc<El>> {
        let z = self.cam.z;
        let tol = tol_px / z;
        let order = self.board.paint_order().to_vec();
        for base in order.iter().rev() {
            let el = self.preview.get(&base.id).unwrap_or(base);
            if el.locked || el.hidden {
                continue;
            }
            if el.is_section() {
                let t = text::section_title_box(el, z);
                if (BBox { x: t.x, y: t.y, w: t.w, h: t.h }).contains(p) {
                    return Some(el.clone());
                }
            }
            if el.is_comment() {
                if expand(pin_box(el, z), tol / 2.0).contains(p) {
                    return Some(el.clone());
                }
                continue;
            }
            if !intersects(&expand(aabb(el), tol), &BBox { x: p.x, y: p.y, w: 0.0, h: 0.0 }) {
                continue;
            }
            if hit_test(el, p.x, p.y, tol, false) {
                return Some(el.clone());
            }
        }
        // Second pass: if no stroke, text, or filled element was hit directly, allow clicking
        // inside an unfilled shape so hollow shapes can be easily selected and moved.
        for base in order.iter().rev() {
            let el = self.preview.get(&base.id).unwrap_or(base);
            if el.locked || el.hidden {
                continue;
            }
            if intersects(&expand(aabb(el), tol), &BBox { x: p.x, y: p.y, w: 0.0, h: 0.0 })
                && shape_interior(el, p.x, p.y, tol)
            {
                return Some(el.clone());
            }
        }
        None
    }

    /* ---------- camera ---------- */

    /// Moves the camera at once, stopping any camera move in progress (wheel, drag, pinch).
    pub fn set_cam(&mut self, c: Camera) {
        self.fly = None;
        self.apply_cam(c);
    }

    fn apply_cam(&mut self, c: Camera) {
        let c = Camera { x: c.x, y: c.y, z: c.z.clamp(MIN_ZOOM, MAX_ZOOM) };
        if c != self.cam {
            self.cam = c;
            self.cam_dirty = true;
            let (w, h) = self.safe_center();
            self.presence.set("view", json!({ "cx": r2((w - c.x) / c.z), "cy": r2((h - c.y) / c.z), "z": (c.z * 1000.0).round() / 1000.0 }));
        }
    }

    pub fn zoom_at(&mut self, sx: f64, sy: f64, z: f64) {
        let nz = z.clamp(MIN_ZOOM, MAX_ZOOM);
        let w = self.cam.to_world(sx, sy);
        self.set_cam(Camera { x: sx - w.x * nz, y: sy - w.y * nz, z: nz });
    }

    /// Glides the camera to `c`: a quick ease-out, so zooming and jumping keep you oriented.
    /// A new target picks up from wherever the camera is, so repeated calls follow smoothly.
    pub fn fly_to(&mut self, c: Camera) {
        let c = Camera { x: c.x, y: c.y, z: c.z.clamp(MIN_ZOOM, MAX_ZOOM) };
        if c != self.cam {
            self.fly = Some((self.cam, c, crate::platform::now_ms()));
        }
    }

    /// Advances a camera move; true while it goes on.
    pub fn tick_camera(&mut self, now: f64) -> bool {
        let Some((a, b, start)) = self.fly else { return false };
        const MS: f64 = 320.0;
        let t = ((now - start) / MS).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - t).powi(3);
        // Zoom moves evenly on a log scale and the screen centre slides in board space, so a
        // big zoom change neither rushes nor swings sideways.
        let (cx, cy) = self.safe_center();
        let (wa, wb) = (a.to_world(cx, cy), b.to_world(cx, cy));
        let z = (a.z.ln() + (b.z.ln() - a.z.ln()) * e).exp();
        let (wx, wy) = (wa.x + (wb.x - wa.x) * e, wa.y + (wb.y - wa.y) * e);
        self.apply_cam(Camera { x: cx - wx * z, y: cy - wy * z, z });
        if t >= 1.0 {
            self.apply_cam(b);
            self.fly = None;
        }
        self.fly.is_some()
    }

    /// Centre of the part of the canvas not covered by panels, in canvas coordinates.
    pub fn safe_center(&self) -> (f64, f64) {
        let [l, t, r, b] = self.safe;
        ((l + self.size.0 - r) / 2.0, (t + self.size.1 - b) / 2.0)
    }

    fn safe_size(&self) -> (f64, f64) {
        let [l, t, r, b] = self.safe;
        ((self.size.0 - l - r).max(100.0), (self.size.1 - t - b).max(100.0))
    }

    /// Camera that shows `b` in the visible area, at most at `max_zoom`.
    fn fitted(&self, b: Option<BBox>, max_zoom: f64) -> Camera {
        let (cx, cy) = self.safe_center();
        let Some(b) = b else {
            return Camera { x: cx, y: cy, z: 1.0 };
        };
        let (w, h) = self.safe_size();
        let pad = 64.0f64.min(w / 6.0);
        let z = ((w - pad * 2.0) / b.w.max(1.0)).min((h - pad * 2.0) / b.h.max(1.0)).min(max_zoom).clamp(MIN_ZOOM, MAX_ZOOM);
        Camera { x: cx - (b.x + b.w / 2.0) * z, y: cy - (b.y + b.h / 2.0) * z, z }
    }

    pub fn fit_box(&mut self, b: Option<BBox>, max_zoom: f64) {
        let c = self.fitted(b, max_zoom);
        self.fly_to(c);
    }

    pub fn stop_following(&mut self) {
        self.following = None;
    }

    /* ---------- selection frame and handles ---------- */

    pub fn selection_frame(&self) -> Option<(BBox, f64)> {
        let els = self.selected();
        match els.as_slice() {
            [] => None,
            [one] if one.is_comment() => Some((pin_box(one, self.cam.z), 0.0)),
            [one] if !one.is_line() => Some((frame(one), one.rotation)),
            _ => Some((union(els.iter().map(|e| frame_box(e)))?, 0.0)),
        }
    }

    fn handle_scale(&self) -> f32 {
        if self.prefs.big_handles { 1.6 } else { 1.0 }
    }

    /// Handles of the selection, in canvas coordinates.
    pub fn handle_positions(&self) -> Vec<(Handle, Pos2)> {
        let els = self.selected();
        if self.read_only || els.is_empty() || els.iter().any(|e| e.locked) || (els.len() == 1 && els[0].is_comment()) {
            return Vec::new();
        }
        if let [one] = els.as_slice()
            && let Some(l) = one.line()
        {
            let (a, b) = line_world_ends(one, l);
            return vec![(Handle::P0, self.to_screen(a.x, a.y)), (Handle::P1, self.to_screen(b.x, b.y))];
        }
        let Some((b, rot)) = self.selection_frame() else { return Vec::new() };
        let c = b.center();
        let at = |h: Handle, lx: f64, ly: f64| {
            let p = rotate(lx, ly, c.x, c.y, rot);
            (h, self.to_screen(p.x, p.y))
        };
        let z = self.cam.z;
        let (sw, sh) = (b.w * z, b.h * z);
        let mut list = vec![at(Handle::Nw, b.x, b.y), at(Handle::Ne, b.right(), b.y), at(Handle::Se, b.right(), b.bottom()), at(Handle::Sw, b.x, b.bottom())];
        if sw > 28.0 {
            list.push(at(Handle::N, c.x, b.y));
            list.push(at(Handle::S, c.x, b.bottom()));
        }
        if sh > 28.0 {
            list.push(at(Handle::E, b.right(), c.y));
            list.push(at(Handle::W, b.x, c.y));
        }
        let one = if els.len() == 1 { Some(&els[0]) } else { None };
        let adds = one.is_some_and(|e| matches!(e.kind, Kind::Shape(_) | Kind::Sticky(_))) && sw > 24.0 && sh > 24.0;
        if adds {
            let d = 22.0 / z;
            list.extend([at(Handle::AddN, c.x, b.y - d), at(Handle::AddE, b.right() + d, c.y), at(Handle::AddS, c.x, b.bottom() + d), at(Handle::AddW, b.x - d, c.y)]);
        }
        if let Some(e) = one.filter(|e| e.table().is_some()) {
            // FigJam's "+" to grow a table: a row under it, a column beside it.
            let d = 18.0 / z;
            let _ = e;
            list.push(at(Handle::AddRow, c.x, b.bottom() + d));
            list.push(at(Handle::AddCol, b.right() + d, c.y));
        }
        if one.is_some_and(|e| e.is_section()) {
            return list;
        }
        if !(one.is_some_and(|e| matches!(e.kind, Kind::Stamp { .. })) && sw < 16.0) {
            let d = 22.0 * self.handle_scale();
            if adds {
                // The top "+" sits where the rotation knob would: the knob goes out from the top-right corner.
                let corner = rotate(b.right(), b.y, c.x, c.y, rot);
                let s = self.to_screen(corner.x, corner.y);
                let a = (rot - std::f64::consts::FRAC_PI_4 + std::f64::consts::FRAC_PI_2) as f32;
                list.push((Handle::Rot, pos2(s.x + a.sin() * d * 0.8, s.y - a.cos() * d * 0.8)));
            } else {
                let top = rotate(c.x, b.y, c.x, c.y, rot);
                let s = self.to_screen(top.x, top.y);
                list.push((Handle::Rot, pos2(s.x + (rot as f32).sin() * d, s.y - (rot as f32).cos() * d)));
            }
        }
        list
    }

    pub fn hit_handle(&self, s: Pos2, touch: bool) -> Option<Handle> {
        let r = (if touch { 18.0 } else { 9.0 }) * self.handle_scale();
        let mut best = None;
        let mut best_d = r;
        for (h, p) in self.handle_positions() {
            let d = p.distance(s);
            if d <= best_d {
                best_d = d;
                best = Some(h);
            }
        }
        if best.is_none() {
            let els = self.selected();
            if !self.read_only
                && !els.is_empty()
                && !els.iter().any(|e| e.locked)
                && !(els.len() == 1 && (els[0].is_comment() || els[0].line().is_some() || matches!(els[0].kind, Kind::Stamp { .. })))
                && let Some((b, rot)) = self.selection_frame()
            {
                let c = b.center();
                let corner = |lx: f64, ly: f64| {
                    let p = rotate(lx, ly, c.x, c.y, rot);
                    self.to_screen(p.x, p.y)
                };
                let (nw, ne, se, sw) = (corner(b.x, b.y), corner(b.right(), b.y), corner(b.right(), b.bottom()), corner(b.x, b.bottom()));
                let seg_d = |a: Pos2, b: Pos2| -> f32 {
                    let ab = b - a;
                    let len2 = ab.length_sq();
                    if len2 <= 1e-4 {
                        return s.distance(a);
                    }
                    let t = ((s - a).dot(ab) / len2).clamp(0.0, 1.0);
                    s.distance(a + ab * t)
                };
                let mut side_d = r * 0.75;
                for (h, p0, p1) in [(Handle::N, nw, ne), (Handle::S, sw, se), (Handle::E, ne, se), (Handle::W, nw, sw)] {
                    if matches!(h, Handle::N | Handle::S) && els.len() == 1 && els[0].text().is_some() {
                        continue;
                    }
                    let d = seg_d(p0, p1);
                    if d <= side_d {
                        side_d = d;
                        best = Some(h);
                    }
                }
            }
        }
        best
    }

    /// The four "+" beside a shape or note, in canvas coordinates.
    pub fn add_handles(&self, el: &El) -> Vec<(Handle, Pos2)> {
        let (b, rot) = (frame(el), el.rotation);
        let z = self.cam.z;
        if b.w * z <= 24.0 || b.h * z <= 24.0 {
            return Vec::new();
        }
        let c = b.center();
        let d = 22.0 / z;
        [(Handle::AddN, c.x, b.y - d), (Handle::AddE, b.right() + d, c.y), (Handle::AddS, c.x, b.bottom() + d), (Handle::AddW, b.x - d, c.y)]
            .into_iter()
            .map(|(h, x, y)| {
                let p = rotate(x, y, c.x, c.y, rot);
                (h, self.to_screen(p.x, p.y))
            })
            .collect()
    }

    /// Whether "+" can grow a diagram from this element.
    pub fn can_branch(el: &El) -> bool {
        matches!(el.kind, Kind::Shape(_) | Kind::Sticky(_)) && editable(el)
    }

    /// The "+" of the element under the pointer (not selected) at a screen point.
    pub(crate) fn hit_hover_add(&self, s: Pos2, touch: bool) -> Option<(Handle, Arc<El>)> {
        let id = self.hover_adds.as_ref()?;
        if self.read_only || self.selection.contains(id) {
            return None;
        }
        let el = self.current(id)?;
        let r = if touch { 18.0 } else { 10.0 };
        self.add_handles(&el).into_iter().map(|(h, p)| (h, p.distance(s))).filter(|(_, d)| *d <= r).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(h, _)| (h, el.clone()))
    }

    /// Keeps the "+" of the shape or note under the pointer, while the pointer stays on it or
    /// on its way to one of them.
    pub(crate) fn track_hover_adds(&mut self, s: Pos2, hit: Option<&Arc<El>>) {
        if self.read_only || self.tool != Tool::Select {
            self.hover_adds = None;
            return;
        }
        if let Some(h) = hit.filter(|e| Editor::can_branch(e)) {
            self.hover_adds = Some(h.id.clone());
            return;
        }
        let keep = self.hover_adds.as_ref().and_then(|id| self.current(id)).is_some_and(|el| {
            let p = self.to_world(s);
            let l = to_local(&el, p.x, p.y);
            let m = 40.0 / self.cam.z;
            l.x > -m && l.y > -m && l.x < el.w + m && l.y < el.h + m
        });
        if !keep {
            self.hover_adds = None;
        }
    }

    /// The selected table and the column whose right-hand line is under a screen point.
    pub(crate) fn column_line_at(&self, s: Pos2) -> Option<(Arc<El>, usize)> {
        let [one] = self.selected().try_into().ok()?;
        let t = one.table()?;
        if one.locked || self.read_only {
            return None;
        }
        let p = self.to_world(s);
        let l = to_local(&one, p.x, p.y);
        if l.y < 0.0 || l.y > one.h {
            return None;
        }
        let reach = 4.0 / self.cam.z;
        let edges = t.col_edges();
        (1..edges.len() - 1).find(|&i| (l.x - edges[i]).abs() <= reach).map(|i| (one.clone(), i - 1))
    }

    /// A table grows by a row or a column from its "+".
    pub(crate) fn grow_table(&mut self, col: bool) {
        let sel = self.selected();
        let [one] = sel.as_slice() else { return };
        self.board.stop_capturing();
        self.board.update(&[one.id.clone()], |el| {
            if let Kind::Table(t) = &mut el.kind {
                if col { t.insert_col(t.cols.len()) } else { t.insert_row(t.rows.len()) }
            }
            text::fit_text(el);
        });
    }

    pub fn inside_selection(&self, p: Pt) -> bool {
        let Some((b, rot)) = self.selection_frame() else { return false };
        let c = b.center();
        b.contains(rotate(p.x, p.y, c.x, c.y, -rot))
    }

    /* ---------- committing edits ---------- */

    pub fn flush_preview(&mut self) {
        self.flush_due = None;
        if self.preview.is_empty() || self.read_only {
            return;
        }
        let els: Vec<Arc<El>> = self.preview.values().filter(|el| self.board.get(&el.id).is_some()).cloned().collect();
        self.board.transact(|e| {
            for el in &els {
                e.put(el);
            }
        });
    }

    fn schedule_flush(&mut self) {
        if self.flush_due.is_none() {
            // With others on the board, about 30 times a second, so they see it move smoothly.
            self.flush_due = Some(now() + if self.presence.peers.is_empty() { 110.0 } else { 33.0 });
        }
    }

    /// Called every frame: writes a gesture's changes to the board now and then, so others see them.
    pub fn tick(&mut self) {
        if self.flush_due.is_some_and(|t| now() >= t) {
            self.flush_preview();
        }
        // The ruler, for the others: where it lies on the board and how long it is there.
        let r = self.ruler;
        let shown = if r.visible {
            let c = self.cam.to_world(r.x, r.y);
            json!({ "x": r2(c.x), "y": r2(c.y), "a": (r.angle * 1000.0).round() / 1000.0, "len": r2(crate::ink::RULER_LENGTH / self.cam.z) })
        } else {
            Value::Null
        };
        self.presence.set("ruler", shown);
        if self.laser_clear.is_some_and(|t| now() >= t) {
            self.laser_clear = None;
            if !matches!(self.gesture, Some(Gesture::Laser)) {
                self.presence.set("laser", Value::Null);
            }
        }
        if let Some((at, since)) = self.press
            && now() - since > 550.0
        {
            self.press = None;
            if self.touches.len() <= 1 && matches!(self.gesture, Some(Gesture::Move { .. } | Gesture::Marquee { .. } | Gesture::Pan { .. } | Gesture::Lasso { .. })) {
                self.open_menu(at);
            }
        }
        // QuickShape: holding the pen still at the end of a stroke snaps it into a clean shape in-place.
        if self.prefs.quick_shape
            && let Some(Gesture::Draw(g)) = &mut self.gesture
            && !g.hl
            && g.brush != Some(Brush::Smudge)
            && g.snap.is_none()
            && !g.quick_shape_snapped
            && g.pts.len() >= 18
            && g.last_move_ms > 0.0
            && now() - g.last_move_ms >= 420.0
        {
            let flat: Vec<f32> = g.pts.iter().map(|&v| v as f32).collect();
            if let Some(rec) = ink::recognize(&flat, 1.0 / self.cam.z) {
                g.pts = recognized_stroke_pts(&rec);
                g.quick_shape_snapped = true;
                g.pieces.clear();
                g.pieces_pts = usize::MAX;
                self.presence.dirty = true;
            }
        }
        // Lose selection of elements that are gone (deleted by someone else, undone…).
        if !matches!(self.gesture, Some(Gesture::Move { .. })) {
            self.prune();
        }
    }

    fn base(&mut self, mut el: El) -> El {
        el.rotation = 0.0;
        el.z = self.board.top_z();
        el.opacity = 1.0;
        // The shadow chosen in the settings for new notes, code and widgets (without one, or
        // another than the usual, it is written into the element so everyone sees the same).
        if el.shadow.is_none() && crate::style::default_shade(&el).is_some() {
            let p = &self.prefs;
            let usual = Prefs::default();
            if !p.shadows {
                el.shadow = Some(crate::model::Shade { opacity: 0.0, blur: p.shadow_blur, y: 3.0 });
            } else if (p.shadow_opacity - usual.shadow_opacity).abs() > 1e-6 || (p.shadow_blur - usual.shadow_blur).abs() > 1e-6 {
                el.shadow = Some(crate::model::Shade { opacity: p.shadow_opacity, blur: p.shadow_blur, y: (p.shadow_blur * 0.3).round() });
            }
        }
        el
    }

    /// Sizes chosen in the toolbar are what you see on screen, whatever the zoom.
    pub fn world_size(&self, screen_px: f64) -> f64 {
        r2(screen_px / self.cam.z)
    }

    /// Current board-space centre used for drawing symmetry axes.
    pub fn symmetry_center(&self) -> Pt {
        if let Some(Gesture::Draw(g)) = &self.gesture {
            return g.sym_center;
        }
        let (cx, cy) = self.safe_center();
        self.cam.to_world(cx, cy)
    }

    fn ink_element(&mut self, pts: &[f64], size: f64, color: &str, hl: bool, opacity: f64, brush: Option<Brush>) -> El {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in pts.chunks_exact(3) {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let rnd = stroke_round(size);
        let points = pts.chunks_exact(3).flat_map(|p| [rnd(p[0] - x0), rnd(p[1] - y0), ((p[2] * 1000.0).round() / 1000.0) as f32]).collect();
        let ink = Ink { points, color: color.into(), size, brush, colors: Vec::new() };
        let mut el = self.base(El::new(if hl { Kind::Highlighter(ink) } else { Kind::Ink(ink) }));
        (el.x, el.y, el.w, el.h, el.opacity) = (x0, y0, x1 - x0, y1 - y0, opacity);
        el
    }

    fn line_element(&mut self, a: Pt, b: Pt, style: impl FnOnce(&mut El, &mut Line)) -> El {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        let mut line = Line { points: vec![(a.x - x0) as f32, (a.y - y0) as f32, (b.x - x0) as f32, (b.y - y0) as f32], stroke: "#1E1E1E".into(), stroke_width: 3.0, dash: false, arrow_start: false, arrow_end: false, from: None, to: None, tape: false, route: crate::model::Route::Straight, label: None, ..Default::default() };
        let mut el = self.base(El::new(Kind::Line(line.clone())));
        (el.x, el.y, el.w, el.h) = (x0, y0, (b.x - a.x).abs(), (b.y - a.y).abs());
        style(&mut el, &mut line);
        el.kind = Kind::Line(line);
        el
    }

    fn connector(&mut self, at: Pt, from: Option<String>, arrow: bool) -> El {
        let (stroke, width) = (self.prefs.shape_style.stroke.clone(), self.world_size(self.prefs.shape_style.stroke_width.max(2.0)));
        let (route, head) = (self.prefs.route, self.prefs.arrow_head);
        self.line_element(at, at, |_, l| {
            l.stroke = stroke;
            l.stroke_width = width;
            l.arrow_end = arrow;
            l.head_end = head;
            l.from = from;
            l.route = route;
        })
    }

    /* ---------- drawing ---------- */

    /// What the drawing tool in use lays down: its size on screen and its opacity (the eraser:
    /// its size and strength). None for the other tools.
    pub(crate) fn brush_feel(&self) -> Option<(f64, f64)> {
        match self.tool {
            Tool::Pen => self.prefs.pens.get(self.pen).map(|p| (p.size, p.opacity)),
            Tool::Highlighter => Some((self.prefs.highlighter.size, self.prefs.highlighter.opacity)),
            Tool::Eraser => Some((self.prefs.eraser.size, self.prefs.eraser.strength)),
            _ => None,
        }
    }

    /// Sets what `brush_feel` tells, each kept within what suits the tool.
    pub(crate) fn set_brush_feel(&mut self, size: Option<f64>, opacity: Option<f64>) {
        let (lo, hi) = match self.tool {
            Tool::Pen => self.prefs.pens.get(self.pen).map_or((1.0, 64.0), |p| crate::app::toolbar::size_range(p.brush)),
            Tool::Highlighter => (4.0, 80.0),
            _ => (4.0, 200.0),
        };
        let size = size.map(|s| (s.clamp(lo, hi) * 2.0).round() / 2.0);
        let opacity = opacity.map(|o| (o.clamp(0.05, 1.0) * 100.0).round() / 100.0);
        let (s, o) = match self.tool {
            Tool::Pen => match self.prefs.pens.get_mut(self.pen) {
                Some(p) => (&mut p.size, &mut p.opacity),
                None => return,
            },
            Tool::Highlighter => (&mut self.prefs.highlighter.size, &mut self.prefs.highlighter.opacity),
            Tool::Eraser => (&mut self.prefs.eraser.size, &mut self.prefs.eraser.strength),
            _ => return,
        };
        if let Some(size) = size {
            *s = size;
        }
        if let Some(opacity) = opacity {
            *o = opacity;
        }
    }

    pub(crate) fn start_draw(&mut self, s: Pos2, hl: bool, kind: PointerKind, pointer: u64) {
        let pen = if hl { self.prefs.highlighter.clone() } else { self.prefs.pens.get(self.pen).or(self.prefs.pens.first()).cloned().unwrap_or_default() };
        let size = self.world_size(pen.size);
        let cutoff = if kind == PointerKind::Mouse { self.prefs.ink_smoothing.cutoff() * 1.2 } else { self.prefs.ink_smoothing.cutoff() };
        let color = pen.color;
        // Tilted, the shading brushes lay a broader stroke: its width set as it starts, the same
        // while it is drawn and once it is down.
        let tilt = if self.prefs.tilt_shading && !hl { self.input.tilt } else { 0.0 };
        let size = size * ink::tilt_scale(pen.brush, tilt);
        let smear = (!hl && pen.brush == Brush::Smudge).then(|| Smear::of(self, size * 2.0).map(Box::new)).flatten();
        let (cx, cy) = self.safe_center();
        let sym_center = self.cam.to_world(cx, cy);
        self.gesture = Some(Gesture::Draw(Box::new(Draw {
            hl,
            brush: (!hl).then_some(pen.brush),
            color,
            size,
            opacity: pen.opacity.clamp(0.05, 1.0),
            pts: Vec::new(),
            snap: ink::ruler_snapper(&self.ruler, s.x as f64, s.y as f64, size * self.cam.z / 2.0),
            filter: Some(OneEuro::with_streamline(cutoff, self.prefs.streamline)),
            pressure: None,
            use_pressure: (kind == PointerKind::Pen || self.input.force.is_some()) && self.prefs.pressure && !hl,
            first_force: None,
            force_varied: false,
            last_move_ms: now(),
            quick_shape_snapped: false,
            sym_center,
            gamma: self.prefs.sensitivity.gamma(),
            speed: None,
            last_raw: None,
            raw: None,
            down_ms: None,
            pointer,
            sid: (now() * 1000.0) as u64,
            pieces: Vec::new(),
            pieces_cam: (0, 0, 0, 0, 0),
            pieces_pts: usize::MAX,
            colors: Vec::new(),
            smear,
        })));
    }

    /// A new pen position (canvas coordinates), with the pen pressure if any.
    pub(crate) fn add_draw_point(&mut self, raw: Pos2, t_ms: f64, force: Option<f32>) {
        let cam = self.cam;
        let pressure_pref = self.prefs.pressure;
        let Some(Gesture::Draw(g)) = &mut self.gesture else { return };
        if g.quick_shape_snapped {
            if g.raw.is_some_and(|p0| p0.distance(raw) < 6.0) {
                return;
            }
            // Moved significantly after QuickShape snap: keep the snapped shape clean on release.
            return;
        }
        g.raw = Some(raw);
        if let Some((p0, t0)) = g.last_raw {
            if p0.distance(raw) > 2.2 {
                g.last_move_ms = t_ms;
            }
            if t_ms > t0 {
                let v = p0.distance(raw) as f64 / (t_ms - t0);
                g.speed = Some(g.speed.map_or(v, |s| s + (v - s) * 0.3));
            }
        } else {
            g.last_move_ms = t_ms;
        }
        // Time since the previous sample (ms), for a smoothing that is the same whatever the
        // pen's rate; and since the pen touched down.
        let dt = g.last_raw.map_or(8.0, |(_, t0)| (t_ms - t0).clamp(0.0, 100.0));
        let since_down = t_ms - *g.down_ms.get_or_insert(t_ms);
        g.last_raw = Some((raw, t_ms));
        let s = match &mut g.filter {
            Some(f) => f.filter(raw.x as f64, raw.y as f64, t_ms),
            None => pt(raw.x as f64, raw.y as f64),
        };
        let force = force.or_else(crate::pen::touch_pressure).or_else(|| crate::pen::mouse_pen().map(|p| p.pressure));
        if force.is_some() && pressure_pref && !g.hl {
            g.use_pressure = true;
        }
        let mut pressure = -1.0;
        if g.use_pressure
            && let Some(f) = force
        {
            let f0 = *g.first_force.get_or_insert(f);
            if (f - f0).abs() > 0.012 || (f - 0.5).abs() > 0.025 {
                g.force_varied = true;
            }
            if g.force_varied || (f - 0.5).abs() > 0.025 {
                // Real hardware pressure from Windows Ink, Wintab, or PointerEvents: map across
                // the full 0.02..=1.0 range through the user's sensitivity curve.
                let raw_p = (f as f64).clamp(0.02, 1.0).powf(g.gamma);
                // Eased over about 12 ms, and never faster than full in 40 ms: a driver's odd
                // spike leaves no blot. The first touch eases in over 30 ms, a fine start.
                let ease = 1.0 - (-dt / 12.0).exp();
                let p = match g.pressure {
                    None => raw_p * 0.35,
                    Some(prev) => {
                        let step = ((raw_p - prev) * ease).clamp(-dt / 40.0, dt / 40.0);
                        let p = prev + step;
                        if since_down < 30.0 { p.min(raw_p * (0.35 + 0.65 * since_down / 30.0)) } else { p }
                    }
                };
                g.pressure = Some(p);
                pressure = (p * 1000.0).round() / 1000.0;
            } else {
                // Stylus driver reporting constant 0.5 dummy force: synthesize natural velocity
                // pressure modulated by sensitivity so the stroke still varies dynamically.
                let v = g.speed.unwrap_or(0.0);
                let sim = ((1.0 - (v - 0.25) / 2.4).clamp(0.18, 0.98)).powf(g.gamma);
                let p = g.pressure.map_or(sim, |prev| prev + (sim - prev) * 0.35);
                g.pressure = Some(p);
                pressure = (p * 1000.0).round() / 1000.0;
            }
        } else if g.brush == Some(Brush::Brush) {
            // A brush pen with a mouse (or a pen that has no pressure): thinner where it moves fast,
            // as ink thins on a quick flick.
            let v = g.speed.unwrap_or(0.0);
            pressure = ((1.0 - (v - 0.3) / 2.5).clamp(0.15, 1.0) * 1000.0).round() / 1000.0;
        }
        push_draw_point(g, &cam, pt(s.x, s.y), pressure, false);
        smear_points(g);
        // The others get the new points with the next presence message.
        self.presence.dirty = true;
    }

    /// What goes to the others now: this person's state, with the stroke being drawn as the
    /// points added since the last message (they put the pieces back together).
    pub fn presence_message(&mut self) -> Value {
        let mut state = self.presence.local.clone();
        if let Some(Gesture::Draw(g)) = &self.gesture {
            let n = g.pts.len() / 3;
            let from = if self.presence.live_sid == g.sid { self.presence.live_sent.min(n) } else { 0 };
            let rnd = stroke_round(g.size);
            let pts: Vec<Value> = g.pts[from * 3..].iter().enumerate().map(|(i, &v)| json!(if i % 3 == 2 { r2(v) } else { rnd(v) as f64 })).collect();
            state.insert("live".into(), json!({ "sid": g.sid, "from": from, "pts": pts, "color": g.color, "size": g.size, "hl": g.hl, "op": g.opacity, "brush": g.brush }));
            (self.presence.live_sid, self.presence.live_sent) = (g.sid, n);
        }
        Value::Object(state)
    }

    fn commit_draw(&mut self, mut g: Box<Draw>) {
        self.presence.end_live();
        // The filter trails the pen slightly: finish exactly where it was lifted.
        if !g.quick_shape_snapped
            && let Some(raw) = g.raw
            && !g.pts.is_empty()
        {
            let last = g.pts[g.pts.len() - 1];
            push_draw_point(&mut g, &self.cam, pt(raw.x as f64, raw.y as f64), last, false);
            smear_points(&mut g);
        }
        if g.pts.is_empty() || self.read_only {
            return;
        }
        // With pressure, the end thins away as the pen lifts (over the last few points, as
        // long as the stroke is a few widths long): the tail Procreate gives its ink.
        if g.use_pressure && !g.hl && !g.quick_shape_snapped {
            let n = g.pts.len() / 3;
            let length: f64 = (1..n).map(|i| (g.pts[i * 3] - g.pts[i * 3 - 3]).hypot(g.pts[i * 3 + 1] - g.pts[i * 3 - 2])).sum();
            if n >= 6 && length > g.size * 4.0 {
                let tail = (n / 4).clamp(2, 6);
                for k in 0..tail {
                    let p = &mut g.pts[(n - 1 - k) * 3 + 2];
                    if *p >= 0.0 {
                        *p *= 0.35 + 0.65 * (k as f64 / tail as f64);
                    }
                }
            }
        }
        let z = self.cam.z;
        let flat: Vec<f32> = g.pts.iter().map(|&v| v as f32).collect();
        let kept = ink::thin_indices(&flat, (0.2 / z).min(g.size * 0.08));
        let thin: Vec<f64> = kept.iter().flat_map(|&i| g.pts[i * 3..i * 3 + 3].iter().copied()).collect();
        let colors: Vec<u32> = if g.colors.is_empty() { Vec::new() } else { kept.iter().map(|&i| g.colors.get(i).or(g.colors.last()).copied().unwrap_or(0)).collect() };
        let sym = self.prefs.symmetry;
        let mut el = self.ink_element(&thin, g.size, &g.color, g.hl, g.opacity, g.brush);
        if let Kind::Ink(i) = &mut el.kind {
            i.colors = colors.clone();
        }
        // QuickShape keeps the stroke, with its brush, in its perfect shape (as in Procreate); the
        // setting "ink to shapes" makes shapes of what is drawn.
        if !g.hl && self.prefs.ink_to_shape && g.snap.is_none() && sym == crate::prefs::Symmetry::Off {
            let flat: Vec<f32> = thin.iter().map(|&v| v as f32).collect();
            if let Some(r) = ink::recognize(&flat, 1.0 / z) {
                let (color, size, op) = (g.color.clone(), g.size, g.opacity);
                let shape = |kind: ShapeKind, points: Option<Vec<f32>>| Kind::Shape(Shape { shape: kind, fill: "transparent".into(), stroke: color.clone(), stroke_width: size, points, ..Default::default() });
                // A polygon through these corners, kept in its own box.
                let polygon = |ed: &mut Editor, p: &[Pt]| {
                    let b = bounds_of(p.iter().copied());
                    let pts = p.iter().flat_map(|q| [((q.x - b.x) / b.w.max(1e-9)) as f32, ((q.y - b.y) / b.h.max(1e-9)) as f32]).collect();
                    let mut s = ed.base(El::new(shape(ShapeKind::Polygon, Some(pts))));
                    (s.x, s.y, s.w, s.h, s.opacity) = (b.x, b.y, b.w, b.h, op);
                    s
                };
                el = match r {
                    Recognized::Line { x1, y1, x2, y2 } | Recognized::Arrow { x1, y1, x2, y2 } => {
                        let c = color.clone();
                        let arrow = matches!(r, Recognized::Arrow { .. });
                        self.line_element(pt(x1, y1), pt(x2, y2), |el, l| {
                            l.stroke = c;
                            l.stroke_width = size;
                            l.arrow_end = arrow;
                            el.opacity = op;
                        })
                    }
                    Recognized::Triangle(p) => polygon(self, &p),
                    Recognized::Polygon(p) => polygon(self, &p),
                    Recognized::Turned { cx, cy, w, h, angle } => {
                        let mut s = self.base(El::new(shape(ShapeKind::Rect, None)));
                        (s.x, s.y, s.w, s.h, s.rotation, s.opacity) = (cx - w / 2.0, cy - h / 2.0, w, h, angle, op);
                        s
                    }
                    Recognized::Star { x, y, w, h } => {
                        let mut s = self.base(El::new(shape(ShapeKind::Star, None)));
                        (s.x, s.y, s.w, s.h, s.opacity) = (x, y, w, h, op);
                        s
                    }
                    Recognized::Rect { x, y, w, h } | Recognized::Ellipse { x, y, w, h } | Recognized::Diamond { x, y, w, h } => {
                        let kind = match r {
                            Recognized::Rect { .. } => ShapeKind::Rect,
                            Recognized::Ellipse { .. } => ShapeKind::Ellipse,
                            _ => ShapeKind::Diamond,
                        };
                        let mut s = self.base(El::new(shape(kind, None)));
                        (s.x, s.y, s.w, s.h, s.opacity) = (x, y, w, h, op);
                        s
                    }
                };
            }
        }
        let mut els = vec![el];
        if sym != crate::prefs::Symmetry::Off {
            for m_pts in symmetry_pts(sym, g.sym_center, &thin) {
                let mut m = self.ink_element(&m_pts, g.size, &g.color, g.hl, g.opacity, g.brush);
                if let Kind::Ink(i) = &mut m.kind {
                    i.colors = colors.clone();
                }
                els.push(m);
            }
        }
        self.board.stop_capturing();
        self.board.put(els);
    }

    /* ---------- erasing ---------- */

    pub(crate) fn start_erase(&mut self, p: Pt) {
        self.board.stop_capturing();
        self.gesture = Some(Gesture::Erase { last: p, seg: 0, touched: HashMap::new() });
        self.erase_along(p, p);
    }

    pub(crate) fn erase_along(&mut self, a: Pt, b: Pt) {
        let radius = self.prefs.eraser.size / 2.0 / self.cam.z;
        if self.prefs.eraser.mode == crate::prefs::EraserMode::Stroke {
            return self.erase_strokes(a, b, radius);
        }
        // Pixel eraser (Paint): each element touched gets this pass as a mark in its own coordinates.
        let area = expand(normalize_box(a.x, a.y, b.x, b.y), radius);
        let steps = ((b.x - a.x).hypot(b.y - a.y) / radius.max(1e-6)).ceil().max(1.0) as usize;
        let rnd = stroke_round(radius);
        let strength = self.prefs.eraser.strength.clamp(0.05, 1.0);
        let all = self.board.all().to_vec();
        let Some(Gesture::Erase { seg, touched, .. }) = &mut self.gesture else { return };
        *seg += 1;
        for el in all {
            if !matches!(el.kind, Kind::Ink(_) | Kind::Highlighter(_) | Kind::Line(_) | Kind::Shape(_) | Kind::Image { .. } | Kind::Stamp { .. }) || !editable(&el) || !intersects(&aabb(&el), &area) {
                continue;
            }
            let hit = (0..=steps).any(|i| {
                let t = i as f64 / steps as f64;
                hit_test(&el, a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, radius, true)
            });
            if !hit {
                continue;
            }
            let t = touched.entry(el.id.clone()).or_insert_with(|| Touched { orig: (*el).clone(), strokes: Vec::new(), last_seg: -1 });
            let (pa, pb) = (to_local(&t.orig, a.x, a.y), to_local(&t.orig, b.x, b.y));
            if t.last_seg == *seg - 1
                && let Some(run) = t.strokes.last_mut()
            {
                // Points closer than a quarter of the eraser add nothing visible.
                let (lx, ly) = (run[run.len() - 2] as f64, run[run.len() - 1] as f64);
                if (pb.x - lx).hypot(pb.y - ly) > radius / 4.0 {
                    run.push(rnd(pb.x));
                    run.push(rnd(pb.y));
                }
            } else {
                t.strokes.push(vec![rnd(pa.x), rnd(pa.y), rnd(pb.x), rnd(pb.y)]);
            }
            t.last_seg = *seg;
            let s = rnd(radius * 2.0) as f64;
            let mut next = t.orig.clone();
            next.erase.extend(t.strokes.iter().map(|p| EraseMark { p: p.clone(), s, a: strength }));
            self.preview.insert(el.id.clone(), Arc::new(next));
        }
        self.schedule_flush();
    }

    /// End of a pixel-eraser pass: elements with nothing visible left are removed for good.
    fn finish_erase(&mut self, touched: HashMap<String, Touched>) {
        self.flush_due = None;
        if touched.is_empty() || self.read_only {
            self.preview.clear();
            return;
        }
        let mut puts = Vec::new();
        let mut gone = Vec::new();
        for id in touched.keys() {
            let Some(el) = self.preview.get(id) else { continue };
            if self.board.get(id).is_none() {
                continue;
            }
            let images = &self.painter.images;
            if crate::raster::nothing_left(el, &|k| images.pixmap(k)) { gone.push(id.clone()) } else { puts.push(el.clone()) }
        }
        self.board.transact(|e| {
            for el in &puts {
                e.put(el);
            }
            for id in &gone {
                e.remove(id);
            }
        });
        self.preview.clear();
    }

    /// Stroke mode: whatever the eraser touches goes away whole.
    fn erase_strokes(&mut self, a: Pt, b: Pt, radius: f64) {
        let area = expand(normalize_box(a.x, a.y, b.x, b.y), radius);
        let mut remove = Vec::new();
        for el in self.board.all().to_vec() {
            if !matches!(el.kind, Kind::Ink(_) | Kind::Highlighter(_) | Kind::Line(_) | Kind::Shape(_)) || !editable(&el) || !intersects(&aabb(&el), &area) {
                continue;
            }
            let (la, lb) = (to_local(&el, a.x, a.y), to_local(&el, b.x, b.y));
            let hit = match &el.kind {
                Kind::Ink(i) | Kind::Highlighter(i) => {
                    let reach = radius + i.size / 2.0;
                    let p = &i.points;
                    p.chunks_exact(3).any(|q| dist_to_segment(q[0] as f64, q[1] as f64, la.x, la.y, lb.x, lb.y) <= reach)
                        // A fast swipe can cross a stroke between two of its points.
                        || p.chunks_exact(3).zip(p.chunks_exact(3).skip(1)).any(|(q, r)| segment_distance(q[0] as f64, q[1] as f64, r[0] as f64, r[1] as f64, la.x, la.y, lb.x, lb.y) <= reach)
                }
                Kind::Line(l) => {
                    let q = &l.points;
                    segment_distance(q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64, la.x, la.y, lb.x, lb.y) <= radius + l.stroke_width / 2.0
                }
                _ => {
                    let steps = ((b.x - a.x).hypot(b.y - a.y) / radius.max(1.0)).ceil().max(1.0) as usize;
                    (0..=steps).any(|i| {
                        let t = i as f64 / steps as f64;
                        hit_test(&el, a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, radius, false)
                    })
                }
            };
            if hit {
                remove.push(el.id.clone());
            }
        }
        if !remove.is_empty() {
            self.board.remove(remove);
        }
    }

    /* ---------- moving ---------- */

    /// The elements plus everything inside the sections among them (a section moves with its content).
    pub fn with_content(&mut self, els: &[Arc<El>]) -> Vec<Arc<El>> {
        let mut seen: HashSet<String> = els.iter().map(|e| e.id.clone()).collect();
        let mut out = els.to_vec();
        let all = self.board.all().to_vec();
        for sec in els.iter().filter(|e| e.is_section()) {
            let f = frame_box(sec);
            for el in &all {
                if !seen.contains(&el.id) && editable(el) && f.holds(&frame_box(el)) {
                    seen.insert(el.id.clone());
                    out.push(el.clone());
                }
            }
        }
        out
    }

    /// Copies with new ids, stacked on top in the same order. Connectors copied with both ends'
    /// elements join the copies; an end whose element stays behind is let go.
    pub fn clone_all(&mut self, els: &[Arc<El>], dx: f64, dy: f64) -> (Vec<El>, HashMap<String, String>) {
        let ids: HashMap<String, String> = els.iter().map(|e| (e.id.clone(), uid())).collect();
        let mut z = self.board.top_z();
        let mut sorted = els.to_vec();
        sorted.sort_by(|a, b| a.z.total_cmp(&b.z));
        let clones = sorted
            .iter()
            .map(|el| {
                let mut c = (**el).clone();
                c.id = ids[&el.id].clone();
                c.x += dx;
                c.y += dy;
                c.z = z;
                c.locked = false;
                z += 1.0;
                if let Some(l) = c.line_mut() {
                    l.from = l.from.as_ref().and_then(|f| ids.get(f).cloned());
                    l.to = l.to.as_ref().and_then(|t| ids.get(t).cloned());
                }
                c
            })
            .collect();
        (clones, ids)
    }

    pub(crate) fn start_move(&mut self, p: Pt, s: Pos2, duplicate: bool) {
        let picked: Vec<Arc<El>> = self.selection.iter().filter_map(|id| self.board.get(id)).filter(|e| editable(e)).cloned().collect();
        if picked.is_empty() {
            return;
        }
        let mut els = self.with_content(&picked);
        self.board.stop_capturing();
        if duplicate {
            let (clones, ids) = self.clone_all(&els, 0.0, 0.0);
            self.board.put(clones.clone());
            els = clones.into_iter().map(Arc::new).collect();
            self.selection = picked.iter().map(|e| ids[&e.id].clone()).collect();
        }
        let moving: HashSet<&str> = els.iter().map(|e| e.id.as_str()).collect();
        // A connector dragged on its own lets go of the elements it joined.
        let orig: Vec<El> = els
            .iter()
            .map(|el| match el.line() {
                Some(l) => unbind(el, l.from.as_deref().is_some_and(|f| !moving.contains(f)), l.to.as_deref().is_some_and(|t| !moving.contains(t))),
                None => (**el).clone(),
            })
            .collect();
        let ids: HashSet<String> = orig.iter().map(|e| e.id.clone()).collect();
        let v = expand(self.view(), 200.0 / self.cam.z);
        let cands: Vec<BBox> = self.board.all().iter().filter(|e| !ids.contains(&e.id) && !e.hidden && intersects(&aabb(e), &v)).take(400).map(|e| frame_box(e)).collect();
        let bbox = union(orig.iter().map(frame_box)).unwrap_or_default();
        self.gesture = Some(Gesture::Move { start: p, orig, bbox, cands, moved: false, s0: s, edit: None });
    }

    pub(crate) fn snap_move(&mut self, b: BBox, cands: &[BBox]) -> (f64, f64) {
        let tol = 6.0 / self.cam.z;
        let xs = |r: &BBox| [r.x, r.x + r.w / 2.0, r.right()];
        let ys = |r: &BBox| [r.y, r.y + r.h / 2.0, r.bottom()];
        let (mut bx, mut by): ((f64, f64, Option<BBox>), (f64, f64, Option<BBox>)) = ((tol, 0.0, None), (tol, 0.0, None));
        for c in cands {
            for a in xs(&b) {
                for t in xs(c) {
                    if (t - a).abs() < bx.0.abs() {
                        bx = (t - a, t, Some(*c));
                    }
                }
            }
            for a in ys(&b) {
                for t in ys(c) {
                    if (t - a).abs() < by.0.abs() {
                        by = (t - a, t, Some(*c));
                    }
                }
            }
        }
        self.guides.clear();
        let (mut dx, mut dy) = (0.0, 0.0);
        if let Some(c) = bx.2 {
            dx = bx.0;
            self.guides.push([bx.1, b.y.min(c.y), bx.1, b.bottom().max(c.bottom())]);
        }
        if let Some(c) = by.2 {
            dy = by.0;
            self.guides.push([(b.x + dx).min(c.x), by.1, (b.x + dx + b.w).max(c.right()), by.1]);
        }
        (dx, dy)
    }

    /* ---------- creating ---------- */

    pub(crate) fn start_create(&mut self, tool: Tool, p: Pt) {
        let style_stroke = self.prefs.shape_style.stroke.clone();
        let style_width = self.world_size(self.prefs.shape_style.stroke_width);
        let el = match tool {
            Tool::Section => {
                let n = self.board.all().iter().filter(|e| e.is_section()).count() + 1;
                let mut el = self.base(El::new(Kind::Section { fill: SECTION_COLORS[0].into() }));
                el.name = Some(format!("Sezione {n}"));
                el.z = self.board.bottom_z();
                (el.x, el.y) = (p.x, p.y);
                el
            }
            Tool::Shape => {
                let round = self.prefs.last_shape == ShapeTool::RoundRect;
                let kind = match self.prefs.last_shape {
                    ShapeTool::Kind(k) => k,
                    ShapeTool::RoundRect => ShapeKind::Rect,
                };
                let shape = Shape { shape: kind, fill: self.prefs.shape_style.fill.clone(), stroke: style_stroke, stroke_width: style_width, radius: if round { self.world_size(16.0) } else { 0.0 }, ..Default::default() };
                let mut el = self.base(El::new(Kind::Shape(shape)));
                (el.x, el.y) = (p.x, p.y);
                el
            }
            Tool::Tape => {
                let (color, width) = (self.prefs.tape.color.clone(), self.world_size(self.prefs.tape.size));
                self.line_element(p, p, |_, l| {
                    l.stroke = color;
                    l.stroke_width = width;
                    l.tape = true;
                })
            }
            _ => {
                // A connector starting on a shape, sticky, text… is attached to it.
                let from = self.hit_element(p, 6.0).filter(|e| connectable(Some(e))).map(|e| e.id.clone());
                let (route, head) = (self.prefs.route, self.prefs.arrow_head);
                self.line_element(p, p, |_, l| {
                    l.stroke = style_stroke;
                    l.stroke_width = style_width;
                    l.arrow_end = tool == Tool::Arrow;
                    l.head_end = head;
                    l.from = from;
                    l.route = route;
                })
            }
        };
        self.gesture = Some(Gesture::Create { el, start: p, moved: false, spawn: None });
    }

    /// Connector being drawn or re-attached: the element under the pointer it would join (outlined).
    fn connect_target(&mut self, p: Pt, not: Option<&str>) -> Option<Arc<El>> {
        let t = self.hit_element(p, 8.0).filter(|e| connectable(Some(e)) && Some(e.id.as_str()) != not);
        self.hover = t.as_ref().map(|e| e.id.clone());
        t
    }

    pub(crate) fn update_create(&mut self, p: Pt, shift: bool, alt: bool) {
        let Some(Gesture::Create { el, start, moved, .. }) = &mut self.gesture else { return };
        *moved = true;
        let (start, mut el) = (*start, el.clone());
        if let Some(l) = el.line() {
            let mut end = p;
            if shift {
                let a = snap_angle((p.y - start.y).atan2(p.x - start.x), std::f64::consts::FRAC_PI_4);
                let len = (p.x - start.x).hypot(p.y - start.y);
                end = pt(start.x + a.cos() * len, start.y + a.sin() * len);
            }
            if l.tape {
                el = set_line_ends(&el, start, end);
            } else {
                let from = l.from.clone();
                let target = self.connect_target(p, from.as_deref());
                let mut line = unbind(&set_line_ends(&el, start, end), false, true);
                if let Some(t) = target {
                    line.line_mut().unwrap().to = Some(t.id.clone());
                }
                el = route_connector(&line, |id| self.board.get(id).map(|e| &**e)).unwrap_or(line);
            }
        } else {
            let (mut dx, mut dy) = (p.x - start.x, p.y - start.y);
            if shift {
                let m = dx.abs().max(dy.abs());
                dx = if dx == 0.0 { m } else { m * dx.signum() };
                dy = if dy == 0.0 { m } else { m * dy.signum() };
            }
            let b = if alt { normalize_box(start.x - dx, start.y - dy, start.x + dx, start.y + dy) } else { normalize_box(start.x, start.y, start.x + dx, start.y + dy) };
            (el.x, el.y, el.w, el.h) = (b.x, b.y, b.w, b.h);
        }
        if let Some(Gesture::Create { el: e, .. }) = &mut self.gesture {
            *e = el;
        }
    }

    fn finish_create(&mut self, el: El, start: Pt, moved: bool, spawn: Option<El>) {
        let z = self.cam.z;
        self.hover = None;
        if let Some(src) = spawn {
            return self.finish_spawn(el, src);
        }
        let mut el = el;
        if !moved || (el.w * z < 4.0 && el.h * z < 4.0) {
            if el.is_line() {
                el = unbind(&set_line_ends(&el, pt(start.x - 80.0 / z, start.y), pt(start.x + 80.0 / z, start.y)), true, true);
            } else if el.is_section() {
                (el.x, el.y, el.w, el.h) = (start.x - 320.0 / z, start.y - 200.0 / z, 640.0 / z, 400.0 / z);
            } else {
                (el.x, el.y, el.w, el.h) = (start.x - 60.0 / z, start.y - 60.0 / z, 120.0 / z, 120.0 / z);
            }
        }
        self.board.stop_capturing();
        let (id, section) = (el.id.clone(), el.is_section());
        self.board.put([el]);
        self.selection = vec![id.clone()];
        self.set_tool(Tool::Select);
        if section && !moved {
            self.editing = Some(id);
        }
    }

    /// Copy of a shape or sticky for the "+" handles: same look, no text, on top.
    fn sibling(&mut self, el: &El, cx: f64, cy: f64) -> El {
        let mut c = el.clone();
        (c.id, c.x, c.y, c.z, c.locked) = (uid(), cx - el.w / 2.0, cy - el.h / 2.0, self.board.top_z(), false);
        c.erase.clear();
        match &mut c.kind {
            Kind::Sticky(s) => {
                s.text.clear();
                s.author = self.name.clone();
            }
            Kind::Shape(s) => s.text = None,
            _ => {}
        }
        c
    }

    /// Released after dragging from a "+": joined to what is under the pointer, or to a new copy there.
    fn finish_spawn(&mut self, line: El, src: El) {
        let mut line = line;
        let mut add = Vec::new();
        if line.line().is_some_and(|l| l.to.is_none()) {
            let end = ends_of(&line).1;
            let copy = self.sibling(&src, end.x, end.y);
            line.line_mut().unwrap().to = Some(copy.id.clone());
            line = route_connector(&line, |id| if id == copy.id { Some(&copy) } else { self.board.get(id).map(|e| &**e) }).unwrap_or(line);
            add.push(copy);
        }
        self.board.stop_capturing();
        let first = add.first().map(|e| (e.id.clone(), matches!(e.kind, Kind::Sticky(_))));
        let line_id = line.id.clone();
        add.push(line);
        self.board.put(add);
        self.selection = vec![first.as_ref().map_or(line_id, |f| f.0.clone())];
        self.set_tool(Tool::Select);
        if let Some((id, true)) = first {
            self.editing = Some(id);
        }
    }

    /// Click on a "+": a connected copy beside the element, on that side. Clicked again, the
    /// next copy goes beside the first instead of on it: branches, as for the outcomes of a
    /// decision. The copy is ready to be written in.
    pub fn add_beside(&mut self, el: &El, side: Handle) {
        let z = self.cam.z;
        // Notes, as in FigJam: a new note right beside, no arrow (unless chosen in the settings);
        // shapes are diagrams: further away, joined by an arrow.
        let arrow = el.sticky().is_none() || self.prefs.sticky_arrows;
        let gap = if arrow { (80.0 / z).max(el.w.min(el.h) * 0.5) } else { el.w.min(el.h) * 0.1 };
        let c = center(el);
        let (dx, dy) = match side {
            Handle::AddN => (0.0, -1.0),
            Handle::AddE => (1.0, 0.0),
            Handle::AddS => (0.0, 1.0),
            _ => (-1.0, 0.0),
        };
        let (bx, by) = (c.x + dx * (el.w + gap), c.y + dy * (el.h + gap));
        // Across the direction of the branch: fan out until there is room.
        let (px, py) = (dy.abs(), dx.abs());
        let step = if px > 0.0 { el.w + gap * 0.5 } else { el.h + gap * 0.5 };
        let others: Vec<BBox> = self.board.all().iter().filter(|e| e.id != el.id && !e.is_line() && !e.is_section() && !e.hidden).map(|e| expand(frame_box(e), gap * 0.25)).collect();
        let free = |x: f64, y: f64| {
            let b = BBox { x: x - el.w / 2.0, y: y - el.h / 2.0, w: el.w, h: el.h };
            !others.iter().any(|o| intersects(o, &b))
        };
        let (mut x, mut y) = (bx, by);
        for k in [0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0] {
            let (tx, ty) = (bx + px * step * k, by + py * step * k);
            if free(tx, ty) {
                (x, y) = (tx, ty);
                break;
            }
        }
        let copy = self.sibling(el, x, y);
        self.board.stop_capturing();
        let id = copy.id.clone();
        if arrow {
            let mut base = self.connector(c, Some(el.id.clone()), true);
            base.line_mut().unwrap().to = Some(copy.id.clone());
            let line = route_connector(&base, |id| if id == copy.id { Some(&copy) } else { self.board.get(id).map(|e| &**e) }).unwrap_or(base);
            self.board.put([copy, line]);
        } else {
            self.board.put([copy]);
        }
        self.set_tool_keep(Tool::Select);
        self.selection = vec![id.clone()];
        self.editing = Some(id.clone());
        self.editing_cell = None;
        self.reveal(&[id]);
    }

    /// The element that points to this one (the parent in a diagram), if any.
    pub fn parent_of(&mut self, id: &str) -> Option<Arc<El>> {
        let from = self.board.all().iter().find_map(|e| e.line().filter(|l| l.to.as_deref() == Some(id)).and_then(|l| l.from.clone()))?;
        self.board.get(&from).cloned()
    }

    /// Tab in a diagram: a new node after this one (to the right, or the way the diagram runs);
    /// Shift+Tab: one more beside it, from the same parent.
    pub fn grow_diagram(&mut self, id: &str, sibling: bool) -> bool {
        let Some(el) = self.board.get(id).cloned().filter(|e| Editor::can_branch(e)) else { return false };
        self.stop_editing();
        if sibling && let Some(parent) = self.parent_of(id) {
            let side = Editor::side_towards(&parent, &el);
            self.add_beside(&parent, side);
        } else if sibling {
            self.add_beside(&el, Handle::AddS);
        } else {
            let side = self.parent_of(id).map_or(Handle::AddE, |p| Editor::side_towards(&p, &el));
            self.add_beside(&el, side);
        }
        true
    }

    /// Which side of `from` faces `to`.
    fn side_towards(from: &El, to: &El) -> Handle {
        let (a, b) = (center(from), center(to));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        if dx.abs() >= dy.abs() {
            if dx >= 0.0 { Handle::AddE } else { Handle::AddW }
        } else if dy >= 0.0 {
            Handle::AddS
        } else {
            Handle::AddN
        }
    }

    /// "Riordina": the diagram that grows from `root` (following its arrows) laid out as a
    /// tidy tree, each level in a column (or a row, when the diagram runs downwards), children
    /// centred on their parent. Arrows follow.
    pub fn tidy(&mut self, root: &str) {
        let all = self.board.all().to_vec();
        let get = |id: &str| all.iter().find(|e| e.id == id).cloned();
        let Some(r) = get(root) else { return };
        // Arrows out of each element, in the order their ends sit (so the layout keeps it).
        let mut out: HashMap<String, Vec<Arc<El>>> = HashMap::new();
        for l in all.iter().filter_map(|e| e.line()) {
            if let (Some(f), Some(t)) = (&l.from, &l.to)
                && let Some(to) = get(t).filter(|e| !e.is_line() && editable(e))
            {
                out.entry(f.clone()).or_default().push(to);
            }
        }
        // Which way it runs: where the root's children mostly are.
        let rc = center(&r);
        let kids = out.get(root).cloned().unwrap_or_default();
        let (sx, sy) = kids.iter().fold((0.0, 0.0), |(x, y), k| {
            let c = center(k);
            (x + (c.x - rc.x).abs(), y + (c.y - rc.y).abs())
        });
        let down = sy > sx;
        for v in out.values_mut() {
            v.sort_by(|a, b| if down { center(a).x.total_cmp(&center(b).x) } else { center(a).y.total_cmp(&center(b).y) });
        }
        // The tree: each element once, under the first parent that reaches it.
        let mut tree: HashMap<String, Vec<Arc<El>>> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::from([root.to_string()]);
        let mut queue = vec![r.clone()];
        while let Some(n) = queue.pop() {
            for k in out.get(&n.id).cloned().unwrap_or_default() {
                if seen.insert(k.id.clone()) {
                    tree.entry(n.id.clone()).or_default().push(k.clone());
                    queue.insert(0, k);
                }
            }
        }
        if tree.is_empty() {
            return;
        }
        // Sizes along the levels (main) and across them (cross).
        let main = |e: &El| if down { e.h } else { e.w };
        let cross = |e: &El| if down { e.w } else { e.h };
        let gap_main = 90.0 / self.cam.z.max(0.25);
        let gap_cross = 36.0 / self.cam.z.max(0.25);
        fn span(id: &str, tree: &HashMap<String, Vec<Arc<El>>>, cross: &dyn Fn(&El) -> f64, gap: f64, own: f64) -> f64 {
            match tree.get(id) {
                Some(kids) => {
                    let sum: f64 = kids.iter().map(|k| span(&k.id, tree, cross, gap, cross(k))).sum::<f64>() + gap * (kids.len() - 1) as f64;
                    sum.max(own)
                }
                None => own,
            }
        }
        // Each level starts after the widest element of the level before.
        let mut depth_of: HashMap<String, usize> = HashMap::from([(root.to_string(), 0)]);
        let mut levels: Vec<f64> = vec![main(&r)];
        let mut order = vec![r.clone()];
        let mut i = 0;
        while i < order.len() {
            let n = order[i].clone();
            let d = depth_of[&n.id];
            for k in tree.get(&n.id).cloned().unwrap_or_default() {
                depth_of.insert(k.id.clone(), d + 1);
                if levels.len() <= d + 1 {
                    levels.push(0.0);
                }
                levels[d + 1] = levels[d + 1].max(main(&k));
                order.push(k);
            }
            i += 1;
        }
        let mut start = vec![0.0; levels.len()];
        let origin = if down { r.y } else { r.x };
        start[0] = origin;
        for d in 1..levels.len() {
            start[d] = start[d - 1] + levels[d - 1] + gap_main;
        }
        let mut moved: Vec<El> = Vec::new();
        let place = |id: &str, mid: f64, stack: &mut Vec<(String, f64)>| stack.push((id.to_string(), mid));
        let mut stack: Vec<(String, f64)> = Vec::new();
        place(root, if down { rc.x } else { rc.y }, &mut stack);
        while let Some((id, mid)) = stack.pop() {
            let Some(e) = get(&id) else { continue };
            let d = depth_of[&id];
            let mut next = (*e).clone();
            if id != root {
                // Centred in its level, at its place across.
                let m0 = start[d] + (levels[d] - main(&e)) / 2.0;
                if down {
                    (next.x, next.y) = (mid - e.w / 2.0, m0);
                } else {
                    (next.x, next.y) = (m0, mid - e.h / 2.0);
                }
                moved.push(next);
            }
            if let Some(kids) = tree.get(&id) {
                let total: f64 = kids.iter().map(|k| span(&k.id, &tree, &cross, gap_cross, cross(k))).sum::<f64>() + gap_cross * (kids.len() - 1) as f64;
                let mut at = mid - total / 2.0;
                for k in kids {
                    let s = span(&k.id, &tree, &cross, gap_cross, cross(k));
                    place(&k.id, at + s / 2.0, &mut stack);
                    at += s + gap_cross;
                }
            }
        }
        if !moved.is_empty() {
            self.board.stop_capturing();
            self.board.put(moved);
        }
    }

    pub(crate) fn start_text(&mut self, p: Pt) {
        let t = self.prefs.text.clone();
        let font_size = self.world_size(t.font_size);
        let mut el = self.base(El::new(Kind::Text(Text { text: String::new(), color: t.color, font_size, font: t.font, align: Align::Left, bold: false, italic: false, fixed_width: false, strike: false })));
        (el.x, el.y, el.w, el.h) = (p.x, p.y - font_size * LINE_HEIGHT / 2.0, font_size / 6.0, font_size * LINE_HEIGHT);
        self.board.stop_capturing();
        let id = el.id.clone();
        self.board.put([el]);
        self.set_tool(Tool::Select);
        self.selection = vec![id.clone()];
        self.editing = Some(id);
    }

    /// A shape in the shape style, centred on `p` at its usual size, selected.
    pub fn place_shape(&mut self, tool: ShapeTool, p: Pt) {
        let s = self.world_size(120.0);
        let (kind, radius) = match tool {
            ShapeTool::Kind(k) => (k, 0.0),
            ShapeTool::RoundRect => (ShapeKind::Rect, self.world_size(16.0)),
        };
        let shape = Shape { shape: kind, fill: self.prefs.shape_style.fill.clone(), stroke: self.prefs.shape_style.stroke.clone(), stroke_width: self.world_size(self.prefs.shape_style.stroke_width), radius, ..Default::default() };
        let mut el = self.base(El::new(Kind::Shape(shape)));
        (el.x, el.y, el.w, el.h) = (p.x - s / 2.0, p.y - s / 2.0, s, s);
        self.board.stop_capturing();
        let id = el.id.clone();
        self.board.put([el]);
        self.board.stop_capturing();
        self.selection = vec![id];
        self.set_tool(Tool::Select);
    }

    pub(crate) fn start_sticky(&mut self, p: Pt) {
        let s = self.world_size(220.0);
        let mut el = self.base(El::new(Kind::Sticky(Sticky { color: self.prefs.sticky_color.clone(), author: self.name.clone(), ..Default::default() })));
        (el.x, el.y, el.w, el.h) = (p.x - s / 2.0, p.y - s / 2.0, s, s);
        self.board.stop_capturing();
        let id = el.id.clone();
        self.board.put([el]);
        self.set_tool(Tool::Select);
        self.selection = vec![id.clone()];
        self.editing = Some(id);
    }

    pub(crate) fn place_stamp(&mut self, p: Pt) {
        let s = self.world_size(64.0);
        let mut el = self.base(El::new(Kind::Stamp { emoji: self.prefs.stamp.clone() }));
        (el.x, el.y, el.w, el.h) = (p.x - s / 2.0, p.y - s / 2.0, s, s);
        self.board.stop_capturing();
        self.board.put([el]);
    }

    /* ---------- resizing and rotating ---------- */

    pub(crate) fn resize_to(&mut self, p: Pt, shift: bool, alt: bool) {
        let Some(Gesture::Resize { handle: h, start, orig, frame: f, rotation }) = &self.gesture else { return };
        let (h, start, f, rot) = (*h, *start, *f, *rotation);
        let els = orig.clone();
        let c = f.center();
        let lp = rotate(p.x, p.y, c.x, c.y, -rot);
        let l0 = rotate(start.x, start.y, c.x, c.y, -rot);
        let (dx, dy) = (lp.x - l0.x, lp.y - l0.y);
        let (mut left, mut right, mut top, mut bottom) = (f.x, f.right(), f.y, f.bottom());
        if h.w() {
            left += dx;
            if alt {
                right -= dx;
            }
        }
        if h.e() {
            right += dx;
            if alt {
                left -= dx;
            }
        }
        if h.n() {
            top += dy;
            if alt {
                bottom -= dy;
            }
        }
        if h.s() {
            bottom += dy;
            if alt {
                top -= dy;
            }
        }
        // Pictures keep their shape; text and code too from a corner; a widget from anywhere but
        // its sides (it grows whole, letters and buttons too).
        let lock = shift
            || els.iter().any(|e| matches!(e.kind, Kind::Image { .. } | Kind::Stamp { .. }))
            || (els.len() == 1 && (els[0].text().is_some() || els[0].code().is_some()) && h.corner())
            || (els.len() == 1 && els[0].widget().is_some() && h != Handle::E && h != Handle::W);
        let min = 1.0 / self.cam.z;
        let mut w = (right - left).max(min);
        let mut hh = (bottom - top).max(min);
        if right - left < min {
            if h.w() { left = right - min } else { right = left + min }
        }
        if bottom - top < min {
            if h.n() { top = bottom - min } else { bottom = top + min }
        }
        let _ = (right, bottom);
        if lock && f.w > 0.0 && f.h > 0.0 {
            let ratio = f.w / f.h;
            if h.corner() {
                if w / hh > ratio { hh = w / ratio } else { w = hh * ratio }
            } else if h == Handle::N || h == Handle::S {
                w = hh * ratio;
            } else {
                hh = w / ratio;
            }
            // Re-anchor on the side opposite the dragged handle.
            if alt {
                left = c.x - w / 2.0;
                top = c.y - hh / 2.0;
            } else {
                left = if h.w() { f.right() - w } else if h.e() { f.x } else { c.x - w / 2.0 };
                top = if h.n() { f.bottom() - hh } else if h.s() { f.y } else { c.y - hh / 2.0 };
            }
        }
        // New box in the unrotated frame; its centre back to board space.
        let nc = rotate(left + w / 2.0, top + hh / 2.0, c.x, c.y, rot);
        let to = BBox { x: nc.x - w / 2.0, y: nc.y - hh / 2.0, w, h: hh };
        self.preview.clear();
        for el in &els {
            let next = if els.len() == 1 {
                let base = El { rotation: 0.0, ..el.clone() };
                let mut next = El { rotation: el.rotation, ..scale_element(&base, f, to) };
                if el.text().is_some() {
                    next = resize_text(el, next, h);
                }
                if el.code().is_some() {
                    next = resize_code(el, next, h, to);
                }
                if el.widget().is_some() && el.rotation == 0.0 {
                    // The side or corner across from the one dragged stays put.
                    next.x = if h.w() { to.right() - next.w } else if h.e() { to.x } else { el.x + (el.w - next.w) / 2.0 };
                    next.y = if h.n() { to.bottom() - next.h } else { to.y };
                }
                next
            } else {
                let mut next = scale_element(el, f, to);
                if let (Kind::Text(t), Kind::Text(o)) = (&mut next.kind, &el.kind) {
                    t.font_size = (o.font_size * ((to.w / f.w) * (to.h / f.h)).sqrt()).max(MIN_SIZE);
                }
                next
            };
            self.preview.insert(el.id.clone(), Arc::new(next));
        }
        self.follow_connectors(els.iter().map(|e| e.id.clone()).collect());
        self.schedule_flush();
    }

    /// Connectors attached to elements being changed live follow them on screen right away.
    pub(crate) fn follow_connectors(&mut self, changed: HashSet<String>) {
        let all = self.board.all().to_vec();
        for el in all {
            let Some(l) = el.line() else { continue };
            let hit = |o: &Option<String>| o.as_ref().is_some_and(|i| changed.contains(i));
            if changed.contains(&el.id) || !(hit(&l.from) || hit(&l.to)) {
                continue;
            }
            let next = route_connector(&el, |id| self.preview.get(id).or_else(|| self.board.get(id)).map(|e| &**e));
            if let Some(next) = next {
                self.preview.insert(el.id.clone(), Arc::new(next));
            }
        }
    }

    pub(crate) fn rotate_to(&mut self, p: Pt, shift: bool) {
        let Some(Gesture::Rotate { c, a0, orig }) = &self.gesture else { return };
        let (c, a0, els) = (*c, *a0, orig.clone());
        let mut delta = (p.y - c.y).atan2(p.x - c.x) - a0;
        if shift || els.len() == 1 {
            let base = if els.len() == 1 { els[0].rotation } else { 0.0 };
            let step = if shift { std::f64::consts::PI / 12.0 } else { std::f64::consts::PI / 180.0 };
            let mut target = snap_angle(base + delta, step);
            // Gently stick to right angles.
            let right = snap_angle(target, std::f64::consts::FRAC_PI_2);
            if !shift && (right - target).abs() < 2.0_f64.to_radians() {
                target = right;
            }
            delta = target - base;
        }
        self.preview.clear();
        for el in &els {
            let next = if let Some(l) = el.line() {
                let (a, b) = line_world_ends(el, l);
                set_line_ends(el, rotate(a.x, a.y, c.x, c.y, delta), rotate(b.x, b.y, c.x, c.y, delta))
            } else {
                let ec = center(el);
                let nc = rotate(ec.x, ec.y, c.x, c.y, delta);
                El { x: nc.x - el.w / 2.0, y: nc.y - el.h / 2.0, rotation: el.rotation + delta, ..el.clone() }
            };
            self.preview.insert(el.id.clone(), Arc::new(next));
        }
        self.follow_connectors(els.iter().map(|e| e.id.clone()).collect());
        self.schedule_flush();
    }

    pub(crate) fn finish_gesture(&mut self) {
        self.flush_preview();
        self.preview.clear();
        self.guides.clear();
        self.hover = None;
    }

    /// Ends whatever gesture is in progress (Escape, a second finger…).
    pub fn cancel_gesture(&mut self) {
        if matches!(self.gesture, Some(Gesture::Draw(_))) {
            self.presence.end_live();
        }
        self.gesture = None;
        self.preview.clear();
        self.guides.clear();
    }

    /* ---------- menus ---------- */

    /// Right click acts on what is under the pointer, or on the selection if it's inside it.
    pub fn select_for_menu(&mut self, s: Pos2) {
        let p = self.to_world(s);
        let hit = self.hit_element(p, 6.0);
        match hit {
            Some(h) if !self.selection.contains(&h.id) => self.selection = vec![h.id.clone()],
            None if !self.inside_selection(p) => self.selection.clear(),
            _ => {}
        }
    }

    pub(crate) fn open_menu(&mut self, s: Pos2) {
        self.gesture = None;
        self.finish_gesture();
        self.select_for_menu(s);
        self.requests.push(Request::Menu(pos2(s.x + self.origin.x, s.y + self.origin.y)));
    }

    /* ---------- commands ---------- */

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.editing = None;
        if tool != Tool::Select && tool != Tool::Lasso {
            self.selection.clear();
        }
        self.eraser_at = None;
        self.hover_handle = None;
        self.hover = None;
        self.hover_adds = None;
    }

    fn prune(&mut self) {
        if self.selection.iter().any(|id| self.board.get(id).is_none()) {
            let keep: Vec<String> = self.selection.iter().filter(|id| self.board.get(id).is_some()).cloned().collect();
            self.selection = keep;
        }
        if self.editing.as_ref().is_some_and(|id| self.board.get(id).is_none()) {
            self.editing = None;
        }
        if self.comment.as_ref().is_some_and(|id| self.board.get(id).is_none()) {
            self.comment = None;
        }
    }

    pub fn zoom_by(&mut self, f: f64) {
        // Successive presses build on the zoom being flown to, not on the one half-way there.
        let z = self.fly.map_or(self.cam.z, |(_, b, _)| b.z);
        self.zoom_to(z * f);
    }

    pub fn zoom_to(&mut self, z: f64) {
        let (cx, cy) = self.safe_center();
        let base = self.fly.map_or(self.cam, |(_, b, _)| b);
        let nz = z.clamp(MIN_ZOOM, MAX_ZOOM);
        let w = base.to_world(cx, cy);
        self.fly_to(Camera { x: cx - w.x * nz, y: cy - w.y * nz, z: nz });
    }

    pub fn content_box(&mut self) -> Option<BBox> {
        union(self.board.all().iter().filter(|e| !e.hidden).map(|e| aabb(e)))
    }

    /// Shows the whole board at once (on opening it).
    pub fn fit_now(&mut self) {
        let b = self.content_box();
        let c = self.fitted(b, 2.0);
        self.set_cam(c);
    }

    pub fn fit(&mut self) {
        let b = self.content_box();
        self.fit_box(b, 2.0);
    }

    pub fn fit_selection(&mut self) {
        let b = union(self.selected().iter().map(|e| aabb(e)));
        if b.is_some() {
            self.fit_box(b, 4.0);
        }
    }

    pub fn undo(&mut self) {
        if !self.read_only {
            self.board.undo();
            self.prune();
        }
    }

    pub fn redo(&mut self) {
        if !self.read_only {
            self.board.redo();
            self.prune();
        }
    }

    pub fn remove(&mut self) {
        if self.read_only {
            return;
        }
        let ids: Vec<String> = self.selected().iter().filter(|e| !e.locked).map(|e| e.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        self.board.stop_capturing();
        self.board.remove(ids);
        self.selection.clear();
    }

    /// Copies the selection (with what its sections hold) for pasting; returns the clipboard text.
    pub fn copy(&mut self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        let sel = self.selected();
        let els = self.with_content(&sel);
        self.clipboard = els.iter().map(|e| (**e).clone()).collect();
        self.paste_count = 0;
        serde_json::to_string(&self.clipboard).ok().map(|j| format!("{CLIP_PREFIX}{j}"))
    }

    pub fn cut(&mut self) -> Option<String> {
        if self.read_only {
            return None;
        }
        let text = self.copy();
        self.remove();
        text
    }

    pub fn duplicate(&mut self) {
        self.copy();
        let els = self.clipboard.clone();
        self.paste_elements(els);
    }

    pub fn paste(&mut self) {
        let els = self.clipboard.clone();
        self.paste_elements(els);
    }

    /// Paste in place, nudged; if the originals are off screen, in the middle of the view.
    pub fn paste_elements(&mut self, els: Vec<El>) {
        let els: Vec<Arc<El>> = els.into_iter().filter(|e| e.is_valid()).map(Arc::new).collect();
        if els.is_empty() || self.read_only {
            return;
        }
        self.paste_count += 1;
        let b = union(els.iter().map(|e| frame_box(e))).unwrap();
        let v = self.view();
        let (mut dx, mut dy) = (16.0 * self.paste_count as f64 / self.cam.z, 16.0 * self.paste_count as f64 / self.cam.z);
        if !intersects(&b, &v) {
            dx = v.x + v.w / 2.0 - (b.x + b.w / 2.0);
            dy = v.y + v.h / 2.0 - (b.y + b.h / 2.0);
        }
        let (clones, _) = self.clone_all(&els, dx, dy);
        self.board.stop_capturing();
        self.selection = clones.iter().map(|e| e.id.clone()).collect();
        self.board.put(clones);
        self.set_tool_keep(Tool::Select);
    }

    /// Plain text pasted on the board becomes a text box in the middle of the view.
    pub fn paste_text(&mut self, text: &str) {
        if self.read_only || text.trim().is_empty() {
            return;
        }
        if let Some(json) = text.strip_prefix(CLIP_PREFIX) {
            if let Ok(els) = serde_json::from_str::<Vec<El>>(json) {
                self.paste_elements(els);
            }
            return;
        }
        let text: String = text.chars().take(20_000).collect();
        let c = self.viewport_center();
        let t = self.prefs.text.clone();
        let font_size = self.world_size(t.font_size);
        let long = text.chars().count() > 80;
        let f = text::font(t.font, false, false).face;
        let layout = text::layout_text(&text, f, font_size, long.then_some(640.0 / self.cam.z));
        let mut el = self.base(El::new(Kind::Text(Text { text, color: t.color, font_size, font: t.font, align: Align::Left, bold: false, italic: false, fixed_width: long, strike: false })));
        (el.x, el.y, el.w, el.h) = (c.x - layout.width / 2.0, c.y - layout.height / 2.0, layout.width, layout.height);
        self.board.stop_capturing();
        self.selection = vec![el.id.clone()];
        self.board.put([el]);
        self.set_tool_keep(Tool::Select);
    }

    /// Changes tool without dropping the selection.
    pub(crate) fn set_tool_keep(&mut self, tool: Tool) {
        self.tool = tool;
        self.editing = None;
    }

    /// The colour seen at a board point, as it is drawn: everything there over the board's
    /// background, see-through parts and grains included (a pixel of the board, drawn alone).
    pub fn color_at(&mut self, p: Pt) -> String {
        let meta = self.board.meta();
        let bg = color_or(&meta.background, Color32::from_gray(0xF5));
        let Some(mut pm) = tiny_skia::Pixmap::new(1, 1) else { return hex(bg) };
        let [r, g, b, a] = bg.to_srgba_unmultiplied();
        pm.fill(tiny_skia::Color::from_rgba8(r, g, b, a));
        let env = crate::prims::Env { dark_board: is_dark(&meta.background), ..Default::default() };
        let t = tiny_skia::Transform::from_translate(0.5 - p.x as f32, 0.5 - p.y as f32);
        let images = &self.painter.images;
        let fetch = |k: &crate::prims::ImageKey| images.pixmap(k);
        let at = BBox { x: p.x - 0.5, y: p.y - 0.5, w: 1.0, h: 1.0 };
        for el in self.board.paint_order().iter().filter(|e| !e.hidden && intersects(&aabb(e), &at)) {
            crate::raster::draw_element(&mut pm, el, &env, t, &fetch);
        }
        let c = pm.pixel(0, 0).map(|p| p.demultiply()).map_or(bg, |c| Color32::from_rgb(c.red(), c.green(), c.blue()));
        hex(c)
    }

    /// Takes the colour at a board point for the tool in hand (the highlighter, or the pen).
    pub(crate) fn pick_color(&mut self, p: Pt) {
        let c = self.color_at(p);
        if self.tool == Tool::Highlighter {
            self.prefs.highlighter.color = c.clone();
        } else if let Some(pen) = self.prefs.pens.get_mut(self.pen) {
            pen.color = c.clone();
        }
        self.eyedropper = false;
        self.requests.push(Request::Notice(format!("Colore preso: {c}")));
    }

    /// Points of the stroke being drawn (0 when none is), for the frame trace.
    pub fn live_points(&self) -> usize {
        match &self.gesture {
            Some(Gesture::Draw(d)) => d.pts.len() / 3,
            _ => 0,
        }
    }

    pub fn order(&mut self, dir: Order) {
        if self.read_only || self.selection.is_empty() {
            return;
        }
        let sel: HashSet<String> = self.selection.iter().cloned().collect();
        let all = self.board.all().to_vec();
        let mut next = Vec::new();
        match dir {
            Order::Front | Order::Back => {
                let mut z = if dir == Order::Front { self.board.top_z() } else { self.board.bottom_z() - sel.len() as f64 };
                for el in all.iter().filter(|e| sel.contains(&e.id)) {
                    next.push(El { z, ..(**el).clone() });
                    z += 1.0;
                }
            }
            _ => {
                // Swap with the nearest non-selected neighbour in stacking order.
                let mut order = all.clone();
                let n = order.len();
                for k in 0..n {
                    let i = if dir == Order::Forward { n - 1 - k } else { k };
                    let j = if dir == Order::Forward { i + 1 } else { i.wrapping_sub(1) };
                    if j < n && sel.contains(&order[i].id) && !sel.contains(&order[j].id) {
                        order.swap(i, j);
                    }
                }
                for (i, el) in order.iter().enumerate() {
                    if el.z != i as f64 {
                        next.push(El { z: i as f64, ..(**el).clone() });
                    }
                }
            }
        }
        self.board.stop_capturing();
        self.board.put(next);
    }

    pub fn toggle_lock(&mut self) {
        if self.read_only {
            return;
        }
        let sel = self.selected();
        let lock = !sel.iter().all(|e| e.locked);
        let ids: Vec<String> = sel.iter().map(|e| e.id.clone()).collect();
        self.board.update(&ids, |e| e.locked = lock);
        if lock {
            self.selection.clear();
        }
    }

    pub fn toggle_hide(&mut self) {
        if self.read_only {
            return;
        }
        let sel = self.selected();
        let hide = !sel.iter().all(|e| e.hidden);
        let ids: Vec<String> = sel.iter().map(|e| e.id.clone()).collect();
        self.board.update(&ids, |e| e.hidden = hide);
        if hide {
            self.selection.clear();
        }
    }

    pub fn unlock_all(&mut self) {
        let ids: Vec<String> = self.board.all().iter().filter(|e| e.locked || e.hidden).map(|e| e.id.clone()).collect();
        self.board.stop_capturing();
        self.board.update(&ids, |e| {
            e.locked = false;
            e.hidden = false;
        });
    }

    pub fn select_all(&mut self) {
        self.selection = self.board.all().iter().filter(|e| editable(e)).map(|e| e.id.clone()).collect();
        self.set_tool_keep(Tool::Select);
    }

    pub fn align(&mut self, kind: AlignKind) {
        let sel: Vec<Arc<El>> = self.selected().into_iter().filter(|e| editable(e)).collect();
        if sel.len() < 2 {
            return;
        }
        let b = union(sel.iter().map(|e| frame_box(e))).unwrap();
        let next = sel
            .iter()
            .map(|el| {
                let f = frame_box(el);
                let (dx, dy) = match kind {
                    AlignKind::Left => (b.x - f.x, 0.0),
                    AlignKind::Right => (b.right() - f.right(), 0.0),
                    AlignKind::HCenter => (b.x + b.w / 2.0 - (f.x + f.w / 2.0), 0.0),
                    AlignKind::Top => (0.0, b.y - f.y),
                    AlignKind::Bottom => (0.0, b.bottom() - f.bottom()),
                    AlignKind::VCenter => (0.0, b.y + b.h / 2.0 - (f.y + f.h / 2.0)),
                };
                El { x: el.x + dx, y: el.y + dy, ..(**el).clone() }
            })
            .collect::<Vec<_>>();
        self.board.stop_capturing();
        self.board.put(next);
    }

    pub fn distribute(&mut self, vertical: bool) {
        let sel: Vec<Arc<El>> = self.selected().into_iter().filter(|e| editable(e)).collect();
        if sel.len() < 3 {
            return;
        }
        let mut items: Vec<(Arc<El>, BBox)> = sel.iter().map(|e| (e.clone(), frame_box(e))).collect();
        let key = |b: &BBox| if vertical { b.y } else { b.x };
        let size = |b: &BBox| if vertical { b.h } else { b.w };
        items.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
        let (first, last) = (items[0].1, items[items.len() - 1].1);
        let span = key(&last) + size(&last) - key(&first);
        let total: f64 = items.iter().map(|i| size(&i.1)).sum();
        let gap = (span - total) / (items.len() - 1) as f64;
        let mut pos = key(&first);
        let mut next = Vec::new();
        for (el, f) in &items {
            let d = pos - key(f);
            next.push(if vertical { El { y: el.y + d, ..(**el).clone() } } else { El { x: el.x + d, ..(**el).clone() } });
            pos += size(f) + gap;
        }
        self.board.stop_capturing();
        self.board.put(next);
    }

    /// Puts new elements on the board, centred on `at` (or the view); `avoid_overlap` places a
    /// template in free space beside what's already there, `focus` brings it into view.
    pub fn insert(&mut self, els: Vec<El>, at: Option<Pt>, avoid_overlap: bool, focus: bool) {
        if els.is_empty() || self.read_only {
            return;
        }
        let b = union(els.iter().map(frame_box)).unwrap();
        let target = at.unwrap_or_else(|| self.viewport_center());
        let (mut dx, mut dy) = (target.x - (b.x + b.w / 2.0), target.y - (b.y + b.h / 2.0));
        if avoid_overlap
            && let Some(content) = self.content_box()
            && intersects(&BBox { x: b.x + dx, y: b.y + dy, ..b }, &content)
        {
            dx = content.right() + 160.0 - b.x;
            dy = content.y - b.y;
        }
        let mut z = self.board.top_z();
        let placed: Vec<El> = els
            .into_iter()
            .map(|mut e| {
                e.x += dx;
                e.y += dy;
                e.z = z;
                z += 1.0;
                e
            })
            .collect();
        self.board.stop_capturing();
        self.selection = placed.iter().map(|e| e.id.clone()).collect();
        let fit = union(placed.iter().map(frame_box));
        self.board.put(placed);
        self.set_tool_keep(Tool::Select);
        if focus {
            self.fit_box(fit, 1.0);
        }
    }

    pub fn viewport_center(&self) -> Pt {
        let (cx, cy) = self.safe_center();
        self.cam.to_world(cx, cy)
    }

    pub fn center_on(&mut self, p: Pt) {
        self.stop_following();
        let z = self.fly.map_or(self.cam.z, |(_, b, _)| b.z);
        let (cx, cy) = self.safe_center();
        self.fly_to(Camera { x: cx - p.x * z, y: cy - p.y * z, z });
    }

    /// Brings elements into view without changing the zoom unless they don't fit.
    pub fn reveal(&mut self, ids: &[String]) {
        let Some(b) = union(ids.iter().filter_map(|id| self.board.get(id)).map(|e| aabb(e))) else { return };
        let v = expand(self.view(), -40.0 / self.cam.z);
        if v.holds(&b) {
            return;
        }
        self.stop_following();
        if b.w <= v.w && b.h <= v.h {
            self.center_on(b.center());
        } else {
            let z = self.cam.z;
            self.fit_box(Some(b), z);
        }
    }

    pub fn group(&mut self) {
        let ids: Vec<String> = self.selection.iter().filter(|id| self.board.get(id).is_some()).cloned().collect();
        if self.read_only || ids.is_empty() {
            return;
        }
        self.board.stop_capturing();
        let id = self.board.create_group(&ids, None);
        self.requests.push(Request::FolderCreated(id));
    }

    pub fn ungroup(&mut self) {
        let ids: Vec<String> = self.selected().iter().filter(|e| e.group_id.is_some()).map(|e| e.id.clone()).collect();
        if self.read_only || ids.is_empty() {
            return;
        }
        self.board.stop_capturing();
        self.board.set_group(&ids, None);
    }

    pub fn follow(&mut self, id: Option<u64>) {
        self.following = id;
        self.apply_follow();
    }

    /// Takes the view of the person being followed.
    pub fn apply_follow(&mut self) {
        let Some(id) = self.following else { return };
        let Some(v) = self.presence.peer(id).map(|s| s["view"].clone()) else { return };
        let (Some(cx), Some(cy), Some(z)) = (v["cx"].as_f64(), v["cy"].as_f64(), v["z"].as_f64()) else { return };
        if !(cx.is_finite() && cy.is_finite() && z.is_finite()) {
            return;
        }
        let z = z.clamp(MIN_ZOOM, MAX_ZOOM);
        let (x, y) = self.safe_center();
        // Their view arrives a few times a second: gliding to it keeps the motion smooth.
        self.fly_to(Camera { x: x - cx * z, y: y - cy * z, z });
    }

    /// Votes: a click puts one of this person's votes on what is under it (Shift or Alt takes it back).
    pub(crate) fn try_vote(&mut self, p: Pt, back: bool) -> bool {
        let Some((_, ended)) = self.board.voting() else { return false };
        if ended || self.read_only || self.tool != Tool::Select {
            return false;
        }
        match self.hit_element(p, 6.0) {
            Some(hit) if votable(&hit) => {
                let voter = self.voter.clone();
                self.board.cast_vote(&voter, &hit.id, if back { -1 } else { 1 });
                true
            }
            _ => false,
        }
    }

    /// Text box being typed into changed: the box follows its text.
    pub fn set_text(&mut self, id: &str, value: String) {
        let Some(el) = self.board.get(id) else { return };
        let mut next = (**el).clone();
        match &mut next.kind {
            Kind::Text(t) => t.text = value,
            Kind::Sticky(s) => s.text = value.chars().take(4000).collect(),
            Kind::Shape(s) => s.text = Some(value.chars().take(4000).collect()),
            Kind::Section { .. } => next.name = Some(value.chars().take(80).collect()),
            Kind::Code(c) => c.code = value.chars().take(100_000).collect(),
            Kind::Line(l) => l.label = Some(value.chars().take(500).collect()).filter(|s: &String| !s.is_empty()),
            Kind::Table(t) => {
                let (r, c) = self.editing_cell.unwrap_or((0, 0));
                let Some(cell) = t.cells.get_mut(r).and_then(|row| row.get_mut(c)) else { return };
                cell.text = value.chars().take(10_000).collect();
            }
            _ => return,
        }
        text::fit_text(&mut next);
        self.board.put([next]);
    }

    /// Starts typing in an element; for a table, in the cell under `p` (or the first one).
    pub fn edit_at(&mut self, el: &El, p: Option<Pt>) {
        self.editing_cell = el.table().map(|t| p.and_then(|p| {
            let l = to_local(el, p.x, p.y);
            t.cell_at(l.x, l.y)
        }).unwrap_or((0, 0)));
        self.editing = Some(el.id.clone());
    }

    /// Moves the text box to the next (or previous) cell of the table being typed in.
    pub fn next_cell(&mut self, back: bool) {
        let (Some(id), Some((r, c))) = (self.editing.clone(), self.editing_cell) else { return };
        let Some(t) = self.board.get(&id).and_then(|e| e.table().cloned()) else { return };
        let (rows, cols) = (t.rows.len(), t.cols.len());
        let i = r * cols + c;
        let n = if back { i.checked_sub(1) } else { Some(i + 1) };
        match n {
            Some(n) if n < rows * cols => self.editing_cell = Some((n / cols, n % cols)),
            // Tab in the last cell adds a row, like FigJam.
            Some(_) if !back => {
                self.board.update(&[id], |el| {
                    if let Kind::Table(t) = &mut el.kind {
                        t.insert_row(rows);
                    }
                    text::fit_text(el);
                });
                self.editing_cell = Some((rows, 0));
            }
            _ => {}
        }
    }

    /// Puts a new element in the middle of the view (or at `p`), selected.
    fn insert_here(&mut self, el: El, p: Option<Pt>, edit: bool) {
        let mut el = self.base(el);
        text::fit_text(&mut el);
        let c = p.unwrap_or_else(|| self.viewport_center());
        (el.x, el.y) = (c.x - el.w / 2.0, c.y - el.h / 2.0);
        self.board.stop_capturing();
        let id = el.id.clone();
        self.board.put([el.clone()]);
        self.board.stop_capturing();
        self.set_tool(Tool::Select);
        self.selection = vec![id];
        if edit {
            self.edit_at(&el, None);
        }
    }

    /// A table (3 × 3 unless chosen otherwise in the tray) sized for the current zoom, its first
    /// row a header.
    pub fn insert_table(&mut self, p: Option<Pt>) {
        let k = self.world_size(1.0);
        let [rows, cols] = self.prefs.table_size;
        let mut t = Table::new(rows.clamp(1, 20) as usize, cols.clamp(1, 12) as usize);
        t.font_size *= k;
        t.cols.iter_mut().for_each(|c| *c *= k);
        self.insert_here(El::new(Kind::Table(t)), p, false);
    }

    pub fn insert_code(&mut self, at: Option<Pt>) {
        let k = self.world_size(1.0);
        let mut el = El::new(Kind::Code(Code { code: String::new(), language: self.prefs.code_language.clone(), light: false, font_size: 14.0 * k, radius: None, fixed_height: false }));
        el.w = 520.0 * k;
        self.insert_here(el, at, true);
    }

    pub fn insert_widget(&mut self, w: Widget, at: Option<Pt>) {
        let mut el = El::new(Kind::Widget(w));
        el.w = WIDGET_W;
        self.insert_here(el, at, false);
        // Its texts are edited in the side panel.
        self.prefs.right_panel = true;
        self.prefs.focus = false;
    }

    /// A click on a widget's button, box or option: does what it says.
    pub(crate) fn try_widget(&mut self, p: Pt) -> bool {
        if self.read_only {
            return false;
        }
        let Some(hit) = self.hit_element(p, 2.0).filter(|e| e.widget().is_some()) else { return false };
        let l = to_local(&hit, p.x, p.y);
        let Some(hot) = crate::widgets::hot_at(&hit, l.x, l.y) else { return false };
        let voter = self.voter.clone();
        self.board.stop_capturing();
        self.board.update(&[hit.id.clone()], |el| {
            if let Kind::Widget(w) = &mut el.kind {
                crate::widgets::press(w, hot, &voter);
            }
            text::fit_text(el);
        });
        self.board.stop_capturing();
        true
    }

    /// Leaves the text being edited; an empty new text box goes away.
    pub fn stop_editing(&mut self) {
        self.editing_cell = None;
        let Some(id) = self.editing.take() else { return };
        self.board.stop_capturing();
        if let Some(el) = self.board.get(&id)
            && el.text().is_some_and(|t| t.text.trim().is_empty())
        {
            self.board.remove([id]);
        }
    }
}

/// The sfumino: a colour for each point added since the last call, picked up under it.
fn smear_points(g: &mut Draw) {
    let Some(smear) = g.smear.as_mut() else { return };
    let n = g.pts.len() / 3;
    while g.colors.len() < n {
        let i = g.colors.len();
        g.colors.push(smear.carry(pt(g.pts[i * 3], g.pts[i * 3 + 1]), g.size));
    }
}

fn push_draw_point(g: &mut Draw, cam: &Camera, s: Pt, pressure: f64, force: bool) {
    let s = match g.snap {
        Some(snap) => snap.apply(s.x, s.y),
        None => s,
    };
    let w = cam.to_world(s.x, s.y);
    let n = g.pts.len();
    if !force && n > 0 && (g.pts[n - 3] - w.x).hypot(g.pts[n - 2] - w.y) < 0.25 / cam.z {
        return;
    }
    g.pts.extend([w.x, w.y, pressure]);
}

/// Converts a recognized shape into clean stroke points (`[x, y, pressure, …]`) for QuickShape live preview.
fn recognized_stroke_pts(rec: &Recognized) -> Vec<f64> {
    let seg = |out: &mut Vec<f64>, a: Pt, b: Pt, steps: usize| {
        for i in 0..=steps {
            let t = i as f64 / steps.max(1) as f64;
            out.extend([a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, -1.0]);
        }
    };
    let closed_poly = |corners: &[Pt]| -> Vec<f64> {
        let mut out = Vec::new();
        for i in 0..corners.len() {
            seg(&mut out, corners[i], corners[(i + 1) % corners.len()], 8);
        }
        out
    };
    match rec {
        Recognized::Line { x1, y1, x2, y2 } | Recognized::Arrow { x1, y1, x2, y2 } => {
            let mut out = Vec::new();
            seg(&mut out, pt(*x1, *y1), pt(*x2, *y2), 20);
            out
        }
        Recognized::Rect { x, y, w, h } => closed_poly(&[pt(*x, *y), pt(*x + *w, *y), pt(*x + *w, *y + *h), pt(*x, *y + *h)]),
        Recognized::Turned { cx, cy, w, h, angle } => {
            let (hw, hh) = (*w / 2.0, *h / 2.0);
            let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(dx, dy)| rotate(*cx + dx, *cy + dy, *cx, *cy, *angle));
            closed_poly(&corners)
        }
        Recognized::Diamond { x, y, w, h } => closed_poly(&[pt(*x + *w / 2.0, *y), pt(*x + *w, *y + *h / 2.0), pt(*x + *w / 2.0, *y + *h), pt(*x, *y + *h / 2.0)]),
        Recognized::Triangle(p) => closed_poly(p),
        Recognized::Polygon(p) => closed_poly(p),
        Recognized::Star { x, y, w, h } => {
            let (cx, cy, rx, ry) = (*x + *w / 2.0, *y + *h / 2.0, *w / 2.0, *h / 2.0);
            let pts: Vec<Pt> = (0..10)
                .map(|i| {
                    let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
                    let r = if i % 2 == 0 { 1.0 } else { 0.4 };
                    pt(cx + rx * r * a.cos(), cy + ry * r * a.sin())
                })
                .collect();
            closed_poly(&pts)
        }
        Recognized::Ellipse { x, y, w, h } => {
            let (cx, cy, rx, ry) = (*x + *w / 2.0, *y + *h / 2.0, *w / 2.0, *h / 2.0);
            (0..=48)
                .flat_map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 48.0;
                    [cx + rx * a.cos(), cy + ry * a.sin(), -1.0]
                })
                .collect()
        }
    }
}

/// Generates mirrored/rotated stroke copies (`[x, y, pressure, …]`) around `center` for the active symmetry mode.
pub(crate) fn symmetry_pts(sym: crate::prefs::Symmetry, center: Pt, pts: &[f64]) -> Vec<Vec<f64>> {
    use crate::prefs::Symmetry;
    let map = |f: &dyn Fn(f64, f64) -> (f64, f64)| -> Vec<f64> {
        pts.chunks_exact(3)
            .flat_map(|p| {
                let (nx, ny) = f(p[0] - center.x, p[1] - center.y);
                [center.x + nx, center.y + ny, p[2]]
            })
            .collect()
    };
    match sym {
        Symmetry::Off => Vec::new(),
        Symmetry::Vertical => vec![map(&|dx, dy| (-dx, dy))],
        Symmetry::Horizontal => vec![map(&|dx, dy| (dx, -dy))],
        Symmetry::Quad => vec![map(&|dx, dy| (-dx, dy)), map(&|dx, dy| (dx, -dy)), map(&|dx, dy| (-dx, -dy))],
        Symmetry::Radial => (1..8)
            .map(|k| {
                let (s, c) = (k as f64 * std::f64::consts::FRAC_PI_4).sin_cos();
                map(&move |dx, dy| (dx * c - dy * s, dx * s + dy * c))
            })
            .collect(),
    }
}

/// Text: corner handles scale the type, side handles set a wrapping width.
fn resize_text(orig: &El, mut next: El, h: Handle) -> El {
    let Kind::Text(o) = &orig.kind else { return next };
    if h.corner() {
        let k = next.w / orig.w.max(0.01);
        let font_size = (o.font_size * k).max(MIN_SIZE);
        let w = if o.fixed_width { Some(orig.w * k) } else { None };
        let layout = text::layout_text(if o.text.is_empty() { " " } else { &o.text }, text::text_font(o).face, font_size, w);
        if let Kind::Text(t) = &mut next.kind {
            t.font_size = font_size;
        }
        next.h = layout.height;
        next.w = w.unwrap_or(layout.width);
        return next;
    }
    if h == Handle::E || h == Handle::W {
        let layout = text::layout_text(if o.text.is_empty() { " " } else { &o.text }, text::text_font(o).face, o.font_size, Some(next.w));
        if let Kind::Text(t) = &mut next.kind {
            t.fixed_width = true;
        }
        next.h = layout.height;
        return next;
    }
    orig.clone()
}

/// A code block from a corner grows or shrinks whole, letters too; from a side it only gets
/// wider or narrower; from the top or the bottom, taller (never shorter than its lines). The
/// side that stays put is the one across from the one dragged.
fn resize_code(orig: &El, mut next: El, h: Handle, to: BBox) -> El {
    let Kind::Code(o) = &orig.kind else { return next };
    if let Kind::Code(c) = &mut next.kind {
        if h.corner() {
            let k = (to.w / orig.w.max(0.01)).clamp(MIN_SIZE / o.font_size.max(MIN_SIZE), 400.0 / o.font_size.max(MIN_SIZE));
            c.font_size = o.font_size * k;
            c.radius = o.radius.map(|r| r * k);
            next.w = orig.w * k;
            next.h = orig.h * k;
        } else if h == Handle::E || h == Handle::W {
            next.w = to.w.max(c.font_size * 10.0);
            next.h = orig.h;
        } else {
            c.fixed_height = true;
            next.w = orig.w;
            next.h = to.h;
        }
    }
    crate::code::fit(&mut next);
    if orig.rotation == 0.0 {
        next.x = if h.w() { to.right() - next.w } else { to.x };
        next.y = if h.n() { orig.y + orig.h - next.h } else { orig.y };
    } else {
        // Turned: about the middle of the box dragged.
        let c = to.center();
        (next.x, next.y) = (c.x - next.w / 2.0, c.y - next.h / 2.0);
    }
    next
}

/// The pin of a comment, in board units: it keeps its size on screen whatever the zoom.
pub fn pin_box(el: &El, z: f64) -> BBox {
    let s = crate::prims::COMMENT_PIN / z;
    BBox { x: el.x, y: el.y - s, w: s, h: s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let board = Board::new(yrs::Doc::new(), Arc::new(|| {}));
        Editor::new(board, "b".into(), Painter::new(crate::paint::Images::new(None)), Prefs::default(), "v".into())
    }

    /// With a pen's pressure the stroke starts fine and swells, a spike of the driver makes no
    /// blot, and the end thins away as the pen lifts.
    #[test]
    fn pressure_starts_fine_ignores_spikes_and_thins_at_the_end() {
        let mut ed = editor();
        ed.size = (800.0, 600.0);
        ed.tool = Tool::Pen;
        ed.input.force = Some(0.7);
        ed.start_draw(pos2(100.0, 100.0), false, PointerKind::Pen, 1);
        for i in 0..=80 {
            // Pressing about 0.7, a single reading of 1.0 halfway.
            let f = if i == 40 { 1.0 } else { 0.7 + 0.01 * (i % 3) as f32 };
            ed.add_draw_point(pos2(100.0 + i as f32 * 4.0, 100.0), i as f64 * 5.0, Some(f));
        }
        let Some(Gesture::Draw(g)) = ed.gesture.take() else { panic!("drawing") };
        ed.commit_draw(g);
        let ink = ed.board.all().iter().find_map(|e| e.ink().cloned()).expect("a stroke");
        let p: Vec<f32> = ink.points.chunks_exact(3).map(|q| q[2]).collect();
        let n = p.len();
        let mid = p[n / 4..n / 2 - 4].iter().sum::<f32>() / (n / 4 - 4) as f32;
        assert!(p[0] < mid * 0.6, "a fine start: {} against {mid}", p[0]);
        let peak = p.iter().copied().fold(0.0, f32::max);
        assert!(peak < mid + 0.2, "no blot from the spike: {peak} against {mid}");
        assert!(p[n - 1] < mid * 0.6, "a thin end: {} against {mid}", p[n - 1]);
    }

    /// The sfumino drags colours along: from red into blue, it starts red, carries red a little
    /// way into the blue and ends up blue; each point keeps its colour.
    #[test]
    fn the_sfumino_drags_the_colours_under_it_along() {
        let mut ed = editor();
        ed.size = (800.0, 600.0);
        let fill = |id: &str, x: f64, color: &str| {
            let mut e = El::new(Kind::Shape(Shape { fill: color.into(), stroke: "transparent".into(), stroke_width: 0.0, ..Default::default() }));
            (e.id, e.x, e.y, e.w, e.h) = (id.into(), x, 0.0, 200.0, 200.0);
            e
        };
        ed.board.put([fill("r", 0.0, "#E03131"), fill("b", 200.0, "#1971C2")]);
        ed.tool = Tool::Pen;
        ed.prefs.pens[0] = crate::prefs::Pen { brush: Brush::Smudge, size: 20.0, opacity: 1.0, ..Default::default() };
        ed.start_draw(pos2(40.0, 100.0), false, PointerKind::Mouse, 0);
        for i in 0..=64 {
            ed.add_draw_point(pos2(40.0 + i as f32 * 5.0, 100.0), i as f64 * 8.0, None);
        }
        let Some(Gesture::Draw(g)) = ed.gesture.take() else { panic!("drawing") };
        ed.commit_draw(g);
        let ink = ed.board.all().iter().find_map(|e| e.ink().cloned()).expect("a stroke");
        assert_eq!(ink.colors.len(), ink.points.len() / 3, "a colour a point");
        let rgb = |c: u32| ((c >> 16) as u8, (c >> 8) as u8, c as u8);
        let at = |x: f32| {
            let i = ink.points.chunks_exact(3).position(|p| p[0] + 40.0 >= x).unwrap_or(0);
            rgb(ink.colors[i])
        };
        let (r0, _, b0) = at(40.0);
        assert!(r0 > 180 && b0 < 90, "red where it starts: {:?}", at(40.0));
        let (r1, _, b1) = at(215.0);
        assert!(r1 > 80 && b1 > 80, "red dragged into the blue: {:?}", at(215.0));
        let (r2, _, b2) = at(355.0);
        assert!(r2 < 70 && b2 > 150, "blue at the end: {:?}", at(355.0));
        // Drawn with those colours, and the same once saved and read back.
        let prims = crate::prims::smudge_prims(&ink.points, &ink.colors, ink.size, Color32::WHITE, None);
        assert!(prims.iter().any(|p| matches!(p, crate::prims::Prim::Dabs { tints, .. } if !tints.is_empty())));
        let back: Ink = serde_json::from_value(serde_json::to_value(&ink).unwrap()).unwrap();
        assert_eq!(back.colors, ink.colors);
        // TRATTO_SHOT=out.png: a few more strokes, back and forth, and the picture.
        if let Ok(path) = std::env::var("TRATTO_SHOT") {
            ed.prefs.pens[0].size = 46.0;
            for (k, y) in [60.0f32, 140.0, 175.0].into_iter().enumerate() {
                let x0 = if k % 2 == 0 { 360.0 } else { 40.0 };
                ed.start_draw(pos2(x0, y), false, PointerKind::Mouse, 0);
                for i in 0..=64 {
                    let x = if k % 2 == 0 { 360.0 - i as f32 * 5.0 } else { 40.0 + i as f32 * 5.0 };
                    ed.add_draw_point(pos2(x, y + (i as f32 * 0.3).sin() * 12.0), i as f64 * 8.0, Some(0.3 + 0.6 * (i as f32 / 64.0)));
                }
                let Some(Gesture::Draw(g)) = ed.gesture.take() else { panic!("drawing") };
                ed.commit_draw(g);
            }
            let els = ed.board.paint_order().to_vec();
            let env = crate::prims::Env::default();
            let pm = crate::raster::render(&els, Some(Color32::from_gray(0xF5)), 800, 400, BBox { x: 0.0, y: 0.0, w: 400.0, h: 200.0 }, &env, &|_| None).unwrap();
            pm.save_png(path).unwrap();
        }
    }

    /// The eyedropper sees what is drawn: a shape's fill, the board where nothing is, and a
    /// see-through stroke mixed with what is under it; the pen in use takes it.
    #[test]
    fn the_eyedropper_takes_the_colour_as_it_is_seen() {
        let mut ed = editor();
        let mut s = El::new(Kind::Shape(Shape { fill: "#E03131".into(), stroke: "transparent".into(), stroke_width: 0.0, ..Default::default() }));
        (s.x, s.y, s.w, s.h) = (0.0, 0.0, 100.0, 100.0);
        let mut ink = El::new(Kind::Ink(Ink { colors: Vec::new(), points: vec![0.0, 0.0, -1.0, 100.0, 0.0, -1.0], color: "#1971C2".into(), size: 20.0, brush: Some(Brush::Marker) }));
        (ink.x, ink.y, ink.w, ink.h, ink.z, ink.opacity) = (0.0, 50.0, 100.0, 0.0, 1.0, 0.5);
        ed.board.put([s, ink]);
        assert_eq!(ed.color_at(pt(20.0, 20.0)), "#E03131");
        assert_eq!(ed.color_at(pt(500.0, 500.0)), "#F5F5F5", "the board itself");
        let mixed = parse_color(&ed.color_at(pt(50.0, 50.0))).unwrap();
        assert!(mixed.r() > 0x30 && mixed.r() < 0xD0 && mixed.b() > 0x40, "blue over red, half see-through: {mixed:?}");
        ed.tool = Tool::Pen;
        ed.pick_color(pt(20.0, 20.0));
        assert_eq!(ed.prefs.pens[ed.pen].color, "#E03131");
        assert!(!ed.eyedropper);
    }

    #[test]
    fn a_code_block_grows_whole_from_a_corner_wider_from_a_side_and_taller_from_the_bottom() {
        let mut ed = editor();
        let mut el = El::new(Kind::Code(Code { code: "fn main() {\n    println!(\"ciao\");\n}".into(), language: "rust".into(), light: false, font_size: 14.0, radius: None, fixed_height: false }));
        (el.id, el.x, el.y, el.w) = ("c".into(), 100.0, 100.0, 400.0);
        crate::code::fit(&mut el);
        let h0 = el.h;
        let drag = |ed: &mut Editor, handle: Handle, from: Pt, to: Pt| {
            let orig = ed.board.get("c").unwrap();
            let frame = BBox { x: orig.x, y: orig.y, w: orig.w, h: orig.h };
            ed.gesture = Some(Gesture::Resize { handle, start: from, orig: vec![(**orig).clone()], frame, rotation: 0.0 });
            ed.resize_to(to, false, false);
            ed.preview.get("c").map(|e| (**e).clone()).unwrap()
        };
        ed.board.put([el]);
        // The bottom-right corner dragged to twice the size: everything twice as big, the top
        // left corner where it was.
        let big = drag(&mut ed, Handle::Se, pt(500.0, 100.0 + h0), pt(900.0, 100.0 + h0 * 2.0));
        assert_eq!((big.x, big.y), (100.0, 100.0));
        assert!((big.w - 800.0).abs() < 1e-6 && (big.code().unwrap().font_size - 28.0).abs() < 1e-6);
        assert!((big.h - h0 * 2.0).abs() < 2.0, "{} vs {}", big.h, h0 * 2.0);
        // The right side: only wider, the same letters, the top where it was.
        let wide = drag(&mut ed, Handle::E, pt(500.0, 120.0), pt(700.0, 160.0));
        assert_eq!((wide.x, wide.y, wide.code().unwrap().font_size), (100.0, 100.0, 14.0));
        assert!((wide.w - 600.0).abs() < 1e-6);
        // The bottom: taller, right to the pointer, and it stays so; never shorter than its lines.
        let tall = drag(&mut ed, Handle::S, pt(300.0, 100.0 + h0), pt(300.0, 100.0 + h0 + 150.0));
        assert_eq!((tall.x, tall.y, tall.w), (100.0, 100.0, 400.0));
        assert!((tall.h - (h0 + 150.0)).abs() < 1e-6 && tall.code().unwrap().fixed_height);
        let mut kept = tall.clone();
        crate::code::fit(&mut kept);
        assert_eq!(kept.h, tall.h);
        let short = drag(&mut ed, Handle::S, pt(300.0, 100.0 + h0), pt(300.0, 120.0));
        assert!((short.h - h0).abs() < 1e-6);
    }

    #[test]
    fn a_widget_grows_whole_from_a_corner_and_follows_the_pointer() {
        let mut ed = editor();
        let mut el = El::new(Kind::Widget(Widget::Counter { label: "Idee".into(), value: 7 }));
        (el.id, el.x, el.y) = ("w".into(), 100.0, 100.0);
        crate::widgets::fit(&mut el);
        let (w0, h0) = (el.w, el.h);
        ed.board.put([el.clone()]);
        let drag = |ed: &mut Editor, handle: Handle, from: Pt, to: Pt| {
            let orig = ed.board.get("w").unwrap();
            let frame = BBox { x: orig.x, y: orig.y, w: orig.w, h: orig.h };
            ed.gesture = Some(Gesture::Resize { handle, start: from, orig: vec![(**orig).clone()], frame, rotation: 0.0 });
            ed.resize_to(to, false, false);
            ed.preview.get("w").map(|e| (**e).clone()).unwrap()
        };
        // The bottom-right corner to twice the size: all of it twice as big, the top left where it was.
        let big = drag(&mut ed, Handle::Se, pt(100.0 + w0, 100.0 + h0), pt(100.0 + w0 * 2.0, 100.0 + h0 * 2.0));
        assert_eq!((big.x, big.y, big.scale), (100.0, 100.0, 2.0));
        assert!((big.w - w0 * 2.0).abs() < 1e-6 && (big.h - h0 * 2.0).abs() < 1e-6);
        // Its "+" is where it is drawn, twice as far in.
        let plus = crate::widgets::layout(&el, el.widget().unwrap()).parts.into_iter().find(|p| p.hot == crate::widgets::Hot::Add(1)).unwrap();
        let at = |k: f64| crate::widgets::hot_at(&big, (plus.x + plus.w / 2.0) * k, (plus.y + plus.h / 2.0) * k);
        assert_eq!(at(2.0), Some(crate::widgets::Hot::Add(1)));
        // The bottom: as tall as dragged, as much wider about its middle, the top where it was.
        let tall = drag(&mut ed, Handle::S, pt(100.0 + w0 / 2.0, 100.0 + h0), pt(100.0 + w0 / 2.0, 100.0 + h0 * 1.5));
        assert!((tall.h - h0 * 1.5).abs() < 1e-6 && (tall.scale - 1.5).abs() < 1e-9 && tall.y == 100.0);
        assert!((tall.x + tall.w / 2.0 - (100.0 + w0 / 2.0)).abs() < 1e-6);
        // The right side: only wider, the same size of letters, the top where it was.
        let wide = drag(&mut ed, Handle::E, pt(100.0 + w0, 120.0), pt(100.0 + w0 + 80.0, 160.0));
        assert_eq!((wide.x, wide.y, wide.scale, wide.h), (100.0, 100.0, 1.0, h0));
        assert!((wide.w - (w0 + 80.0)).abs() < 1e-6);
    }

    #[test]
    fn a_plus_pressed_shows_its_arrow_only_once_dragged() {
        let mut ed = editor();
        let mut note = El::new(Kind::Sticky(Sticky { text: "a".into(), ..Default::default() }));
        (note.id, note.w, note.h) = ("n".into(), 200.0, 200.0);
        ed.board.put([note.clone()]);
        let el = ed.connector(pt(220.0, 100.0), Some("n".into()), true);
        ed.gesture = Some(Gesture::Create { el, start: pt(220.0, 100.0), moved: false, spawn: Some(note) });
        assert_eq!(ed.paint_list().len(), 1, "a click adds a note, not this arrow");
        if let Some(Gesture::Create { moved, .. }) = &mut ed.gesture {
            *moved = true;
        }
        assert_eq!(ed.paint_list().len(), 2, "dragged: a connector");
    }

    #[test]
    fn the_plus_beside_a_note_adds_a_note_and_beside_a_shape_an_arrow_too() {
        let mut ed = editor();
        let mut note = El::new(Kind::Sticky(Sticky { text: "a".into(), ..Default::default() }));
        (note.id, note.w, note.h) = ("n".into(), 200.0, 200.0);
        ed.board.put([note.clone(), node("s", 0.0, 600.0)]);
        let lines = |ed: &mut Editor| ed.board.all().iter().filter(|e| e.is_line()).count();
        ed.add_beside(&note, Handle::AddE);
        assert_eq!(lines(&mut ed), 0, "as in FigJam: just another note");
        let new = ed.board.get(&ed.selection[0]).unwrap().clone();
        assert!(new.sticky().is_some() && new.x > 200.0 && new.x < 260.0, "right beside it: {}", new.x);
        let shape = ed.board.get("s").unwrap().clone();
        ed.add_beside(&shape, Handle::AddE);
        assert_eq!(lines(&mut ed), 1, "a diagram node comes with its arrow");
        ed.prefs.sticky_arrows = true;
        ed.add_beside(&note, Handle::AddS);
        assert_eq!(lines(&mut ed), 2, "arrows for notes too, when chosen");
    }

    #[test]
    fn sorting_notes_puts_each_colour_in_its_own_column() {
        let mut ed = editor();
        let note = |id: &str, x: f64, y: f64, color: &str| {
            let mut el = El::new(Kind::Sticky(Sticky { text: id.into(), color: color.into(), ..Default::default() }));
            (el.id, el.x, el.y, el.w, el.h) = (id.into(), x, y, 100.0, 100.0);
            el
        };
        ed.board.put([note("a", 0.0, 0.0, "#FFF3A3"), note("b", 500.0, 40.0, "#C7E5FF"), note("c", 200.0, 300.0, "#FFF3A3"), note("d", 900.0, 900.0, "#C7E5FF")]);
        ed.selection = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        ed.sort_stickies(false);
        let at = |id: &str| { let e = ed.board.get(id).unwrap(); (e.x, e.y) };
        assert_eq!(at("a"), (0.0, 0.0));
        assert_eq!(at("c"), (0.0, 116.0), "same colour: under the first");
        assert_eq!(at("b").1, 0.0);
        assert!(at("b").0 > 100.0, "the other colour: a column to the right");
        assert_eq!(at("d"), (at("b").0, 116.0));
    }

    #[test]
    fn emotes_point_round_the_wheel_and_float_once_for_everyone() {
        let mut ed = editor();
        ed.emote_hold = Some((0.0, pos2(100.0, 100.0)));
        assert_eq!(ed.emote_under(pos2(100.0, 50.0)), Some(EMOTES[0]), "straight up: the first");
        assert_eq!(ed.emote_under(pos2(150.0, 100.0)), Some(EMOTES[2]), "right: a quarter round");
        assert_eq!(ed.emote_under(pos2(100.0, 150.0)), Some(EMOTES[4]), "down: half round");
        assert_eq!(ed.emote_under(pos2(104.0, 102.0)), None, "in the middle: none");
        // Someone already there with an old emote: nothing floats; a new one floats once.
        ed.presence.peers = vec![(7, json!({ "emote": { "e": "🎉", "x": 1.0, "y": 2.0, "n": 3 } }))];
        ed.catch_emotes();
        assert!(ed.floaters.is_empty());
        ed.presence.peers = vec![(7, json!({ "emote": { "e": "🔥", "x": 1.0, "y": 2.0, "n": 4 } }))];
        ed.catch_emotes();
        ed.catch_emotes();
        assert_eq!(ed.floaters.len(), 1);
        assert_eq!(ed.floaters[0].0, "🔥");
        // Our own: floats here, and goes to the others.
        ed.send_emote("❤️", pt(5.0, 5.0));
        assert_eq!(ed.floaters.len(), 2);
        assert_eq!(ed.presence.local["emote"]["e"], "❤️");
    }

    fn node(id: &str, x: f64, y: f64) -> El {
        let mut el = El::new(Kind::Shape(Shape { fill: "#C7E5FF".into(), ..Default::default() }));
        (el.id, el.x, el.y, el.w, el.h) = (id.into(), x, y, 160.0, 80.0);
        el
    }

    fn arrows_from(ed: &mut Editor, id: &str) -> Vec<String> {
        ed.board.all().iter().filter_map(|e| e.line()).filter(|l| l.from.as_deref() == Some(id)).filter_map(|l| l.to.clone()).collect()
    }

    #[test]
    fn the_same_plus_twice_makes_two_branches() {
        let mut ed = editor();
        let root = node("root", 0.0, 0.0);
        ed.board.put([root.clone()]);
        ed.add_beside(&root, Handle::AddE);
        let first = ed.selection[0].clone();
        assert_eq!(ed.editing.as_deref(), Some(first.as_str()), "the new node is ready to write in");
        ed.add_beside(&root, Handle::AddE);
        let kids = arrows_from(&mut ed, "root");
        assert_eq!(kids.len(), 2, "both joined to the root by an arrow");
        let boxes: Vec<BBox> = kids.iter().map(|k| frame_box(ed.board.get(k).unwrap())).collect();
        assert!(!intersects(&boxes[0], &boxes[1]), "side by side, not on top of each other: {boxes:?}");
        assert!(boxes.iter().all(|b| b.x > 160.0), "to the right of the root");
        // Tab from the first child goes on to the right; Shift+Tab adds a third branch.
        assert!(ed.grow_diagram(&kids[0], false));
        assert_eq!(arrows_from(&mut ed, &kids[0]).len(), 1);
        assert!(ed.grow_diagram(&kids[0], true));
        assert_eq!(arrows_from(&mut ed, "root").len(), 3);
    }

    #[test]
    fn tidy_puts_each_level_in_a_column() {
        let mut ed = editor();
        let root = node("root", 0.0, 0.0);
        ed.board.put([root.clone()]);
        ed.add_beside(&root, Handle::AddE);
        ed.add_beside(&root, Handle::AddE);
        ed.add_beside(&root, Handle::AddS);
        let kids = arrows_from(&mut ed, "root");
        // Scatter them, then tidy.
        let scattered: Vec<El> = kids.iter().enumerate().map(|(i, k)| El { x: 500.0 + i as f64 * 150.0, y: -300.0 + i as f64 * 200.0, ..(**ed.board.get(k).unwrap()).clone() }).collect();
        ed.board.put(scattered);
        ed.tidy("root");
        let boxes: Vec<BBox> = kids.iter().map(|k| frame_box(ed.board.get(k).unwrap())).collect();
        let x0 = boxes[0].x;
        assert!(boxes.iter().all(|b| (b.x - x0).abs() < 1e-6 && b.x > 160.0), "one column right of the root: {boxes:?}");
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                assert!(!intersects(a, b), "no overlaps: {boxes:?}");
            }
        }
        let mid = boxes.iter().map(|b| b.center().y).sum::<f64>() / boxes.len() as f64;
        assert!((mid - 40.0).abs() < 1.0, "centred on the root: {mid}");
    }

    #[test]
    fn tables_and_sections_are_not_votable() {
        let mut ed = editor();
        ed.tool = Tool::Select;
        let mut table = El::new(Kind::Table(crate::model::Table::new(3, 3)));
        table.id = "table-1".into();
        table.w = 300.0;
        table.h = 90.0;

        let mut sticky = El::new(Kind::Sticky(crate::model::Sticky::default()));
        sticky.id = "sticky-1".into();
        sticky.x = 400.0;
        sticky.w = 100.0;
        sticky.h = 100.0;

        ed.board.put([table.clone(), sticky.clone()]);
        ed.board.start_voting(3, "v");

        assert!(!super::votable(&table), "tables are not votable items");
        assert!(super::votable(&sticky), "stickies are votable");

        // try_vote at the table returns false so that clicks can select/move the table!
        assert!(!ed.try_vote(Pt { x: 50.0, y: 50.0 }, false), "try_vote on table must return false");
        // table can be selected and moved during voting!
        ed.selection = vec![table.id.clone()];
        ed.start_move(Pt { x: 50.0, y: 50.0 }, egui::pos2(50.0, 50.0), false);
        assert!(matches!(ed.gesture, Some(super::Gesture::Move { .. })), "table can be moved during voting");
        // try_vote at the sticky returns true and casts a vote
        assert!(ed.try_vote(Pt { x: 450.0, y: 50.0 }, false), "try_vote on sticky casts a vote");
    }
}
