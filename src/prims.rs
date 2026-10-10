//! What each element paints, as a few primitives shared by every backend: the screen (egui
//! meshes), PNG/PDF (tiny-skia) and SVG. A port of the drawing code of the first Tratto.

use egui::Color32;

use crate::geom::{MIN_SIZE, shape_polygon};
use crate::ink::stroke_outline;
use crate::model::{Align, DARK, El, FontKind, Kind, Line, Shape, ShapeKind, color_or, is_dark};
use crate::text::{self, LINE_HEIGHT, Placed, STICKY_PAD, place_lines};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub cmds: Vec<Cmd>,
}

const KAPPA: f32 = 0.552_284_8;

impl Path {
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.cmds.push(Cmd::Move(x, y));
    }
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.cmds.push(Cmd::Line(x, y));
    }
    pub fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.cmds.push(Cmd::Quad(x1, y1, x, y));
    }
    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.cmds.push(Cmd::Cubic(x1, y1, x2, y2, x, y));
    }
    pub fn close(&mut self) {
        self.cmds.push(Cmd::Close);
    }

    /// Closed polygon from flat [x, y, …].
    pub fn polygon(pts: &[f64]) -> Path {
        let mut p = Path::default();
        p.add_polygon(pts);
        p
    }
    pub fn add_polygon(&mut self, pts: &[f64]) {
        for (i, q) in pts.chunks_exact(2).enumerate() {
            if i == 0 { self.move_to(q[0] as f32, q[1] as f32) } else { self.line_to(q[0] as f32, q[1] as f32) }
        }
        if pts.len() >= 4 {
            self.close();
        }
    }

    /// Open polyline from flat [x, y, …].
    pub fn polyline(pts: &[f64]) -> Path {
        let mut p = Path::default();
        for (i, q) in pts.chunks_exact(2).enumerate() {
            if i == 0 { p.move_to(q[0] as f32, q[1] as f32) } else { p.line_to(q[0] as f32, q[1] as f32) }
        }
        p
    }

    /// Rounded rectangle; `radii` are top-left, top-right, bottom-right, bottom-left.
    pub fn round_rect(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Path {
        let lim = (w / 2.0).min(h / 2.0).max(0.0);
        let [tl, tr, br, bl] = radii.map(|r| r.clamp(0.0, lim));
        let mut p = Path::default();
        p.move_to(x + tl, y);
        p.line_to(x + w - tr, y);
        if tr > 0.0 {
            p.cubic_to(x + w - tr + tr * KAPPA, y, x + w, y + tr - tr * KAPPA, x + w, y + tr);
        }
        p.line_to(x + w, y + h - br);
        if br > 0.0 {
            p.cubic_to(x + w, y + h - br + br * KAPPA, x + w - br + br * KAPPA, y + h, x + w - br, y + h);
        }
        p.line_to(x + bl, y + h);
        if bl > 0.0 {
            p.cubic_to(x + bl - bl * KAPPA, y + h, x, y + h - bl + bl * KAPPA, x, y + h - bl);
        }
        p.line_to(x, y + tl);
        if tl > 0.0 {
            p.cubic_to(x, y + tl - tl * KAPPA, x + tl - tl * KAPPA, y, x + tl, y);
        }
        p.close();
        p
    }

    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Path {
        Path::round_rect(x, y, w, h, [0.0; 4])
    }

    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        let mut p = Path::default();
        p.move_to(cx + rx, cy);
        p.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
        p.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
        p.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
        p.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
        p.close();
        p
    }

    /// Closed outline through the midpoints of a polygon, with its points as controls: how
    /// stroke outlines are drawn smoothly.
    pub fn smooth_outline(pts: &[crate::geom::Pt]) -> Path {
        let mut p = Path::default();
        let n = pts.len();
        if n == 0 {
            return p;
        }
        p.move_to(pts[0].x as f32, pts[0].y as f32);
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            p.quad_to(a.x as f32, a.y as f32, ((a.x + b.x) / 2.0) as f32, ((a.y + b.y) / 2.0) as f32);
        }
        p.close();
        p
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
}

