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
}

impl Presence {
    pub fn set(&mut self, k: &str, v: Value) {
        if self.local.get(k) != Some(&v) {
            self.local.insert(k.into(), v);
            self.dirty = true;
        }
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
}

pub(crate) struct Draw {
    hl: bool,
    color: String,
    size: f64,
    opacity: f64,
    /// Board coordinates, flat [x, y, pressure, …].
    pts: Vec<f64>,
    snap: Option<RulerSnap>,
    filter: OneEuro,
    /// Smoothed pen pressure; None until the first sample.
    pressure: Option<f64>,
    use_pressure: bool,
    /// Last raw position, added at release so the line ends exactly where the pen lifted.
    raw: Option<Pos2>,
    pointer: u64,
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
    Move { start: Pt, orig: Vec<El>, bbox: BBox, cands: Vec<BBox>, moved: bool, s0: Pos2 },
    Resize { handle: Handle, start: Pt, orig: Vec<El>, frame: BBox, rotation: f64 },
    Rotate { c: Pt, a0: f64, orig: Vec<El> },
    Endpoint { which: usize, orig: El },
    /// `spawn`: dragged from a "+": released on empty board, a copy of it is made there and joined.
    Create { el: El, start: Pt, moved: bool, spawn: Option<El> },
    Laser,
    Ruler { s0: Pos2, x: f64, y: f64 },
    RulerRotate,
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
    pub(crate) hover_handle: Option<Handle>,
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
}

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

/// What can receive votes: things with content, not lines, strokes or comments.
fn votable(el: &El) -> bool {
    !matches!(el.kind, Kind::Line(_) | Kind::Ink(_) | Kind::Highlighter(_) | Kind::Comment { .. })
}

/// Elements with text to type into: text, stickies, shapes, section titles.
pub fn writable(el: &El) -> bool {
    !el.locked && matches!(el.kind, Kind::Text(_) | Kind::Sticky(_) | Kind::Shape(_) | Kind::Section { .. })
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
            comment: None,
            read_only: false,
            following: None,
            ruler: Ruler { visible: false, x: 600.0, y: 400.0, angle: 0.0 },
            lost: false,
            size: (800.0, 600.0),
            origin: pos2(0.0, 0.0),
            voter,
            name: None,
            requests: Vec::new(),
            gesture: None,
            preview: HashMap::new(),
            hover: None,
            hover_handle: None,
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
        // A section being drawn goes behind everything, anything else on top.
        if let Some(Gesture::Create { el, .. }) = &self.gesture {
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
        None
    }

    /* ---------- camera ---------- */

    pub fn set_cam(&mut self, c: Camera) {
        let c = Camera { x: c.x, y: c.y, z: c.z.clamp(MIN_ZOOM, MAX_ZOOM) };
        if c != self.cam {
            self.cam = c;
            self.cam_dirty = true;
            let (w, h) = self.size;
            self.presence.set("view", json!({ "cx": r2((w / 2.0 - c.x) / c.z), "cy": r2((h / 2.0 - c.y) / c.z), "z": (c.z * 1000.0).round() / 1000.0 }));
        }
    }

    pub fn zoom_at(&mut self, sx: f64, sy: f64, z: f64) {
        let nz = z.clamp(MIN_ZOOM, MAX_ZOOM);
        let w = self.cam.to_world(sx, sy);
        self.set_cam(Camera { x: sx - w.x * nz, y: sy - w.y * nz, z: nz });
    }

    pub fn fit_box(&mut self, b: Option<BBox>, max_zoom: f64) {
        let (w, h) = self.size;
        let Some(b) = b else {
            return self.set_cam(Camera { x: w / 2.0, y: h / 2.0, z: 1.0 });
        };
        let pad = 80.0;
        let z = ((w - pad * 2.0) / b.w.max(1.0)).min((h - pad * 2.0) / b.h.max(1.0)).min(max_zoom).clamp(MIN_ZOOM, MAX_ZOOM);
        self.set_cam(Camera { x: w / 2.0 - (b.x + b.w / 2.0) * z, y: h / 2.0 - (b.y + b.h / 2.0) * z, z });
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
        best
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
            self.flush_due = Some(now() + 110.0);
        }
    }

