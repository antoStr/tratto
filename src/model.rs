//! Board elements, stored as plain JSON objects in the shared document. The field names are
//! those of the first (TypeScript) Tratto, so boards made with it open unchanged.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Triangle,
    TriangleDown,
    Diamond,
    Parallelogram,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
    Plus,
    ArrowRight,
    ArrowLeft,
    Polygon,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontKind {
    #[default]
    Sans,
    Rounded,
    Geometric,
    Condensed,
    Serif,
    Book,
    Display,
    Mono,
    Hand,
    Print,
    Marker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// One pass of the pixel eraser: polyline `p` (flat [x, y, …]) in the element's own
/// coordinates, width `s`, strength `a` (0–1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EraseMark {
    pub p: Vec<f32>,
    pub s: f64,
    pub a: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentMsg {
    pub author: String,
    pub color: String,
    pub text: String,
    /// Time written, ms since 1970.
    pub t: f64,
}

/// Freehand stroke. `points` is flat [x, y, pressure, …] relative to (x, y); pressure < 0
/// means the device had none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ink {
    pub points: Vec<f32>,
    pub color: String,
    pub size: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub shape: ShapeKind,
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub dash: bool,
    /// Only for polygons: flat [x, y, …] normalised to 0..1 inside the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<f32>>,
    /// Text written inside the shape, centred and shrunk to fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontKind>,
}

