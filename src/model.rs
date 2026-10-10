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
    /// Flowchart start and end: a rectangle with round ends.
    Pill,
    /// Database.
    Cylinder,
    /// A page with a wavy bottom.
    Document,
    Speech,
    Chevron,
    Trapezoid,
    /// Predefined process: a box with a bar at each side.
    Process,
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
    /// Largest size of the text: it shrinks below it to fit (None: up to 24).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Colour of the text (None: dark, or white on a dark fill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    /// Alignment of the text (None: centred).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { shape: ShapeKind::Rect, fill: "transparent".into(), stroke: "#1E1E1E".into(), stroke_width: 3.0, radius: 0.0, dash: false, points: None, text: None, font: None, font_size: None, bold: false, italic: false, text_color: None, align: None }
    }
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
    #[serde(default, skip_serializing_if = "Route::is_straight")]
    pub route: Route,
    /// Text in the middle of the line, on a small card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The path a connector takes between its ends, like FigJam's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Route {
    #[default]
    Straight,
    /// Right angles, rounded.
    Elbow,
    Curved,
}

impl Route {
    pub fn is_straight(&self) -> bool {
        *self == Route::Straight
    }
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strike: bool,
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
    /// Largest size of the text: it shrinks below it to fit (None: up to 28, as in FigJam).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Colour of the text (None: dark, or white on a dark note).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    /// Corner radius (None: 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

impl Default for Sticky {
    fn default() -> Self {
        Sticky { text: String::new(), color: "#FFF3A3".into(), font: FontKind::Sans, align: Align::Center, author: None, hide_author: false, font_size: None, bold: false, italic: false, text_color: None, radius: None }
    }
}

/// A table: rows × columns of text cells. `cols` are the column widths; `rows` the row heights,
/// which grow to fit their tallest cell. The element's size is their sum.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub cols: Vec<f64>,
    pub rows: Vec<f64>,
    /// `cells[row][col]`.
    pub cells: Vec<Vec<Cell>>,
    /// First row in bold on a tinted background.
    #[serde(default)]
    pub header: bool,
    #[serde(default)]
    pub font: FontKind,
    #[serde(default = "cell_font_size")]
    pub font_size: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Colour of the text (None: dark, or white on a dark cell).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// Corner radius (None: 6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

fn cell_font_size() -> f64 {
    16.0
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
}

pub const TABLE_ROW: f64 = 44.0;
pub const TABLE_COL: f64 = 160.0;
pub const CELL_PAD: f64 = 10.0;

impl Table {
    pub fn new(rows: usize, cols: usize) -> Table {
        Table { cols: vec![TABLE_COL; cols], rows: vec![TABLE_ROW; rows], cells: vec![vec![Cell::default(); cols]; rows], header: true, font: FontKind::Sans, font_size: cell_font_size(), bold: false, italic: false, color: None, align: None, radius: None }
    }
    /// Left edge of each column and top edge of each row, plus the far edge.
    pub fn col_edges(&self) -> Vec<f64> {
        std::iter::once(0.0).chain(self.cols.iter().scan(0.0, |x, w| {
            *x += w;
            Some(*x)
        })).collect()
    }
    pub fn row_edges(&self) -> Vec<f64> {
        std::iter::once(0.0).chain(self.rows.iter().scan(0.0, |y, h| {
            *y += h;
            Some(*y)
        })).collect()
    }
    /// Cell under a point in the table's own coordinates.
    pub fn cell_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let c = self.col_edges().windows(2).position(|e| x >= e[0] && x < e[1])?;
        let r = self.row_edges().windows(2).position(|e| y >= e[0] && y < e[1])?;
        Some((r, c))
    }
    pub fn insert_row(&mut self, at: usize) {
        let at = at.min(self.rows.len());
        self.rows.insert(at, TABLE_ROW);
        self.cells.insert(at, vec![Cell::default(); self.cols.len()]);
    }
    pub fn insert_col(&mut self, at: usize) {
        let at = at.min(self.cols.len());
        self.cols.insert(at, TABLE_COL);
        for row in &mut self.cells {
            row.insert(at.min(row.len()), Cell::default());
        }
    }
    pub fn remove_row(&mut self, at: usize) {
        if self.rows.len() > 1 && at < self.rows.len() {
            self.rows.remove(at);
            self.cells.remove(at);
        }
    }
    pub fn remove_col(&mut self, at: usize) {
        if self.cols.len() > 1 && at < self.cols.len() {
            self.cols.remove(at);
            for row in &mut self.cells {
                if at < row.len() {
                    row.remove(at);
                }
            }
        }
    }
}