/// Element-local to board coordinates: board = (a·x + c·y + e, b·x + d·y + f).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    /// Turned about the element's centre, like the canvas transform of the first Tratto.
    pub fn of(el: &El) -> Affine {
        let (s, c) = el.rotation.sin_cos();
        let (cx, cy, hw, hh) = (el.x + el.w / 2.0, el.y + el.h / 2.0, el.w / 2.0, el.h / 2.0);
        Affine { a: c, b: s, c: -s, d: c, e: cx - hw * c + hh * s, f: cy - hw * s - hh * c }
    }
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
}

/// Pictures drawn on the board.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageKey {
    /// An uploaded image of this board.
    File(String),
    /// A reaction, drawn from the bundled emoji pictures.
    Stamp(String),
}

pub enum Prim {
    /// Non-zero fill.
    Fill { path: Path, color: Color32 },
    /// `round`: round caps and joins (otherwise butt caps, bevel joins).
    Stroke { path: Path, width: f32, color: Color32, round: bool, dash: Option<[f32; 2]> },
    Text { placed: Placed, color: Color32 },
    Image { key: ImageKey, x: f32, y: f32, w: f32, h: f32, alpha: f32 },
    /// A soft shadow under a rounded rectangle.
    Shadow { x: f32, y: f32, w: f32, h: f32, radius: f32, blur: f32, dy: f32, color: Color32 },
}

/// How the element is being drawn.
#[derive(Clone, Copy, Debug)]
pub struct Env<'a> {
    /// Screen pixels per board unit, for things that keep their size on screen (section
    /// titles, comment pins). 1 for exports.
    pub zoom: f64,
    /// One device pixel in board units, when drawing on screen: strokes thinner than that are
    /// drawn as their centre line, same look and much faster.
    pub pixel: Option<f64>,
    /// Minimap: those strokes stay one pixel wide, and titles and pins are left out.
    pub hairline: bool,
    /// Its text is being edited in a text box over the board.
    pub editing: Option<&'a str>,
    /// Exports leave comments out (they are notes for the people on the board).
    pub comments: bool,
}

impl Default for Env<'_> {
    fn default() -> Self {
        Env { zoom: 1.0, pixel: None, hairline: false, editing: None, comments: false }
    }
}

/// Size of a comment pin on screen, in px.
pub const COMMENT_PIN: f64 = 26.0;

/// Arrow head length: proportional to the line width only.
fn head_length(width: f64) -> f64 {
    width * 3.6
}

/// Width of the outline drawn around an arrow head, which rounds its corners.
pub fn head_stroke(width: f64) -> f64 {
    width * 0.5
}

/// Arrow head triangle at (x2, y2) pointing away from (x1, y1).
pub fn arrow_head(x1: f64, y1: f64, x2: f64, y2: f64, width: f64) -> [f64; 6] {
    let len = head_length(width);
    let a = (y2 - y1).atan2(x2 - x1);
    let spread = std::f64::consts::PI / 7.0;
    [x2, y2, x2 - len * (a - spread).cos(), y2 - len * (a - spread).sin(), x2 - len * (a + spread).cos(), y2 - len * (a + spread).sin()]
}

/// Where the line body ends so it doesn't poke through the arrow tips.
pub fn line_ends(l: &Line) -> [f64; 4] {
    let [x1, y1, x2, y2] = [0, 1, 2, 3].map(|i| l.points[i] as f64);
    let len = (x2 - x1).hypot(y2 - y1);
    let len = if len > 0.0 { len } else { 1.0 };
    let back = head_length(l.stroke_width) * 0.6;
    let (ux, uy) = ((x2 - x1) / len, (y2 - y1) / len);
    let (sx, sy) = if l.arrow_start { (x1 + ux * back, y1 + uy * back) } else { (x1, y1) };
    let (ex, ey) = if l.arrow_end { (x2 - ux * back, y2 - uy * back) } else { (x2, y2) };
    [sx, sy, ex, ey]
}