/// Straight line, arrow or connector. `points` is [x1, y1, x2, y2] relative to (x, y).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub points: Vec<f32>,
    pub stroke: String,
    pub stroke_width: f64,
    #[serde(default)]
    pub dash: bool,
    #[serde(default)]
    pub arrow_start: bool,
    #[serde(default)]
    pub arrow_end: bool,
    /// Ids of the elements the ends are attached to: they follow them when they move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Washi tape: a striped band `stroke_width` wide instead of a line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tape: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    pub text: String,
    pub color: String,
    pub font_size: f64,
    #[serde(default)]
    pub font: FontKind,
    #[serde(default)]
    pub align: Align,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    /// When false the box grows with the text; when true it wraps at `w`.
    #[serde(default)]
    pub fixed_width: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sticky {
    pub text: String,
    pub color: String,
    #[serde(default)]
    pub font: FontKind,
    #[serde(default)]
    pub align: Align,
    /// Who wrote it, shown in a corner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_author: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Kind {
    Ink(Ink),
    Highlighter(Ink),
    Shape(Shape),
    Line(Line),
    Text(Text),
    Sticky(Sticky),
    #[serde(rename_all = "camelCase")]
    Image { file_id: String },
    Stamp { emoji: String },
    /// A named, coloured area; moving it moves what is inside. The title is `name`.
    Section { fill: String },
    /// A comment pin: its point is (x, y); w and h are 0.
    Comment { thread: Vec<CommentMsg> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct El {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub rotation: f64,
    pub z: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Folder (layers panel) the element belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Pixel eraser marks: they move, turn and scale with the element.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub erase: Vec<EraseMark>,
    #[serde(flatten)]
    pub kind: Kind,
}

fn one() -> f64 {
    1.0
}

impl El {
    pub fn new(kind: Kind) -> Self {
        El { id: uid(), x: 0.0, y: 0.0, w: 0.0, h: 0.0, rotation: 0.0, z: 0.0, opacity: 1.0, locked: false, hidden: false, name: None, group_id: None, erase: Vec::new(), kind }
    }

    pub fn type_name(&self) -> &'static str {
        match self.kind {
            Kind::Ink(_) => "ink",
            Kind::Highlighter(_) => "highlighter",
            Kind::Shape(_) => "shape",
            Kind::Line(_) => "line",
            Kind::Text(_) => "text",
            Kind::Sticky(_) => "sticky",
            Kind::Image { .. } => "image",
            Kind::Stamp { .. } => "stamp",
            Kind::Section { .. } => "section",
            Kind::Comment { .. } => "comment",
        }
    }

    pub fn ink(&self) -> Option<&Ink> {
        match &self.kind {
            Kind::Ink(i) | Kind::Highlighter(i) => Some(i),
            _ => None,
        }
    }
    pub fn ink_mut(&mut self) -> Option<&mut Ink> {
        match &mut self.kind {
            Kind::Ink(i) | Kind::Highlighter(i) => Some(i),
            _ => None,
        }
    }
    pub fn line(&self) -> Option<&Line> {
        match &self.kind {
            Kind::Line(l) => Some(l),
            _ => None,
        }
    }
    pub fn line_mut(&mut self) -> Option<&mut Line> {
        match &mut self.kind {
            Kind::Line(l) => Some(l),
            _ => None,
        }
    }
    pub fn shape(&self) -> Option<&Shape> {
        match &self.kind {
            Kind::Shape(s) => Some(s),
            _ => None,
        }
    }
    pub fn text(&self) -> Option<&Text> {
        match &self.kind {
            Kind::Text(t) => Some(t),
            _ => None,
        }
    }
    pub fn sticky(&self) -> Option<&Sticky> {
        match &self.kind {
            Kind::Sticky(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_ink(&self) -> bool {
        matches!(self.kind, Kind::Ink(_) | Kind::Highlighter(_))
    }
    pub fn is_section(&self) -> bool {
        matches!(self.kind, Kind::Section { .. })
    }
    pub fn is_comment(&self) -> bool {
        matches!(self.kind, Kind::Comment { .. })
    }
    pub fn is_line(&self) -> bool {
        matches!(self.kind, Kind::Line(_))
    }

    /// Elements come from other people's machines: anything malformed is refused instead of
    /// breaking rendering. Image ids are checked so a guest can't point others at an outside URL.
    pub fn is_valid(&self) -> bool {
        let num = |v: f64| v.is_finite();
        let opt_id = |v: &Option<String>| v.as_ref().is_none_or(|s| s.len() <= 64);
        if self.id.is_empty() || self.id.len() > 64 || !opt_id(&self.group_id) {
            return false;
        }
        if ![self.x, self.y, self.w, self.h, self.rotation, self.z, self.opacity].into_iter().all(num) {
            return false;
        }
        if self.erase.len() > 4000 || !self.erase.iter().all(|m| m.p.len() >= 2 && m.p.len() <= 20_000 && num(m.s) && num(m.a) && m.p.iter().all(|v| v.is_finite())) {
            return false;
        }
        if self.name.as_ref().is_some_and(|n| n.len() > 400) {
            return false;
        }
        match &self.kind {
            Kind::Ink(i) | Kind::Highlighter(i) => i.points.len() >= 3 && i.points.len() < 200_000 && num(i.size) && i.points.iter().all(|v| v.is_finite()),
            Kind::Line(l) => l.points.len() == 4 && l.points.iter().all(|v| v.is_finite()) && num(l.stroke_width) && opt_id(&l.from) && opt_id(&l.to),
            Kind::Comment { thread } => thread.len() <= 500 && thread.iter().all(|m| m.text.len() <= 4000 && m.author.len() <= 64 && num(m.t)),
            Kind::Shape(s) => {
                num(s.stroke_width) && num(s.radius) && s.text.as_ref().is_none_or(|t| t.len() < 10_000) && s.points.as_ref().is_none_or(|p| p.len() <= 2000 && p.iter().all(|v| v.is_finite()))
            }
            Kind::Text(t) => t.text.len() < 100_000 && num(t.font_size),
            Kind::Sticky(s) => s.text.len() < 100_000 && s.author.as_ref().is_none_or(|a| a.len() <= 64),
            Kind::Section { .. } => true,
            Kind::Image { file_id } => valid_file_id(file_id),
            Kind::Stamp { emoji } => emoji.len() <= 16,
        }
    }
}

pub fn valid_file_id(id: &str) -> bool {
    (6..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A random id like the ones the first Tratto made (base 36).
pub fn uid() -> String {
    let mut bytes = [0u8; 12];
    crate::platform::random(&mut bytes);
    let mut out = String::with_capacity(21);
    for chunk in bytes.chunks(4) {
        let mut n = u32::from_le_bytes(chunk.try_into().unwrap());
        let mut digits = [b'0'; 7];
        for d in digits.iter_mut().rev() {
            *d = b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize];
            n /= 36;
        }
        out.push_str(std::str::from_utf8(&digits).unwrap());
    }
    out
}

/* ---------------- board settings ---------------- */

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    None,
    #[default]
    Dots,
    Grid,
    Lines,
    Graph,
    Isometric,
}
pub const PATTERNS: [Pattern; 6] = [Pattern::None, Pattern::Dots, Pattern::Grid, Pattern::Lines, Pattern::Graph, Pattern::Isometric];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardMeta {
    pub background: String,
    pub pattern: Pattern,
    /// Spacing of the pattern in board units.
    pub grid_size: f64,
}

impl Default for BoardMeta {
    fn default() -> Self {
        BoardMeta { background: "#F5F5F5".into(), pattern: Pattern::Dots, grid_size: 24.0 }
    }
}

pub const GRID_SIZES: [f64; 3] = [12.0, 24.0, 48.0];

pub const INK_COLORS: [&str; 12] = ["#1E1E1E", "#757575", "#FFFFFF", "#E03131", "#F76707", "#F5B700", "#2F9E44", "#0C8599", "#1971C2", "#6741D9", "#C2255C", "#8B5A2B"];
pub const HIGHLIGHT_COLORS: [&str; 6] = ["#FFE066", "#8CE99A", "#74C0FC", "#FCC2D7", "#FFC078", "#D0BFFF"];
pub const STICKY_COLORS: [&str; 7] = ["#FFF3A3", "#C9F2C7", "#C7E5FF", "#FFD1E3", "#E2D4FF", "#FFDDB8", "#E9E9E9"];
pub const TAPE_COLORS: [&str; 8] = ["#FFB3C7", "#FFD8A8", "#FFEC99", "#B2F2BB", "#A5D8FF", "#D0BFFF", "#CED4DA", "#F8F9FA"];
pub const SECTION_COLORS: [&str; 8] = ["#FFFFFF", "#F2F2F2", "#FFF8D6", "#E6F6E5", "#E3F1FF", "#FCE8F0", "#EFE9FF", "#FFEEDD"];
pub const BACKGROUNDS: [(&str, &str); 6] = [
    ("Grigio chiaro", "#F5F5F5"),
    ("Bianco", "#FFFFFF"),
    ("Carta", "#FAF7F0"),
    ("Ardesia", "#26292E"),
    ("Lavagna verde", "#23392F"),
    ("Blu notte", "#1B2A41"),
];

/// The font menu, in order: kind, name shown, group.
pub const FONTS: [(FontKind, &str, &str); 11] = [
    (FontKind::Sans, "Inter", "Senza grazie"),
    (FontKind::Rounded, "Nunito", "Senza grazie"),
    (FontKind::Geometric, "Montserrat", "Senza grazie"),
    (FontKind::Condensed, "Oswald", "Senza grazie"),
    (FontKind::Serif, "Gelasio", "Con grazie"),
    (FontKind::Book, "Lora", "Con grazie"),
    (FontKind::Display, "Playfair Display", "Con grazie"),
    (FontKind::Hand, "Caveat", "Scritti a mano"),
    (FontKind::Print, "Patrick Hand", "Scritti a mano"),
    (FontKind::Marker, "Permanent Marker", "Scritti a mano"),
    (FontKind::Mono, "JetBrains Mono", "Altri"),
];

/* ---------------- colours ---------------- */

/// Colour from shared data: only #rgb, #rrggbb, #rrggbbaa and "transparent" are accepted.
pub fn parse_color(c: &str) -> Option<egui::Color32> {
    if c == "transparent" {
        return Some(egui::Color32::TRANSPARENT);
    }
    let hex = c.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
    match hex.len() {
        3 | 4 => {
            let d = |i: usize| v(i, 1).map(|x| x * 17);
            Some(egui::Color32::from_rgba_unmultiplied(d(0)?, d(1)?, d(2)?, if hex.len() == 4 { d(3)? } else { 255 }))
        }
        6 | 8 => Some(egui::Color32::from_rgba_unmultiplied(v(0, 2)?, v(2, 2)?, v(4, 2)?, if hex.len() == 8 { v(6, 2)? } else { 255 })),
        _ => None,
    }
}

pub fn color_or(c: &str, fallback: egui::Color32) -> egui::Color32 {
    parse_color(c).unwrap_or(fallback)
}

pub const DARK: egui::Color32 = egui::Color32::from_rgb(0x1E, 0x1E, 0x1E);

pub fn is_dark(c: &str) -> bool {
    parse_color(c).is_some_and(|c| (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0 < 0.5)
}

pub fn hex(c: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_written_by_the_first_tratto() {
        let json = r##"{"id":"abc","type":"shape","shape":"rect","x":1,"y":2,"w":3,"h":4,"rotation":0,"z":5,"opacity":1,"fill":"transparent","stroke":"#1E1E1E","strokeWidth":3,"radius":0,"dash":false,"locked":false}"##;
        let el: El = serde_json::from_str(json).unwrap();
        assert!(el.is_valid());
        assert!(matches!(el.kind, Kind::Shape(Shape { shape: ShapeKind::Rect, .. })));
        let back = serde_json::to_value(&el).unwrap();
        assert_eq!(back["strokeWidth"], 3.0);
        assert_eq!(back["type"], "shape");
        assert!(back.get("locked").is_none());
        let img: El = serde_json::from_str(r#"{"id":"i","type":"image","fileId":"../evil","x":0,"y":0,"w":1,"h":1,"rotation":0,"z":0,"opacity":1}"#).unwrap();
        assert!(!img.is_valid());
    }

    #[test]
    fn colours() {
        assert_eq!(parse_color("#fff"), Some(egui::Color32::WHITE));
        assert_eq!(parse_color("#0D99FF"), Some(egui::Color32::from_rgb(13, 153, 255)));
        assert_eq!(parse_color("red"), None);
        assert_eq!(parse_color("url(x)"), None);
        assert!(is_dark("#1E1E1E") && !is_dark("#FFF3A3"));
        assert_eq!(uid().len(), 21);
    }
}
