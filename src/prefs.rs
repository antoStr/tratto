//! Preferences kept on this device (same fields as the first Tratto).

use serde::{Deserialize, Serialize};

use crate::model::{BoardMeta, FontKind, ShapeKind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pen {
    pub color: String,
    /// Width on screen, in px.
    pub size: f64,
    /// 0.1–1: how strong the ink is.
    pub opacity: f64,
}

impl Default for Pen {
    fn default() -> Self {
        Pen { color: "#1E1E1E".into(), size: 4.0, opacity: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EraserMode {
    /// Removes whole strokes and shapes.
    Stroke,
    /// Erases only where it passes, like Paint.
    Pixel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Eraser {
    pub mode: EraserMode,
    /// Diameter on screen, in px.
    pub size: f64,
    /// 0.1–1: how much one pass removes (pixel mode).
    pub strength: f64,
}

impl Default for Eraser {
    fn default() -> Self {
        Eraser { mode: EraserMode::Pixel, size: 24.0, strength: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeStyle {
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextStyle {
    pub color: String,
    pub font_size: f64,
    pub font: FontKind,
}

/// Shape tool choices: every shape kind plus the rounded rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeTool {
    RoundRect,
    #[serde(untagged)]
    Kind(ShapeKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Smoothing {
    Low,
    Medium,
    High,
}

impl Smoothing {
    /// Minimum cut-off (Hz) of the pen filter.
    pub fn cutoff(self) -> f64 {
        match self {
            Smoothing::Low => 7.0,
            Smoothing::Medium => 3.5,
            Smoothing::High => 1.8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Wheel {
    Pan,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolbarPos {
    Bottom,
    Top,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub name: String,
    pub theme: Theme,
    pub pens: Vec<Pen>,
    pub highlighter: Pen,
    /// Washi tape colour and width on screen.
    pub tape: Pen,
    pub eraser: Eraser,
    pub ink_to_shape: bool,
    /// How much tremor is smoothed out of freehand strokes.
    pub ink_smoothing: Smoothing,
    /// Use the pen's pressure for line width.
    pub pressure: bool,
    pub finger_draw: bool,
    pub wheel: Wheel,
    pub shape_style: ShapeStyle,
    pub text: TextStyle,
    pub sticky_color: String,
    pub stamp: String,
    pub last_shape: ShapeTool,
    /// Whole-window zoom, 1 = 100%.
    pub ui_scale: f32,
    pub high_contrast: bool,
    pub big_handles: bool,
    /// Brand colour used for selection, buttons and focus.
    pub accent: String,
    pub toolbar_pos: ToolbarPos,
    pub left_panel: bool,
    pub right_panel: bool,
    pub minimap: bool,
    /// Focus mode: both side panels hidden, only the toolbar on the board (Ctrl+\).
    pub focus: bool,
    /// Interface text size, 1 = Figma's 11 px.
    pub text_scale: f32,
    /// Background of new boards.
    pub new_board: BoardMeta,
    /// Language of the next code block.
    pub code_language: String,
    /// Path of new connectors and lines.
    pub route: crate::model::Route,
    /// Head of new arrows.
    pub arrow_head: crate::model::Head,
    /// Only fades: nothing slides, grows or bounces (Settings ▸ Accessibilità).
    pub reduce_motion: bool,
    /// Where the minimap sits: its top-left corner as a fraction of the room it can move in
    /// (None: the top right corner).
    pub minimap_at: Option<[f32; 2]>,
    /// Rows and columns of the next table.
    pub table_size: [u8; 2],
    /// This person's colour: cursor, avatar and comments ("" picks one by itself).
    pub color: String,
    pub laser_color: String,
    /// The options of the current tool show above the toolbar.
    pub tray: bool,
    /// Zen mode: nothing but the board.
    pub zen: bool,
    /// The toolbar's buttons in order (see `TOOLBAR_ITEMS`), and those left out.
    pub toolbar_order: Vec<String>,
    pub toolbar_hidden: Vec<String>,
    /// Toolbars saved under a name, to switch between them.
    pub toolbar_profiles: Vec<ToolbarProfile>,
    /// The countdown also as a banner at the top of the board (always on the timer's button).
    pub timer_banner: bool,
    /// The timer's music for this person: volume 0–1, and silenced.
    pub music_volume: f32,
    pub music_muted: bool,
    /// A chime when the timer runs out.
    pub timer_sound: bool,
    /// The "+" beside a note also draws an arrow to the new one (as for shapes); off: just a
    /// note beside it, as in FigJam.
    pub sticky_arrows: bool,
    /// The others' rulers shown on the board (off: only your own).
    pub peer_rulers: bool,
    /// New notes, code and widgets come with a shadow, this dark (0–1) and this soft (blur).
    pub shadows: bool,
    pub shadow_opacity: f64,
    pub shadow_blur: f64,
    /// The toolbar's size: 0.85 compact, 1 normal, 1.2 large (easier with a pen or fingers).
    pub toolbar_size: f32,
    /// Which default layout these preferences were last brought to (see `from_json`); missing in
    /// what older versions saved, so 0 there.
    #[serde(default)]
    pub layout: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolbarProfile {
    pub name: String,
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}

/// Every button the toolbar can have: id, name, and the group it sits with (a thin line
/// separates groups).
pub const TOOLBAR_ITEMS: [(&str, &str, u8); 21] = [
    ("select", "Seleziona", 0),
    ("hand", "Mano", 0),
    ("draw", "Penne e gomma", 1),
    ("pen", "Penna", 1),
    ("highlighter", "Evidenziatore", 1),
    ("tape", "Nastro adesivo", 1),
    ("eraser", "Gomma", 1),
    ("lasso", "Lazo", 1),
    ("ruler", "Righello", 1),
    ("sticky", "Note adesive", 1),
    ("shape", "Forme e connettori", 1),
    ("arrow", "Connettore", 1),
    ("line", "Linea", 1),
    ("text", "Testo", 2),
    ("section", "Sezione", 2),
    ("table", "Tabella", 2),
    ("stamp", "Reazioni", 2),
    ("comment", "Commento", 2),
    ("chat", "Chat del cursore", 2),
    ("laser", "Puntatore laser", 2),
    ("more", "Altri strumenti", 3),
];

/// FigJam's toolbar: the pens in one drawer, the connectors with the shapes, the chat and the
/// laser among the other tools. Shown one by one if wanted.
pub const TOOLBAR_HIDDEN: [&str; 10] = ["pen", "highlighter", "tape", "eraser", "lasso", "ruler", "arrow", "line", "chat", "laser"];

pub fn toolbar_hidden_default() -> Vec<String> {
    TOOLBAR_HIDDEN.iter().map(|s| s.to_string()).collect()
}

pub fn toolbar_default() -> Vec<String> {
    TOOLBAR_ITEMS.iter().map(|i| i.0.to_string()).collect()
}

/// The toolbar's buttons in order: the saved order, without unknown ids or repeats, and with
/// any button it doesn't know yet (added by a newer Tratto) where it is by default.
pub fn toolbar_items(order: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in order {
        if TOOLBAR_ITEMS.iter().any(|i| i.0 == id) && !out.contains(id) {
            out.push(id.clone());
        }
    }
    for (k, (id, _, _)) in TOOLBAR_ITEMS.iter().enumerate() {
        if !out.iter().any(|o| o == id) {
            // After the button that comes before it by default, if there is one.
            let at = TOOLBAR_ITEMS[..k].iter().rev().find_map(|p| out.iter().position(|o| o == p.0)).map_or(0, |i| i + 1);
            out.insert(at, id.to_string());
        }
    }
    out
}

/// FigJam's violet.
pub const DEFAULT_ACCENT: &str = "#9747FF";

/// The current default layout. 2: side panels and minimap closed (before, both panels and the
/// minimap were open, which made the board feel crowded). 3: FigJam's violet in place of the
/// old default blue.
const LAYOUT: u32 = 3;

impl Default for Prefs {
    fn default() -> Self {
        let pen = |color: &str, size: f64| Pen { color: color.into(), size, opacity: 1.0 };
        Prefs {
            name: String::new(),
            theme: Theme::System,
            pens: vec![pen("#1E1E1E", 4.0), pen("#1971C2", 4.0), pen("#E03131", 4.0), pen("#2F9E44", 8.0)],
            highlighter: pen("#FFE066", 22.0),
            tape: pen("#FFB3C7", 28.0),
            eraser: Eraser::default(),
            ink_to_shape: false,
            ink_smoothing: Smoothing::Medium,
            pressure: true,
            finger_draw: false,
            wheel: Wheel::Pan,
            shape_style: ShapeStyle { fill: "transparent".into(), stroke: "#1E1E1E".into(), stroke_width: 3.0 },
            text: TextStyle { color: "#1E1E1E".into(), font_size: 24.0, font: FontKind::Sans },
            sticky_color: "#FFF3A3".into(),
            stamp: "👍".into(),
            last_shape: ShapeTool::Kind(ShapeKind::Rect),
            ui_scale: 1.0,
            high_contrast: false,
            big_handles: false,
            accent: DEFAULT_ACCENT.into(),
            toolbar_pos: ToolbarPos::Bottom,
            // FigJam's layout: the board, the toolbar and the dark bar over the selection; the
            // side panels and the minimap open from their buttons.
            left_panel: false,
            right_panel: false,
            minimap: false,
            focus: false,
            text_scale: 1.0,
            new_board: BoardMeta::default(),
            code_language: "javascript".into(),
            route: crate::model::Route::Straight,
            arrow_head: crate::model::Head::Triangle,
            reduce_motion: false,
            minimap_at: None,
            table_size: [3, 3],
            color: String::new(),
            laser_color: "#FF3B30".into(),
            tray: true,
            zen: false,
            toolbar_order: toolbar_default(),
            toolbar_hidden: toolbar_hidden_default(),
            toolbar_profiles: Vec::new(),
            timer_banner: false,
            music_volume: 0.6,
            music_muted: false,
            timer_sound: true,
            sticky_arrows: false,
            peer_rulers: false,
            shadows: true,
            shadow_opacity: 0.14,
            shadow_blur: 10.0,
            toolbar_size: 1.0,
            layout: LAYOUT,
        }
    }
}

impl Default for ShapeStyle {
    fn default() -> Self {
        Prefs::default().shape_style
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Prefs::default().text
    }
}

impl Prefs {
    /// Reads saved preferences; anything missing or broken falls back to the defaults.
    pub fn from_json(s: &str) -> Prefs {
        let mut p: Prefs = serde_json::from_str(s).unwrap_or_default();
        if p.pens.is_empty() {
            p.pens = Prefs::default().pens;
        }
        p.pens.truncate(8);
        p.ui_scale = p.ui_scale.clamp(0.75, 2.0);
        p.text_scale = p.text_scale.clamp(0.8, 1.6);
        p.minimap_at = p.minimap_at.filter(|a| a.iter().all(|v| v.is_finite())).map(|a| a.map(|v| v.clamp(0.0, 1.0)));
        p.table_size = p.table_size.map(|v| v.clamp(1, 20));
        p.toolbar_order = toolbar_items(&p.toolbar_order);
        if crate::model::parse_color(&p.laser_color).is_none() || p.laser_color.len() != 7 {
            p.laser_color = Prefs::default().laser_color;
        }
        if !p.color.is_empty() && (p.color.len() != 7 || crate::model::parse_color(&p.color).is_none()) {
            p.color.clear();
        }
        p.toolbar_profiles.truncate(20);
        p.music_volume = if p.music_volume.is_finite() { p.music_volume.clamp(0.0, 1.0) } else { 0.6 };
        p.toolbar_size = if p.toolbar_size.is_finite() { p.toolbar_size.clamp(0.8, 1.4) } else { 1.0 };
        p.shadow_opacity = if p.shadow_opacity.is_finite() { p.shadow_opacity.clamp(0.0, 0.6) } else { 0.14 };
        p.shadow_blur = if p.shadow_blur.is_finite() { p.shadow_blur.clamp(0.0, 40.0) } else { 10.0 };
        // Once, preferences saved by an older Tratto move to the new, cleaner layout; the panels
        // reopen from their buttons and stay as chosen afterwards.
        if p.layout < 2 {
            (p.left_panel, p.right_panel, p.minimap, p.focus) = (false, false, false, false);
        }
        if p.layout < 3 && p.accent.eq_ignore_ascii_case("#0D99FF") {
            p.accent = DEFAULT_ACCENT.into();
        }
        // The toolbar as it came becomes FigJam's; one arranged by hand stays as it was.
        if p.layout < 3 && p.toolbar_hidden.is_empty() {
            p.toolbar_order = toolbar_default();
            p.toolbar_hidden = toolbar_hidden_default();
        }
        p.layout = LAYOUT;
        p
    }

    /// This person's colour, if they chose one.
    pub fn own_color(&self) -> Option<&str> {
        (!self.color.is_empty()).then_some(self.color.as_str())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The colour of the selection on the board: FigJam's blue with its violet, otherwise the
    /// colour chosen.
    pub fn selection_color(&self) -> egui::Color32 {
        if self.accent.eq_ignore_ascii_case(DEFAULT_ACCENT) { egui::Color32::from_rgb(0x0D, 0x99, 0xFF) } else { self.accent() }
    }

    pub fn accent(&self) -> egui::Color32 {
        crate::model::parse_color(&self.accent).filter(|_| self.accent.len() == 7).unwrap_or(egui::Color32::from_rgb(0x97, 0x47, 0xFF))
    }
}

/// Quick picks shown next to the size sliders.
pub const PEN_SIZES: [f64; 4] = [2.0, 4.0, 8.0, 14.0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_old_fields_fall_back_to_defaults() {
        let p = Prefs::from_json(r##"{"theme":"dark","pens":[{"color":"#E03131","size":6}],"eraser":{"mode":"pixel"},"lastShape":"roundRect","bogus":1}"##);
        assert_eq!(p.theme, Theme::Dark);
        assert_eq!(p.pens[0].opacity, 1.0);
        assert_eq!(p.eraser.size, 24.0);
        assert_eq!(p.last_shape, ShapeTool::RoundRect);
        assert_eq!(Prefs::from_json(r#"{"lastShape":"star"}"#).last_shape, ShapeTool::Kind(ShapeKind::Star));
        assert_eq!(Prefs::from_json("not json"), Prefs::default());
        let round = Prefs::from_json(&Prefs::default().to_json());
        assert_eq!(round, Prefs::default());
    }

    #[test]
    fn older_preferences_move_once_to_the_clean_layout() {
        let old = Prefs::from_json(r#"{"leftPanel":true,"rightPanel":true,"minimap":true}"#);
        assert!(!old.left_panel && !old.right_panel && !old.minimap);
        let reopened = Prefs { left_panel: true, ..old };
        assert!(Prefs::from_json(&reopened.to_json()).left_panel, "a panel opened afterwards stays open");
        // The old default blue becomes FigJam's violet, once; a colour chosen stays.
        let blue = Prefs::from_json(r##"{"accent":"#0D99FF","layout":2,"leftPanel":true}"##);
        assert_eq!((blue.accent.as_str(), blue.left_panel), (DEFAULT_ACCENT, true));
        let green = Prefs::from_json(r##"{"accent":"#14AE5C","layout":2}"##);
        assert_eq!(green.accent, "#14AE5C");
        let chosen = Prefs { accent: "#0D99FF".into(), ..blue };
        assert_eq!(Prefs::from_json(&chosen.to_json()).accent, "#0D99FF", "blue picked again stays");
    }

    #[test]
    fn a_saved_toolbar_keeps_its_order_and_gains_new_buttons() {
        let order: Vec<String> = ["laser", "pen", "bogus", "pen", "select"].iter().map(|s| s.to_string()).collect();
        let items = toolbar_items(&order);
        assert_eq!(items.len(), TOOLBAR_ITEMS.len());
        // The saved ones keep their order; "highlighter" comes after "pen" by default, so it
        // lands right after it.
        let at = |id: &str| items.iter().position(|i| i == id).unwrap();
        assert!(at("laser") < at("pen") && at("pen") < at("select"));
        assert_eq!(at("highlighter"), at("pen") + 1);
        assert_eq!(items[0], "laser");
    }

}