pub fn with_alpha(c: Color32, k: f64) -> Color32 {
    let k = k.clamp(0.0, 1.0) as f32;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * k).round() as u8)
}

fn rgba(r: u8, g: u8, b: u8, a: f64) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The outline of a shape, in its local coordinates.
pub fn shape_path(el: &El, s: &Shape) -> Path {
    let (w, h) = (el.w as f32, el.h as f32);
    match s.shape {
        ShapeKind::Rect => {
            let r = (s.radius as f32).min(w / 2.0).min(h / 2.0).max(0.0);
            Path::round_rect(0.0, 0.0, w, h, [r; 4])
        }
        ShapeKind::Ellipse => Path::ellipse(w / 2.0, h / 2.0, w / 2.0, h / 2.0),
        k => Path::polygon(&shape_polygon(k, el.w, el.h, s.points.as_deref()).unwrap_or_default()),
    }
}

/// Washi tape: a translucent striped band with torn (zigzag) ends.
fn tape(l: &Line, alpha: f64, out: &mut Vec<Prim>) {
    let [x1, y1, x2, y2] = [0, 1, 2, 3].map(|i| l.points[i] as f64);
    let len = (x2 - x1).hypot(y2 - y1);
    let w = l.stroke_width;
    let tooth = (w / 6.0).min(len / 4.0);
    let (s, c) = (y2 - y1).atan2(x2 - x1).sin_cos();
    let to = |x: f64, y: f64| [x1 + x * c - y * s, y1 + x * s + y * c];
    let teeth = (w / if tooth > 0.0 { tooth * 2.0 } else { 1.0 }).round().max(2.0) as usize;
    let mut band = vec![to(0.0, -w / 2.0), to(len, -w / 2.0)];
    for i in 1..=teeth {
        band.push(to(len + if i % 2 == 1 { -tooth } else { 0.0 }, -w / 2.0 + w * i as f64 / teeth as f64));
    }
    band.push(to(0.0, w / 2.0));
    for i in 1..=teeth {
        band.push(to(if i % 2 == 1 { tooth } else { 0.0 }, w / 2.0 - w * i as f64 / teeth as f64));
    }
    let flat: Vec<f64> = band.into_iter().flatten().collect();
    out.push(Prim::Fill { path: Path::polygon(&flat), color: with_alpha(color_or(&l.stroke, DARK), alpha * 0.82) });
    // Diagonal stripes, lighter, kept inside the band.
    let (x_lo, x_hi) = (tooth / 2.0, len - tooth / 2.0);
    let n = std::f64::consts::FRAC_1_SQRT_2 * w / 14.0;
    let mut stripes = Path::default();
    let mut x = -w;
    while x < len + w {
        // Segment (x, w/2) → (x + w, −w/2), thickened, clipped to the band's rectangle.
        let quad = [(x - n, w / 2.0 - n), (x + w - n, -w / 2.0 - n), (x + w + n, -w / 2.0 + n), (x + n, w / 2.0 + n)];
        let clipped = clip_rect(&quad, x_lo, -w / 2.0, x_hi, w / 2.0);
        if clipped.len() >= 3 {
            let flat: Vec<f64> = clipped.iter().flat_map(|&(px, py)| to(px, py)).collect();
            stripes.add_polygon(&flat);
        }
        x += w / 2.5;
    }
    out.push(Prim::Fill { path: stripes, color: rgba(255, 255, 255, 0.45 * alpha * 0.82) });
}

