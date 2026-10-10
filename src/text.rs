//! Board text: the bundled fonts, shaping (rustybuzz) and line breaking, measured the way the
//! first Tratto measured it in the browser, and glyph outlines for drawing at any zoom.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use rustybuzz::ttf_parser;

use crate::model::{Align, DARK, El, FontKind, Kind, Shape, ShapeKind, Sticky, Text, color_or, is_dark};
use crate::prims::Path;

pub const LINE_HEIGHT: f64 = 1.3;
pub const STICKY_PAD: f64 = 16.0;
/// How far synthetic italics lean (fonts without an italic face), as browsers do.
const OBLIQUE: f32 = 0.2;

macro_rules! font_files {
    ($($name:literal),* $(,)?) => { [$(($name, include_bytes!(concat!("../assets/fonts/", $name, ".ttf")) as &[u8])),*] };
}

pub static FILES: [(&str, &[u8]); 36] = font_files!(
    "sans", "sans-bold", "sans-italic", "sans-bolditalic",
    "rounded", "rounded-bold", "rounded-italic", "rounded-bolditalic",
    "geometric", "geometric-bold", "geometric-italic", "geometric-bolditalic",
    "condensed", "condensed-bold",
    "serif", "serif-bold", "serif-italic", "serif-bolditalic",
    "book", "book-bold", "book-italic", "book-bolditalic",
    "display", "display-bold", "display-italic", "display-bolditalic",
    "mono", "mono-bold", "mono-italic", "mono-bolditalic",
    "hand", "hand-bold", "print", "marker", "ui", "ui-medium",
);

pub struct FaceInfo {
    pub face: rustybuzz::Face<'static>,
    pub upem: f32,
    pub ascender: f32,
    pub descender: f32,
}

pub fn faces() -> &'static [FaceInfo] {
    static F: OnceLock<Vec<FaceInfo>> = OnceLock::new();
    F.get_or_init(|| {
        FILES
            .iter()
            .map(|(name, data)| {
                let face = rustybuzz::Face::from_slice(data, 0).unwrap_or_else(|| panic!("font {name}"));
                let (upem, ascender, descender) = (face.units_per_em() as f32, face.ascender() as f32, face.descender() as f32);
                FaceInfo { face, upem, ascender, descender }
            })
            .collect()
    })
}

pub fn face_id(name: &str) -> Option<usize> {
    FILES.iter().position(|(n, _)| *n == name)
}

/// Inter at Figma's medium weight, used for section titles.
pub fn ui_medium() -> usize {
    face_id("ui-medium").unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontRef {
    pub face: usize,
    /// Leaned by hand: the family has no italic.
    pub oblique: bool,
}

pub fn prefix(kind: FontKind) -> &'static str {
    match kind {
        FontKind::Sans => "sans",
        FontKind::Rounded => "rounded",
        FontKind::Geometric => "geometric",
        FontKind::Condensed => "condensed",
        FontKind::Serif => "serif",
        FontKind::Book => "book",
        FontKind::Display => "display",
        FontKind::Mono => "mono",
        FontKind::Hand => "hand",
        FontKind::Print => "print",
        FontKind::Marker => "marker",
    }
}

pub fn font(kind: FontKind, bold: bool, italic: bool) -> FontRef {
    let p = prefix(kind);
    let want = match (bold, italic) {
        (true, true) => "-bolditalic",
        (true, false) => "-bold",
        (false, true) => "-italic",
        _ => "",
    };
    if let Some(face) = face_id(&format!("{p}{want}")) {
        return FontRef { face, oblique: false };
    }
    let upright = if bold { face_id(&format!("{p}-bold")) } else { None }.or_else(|| face_id(p)).unwrap();
    FontRef { face: upright, oblique: italic }
}

/* ---------------- shaping ---------------- */

/// A shaped line: glyph id and pen position in font units (y up), and the advance.
#[derive(Default)]
pub struct Run {
    pub glyphs: Vec<(u16, f32, f32)>,
    pub width: f32,
}

