//! The look of Figma UI3 (Inter 11 px, 24 px controls, 5 px radii, dark menus and tooltips),
//! with the WCAG AA adjustments of the first Tratto, and the small widgets built on it.

pub mod icon_data;
pub mod icons;

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Response, RichText, Sense, Stroke, TextStyle, Ui, Vec2, pos2, vec2};

pub use icons::icon;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub canvas: Color32,
    pub bg: Color32,
    pub bg2: Color32,
    pub bg3: Color32,
    pub hover: Color32,
    pub press: Color32,
    pub selected: Color32,
    pub selected_strong: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub icon: Color32,
    pub icon2: Color32,
    pub brand: Color32,
    /// Filled surfaces with white text: the brand colour darkened until it passes WCAG AA.
    pub brand_fill: Color32,
    pub brand_fill_hover: Color32,
    pub danger: Color32,
    pub danger_text: Color32,
    /// Menus, tooltips and the desktop tab bar are dark in both themes.
    pub menu: Color32,
    pub menu_text: Color32,
    pub menu_text2: Color32,
    pub menu_sep: Color32,
    pub chrome: Color32,
    pub chrome2: Color32,
    pub chrome_text: Color32,
    pub chrome_text2: Color32,
}

fn black(a: f32) -> Color32 {
    Color32::from_black_alpha((a * 255.0).round() as u8)
}
fn white(a: f32) -> Color32 {
    Color32::from_white_alpha((a * 255.0).round() as u8)
}
fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
/// `color-mix(in srgb, c k, other)`.
pub fn mix(c: Color32, k: f32, other: Color32) -> Color32 {
    let m = |a: u8, b: u8| (a as f32 * k + b as f32 * (1.0 - k)).round() as u8;
    Color32::from_rgb(m(c.r(), other.r()), m(c.g(), other.g()), m(c.b(), other.b()))
}

impl Theme {
    pub fn new(dark: bool, accent: Color32, high_contrast: bool) -> Theme {
        let custom = accent != hex(0x0D99FF);
        let mut t = if dark {
            Theme {
                dark,
                canvas: hex(0x1E1E1E),
                bg: hex(0x2C2C2C),
                bg2: hex(0x383838),
                bg3: hex(0x444444),
                hover: hex(0x383838),
                press: hex(0x444444),
                selected: if custom { mix(accent, 0.35, hex(0x2C2C2C)) } else { hex(0x4A5878) },
                selected_strong: if custom { mix(accent, 0.5, hex(0x2C2C2C)) } else { hex(0x5B6A8C) },
                border: hex(0x444444),
                border_strong: hex(0x4F4F4F),
                text: Color32::WHITE,
                text2: white(0.7),
                text3: white(0.4),
                icon: Color32::WHITE,
                icon2: white(0.7),
                brand: accent,
                brand_fill: mix(accent, 0.76, Color32::BLACK),
                brand_fill_hover: mix(accent, 0.68, Color32::BLACK),
                danger: hex(0xF24822),
                danger_text: hex(0xFF7A5E),
                menu: hex(0x1E1E1E),
                menu_text: Color32::WHITE,
                menu_text2: white(0.5),
                menu_sep: hex(0x383838),
                chrome: hex(0x2C2C2C),
                chrome2: hex(0x383838),
                chrome_text: white(0.9),
                chrome_text2: white(0.5),
            }
        } else {
            Theme {
                dark,
                canvas: hex(0xF5F5F5),
                bg: Color32::WHITE,
                bg2: hex(0xF5F5F5),
                bg3: hex(0xE6E6E6),
                hover: hex(0xF5F5F5),
                press: hex(0xE6E6E6),
                selected: if custom { mix(accent, 0.12, Color32::WHITE) } else { hex(0xE5F4FF) },
                selected_strong: if custom { mix(accent, 0.3, Color32::WHITE) } else { hex(0xBDE3FF) },
                border: hex(0xE6E6E6),
                border_strong: hex(0xD9D9D9),
                text: black(0.9),
                // 0.58 instead of Figma's 0.5: secondary text needs 4.5:1 (WCAG AA).
                text2: black(0.58),
                text3: black(0.3),
                icon: black(0.9),
                icon2: black(0.5),
                brand: accent,
                brand_fill: mix(accent, 0.76, Color32::BLACK),
                brand_fill_hover: mix(accent, 0.68, Color32::BLACK),
                danger: hex(0xF24822),
                danger_text: hex(0xC8331A),
                menu: hex(0x1E1E1E),
                menu_text: Color32::WHITE,
                menu_text2: white(0.5),
                menu_sep: hex(0x383838),
                chrome: hex(0x2C2C2C),
                chrome2: hex(0x383838),
                chrome_text: white(0.9),
                chrome_text2: white(0.5),
            }
        };
        if high_contrast {
            if dark {
                (t.text2, t.text3, t.icon2, t.border, t.border_strong, t.hover) = (white(0.9), white(0.72), white(0.9), hex(0x8A8A8A), hex(0xB3B3B3), hex(0x4D4D4D));
            } else {
                (t.text2, t.text3, t.icon2, t.border, t.border_strong, t.hover) = (black(0.8), black(0.62), black(0.8), hex(0x8C8C8C), hex(0x595959), hex(0xE0E0E0));
            }
            t.menu_text2 = white(0.8);
            t.chrome_text2 = white(0.8);
        }
        t
    }
}