/// Sutherland–Hodgman clip of a convex polygon to a rectangle.
fn clip_rect(poly: &[(f64, f64)], x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
    let mut pts = poly.to_vec();
    // (axis 0 = x / 1 = y, limit, keep the side ≥ limit)
    for (axis, v, above) in [(0, x0, true), (0, x1, false), (1, y0, true), (1, y1, false)] {
        if pts.is_empty() {
            break;
        }
        let get = |p: (f64, f64)| if axis == 0 { p.0 } else { p.1 };
        let inside = |p: (f64, f64)| if above { get(p) >= v } else { get(p) <= v };
        let cross = |a: (f64, f64), b: (f64, f64)| {
            let t = (v - get(a)) / (get(b) - get(a));
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        };
        let input = std::mem::take(&mut pts);
        for i in 0..input.len() {
            let (a, b) = (input[i], input[(i + 1) % input.len()]);
            match (inside(a), inside(b)) {
                (true, true) => pts.push(b),
                (true, false) => pts.push(cross(a, b)),
                (false, true) => {
                    pts.push(cross(a, b));
                    pts.push(b);
                }
                (false, false) => {}
            }
        }
    }
    pts
}

fn ui_font() -> text::FontRef {
    text::FontRef { face: text::ui_medium(), oblique: false }
}

/// Top of a one-line slot whose text is centred on `cy`.
fn line_top(cy: f64, size: f64) -> f64 {
    cy - size * LINE_HEIGHT / 2.0
}