thread_local! {
    static RUNS: RefCell<HashMap<usize, HashMap<String, Rc<Run>>>> = RefCell::new(HashMap::new());
    static GLYPHS: RefCell<HashMap<(usize, u16), Rc<Path>>> = RefCell::new(HashMap::new());
}

pub fn shape(face: usize, text: &str) -> Rc<Run> {
    RUNS.with(|r| {
        let mut r = r.borrow_mut();
        let per = r.entry(face).or_default();
        if let Some(run) = per.get(text) {
            return run.clone();
        }
        if per.len() > 50_000 {
            per.clear();
        }
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        let out = rustybuzz::shape(&faces()[face].face, &[], buf);
        let mut x = 0.0;
        let glyphs = out
            .glyph_infos()
            .iter()
            .zip(out.glyph_positions())
            .map(|(info, pos)| {
                let g = (info.glyph_id as u16, x + pos.x_offset as f32, pos.y_offset as f32);
                x += pos.x_advance as f32;
                g
            })
            .collect();
        let run = Rc::new(Run { glyphs, width: x });
        per.insert(text.to_string(), run.clone());
        run
    })
}

/// Width of a line of text at `size`, in the same units as `size`.
pub fn measure(face: usize, text: &str, size: f64) -> f64 {
    shape(face, text).width as f64 * size / faces()[face].upem as f64
}

/* ---------------- line breaking ---------------- */

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub lines: Vec<String>,
    pub width: f64,
    pub height: f64,
}

/// Splits text into lines, wrapping words at `max_width` (None = only at newlines).
pub fn layout_text(text: &str, face: usize, size: f64, max_width: Option<f64>) -> Layout {
    let m = |s: &str| measure(face, s, size);
    let mut lines: Vec<String> = Vec::new();
    let mut width: f64 = 0.0;
    let mut push = |line: &str, lines: &mut Vec<String>| {
        width = width.max(m(line));
        lines.push(line.to_string());
    };
    for para in text.split('\n') {
        let Some(max) = max_width else {
            push(para, &mut lines);
            continue;
        };
        let mut line = String::new();
        for word in para.split_inclusive(char::is_whitespace) {
            let next = format!("{line}{word}");
            if m(next.trim_end()) <= max || line.is_empty() {
                line = next;
                // A single word wider than the box is broken by characters.
                while m(line.trim_end()) > max && line.chars().count() > 1 {
                    let ends: Vec<usize> = line.char_indices().map(|(i, _)| i).skip(1).collect();
                    let mut cut = ends.len();
                    while cut > 1 && m(&line[..ends[cut - 1]]) > max {
                        cut -= 1;
                    }
                    let at = ends[cut - 1];
                    push(&line[..at], &mut lines);
                    line = line[at..].to_string();
                }
            } else {
                push(line.trim_end(), &mut lines);
                line = word.to_string();
            }
        }
        push(line.trim_end(), &mut lines);
    }
    let height = lines.len() as f64 * size * LINE_HEIGHT;
    Layout { lines, width, height }
}

/* ---------------- element text ---------------- */

pub fn text_font(t: &Text) -> FontRef {
    font(t.font, t.bold, t.italic)
}

pub fn text_layout(el: &El, t: &Text) -> Layout {
    layout_text(if t.text.is_empty() { " " } else { &t.text }, text_font(t).face, t.font_size, t.fixed_width.then_some(el.w))
}

/// Fits a text box to its text: a self-sizing box fits the text, a fixed-width one only grows taller.
pub fn fit_text(el: &mut El) {
    match &el.kind {
        Kind::Table(_) => return crate::table::fit(el),
        Kind::Code(_) => return crate::code::fit(el),
        Kind::Widget(_) => return crate::widgets::fit(el),
        _ => {}
    }
    let Kind::Text(t) = &el.kind else { return };
    let l = text_layout(el, t);
    let (fixed, min_w) = (t.fixed_width, t.font_size / 6.0);
    el.h = l.height;
    if !fixed {
        // Room for the caret in an empty box, relative to the type.
        el.w = l.width.max(min_w);
    }
}