/// The theme of this frame (set by `setup`).
pub fn theme(ctx: &egui::Context) -> Theme {
    ctx.data(|d| d.get_temp::<Theme>(Id::NULL)).unwrap_or_else(|| Theme::new(false, hex(0x0D99FF), false))
}

pub const RADIUS: u8 = 5;
pub const RADIUS_LG: u8 = 13;

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

/// Interface text sizes, scaled by Settings › Dimensione del testo.
pub struct Sizes {
    pub fs: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

pub fn sizes(ctx: &egui::Context) -> Sizes {
    let k = ctx.data(|d| d.get_temp::<f32>(Id::new("text-scale"))).unwrap_or(1.0);
    Sizes { fs: 11.0 * k, md: 12.0 * k, lg: 13.0 * k, xl: 15.0 * k }
}

/// Inter for the interface; every board font, for typing on the board.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::empty();
    for (name, data) in crate::text::FILES.iter() {
        defs.font_data.insert(name.to_string(), Arc::new(FontData::from_static(data)));
        defs.families.insert(FontFamily::Name((*name).into()), vec![name.to_string(), "ui".into()]);
    }
    defs.families.insert(FontFamily::Proportional, vec!["ui".into(), "sans".into()]);
    defs.families.insert(FontFamily::Monospace, vec!["mono".into(), "ui".into()]);
    defs.families.insert(FontFamily::Name("medium".into()), vec!["ui-medium".into(), "ui".into()]);
    ctx.set_fonts(defs);
}