/// The primitives of one element, in its local coordinates (see `Affine::of`), with its
/// opacity already applied.
pub fn prims_of(el: &El, env: &Env) -> Vec<Prim> {
    let mut out = Vec::new();
    if el.hidden {
        return out;
    }
    let alpha = el.opacity.clamp(0.0, 1.0);
    let editing = env.editing == Some(el.id.as_str());
    let (w, h) = (el.w as f32, el.h as f32);
    match &el.kind {
        Kind::Ink(i) | Kind::Highlighter(i) => {
            let hl = matches!(el.kind, Kind::Highlighter(_));
            let color = with_alpha(color_or(&i.color, DARK), alpha * if hl { 0.45 } else { 1.0 });
            // Zoomed out, points closer than a pixel or two only add vertices.
            let thinned = env.pixel.filter(|px| *px > 0.5).map(|px| crate::ink::thin_points(&i.points, 1.5 * px));
            let points = thinned.as_deref().unwrap_or(&i.points);
            // Thinner than a device pixel (far zoomed out): its centre line, much faster.
            if let Some(px) = env.pixel
                && i.size <= px
                && points.len() > 3
            {
                let pts: Vec<f64> = points.chunks_exact(3).flat_map(|p| [p[0] as f64, p[1] as f64]).collect();
                out.push(Prim::Stroke { path: Path::polyline(&pts), width: (if env.hairline { px } else { i.size }) as f32, color, round: false, dash: None });
            } else {
                let outline = stroke_outline(points, i.size.max(MIN_SIZE), hl);
                out.push(Prim::Fill { path: Path::smooth_outline(&outline), color });
            }
        }
        Kind::Shape(s) => {
            let path = shape_path(el, s);
            if s.fill != "transparent" {
                out.push(Prim::Fill { path: path.clone(), color: with_alpha(color_or(&s.fill, DARK), alpha) });
            }
            if s.stroke_width > 0.0 && s.stroke != "transparent" {
                let sw = s.stroke_width as f32;
                out.push(Prim::Stroke { path, width: sw, color: with_alpha(color_or(&s.stroke, DARK), alpha), round: true, dash: s.dash.then_some([sw * 3.0, sw * 2.2]) });
            }
            if s.text.as_ref().is_some_and(|t| !t.is_empty()) && !editing {
                let t = text::shape_text_layout(el, s);
                let f = text::font(s.font.unwrap_or_default(), false, false);
                out.push(Prim::Text { placed: place_lines(&t.layout.lines, f, t.size, Align::Center, t.width, t.top, t.left), color: with_alpha(text::shape_text_color(s), alpha) });
            }
        }
        Kind::Comment { thread } => {
            if env.hairline || !env.comments {
                return out;
            }
            let s = (COMMENT_PIN / env.zoom) as f32;
            out.push(Prim::Shadow { x: 0.0, y: -s, w: s, h: s, radius: s / 2.0, blur: 6.0 / env.zoom as f32, dy: 1.0 / env.zoom as f32, color: rgba(0, 0, 0, 0.25 * alpha) });
            out.push(Prim::Fill { path: Path::round_rect(0.0, -s, s, s, [s / 2.0, s / 2.0, s / 2.0, 0.0]), color: with_alpha(Color32::WHITE, alpha) });
            let first = thread.first();
            let color = first.and_then(|m| crate::model::parse_color(&m.color)).unwrap_or(Color32::from_rgb(0x0D, 0x99, 0xFF));
            out.push(Prim::Fill { path: Path::ellipse(s / 2.0, -s / 2.0, s * 0.36, s * 0.36), color: with_alpha(color, alpha) });
            let letter = match first {
                Some(m) => m.author.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()),
                None => "+".into(),
            };
            let on = if is_dark(&crate::model::hex(color)) { Color32::WHITE } else { DARK };
            let size = s as f64 * 0.36;
            out.push(Prim::Text { placed: place_lines(&[letter], ui_font(), size, Align::Center, s as f64, line_top(-s as f64 / 2.0, size), 0.0), color: with_alpha(on, alpha) });
            if thread.len() > 1 {
                let size = s as f64 * 0.32;
                out.push(Prim::Text { placed: place_lines(&[thread.len().to_string()], ui_font(), size, Align::Left, 0.0, line_top(-s as f64 / 2.0, size), s as f64 * 1.12), color: with_alpha(DARK, alpha) });
            }
        }
        Kind::Section { fill } => {
            let fill_c = color_or(fill, Color32::WHITE);
            let dark = is_dark(fill);
            let r = ((8.0 / env.zoom) as f32).min(w / 2.0).min(h / 2.0);
            let body = Path::round_rect(0.0, 0.0, w, h, [r; 4]);
            out.push(Prim::Fill { path: body.clone(), color: with_alpha(fill_c, alpha) });
            out.push(Prim::Stroke { path: body, width: (1.0 / env.zoom) as f32, color: if dark { rgba(255, 255, 255, 0.18 * alpha) } else { rgba(0, 0, 0, 0.14 * alpha) }, round: true, dash: None });
            if !env.hairline {
                let t = text::section_title_box(el, env.zoom);
                let (x, y) = ((t.x - el.x) as f32, (t.y - el.y) as f32);
                let pill = Path::round_rect(x, y, t.w as f32, t.h as f32, [(5.0 / env.zoom) as f32; 4]);
                out.push(Prim::Fill { path: pill.clone(), color: with_alpha(fill_c, alpha) });
                out.push(Prim::Fill { path: pill, color: if dark { rgba(255, 255, 255, 0.12 * alpha) } else { rgba(0, 0, 0, 0.07 * alpha) } });
                if !editing {
                    let room = t.w - t.pad * 2.0 + t.size * 0.25;
                    let mut label = t.text.clone();
                    if text::measure(text::ui_medium(), &label, t.size) > room {
                        while label.chars().count() > 1 && text::measure(text::ui_medium(), &format!("{label}…"), t.size) > room {
                            label.pop();
                        }
                        label.push('…');
                    }
                    let color = if dark { Color32::WHITE } else { rgba(0, 0, 0, 0.85) };
                    out.push(Prim::Text { placed: place_lines(&[label], ui_font(), t.size, Align::Left, 0.0, line_top(y as f64 + t.h / 2.0, t.size), x as f64 + t.pad), color: with_alpha(color, alpha) });
                }
            }
        }
        Kind::Line(l) => {
            if l.tape {
                tape(l, alpha, &mut out);
                return out;
            }
            let [sx, sy, ex, ey] = line_ends(l);
            let color = with_alpha(color_or(&l.stroke, DARK), alpha);
            let sw = l.stroke_width as f32;
            out.push(Prim::Stroke { path: Path::polyline(&[sx, sy, ex, ey]), width: sw, color, round: true, dash: l.dash.then_some([sw * 3.0, sw * 2.2]) });
            let [x1, y1, x2, y2] = [0, 1, 2, 3].map(|i| l.points[i] as f64);
            for (on, head) in [(l.arrow_end, arrow_head(x1, y1, x2, y2, l.stroke_width)), (l.arrow_start, arrow_head(x2, y2, x1, y1, l.stroke_width))] {
                if on {
                    let path = Path::polygon(&head);
                    out.push(Prim::Fill { path: path.clone(), color });
                    out.push(Prim::Stroke { path, width: head_stroke(l.stroke_width) as f32, color, round: true, dash: None });
                }
            }
        }
        Kind::Text(t) => {
            if !editing {
                let layout = text::text_layout(el, t);
                out.push(Prim::Text { placed: place_lines(&layout.lines, text::text_font(t), t.font_size, t.align, el.w, 0.0, 0.0), color: with_alpha(color_or(&t.color, DARK), alpha) });
            }
        }
        Kind::Sticky(s) => {
            out.push(Prim::Shadow { x: 0.0, y: 0.0, w, h, radius: 4.0, blur: 10.0, dy: 3.0, color: rgba(0, 0, 0, 0.14 * alpha) });
            out.push(Prim::Fill { path: Path::round_rect(0.0, 0.0, w, h, [4.0; 4]), color: with_alpha(color_or(&s.color, Color32::from_rgb(0xFF, 0xF3, 0xA3)), alpha) });
            let dark = is_dark(&s.color);
            if text::show_author(s) {
                let size = el.h * 0.055;
                // Alphabetic baseline at h − pad·0.75: the line's centre sits a little above it.
                let color = if dark { rgba(255, 255, 255, 0.75) } else { rgba(0, 0, 0, 0.6) };
                let author = s.author.clone().unwrap_or_default();
                let placed = place_lines(&[author], text::font(FontKind::Sans, false, false), size, Align::Left, 0.0, line_top(el.h - STICKY_PAD * 0.75 - size * 0.36, size), STICKY_PAD * 0.75);
                out.push(Prim::Text { placed, color: with_alpha(color, alpha) });
            }
            if !editing && !s.text.is_empty() {
                let (layout, size) = text::sticky_layout(el, s);
                let top = (el.h - text::author_band(el, s) - layout.height) / 2.0;
                let color = if dark { Color32::WHITE } else { DARK };
                out.push(Prim::Text { placed: place_lines(&layout.lines, text::font(s.font, false, false), size, s.align, el.w - STICKY_PAD * 2.0, top, STICKY_PAD), color: with_alpha(color, alpha) });
            }
        }
        Kind::Image { file_id } => out.push(Prim::Image { key: ImageKey::File(file_id.clone()), x: 0.0, y: 0.0, w, h, alpha: alpha as f32 }),
        Kind::Stamp { emoji } => out.push(Prim::Image { key: ImageKey::Stamp(emoji.clone()), x: 0.0, y: 0.0, w, h, alpha: alpha as f32 }),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_keeps_stripes_inside_the_tape() {
        let out = clip_rect(&[(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)], 0.0, 0.0, 10.0, 10.0);
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|&(x, y)| (0.0..=5.0).contains(&x) && (0.0..=5.0).contains(&y)));
        assert!(clip_rect(&[(20.0, 20.0), (30.0, 20.0), (30.0, 30.0)], 0.0, 0.0, 10.0, 10.0).is_empty());
    }

    #[test]
    fn the_transform_turns_about_the_centre() {
        let mut el = El::new(Kind::Section { fill: "#FFFFFF".into() });
        (el.x, el.y, el.w, el.h, el.rotation) = (10.0, 20.0, 100.0, 50.0, std::f64::consts::FRAC_PI_2);
        let t = Affine::of(&el);
        let (x, y) = t.apply(50.0, 25.0);
        assert!((x - 60.0).abs() < 1e-9 && (y - 45.0).abs() < 1e-9, "centre stays put");
        let p = crate::geom::to_world_of(&el, 0.0, 0.0);
        let (x, y) = t.apply(0.0, 0.0);
        assert!((x - p.x).abs() < 1e-9 && (y - p.y).abs() < 1e-9);
    }
}