/// The marker a paragraph starts with, if it is a list item: "• " or "12. ".
fn list_marker(line: &str) -> Option<(usize, Option<u32>)> {
    if line.starts_with("• ") {
        return Some(("• ".len(), None));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && digits <= 4 && line[digits..].starts_with(". ")).then(|| (digits + 2, line[..digits].parse().ok()))
}

/// Turns every paragraph into a list item (bullets, or numbers in order), or back into plain
/// paragraphs when they all already are items of that kind.
pub fn toggle_list(text: &str, numbered: bool) -> String {
    let paras: Vec<&str> = text.split('\n').collect();
    let is_kind = |p: &str| list_marker(p).is_some_and(|(_, n)| n.is_some() == numbered);
    let all = paras.iter().all(|p| is_kind(p));
    paras
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let body = list_marker(p).map_or(*p, |(len, _)| &p[len..]);
            match (all, numbered) {
                (true, _) => body.to_string(),
                (false, true) => format!("{}. {body}", i + 1),
                (false, false) => format!("• {body}"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// After Enter at char `at` (just past the new line): a list carries on with the next marker,
/// or ends when the item left behind was empty. Returns the new text and where the cursor goes.
pub fn continue_list(text: &str, at: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if at == 0 || at > chars.len() || chars[at - 1] != '\n' {
        return None;
    }
    let start = chars[..at - 1].iter().rposition(|c| *c == '\n').map_or(0, |i| i + 1);
    let prev: String = chars[start..at - 1].iter().collect();
    let (len, n) = list_marker(&prev)?;
    if prev.len() == len {
        // An empty item: Enter ends the list there.
        let out: String = chars[..start].iter().chain(chars[at..].iter()).collect();
        return Some((out, start));
    }
    let marker = n.map_or("• ".to_string(), |n| format!("{}. ", n + 1));
    let out: String = chars[..at].iter().copied().chain(marker.chars()).chain(chars[at..].iter().copied()).collect();
    Some((out, at + marker.chars().count()))
}

pub fn show_author(s: &Sticky) -> bool {
    s.author.as_ref().is_some_and(|a| !a.is_empty()) && !s.hide_author
}

/// Height kept free at the bottom of a sticky for its author's name.
pub fn author_band(el: &El, s: &Sticky) -> f64 {
    if show_author(s) { el.h * 0.12 } else { 0.0 }
}

pub fn sticky_font(s: &Sticky) -> FontRef {
    font(s.font, s.bold, s.italic)
}

/// Largest font size (from the note's own, 28 unless chosen, down to 8) at which the sticky's
/// text fits its square.
pub fn sticky_layout(el: &El, s: &Sticky) -> (Layout, f64) {
    let max_w = (el.w - STICKY_PAD * 2.0).max(10.0);
    let max_h = (el.h - STICKY_PAD * 2.0 - author_band(el, s)).max(10.0);
    let face = sticky_font(s).face;
    let top = s.font_size.unwrap_or(28.0).clamp(4.0, 400.0);
    let floor = top.min(8.0);
    let mut size = top;
    let mut layout = layout_text(&s.text, face, size, Some(max_w));
    while size > floor && (layout.height > max_h || layout.width > max_w + 0.5) {
        size = (size * 0.9_f64).floor().max(floor);
        layout = layout_text(&s.text, face, size, Some(max_w));
    }
    (layout, size)
}

/// Colour of a sticky's text: its own, or dark (white on a dark note).
pub fn sticky_text_color(s: &Sticky) -> egui::Color32 {
    match s.text_color.as_deref().and_then(crate::model::parse_color) {
        Some(c) => c,
        None if is_dark(&s.color) => egui::Color32::WHITE,
        None => DARK,
    }
}

/// How much of a shape's box its text may use, and how far the text sits below the centre (both × size).
fn shape_text_room(kind: ShapeKind) -> (f64, f64) {
    use ShapeKind::*;
    match kind {
        Ellipse | Hexagon => (0.72, 0.0),
        Diamond => (0.55, 0.0),
        Triangle => (0.5, 0.17),
        TriangleDown => (0.5, -0.17),
        Star => (0.42, 0.06),
        Plus => (0.34, 0.0),
        Pentagon => (0.68, 0.05),
        Octagon => (0.8, 0.0),
        Parallelogram => (0.62, 0.0),
        ArrowRight | ArrowLeft => (0.5, 0.0),
        Rect | Polygon => (0.86, 0.0),
        Process => (0.7, 0.0),
        Pill => (0.78, 0.0),
        Cylinder => (0.8, 0.06),
        Document => (0.84, -0.04),
        Speech => (0.8, -0.08),
        Chevron => (0.5, 0.0),
        Trapezoid => (0.64, 0.0),
    }
}

pub struct ShapeText {
    pub layout: Layout,
    pub size: f64,
    pub top: f64,
    pub left: f64,
    pub width: f64,
}

pub fn shape_font(s: &Shape) -> FontRef {
    font(s.font.unwrap_or_default(), s.bold, s.italic)
}

/// Text inside a shape: largest size (up to 24, or the shape's own) that fits its usable area,
/// and where it starts.
pub fn shape_text_layout(el: &El, s: &Shape) -> ShapeText {
    let (k, shift) = shape_text_room(s.shape);
    let max_w = (el.w * k).max(1.0);
    let max_h = (el.h * k).max(1.0);
    let face = shape_font(s).face;
    let text = s.text.as_deref().unwrap_or("");
    let start = s.font_size.unwrap_or(24.0).clamp(1.0, 400.0).min(max_h);
    let mut size = start;
    let mut layout = layout_text(text, face, size, Some(max_w));
    while size > start * 0.15 && (layout.height > max_h || layout.width > max_w + 0.5) {
        size *= 0.9;
        layout = layout_text(text, face, size, Some(max_w));
    }
    let top = (el.h - layout.height) / 2.0 + el.h * shift;
    ShapeText { layout, size, top, left: (el.w - max_w) / 2.0, width: max_w }
}

pub fn shape_text_color(s: &Shape) -> egui::Color32 {
    if let Some(c) = s.text_color.as_deref().and_then(crate::model::parse_color) {
        return c;
    }
    if s.fill == "transparent" {
        if s.stroke == "transparent" { DARK } else { color_or(&s.stroke, DARK) }
    } else if is_dark(&s.fill) {
        egui::Color32::WHITE
    } else {
        DARK
    }
}

/// The title pill above a section's top-left corner, in board units, at the given zoom.
pub struct TitleBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub size: f64,
    pub pad: f64,
    pub text: String,
}

pub fn section_title_box(el: &El, zoom: f64) -> TitleBox {
    let size = 12.0 / zoom;
    let pad = 8.0 / zoom;
    let text = el.name.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("Sezione").to_string();
    let w = (measure(ui_medium(), &text, size) + pad * 2.0 + size * 0.25).min(el.w.max(pad * 4.0));
    let h = 22.0 / zoom;
    TitleBox { x: el.x, y: el.y - h - 4.0 / zoom, w, h, size, pad, text }
}

/* ---------------- glyphs ---------------- */

/// Glyphs placed in an element's local coordinates. A glyph outline point (px, py) in font
/// units lands at (x + (px + lean·py)·k, y − py·k).
pub struct Placed {
    pub face: usize,
    pub k: f32,
    pub lean: f32,
    pub glyphs: Vec<(u16, f32, f32)>,
}

/// Lines laid out by `layout_text`, each centred vertically on its slot (the canvas "middle"
/// baseline) and aligned within `width` starting at `left`.
#[allow(clippy::too_many_arguments)]
pub fn place_lines(lines: &[String], f: FontRef, size: f64, align: Align, width: f64, top: f64, left: f64) -> Placed {
    let info = &faces()[f.face];
    let k = size / info.upem as f64;
    // The middle of the em box sits on the line's centre.
    let shift = (info.ascender + info.descender) as f64 / 2.0 * k;
    let mut glyphs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let run = shape(f.face, line);
        let lw = run.width as f64 * k;
        let x0 = left
            + match align {
                Align::Left => 0.0,
                Align::Center => (width - lw) / 2.0,
                Align::Right => width - lw,
            };
        let baseline = top + size * LINE_HEIGHT * (i as f64 + 0.5) + shift;
        glyphs.extend(run.glyphs.iter().map(|&(g, gx, gy)| (g, (x0 + gx as f64 * k) as f32, (baseline - gy as f64 * k) as f32)));
    }
    Placed { face: f.face, k: k as f32, lean: if f.oblique { OBLIQUE } else { 0.0 }, glyphs }
}

struct Outline(Path);

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// A glyph's outline in font units (y up); empty for spaces.
pub fn glyph_path(face: usize, glyph: u16) -> Rc<Path> {
    GLYPHS.with(|g| {
        g.borrow_mut()
            .entry((face, glyph))
            .or_insert_with(|| {
                let mut o = Outline(Path::default());
                faces()[face].face.outline_glyph(ttf_parser::GlyphId(glyph), &mut o);
                Rc::new(o.0)
            })
            .clone()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_toggle_and_carry_on() {
        use super::{continue_list, toggle_list};
        assert_eq!(toggle_list("pane\nlatte", false), "• pane\n• latte");
        assert_eq!(toggle_list("• pane\n• latte", false), "pane\nlatte");
        assert_eq!(toggle_list("• pane\nlatte", true), "1. pane\n2. latte");
        // Enter after an item starts the next one, numbers counting up.
        assert_eq!(continue_list("• pane\n", 7), Some(("• pane\n• ".into(), 9)));
        assert_eq!(continue_list("9. uova\n", 8), Some(("9. uova\n10. ".into(), 12)));
        // Enter on an empty item ends the list.
        assert_eq!(continue_list("• pane\n• \n", 10), Some(("• pane\n".into(), 7)));
        assert_eq!(continue_list("pane\n", 5), None);
    }

    use super::*;

    #[test]
    fn every_font_loads_and_italics_fall_back_to_a_lean() {
        assert_eq!(faces().len(), FILES.len());
        assert_eq!(font(FontKind::Sans, true, true), FontRef { face: face_id("sans-bolditalic").unwrap(), oblique: false });
        assert_eq!(font(FontKind::Hand, false, true), FontRef { face: face_id("hand").unwrap(), oblique: true });
        assert_eq!(font(FontKind::Marker, true, false), FontRef { face: face_id("marker").unwrap(), oblique: false });
        assert!(!glyph_path(0, faces()[0].face.glyph_index('A').unwrap().0).cmds.is_empty());
    }

    #[test]
    fn wrapping_follows_words_then_characters() {
        let f = font(FontKind::Sans, false, false).face;
        let one = layout_text("Ciao mondo", f, 20.0, None);
        assert_eq!(one.lines, ["Ciao mondo"]);
        assert!((one.height - 26.0).abs() < 1e-9);
        let w = measure(f, "Ciao", 20.0);
        let two = layout_text("Ciao mondo", f, 20.0, Some(w + 1.0));
        assert_eq!(two.lines, ["Ciao", "mondo"]);
        let long = layout_text("Supercalifragilistico", f, 20.0, Some(w));
        assert!(long.lines.len() > 2 && long.lines.iter().all(|l| measure(f, l, 20.0) <= w + 1e-6));
        assert_eq!(long.lines.concat(), "Supercalifragilistico");
        assert_eq!(layout_text("a\n\nb", f, 10.0, Some(100.0)).lines, ["a", "", "b"]);
    }

    #[test]
    fn stickies_shrink_their_text_to_fit() {
        let mut el = El::new(Kind::Sticky(Sticky { text: "Una nota con parecchie parole dentro, che a ventotto punti proprio non ci stanno tutte quante".into(), ..Default::default() }));
        el.w = 220.0;
        el.h = 220.0;
        let Kind::Sticky(s) = &el.kind else { unreachable!() };
        let (l, size) = sticky_layout(&el, s);
        assert!(size < 28.0 && size >= 8.0);
        assert!(l.height <= 220.0 - 32.0);
    }
}