/// Applies the theme to egui's own widgets. Popups (menus, tooltips) are dark in both themes.
pub fn setup(ctx: &egui::Context, t: Theme, text_scale: f32) {
    ctx.data_mut(|d| {
        d.insert_temp(Id::NULL, t);
        d.insert_temp(Id::new("text-scale"), text_scale);
    });
    let s = sizes(ctx);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(10.0 * text_scale)),
            (TextStyle::Body, FontId::proportional(s.fs)),
            (TextStyle::Button, FontId::proportional(s.fs)),
            (TextStyle::Heading, medium(s.xl)),
            (TextStyle::Monospace, FontId::monospace(s.fs)),
        ]
        .into();
        let sp = &mut style.spacing;
        sp.item_spacing = vec2(8.0, 4.0);
        sp.button_padding = vec2(8.0, 4.0);
        sp.interact_size = vec2(24.0, 24.0);
        sp.window_margin = egui::Margin::same(16);
        sp.menu_margin = egui::Margin::same(8);
        sp.icon_width = 14.0;
        sp.text_edit_width = 160.0;
        sp.scroll.bar_width = 6.0;
        sp.scroll.floating = true;
        let v = &mut style.visuals;
        *v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = t.bg;
        v.window_fill = t.menu;
        v.window_stroke = Stroke::NONE;
        v.window_corner_radius = CornerRadius::same(RADIUS_LG);
        v.menu_corner_radius = CornerRadius::same(RADIUS_LG);
        v.faint_bg_color = t.bg2;
        v.extreme_bg_color = t.bg2;
        v.text_edit_bg_color = Some(t.bg2);
        v.hyperlink_color = t.brand;
        v.selection.bg_fill = t.selected_strong;
        v.selection.stroke = Stroke::new(1.0, t.brand);
        v.text_cursor.stroke = Stroke::new(1.5, t.brand);
        v.popup_shadow = egui::Shadow { offset: [0, 10], blur: 16, spread: 0, color: black(0.15) };
        v.window_shadow = egui::Shadow { offset: [0, 18], blur: 48, spread: 0, color: black(0.18) };
        for (w, fill) in [(&mut v.widgets.noninteractive, t.bg), (&mut v.widgets.inactive, t.bg2), (&mut v.widgets.hovered, t.hover), (&mut v.widgets.active, t.press), (&mut v.widgets.open, t.press)] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.corner_radius = CornerRadius::same(RADIUS);
            w.fg_stroke = Stroke::new(1.0, t.text);
            w.bg_stroke = Stroke::NONE;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text2);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.border);
        v.widgets.active.bg_stroke = Stroke::new(1.0, t.brand);
        v.interact_cursor = Some(egui::CursorIcon::Default);
        v.button_frame = true;
    });
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

/* ---------------- widgets ---------------- */

/// A dark tooltip with an optional shortcut, like Figma's.
pub fn tip(resp: Response, label: &str, kbd: Option<&str>) -> Response {
    if label.is_empty() {
        return resp;
    }
    let t = theme(&resp.ctx);
    resp.on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).color(t.menu_text));
            if let Some(k) = kbd {
                ui.label(RichText::new(k).color(t.menu_text2));
            }
        });
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
    /// Red text on the panel colour.
    DangerText,
}

/// A text button, 24 px high (32 with `big`).
pub fn button(ui: &mut Ui, text: &str, kind: Kind, icon_name: Option<&str>, big: bool, enabled: bool) -> Response {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let font = medium(if big { s.md } else { s.fs });
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let icon_w = if icon_name.is_some() { 14.0 + 4.0 } else { 0.0 };
    let h = if big { 32.0 } else { 24.0 };
    let pad = if big { 12.0 } else { 8.0 };
    let size = vec2(galley.size().x + icon_w + pad * 2.0, h);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    let (fill, fg, border) = match kind {
        Kind::Primary => (if hovered { t.brand_fill_hover } else { t.brand_fill }, Color32::WHITE, None),
        Kind::Secondary => (if hovered { t.hover } else { t.bg }, t.text, Some(t.border)),
        Kind::Ghost => (if hovered { t.hover } else { Color32::TRANSPARENT }, t.text, None),
        Kind::Danger => (if hovered { mix(hex(0xC8331A), 0.88, Color32::BLACK) } else { hex(0xC8331A) }, Color32::WHITE, None),
        Kind::DangerText => (if hovered { t.hover } else { t.bg }, t.danger_text, Some(t.border)),
    };
    // Disabled: grey like Figma's, so it doesn't look clickable.
    let (fill, fg) = match kind {
        _ if enabled => (fill, fg),
        Kind::Primary | Kind::Danger => (t.bg3, t.text3),
        _ => (fill, t.text3),
    };
    let p = ui.painter();
    p.rect_filled(rect, RADIUS, fill);
    if let Some(b) = border {
        p.rect_stroke(rect, RADIUS, Stroke::new(1.0, b), egui::StrokeKind::Inside);
    }
    let mut x = rect.min.x + pad;
    if let Some(name) = icon_name {
        icon(ui, name, pos2(x + 7.0, rect.center().y), 14.0, fg);
        x += icon_w;
    }
    ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    focus_ring(ui, &resp, rect);
    resp
}

