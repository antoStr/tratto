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
}

pub const DEFAULT_ACCENT: &str = "#0D99FF";

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
            left_panel: true,
            right_panel: true,
            minimap: true,
            focus: false,
            text_scale: 1.0,
            new_board: BoardMeta::default(),
            code_language: "javascript".into(),
            route: crate::model::Route::Straight,
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
        p
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn accent(&self) -> egui::Color32 {
        crate::model::parse_color(&self.accent).filter(|_| self.accent.len() == 7).unwrap_or(egui::Color32::from_rgb(0x0D, 0x99, 0xFF))
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
}