/// A block of code, in a monospaced font with its syntax coloured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Code {
    pub code: String,
    /// One of `crate::code::LANGUAGES` (empty: plain text).
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub light: bool,
    #[serde(default = "code_font_size")]
    pub font_size: f64,
    /// Corner radius (None: a little under the text size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

fn code_font_size() -> f64 {
    14.0
}

/// FigJam-style widgets: small interactive tools living on the board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "widget", rename_all = "camelCase")]
pub enum Widget {
    /// Everyone votes for one option (voters are device ids).
    Poll { question: String, options: Vec<PollOption> },
    Checklist { title: String, items: Vec<CheckItem> },
    Counter { label: String, value: i64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollOption {
    pub text: String,
    #[serde(default)]
    pub votes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

pub const WIDGET_W: f64 = 320.0;

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
    Table(Table),
    Code(Code),
    Widget(Widget),
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
            Kind::Table(_) => "table",
            Kind::Code(_) => "code",
            Kind::Widget(_) => "widget",
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
    pub fn table(&self) -> Option<&Table> {
        match &self.kind {
            Kind::Table(t) => Some(t),
            _ => None,
        }
    }
    pub fn code(&self) -> Option<&Code> {
        match &self.kind {
            Kind::Code(c) => Some(c),
            _ => None,
        }
    }
    pub fn widget(&self) -> Option<&Widget> {
        match &self.kind {
            Kind::Widget(w) => Some(w),
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
            Kind::Line(l) => l.points.len() == 4 && l.points.iter().all(|v| v.is_finite()) && num(l.stroke_width) && opt_id(&l.from) && opt_id(&l.to) && l.label.as_ref().is_none_or(|t| t.len() <= 1000),
            Kind::Comment { thread } => thread.len() <= 500 && thread.iter().all(|m| m.text.len() <= 4000 && m.author.len() <= 64 && num(m.t)),
            Kind::Shape(s) => {
                num(s.stroke_width) && num(s.radius) && s.text.as_ref().is_none_or(|t| t.len() < 10_000) && s.points.as_ref().is_none_or(|p| p.len() <= 2000 && p.iter().all(|v| v.is_finite())) && s.font_size.is_none_or(num) && s.text_color.as_ref().is_none_or(|c| c.len() <= 32)
            }
            Kind::Text(t) => t.text.len() < 100_000 && num(t.font_size),
            Kind::Sticky(s) => s.text.len() < 100_000 && s.author.as_ref().is_none_or(|a| a.len() <= 64) && s.font_size.is_none_or(num) && s.radius.is_none_or(num) && s.text_color.as_ref().is_none_or(|c| c.len() <= 32),
            Kind::Section { .. } => true,
            Kind::Image { file_id } => valid_file_id(file_id),
            Kind::Stamp { emoji } => emoji.len() <= 16,
            Kind::Table(t) => {
                let cells = t.rows.len() * t.cols.len();
                (1..=200).contains(&t.rows.len())
                    && (1..=50).contains(&t.cols.len())
                    && t.cells.len() == t.rows.len()
                    && t.cells.iter().all(|r| r.len() == t.cols.len() && r.iter().all(|c| c.text.len() <= 20_000 && c.fill.as_ref().is_none_or(|f| f.len() <= 32)))
                    && cells <= 5000
                    && t.cols.iter().chain(&t.rows).all(|v| v.is_finite() && *v > 0.0)
                    && num(t.font_size)
                    && t.radius.is_none_or(num)
                    && t.color.as_ref().is_none_or(|c| c.len() <= 32)
            }
            Kind::Code(c) => c.code.len() < 200_000 && c.language.len() <= 32 && num(c.font_size) && c.radius.is_none_or(num),
            Kind::Widget(w) => match w {
                Widget::Poll { question, options } => question.len() <= 1000 && options.len() <= 50 && options.iter().all(|o| o.text.len() <= 1000 && o.votes.len() <= 1000 && o.votes.iter().all(|v| v.len() <= 64)),
                Widget::Checklist { title, items } => title.len() <= 1000 && items.len() <= 200 && items.iter().all(|i| i.text.len() <= 2000),
                Widget::Counter { label, .. } => label.len() <= 1000,
            },
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