/// The keyboard focus ring (WCAG: focus visible).
pub fn focus_ring(ui: &Ui, resp: &Response, rect: egui::Rect) {
    if resp.has_focus() && ui.ctx().input(|i| !i.pointer.any_down()) {
        let t = theme(ui.ctx());
        ui.painter().rect_stroke(rect.expand(1.0), RADIUS + 1, Stroke::new(2.0, t.brand), egui::StrokeKind::Outside);
    }
}

/// Icon button. `on`: a toggle that is on; `active`: the current tool (filled with the brand colour).
#[allow(clippy::too_many_arguments)]
pub fn icon_button(ui: &mut Ui, name: &str, label: &str, kbd: Option<&str>, size: Vec2, icon_size: f32, on: bool, active: bool) -> Response {
    let t = theme(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::Default);
    let hovered = resp.hovered();
    let (fill, fg) = if active {
        (t.brand_fill, Color32::WHITE)
    } else if on {
        (t.selected, if t.dark { Color32::WHITE } else { mix(t.brand, 0.7, Color32::BLACK) })
    } else if hovered {
        (t.hover, t.icon)
    } else {
        (Color32::TRANSPARENT, t.icon)
    };
    ui.painter().rect_filled(rect, RADIUS, fill);
    icon(ui, name, rect.center(), icon_size, fg);
    focus_ring(ui, &resp, rect);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on || active, label));
    tip(resp, label, kbd)
}

/// A small panel icon button (24×24, icon 16).
pub fn small_icon_button(ui: &mut Ui, name: &str, label: &str, on: bool) -> Response {
    icon_button(ui, name, label, None, vec2(24.0, 24.0), 16.0, on, false)
}

/// Mutually exclusive options in a grey track, the chosen one raised.
pub fn segmented<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], width: f32) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, RADIUS, t.bg2);
    let w = (rect.width() - 2.0) / options.len() as f32;
    let mut changed = false;
    for (i, (v, label)) in options.iter().enumerate() {
        let r = egui::Rect::from_min_size(pos2(rect.min.x + 1.0 + i as f32 * w, rect.min.y + 1.0), vec2(w, 22.0));
        let id = ui.id().with(("seg", i, label));
        let resp = ui.interact(r, id, Sense::click());
        let on = *value == *v;
        if on {
            ui.painter().rect_filled(r, RADIUS - 1, t.bg);
            ui.painter().rect_stroke(r, RADIUS - 1, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, RADIUS - 1, t.hover);
        }
        let color = if on { t.text } else { t.text2 };
        if let Some(name) = label.strip_prefix("icon:") {
            icon(ui, name, r.center(), 14.0, color);
        } else {
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *label, if on { medium(s.fs) } else { FontId::proportional(s.fs) }, color);
        }
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label.trim_start_matches("icon:")));
        focus_ring(ui, &resp, r);
        if resp.clicked() && !on {
            *value = v.clone();
            changed = true;
        }
    }
    changed
}

/// A switch with its label; the whole row toggles it.
pub fn switch(ui: &mut Ui, value: &mut bool, label: &str) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let galley = ui.painter().layout(label.to_string(), FontId::proportional(s.fs), t.text, (ui.available_width() - 40.0).max(60.0));
    let h = galley.size().y.max(24.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let track = egui::Rect::from_min_size(pos2(rect.min.x, rect.center().y - 7.0), vec2(24.0, 14.0));
    let k = ui.ctx().animate_bool(resp.id, *value);
    ui.painter().rect_filled(track, 7, mix(t.brand_fill, k, t.bg3));
    ui.painter().circle_filled(pos2(track.min.x + 7.0 + k * 10.0, track.center().y), 5.0, Color32::WHITE);
    ui.painter().galley(pos2(track.max.x + 8.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label));
    focus_ring(ui, &resp, track);
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// Colour chips; returns the colour picked. With `transparent`, a "none" chip first.
pub fn swatches(ui: &mut Ui, colors: &[&str], value: &str, transparent: bool) -> Option<String> {
    let t = theme(ui.ctx());
    let mut picked = None;
    let mut all: Vec<&str> = Vec::new();
    if transparent {
        all.push("transparent");
    }
    all.extend_from_slice(colors);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for c in all {
            let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
            let on = value.eq_ignore_ascii_case(c);
            let fill = crate::model::parse_color(c).unwrap_or(Color32::TRANSPARENT);
            let r = rect.shrink(2.0);
            if c == "transparent" {
                ui.painter().rect_filled(r, 4, Color32::WHITE);
                ui.painter().line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, t.danger));
            } else {
                ui.painter().rect_filled(r, 4, fill);
            }
            ui.painter().rect_stroke(r, 4, Stroke::new(1.0, black(0.12)), egui::StrokeKind::Inside);
            if on {
                ui.painter().rect_stroke(rect, 5, Stroke::new(2.0, t.brand), egui::StrokeKind::Inside);
            } else if resp.hovered() {
                ui.painter().rect_stroke(rect, 5, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
            }
            let name = if c == "transparent" { "Nessuno".to_string() } else { c.to_string() };
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, &name));
            focus_ring(ui, &resp, rect);
            if resp.clicked() {
                picked = Some(c.to_string());
            }
            tip(resp, &name, None);
        }
    });
    picked
}