    /// Called every frame: writes a gesture's changes to the board now and then, so others see them.
    pub fn tick(&mut self) {
        if self.flush_due.is_some_and(|t| now() >= t) {
            self.flush_preview();
        }
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
        // Lose selection of elements that are gone (deleted by someone else, undone…).
        if !matches!(self.gesture, Some(Gesture::Move { .. })) {
            self.prune();
        }
    }

    fn base(&mut self, mut el: El) -> El {
        el.rotation = 0.0;
        el.z = self.board.top_z();
        el.opacity = 1.0;
        el
    }

    /// Sizes chosen in the toolbar are what you see on screen, whatever the zoom.
    pub fn world_size(&self, screen_px: f64) -> f64 {
        r2(screen_px / self.cam.z)
    }

    fn ink_element(&mut self, pts: &[f64], size: f64, color: &str, hl: bool, opacity: f64) -> El {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in pts.chunks_exact(3) {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let rnd = stroke_round(size);
        let points = pts.chunks_exact(3).flat_map(|p| [rnd(p[0] - x0), rnd(p[1] - y0), ((p[2] * 1000.0).round() / 1000.0) as f32]).collect();
        let ink = Ink { points, color: color.into(), size };
        let mut el = self.base(El::new(if hl { Kind::Highlighter(ink) } else { Kind::Ink(ink) }));
        (el.x, el.y, el.w, el.h, el.opacity) = (x0, y0, x1 - x0, y1 - y0, opacity);
        el
    }

    fn line_element(&mut self, a: Pt, b: Pt, style: impl FnOnce(&mut El, &mut Line)) -> El {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        let mut line = Line { points: vec![(a.x - x0) as f32, (a.y - y0) as f32, (b.x - x0) as f32, (b.y - y0) as f32], stroke: "#1E1E1E".into(), stroke_width: 3.0, dash: false, arrow_start: false, arrow_end: false, from: None, to: None, tape: false };
        let mut el = self.base(El::new(Kind::Line(line.clone())));
        (el.x, el.y, el.w, el.h) = (x0, y0, (b.x - a.x).abs(), (b.y - a.y).abs());
        style(&mut el, &mut line);
        el.kind = Kind::Line(line);
        el
    }

    fn connector(&mut self, at: Pt, from: Option<String>, arrow: bool) -> El {
        let (stroke, width) = (self.prefs.shape_style.stroke.clone(), self.world_size(self.prefs.shape_style.stroke_width.max(2.0)));
        self.line_element(at, at, |_, l| {
            l.stroke = stroke;
            l.stroke_width = width;
            l.arrow_end = arrow;
            l.from = from;
        })
    }

    /* ---------- drawing ---------- */

    pub(crate) fn start_draw(&mut self, s: Pos2, hl: bool, kind: PointerKind, pointer: u64) {
        let pen = if hl { self.prefs.highlighter.clone() } else { self.prefs.pens.get(self.pen).or(self.prefs.pens.first()).cloned().unwrap_or_default() };
        let size = self.world_size(pen.size);
        self.gesture = Some(Gesture::Draw(Box::new(Draw {
            hl,
            color: pen.color,
            size,
            opacity: pen.opacity.clamp(0.05, 1.0),
            pts: Vec::new(),
            snap: ink::ruler_snapper(&self.ruler, s.x as f64, s.y as f64, size * self.cam.z / 2.0),
            filter: OneEuro::new(self.prefs.ink_smoothing.cutoff()),
            pressure: None,
            use_pressure: kind == PointerKind::Pen && self.prefs.pressure && !hl,
            raw: None,
            pointer,
        })));
    }

    /// A new pen position (canvas coordinates), with the pen pressure if any.
    pub(crate) fn add_draw_point(&mut self, raw: Pos2, t_ms: f64, force: Option<f32>) {
        let cam = self.cam;
        let Some(Gesture::Draw(g)) = &mut self.gesture else { return };
        g.raw = Some(raw);
        let s = g.filter.filter(raw.x as f64, raw.y as f64, t_ms);
        let mut pressure = -1.0;
        if g.use_pressure
            && let Some(f) = force
        {
            let p = ink::next_pressure(g.pressure, f as f64);
            g.pressure = Some(p);
            pressure = ink::stored_pressure(p) as f64;
        }
        push_draw_point(g, &cam, pt(s.x, s.y), pressure, false);
        let rnd = stroke_round(g.size);
        let tail = &g.pts[g.pts.len().saturating_sub(3000)..];
        let pts: Vec<Value> = tail.iter().enumerate().map(|(i, &v)| json!(if i % 3 == 2 { r2(v) } else { rnd(v) as f64 })).collect();
        let live = json!({ "pts": pts, "color": g.color, "size": g.size, "hl": g.hl, "op": g.opacity });
        self.presence.set("live", live);
    }

    fn commit_draw(&mut self, mut g: Box<Draw>) {
        self.presence.set("live", Value::Null);
        // The filter trails the pen slightly: finish exactly where it was lifted.
        if let Some(raw) = g.raw
            && !g.pts.is_empty()
        {
            let last = g.pts[g.pts.len() - 1];
            push_draw_point(&mut g, &self.cam, pt(raw.x as f64, raw.y as f64), last, false);
        }
        if g.pts.is_empty() || self.read_only {
            return;
        }
        let z = self.cam.z;
        let flat: Vec<f32> = g.pts.iter().map(|&v| v as f32).collect();
        let thin: Vec<f64> = ink::thin_points(&flat, 0.4 / z).into_iter().map(|v| v as f64).collect();
        let mut el = self.ink_element(&thin, g.size, &g.color, g.hl, g.opacity);
        if !g.hl && self.prefs.ink_to_shape && g.snap.is_none() {
            let flat: Vec<f32> = thin.iter().map(|&v| v as f32).collect();
            if let Some(r) = ink::recognize(&flat, 1.0 / z) {
                let (color, size, op) = (g.color.clone(), g.size, g.opacity);
                let shape = |kind: ShapeKind, points: Option<Vec<f32>>| Kind::Shape(Shape { shape: kind, fill: "transparent".into(), stroke: color.clone(), stroke_width: size, radius: 0.0, dash: false, points, text: None, font: None });
                el = match r {
                    Recognized::Line { x1, y1, x2, y2 } => {
                        let c = color.clone();
                        self.line_element(pt(x1, y1), pt(x2, y2), |el, l| {
                            l.stroke = c;
                            l.stroke_width = size;
                            el.opacity = op;
                        })
                    }
                    Recognized::Triangle(p) => {
                        let b = bounds_of(p);
                        let pts = p.iter().flat_map(|q| [r2((q.x - b.x) / b.w) as f32, r2((q.y - b.y) / b.h) as f32]).collect();
                        let mut s = self.base(El::new(shape(ShapeKind::Polygon, Some(pts))));
                        (s.x, s.y, s.w, s.h, s.opacity) = (b.x, b.y, b.w, b.h, op);
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
        self.board.stop_capturing();
        self.board.put([el]);
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
        self.gesture = Some(Gesture::Move { start: p, orig, bbox, cands, moved: false, s0: s });
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
                let shape = Shape { shape: kind, fill: self.prefs.shape_style.fill.clone(), stroke: style_stroke, stroke_width: style_width, radius: if round { self.world_size(16.0) } else { 0.0 }, dash: false, points: None, text: None, font: None };
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
                self.line_element(p, p, |_, l| {
                    l.stroke = style_stroke;
                    l.stroke_width = style_width;
                    l.arrow_end = tool == Tool::Arrow;
                    l.from = from;
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

    /// Click on a "+": a connected copy beside the element, on that side.
    fn add_beside(&mut self, el: &El, side: Handle) {
        let z = self.cam.z;
        let gap = (80.0 / z).max(el.w.min(el.h) * 0.5);
        let c = center(el);
        let (dx, dy) = match side {
            Handle::AddN => (0.0, -1.0),
            Handle::AddE => (1.0, 0.0),
            Handle::AddS => (0.0, 1.0),
            _ => (-1.0, 0.0),
        };
        let copy = self.sibling(el, c.x + dx * (el.w + gap), c.y + dy * (el.h + gap));
        let mut base = self.connector(c, Some(el.id.clone()), true);
        base.line_mut().unwrap().to = Some(copy.id.clone());
        let line = route_connector(&base, |id| if id == copy.id { Some(&copy) } else { self.board.get(id).map(|e| &**e) }).unwrap_or(base);
        self.board.stop_capturing();
        let (id, sticky) = (copy.id.clone(), matches!(copy.kind, Kind::Sticky(_)));
        self.board.put([copy, line]);
        self.selection = vec![id.clone()];
        if sticky {
            self.editing = Some(id);
        }
    }

    pub(crate) fn start_text(&mut self, p: Pt) {
        let t = self.prefs.text.clone();
        let font_size = self.world_size(t.font_size);
        let mut el = self.base(El::new(Kind::Text(Text { text: String::new(), color: t.color, font_size, font: t.font, align: Align::Left, bold: false, italic: false, fixed_width: false })));
        (el.x, el.y, el.w, el.h) = (p.x, p.y - font_size * LINE_HEIGHT / 2.0, font_size / 6.0, font_size * LINE_HEIGHT);
        self.board.stop_capturing();
        let id = el.id.clone();
        self.board.put([el]);
        self.set_tool(Tool::Select);
        self.selection = vec![id.clone()];
        self.editing = Some(id);
    }

    pub(crate) fn start_sticky(&mut self, p: Pt) {
        let s = self.world_size(220.0);
        let mut el = self.base(El::new(Kind::Sticky(Sticky { text: String::new(), color: self.prefs.sticky_color.clone(), font: FontKind::Sans, align: Align::Center, author: self.name.clone(), hide_author: false })));
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
        let lock = shift || els.iter().any(|e| matches!(e.kind, Kind::Image { .. } | Kind::Stamp { .. })) || (els.len() == 1 && els[0].text().is_some() && h.corner());
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
            self.presence.set("live", Value::Null);
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
        let (w, h) = self.size;
        self.zoom_at(w / 2.0, h / 2.0, self.cam.z * f);
    }

    pub fn zoom_to(&mut self, z: f64) {
        let (w, h) = self.size;
        self.zoom_at(w / 2.0, h / 2.0, z);
    }

    pub fn content_box(&mut self) -> Option<BBox> {
        union(self.board.all().iter().filter(|e| !e.hidden).map(|e| aabb(e)))
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
        let mut el = self.base(El::new(Kind::Text(Text { text, color: t.color, font_size, font: t.font, align: Align::Left, bold: false, italic: false, fixed_width: long })));
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
        self.cam.to_world(self.size.0 / 2.0, self.size.1 / 2.0)
    }

    pub fn center_on(&mut self, p: Pt) {
        self.stop_following();
        let z = self.cam.z;
        let (w, h) = self.size;
        self.set_cam(Camera { x: w / 2.0 - p.x * z, y: h / 2.0 - p.y * z, z });
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
        let (w, h) = self.size;
        let following = self.following;
        self.set_cam(Camera { x: w / 2.0 - cx * z, y: h / 2.0 - cy * z, z });
        self.following = following;
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
            _ => return,
        }
        text::fit_text(&mut next);
        self.board.put([next]);
    }

    /// Leaves the text being edited; an empty new text box goes away.
    pub fn stop_editing(&mut self) {
        let Some(id) = self.editing.take() else { return };
        self.board.stop_capturing();
        if let Some(el) = self.board.get(&id)
            && el.text().is_some_and(|t| t.text.trim().is_empty())
        {
            self.board.remove([id]);
        }
    }
}

fn push_draw_point(g: &mut Draw, cam: &Camera, s: Pt, pressure: f64, force: bool) {
    let s = match g.snap {
        Some(snap) => snap.apply(s.x, s.y),
        None => s,
    };
    let w = cam.to_world(s.x, s.y);
    let n = g.pts.len();
    if !force && n > 0 && (g.pts[n - 3] - w.x).hypot(g.pts[n - 2] - w.y) < 0.5 / cam.z {
        return;
    }
    g.pts.extend([w.x, w.y, pressure]);
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

/// The pin of a comment, in board units: it keeps its size on screen whatever the zoom.
pub fn pin_box(el: &El, z: f64) -> BBox {
    let s = crate::prims::COMMENT_PIN / z;
    BBox { x: el.x, y: el.y - s, w: s, h: s }
}