/// A labelled slider with the value on the right; `log` spreads small values out.
#[allow(clippy::too_many_arguments)]
pub fn slider(ui: &mut Ui, label: &str, value: &mut f64, min: f64, max: f64, step: f64, log: bool, format: impl Fn(f64) -> String, width: f32) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let mut changed = false;
    ui.allocate_ui(vec2(width, 32.0), |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(s.fs).color(t.text2));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(format(*value)).size(s.fs).color(t.text)));
            });
            let (rect, resp) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
            let to_k = |v: f64| if log { (v.ln() - min.ln()) / (max.ln() - min.ln()) } else { (v - min) / (max - min) };
            let from_k = |k: f64| if log { (min.ln() + k * (max.ln() - min.ln())).exp() } else { min + k * (max - min) };
            if let Some(p) = resp.interact_pointer_pos() {
                let k = ((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0) as f64;
                let mut v = from_k(k);
                if step > 0.0 {
                    v = (v / step).round() * step;
                }
                v = v.clamp(min, max);
                if v != *value {
                    *value = v;
                    changed = true;
                }
            }
            if resp.has_focus() {
                let (l, r) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp)));
                if l || r {
                    let k = (to_k(*value) + if r { 0.05 } else { -0.05 }).clamp(0.0, 1.0);
                    *value = from_k(k).clamp(min, max);
                    changed = true;
                }
            }
            let k = to_k(*value).clamp(0.0, 1.0) as f32;
            let track = egui::Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
            ui.painter().rect_filled(track, 2, t.bg3);
            ui.painter().rect_filled(egui::Rect::from_min_max(track.min, pos2(track.min.x + track.width() * k, track.max.y)), 2, t.brand_fill);
            let knob = pos2(track.min.x + track.width() * k, track.center().y);
            ui.painter().circle(knob, 6.0, Color32::WHITE, Stroke::new(1.0, black(0.25)));
            resp.widget_info(|| egui::WidgetInfo::slider(true, *value, label));
            focus_ring(ui, &resp, rect);
        });
    });
    changed
}

/// A number field with a short label in front (Figma's X, Y, W, H…). None shows "Misto".
pub fn number_field(ui: &mut Ui, id: Id, label: &str, title: &str, value: Option<f64>, decimals: usize, width: f32) -> Option<f64> {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let shown = value.map(|v| fmt_num(v, decimals)).unwrap_or_else(|| "Misto".into());
    let mut out = None;
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
    ui.allocate_ui(vec2(width, 24.0), |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        ui.painter().rect_filled(rect, RADIUS, t.bg2);
        let lab = ui.painter().text(pos2(rect.min.x + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(s.fs), t.text2);
        let field = egui::Rect::from_min_max(pos2(lab.max.x.max(rect.min.x + 16.0) + 6.0, rect.min.y), rect.max);
        let edit = egui::TextEdit::singleline(&mut text).id(id.with("edit")).frame(egui::Frame::NONE).font(FontId::proportional(s.fs)).text_color(t.text).vertical_align(egui::Align::Center).desired_width(field.width() - 4.0);
        let resp = ui.put(field, edit);
        let resp = tip(resp, title, None);
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
        if resp.lost_focus() || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            if let Ok(v) = text.trim().replace(',', ".").parse::<f64>()
                && v.is_finite()
                && Some(v) != value.map(|x| (x * 10f64.powi(decimals as i32)).round() / 10f64.powi(decimals as i32))
            {
                out = Some(v);
            }
            ui.data_mut(|d| d.remove::<String>(id));
        }
        if resp.hovered() && !resp.has_focus() {
            ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        }
        if resp.has_focus() {
            ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.0, t.brand), egui::StrokeKind::Inside);
        }
    });
    out
}

pub fn fmt_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if decimals > 0 { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// A row of a dark menu: icon, label, shortcut. Returns the click.
pub fn menu_item(ui: &mut Ui, icon_name: Option<&str>, label: &str, kbd: Option<&str>, danger: bool) -> Response {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(s.fs), Color32::PLACEHOLDER);
    let kbd_g = kbd.map(|k| ui.painter().layout_no_wrap(k.to_string(), FontId::proportional(s.fs), Color32::PLACEHOLDER));
    let w = (24.0 + galley.size().x + kbd_g.as_ref().map_or(0.0, |g| g.size().x + 24.0) + 8.0).max(ui.available_width()).max(180.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    let hovered = resp.hovered() || resp.has_focus();
    if hovered {
        ui.painter().rect_filled(rect, RADIUS, if danger { hex(0xC8331A) } else { t.brand_fill });
    }
    let fg = if danger && !hovered { hex(0xFF7A5E) } else { t.menu_text };
    if let Some(name) = icon_name {
        icon(ui, name, pos2(rect.min.x + 12.0, rect.center().y), 14.0, fg);
    }
    ui.painter().galley(pos2(rect.min.x + 24.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    if let Some(g) = kbd_g {
        ui.painter().galley(pos2(rect.max.x - 8.0 - g.size().x, rect.center().y - g.size().y / 2.0), g, if hovered { t.menu_text } else { t.menu_text2 });
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

pub fn menu_sep(ui: &mut Ui) {
    let t = theme(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, t.menu_sep));
}

/// Frame of a floating surface (toolbar, trays, pills): white, 13 px corners, soft shadow.
pub fn float_frame(t: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(t.bg)
        .corner_radius(RADIUS_LG)
        .inner_margin(egui::Margin::same(8))
        .shadow(egui::Shadow { offset: [0, 5], blur: 12, spread: 0, color: black(if t.dark { 0.35 } else { 0.13 }) })
        .stroke(Stroke::new(0.5, black(if t.dark { 0.5 } else { 0.15 })))
}

/// Frame of a dark popup menu.
pub fn menu_frame(t: &Theme) -> egui::Frame {
    egui::Frame::new().fill(t.menu).corner_radius(RADIUS_LG).inner_margin(egui::Margin::same(8)).shadow(egui::Shadow { offset: [0, 10], blur: 16, spread: 0, color: black(0.15) })
}

/// Section title in the panels.
pub fn heading(ui: &mut Ui, text: &str) {
    let t = theme(ui.ctx());
    ui.label(RichText::new(text).font(medium(sizes(ui.ctx()).fs)).color(t.text));
}

pub fn hint(ui: &mut Ui, text: &str) {
    let t = theme(ui.ctx());
    ui.label(RichText::new(text).size(sizes(ui.ctx()).fs).color(t.text2));
}

/// A round avatar with the initial of a name.
pub fn avatar(ui: &mut Ui, name: &str, color: Color32, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
    let letter = name.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    let on = if crate::model::is_dark(&crate::model::hex(color)) { Color32::WHITE } else { crate::model::DARK };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, letter, medium(size * 0.45), on);
    resp
}

/// The Tratto logo.
pub fn logo(ui: &mut Ui, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    icons::logo(ui, rect);
    resp
}
