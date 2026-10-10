//! The look of Figma UI3 (Inter 11 px, 24 px controls, 5 px radii, dark menus and tooltips),
//! with the WCAG AA adjustments of the first Tratto, and the small widgets built on it.

pub mod icon_data;
pub mod icons;
pub mod motion;

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Rect, Response, RichText, Sense, Stroke, TextStyle, Ui, Vec2, pos2, vec2};

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

/// Relative luminance (WCAG).
fn luminance(c: Color32) -> f32 {
    let ch = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
}

/// The accent darkened just enough for white text on it to pass WCAG AA (4.5:1), whatever the
/// accent: a pale custom colour gets darker, a deep one stays close to itself.
fn fill_for_white(accent: Color32, start: f32) -> Color32 {
    let mut k = start;
    let mut c = mix(accent, k, Color32::BLACK);
    while 1.05 / (luminance(c) + 0.05) < 4.5 && k > 0.2 {
        k -= 0.03;
        c = mix(accent, k, Color32::BLACK);
    }
    c
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
                brand_fill: fill_for_white(accent, 0.76),
                brand_fill_hover: mix(fill_for_white(accent, 0.76), 0.9, Color32::BLACK),
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
                brand_fill: fill_for_white(accent, 0.76),
                brand_fill_hover: mix(fill_for_white(accent, 0.76), 0.9, Color32::BLACK),
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
        // Short, quiet fades for popups and tooltips, as in Figma.
        style.animation_time = 0.12;
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
        v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 16, spread: 0, color: black(0.16) };
        v.window_shadow = egui::Shadow { offset: [0, 12], blur: 36, spread: 0, color: black(0.16) };
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

/* ---------------- layout ---------------- */

/// A row `h` points high whose items all sit on one centre line. (egui centres each item on
/// the row height known when it is placed, so a taller item after shorter ones leaves them
/// higher up: this fixes the height first.)
pub fn row<R>(ui: &mut Ui, h: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(h);
        add(ui)
    })
    .inner
}

/// A rounded highlight that eases in and out under the pointer (Apple's and Figma's hovers:
/// a flat tint, no shadows).
pub fn hover_fill(ui: &Ui, id: Id, rect: Rect, on: bool, radius: f32, color: Color32) {
    let k = motion::hover(ui.ctx(), id.with("hover-fill"), on);
    if k > 0.0 {
        ui.painter().rect_filled(rect, radius, color.gamma_multiply(k));
    }
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
    let icon_w = if icon_name.is_some() { 14.0 + 6.0 } else { 0.0 };
    let h = if big { 32.0 } else { 24.0 };
    let pad = if big { 12.0 } else { 8.0 };
    let size = vec2((galley.size().x + icon_w + pad * 2.0).round(), h);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let k = motion::hover(ui.ctx(), resp.id.with("hover"), enabled && resp.hovered());
    let down = motion::hover(ui.ctx(), resp.id.with("down"), enabled && resp.is_pointer_button_down_on());
    let (fill, fg, border) = match kind {
        Kind::Primary => (blend(t.brand_fill, t.brand_fill_hover, k.max(down)), Color32::WHITE, None),
        Kind::Secondary => (blend(blend(t.bg, t.hover, k), t.press, down), t.text, Some(t.border)),
        Kind::Ghost => (blend(t.hover.gamma_multiply(k), t.press, down), t.text, None),
        Kind::Danger => (blend(hex(0xC8331A), mix(hex(0xC8331A), 0.88, Color32::BLACK), k.max(down)), Color32::WHITE, None),
        Kind::DangerText => (blend(blend(t.bg, t.hover, k), t.press, down), t.danger_text, Some(t.border)),
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
    let ctx = ui.ctx().clone();
    // Colours ease between states; pressed, the button darkens a little (no shrinking: the
    // icon stays sharp on the pixel grid).
    let h = motion::hover(&ctx, resp.id.with("hover"), resp.hovered() && !active);
    let a = motion::hover(&ctx, resp.id.with("active"), active);
    let down = motion::hover(&ctx, resp.id.with("down"), resp.is_pointer_button_down_on() && !active);
    let rest = if on { t.selected } else { blend(t.hover.gamma_multiply(h), t.press, down) };
    let fill = blend(rest, t.brand_fill, a);
    let fg = blend(if on && !t.dark { mix(t.brand, 0.7, Color32::BLACK) } else { t.icon }, Color32::WHITE, a);
    let radius = if size.x >= 32.0 { 8.0 } else { RADIUS as f32 };
    ui.painter().rect_filled(rect, radius, fill);
    if !name.is_empty() {
        icon(ui, name, rect.center(), icon_size, fg);
    }
    focus_ring(ui, &resp, rect);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on || active, label));
    tip(resp, label, kbd)
}

/// Straight mix of two colours (premultiplied), `k` 0 gives `a`, 1 gives `b`.
pub fn blend(a: Color32, b: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// A small panel icon button (24×24, icon 16).
pub fn small_icon_button(ui: &mut Ui, name: &str, label: &str, on: bool) -> Response {
    icon_button(ui, name, label, None, vec2(24.0, 24.0), 16.0, on, false)
}

/// Mutually exclusive options in a grey track; the raised thumb slides to the chosen one.
pub fn segmented<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], width: f32) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let ctx = ui.ctx().clone();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    let key: Vec<&str> = options.iter().map(|o| o.1).collect();
    let base = ui.id().with(("seg", key));
    ui.painter().rect_filled(rect, RADIUS, t.bg2);
    let w = (rect.width() - 2.0) / options.len().max(1) as f32;
    let at = options.iter().position(|(v, _)| v == value);
    if let Some(i) = at {
        ctx.data_mut(|d| d.insert_temp(base.with("last"), i));
    }
    let shown = motion::hover(&ctx, base.with("shown"), at.is_some());
    if let Some(i) = at.or_else(|| ctx.data(|d| d.get_temp::<usize>(base.with("last"))))
        && shown > 0.0
    {
        let x = motion::spring(&ctx, base.with("thumb"), i as f32 * w, 0.32, 0.88);
        let thumb = Rect::from_min_size(pos2(rect.min.x + 1.0 + x, rect.min.y + 1.0), vec2(w, 22.0));
        let shadow = egui::Shadow { offset: [0, 1], blur: 2, spread: 0, color: black(if t.dark { 0.3 } else { 0.08 } * shown) };
        ui.painter().add(shadow.as_shape(thumb, RADIUS - 1));
        ui.painter().rect(thumb, RADIUS - 1, t.bg.gamma_multiply(shown), Stroke::new(1.0, t.border.gamma_multiply(shown)), egui::StrokeKind::Inside);
    }
    let mut changed = false;
    for (i, (v, label)) in options.iter().enumerate() {
        let r = egui::Rect::from_min_size(pos2(rect.min.x + 1.0 + i as f32 * w, rect.min.y + 1.0), vec2(w, 22.0));
        let resp = ui.interact(r, base.with(i), Sense::click());
        let on = at == Some(i);
        let h = motion::hover(&ctx, resp.id.with("h"), resp.hovered() && !on);
        let color = blend(t.text2, t.text, if on { 1.0 } else { h });
        if let Some(name) = label.strip_prefix("icon:") {
            icon(ui, name, r.center(), 14.0, color);
        } else {
            let font = if on { medium(s.fs) } else { FontId::proportional(s.fs) };
            ui.painter().with_clip_rect(r.shrink(2.0)).text(r.center(), egui::Align2::CENTER_CENTER, *label, font, color);
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
    let track = egui::Rect::from_min_size(pos2(rect.min.x, rect.center().y - 8.0), vec2(28.0, 16.0));
    let k = motion::spring(ui.ctx(), resp.id.with("knob"), if *value { 1.0 } else { 0.0 }, 0.26, 0.86).clamp(-0.1, 1.1);
    let hk = motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
    ui.painter().rect_filled(track, 8, blend(blend(t.bg3, t.border_strong, hk * 0.6), t.brand_fill, k.clamp(0.0, 1.0)));
    let knob = pos2(track.min.x + 8.0 + k * 12.0, track.center().y);
    ui.painter().circle_filled(knob + vec2(0.0, 0.6), 6.4, black(0.18));
    ui.painter().circle_filled(knob, 6.0, Color32::WHITE);
    ui.painter().galley(pos2(track.max.x + 8.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label));
    focus_ring(ui, &resp, track);
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// One colour chip of a palette (20 px, rounded square).
fn chip(ui: &mut Ui, c: &str, on: bool) -> Response {
    let t = theme(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    let fill = crate::model::parse_color(c).unwrap_or(Color32::TRANSPARENT);
    let h = motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
    let r = rect.shrink(2.0 - h * 0.5);
    if c == "transparent" {
        ui.painter().rect_filled(r, 4, Color32::WHITE);
        ui.painter().line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, t.danger));
    } else {
        ui.painter().rect_filled(r, 4, fill);
    }
    ui.painter().rect_stroke(r, 4, Stroke::new(1.0, black(0.12)), egui::StrokeKind::Inside);
    if on {
        ui.painter().rect_stroke(rect, 5, Stroke::new(2.0, t.brand), egui::StrokeKind::Inside);
    }
    let name = if c == "transparent" { "Nessuno".to_string() } else { c.to_string() };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, &name));
    focus_ring(ui, &resp, rect);
    tip(resp, &name, None)
}

/// Colour chips; returns the colour picked. With `transparent`, a "none" chip first.
pub fn swatches(ui: &mut Ui, colors: &[&str], value: &str, transparent: bool) -> Option<String> {
    let mut picked = None;
    let mut all: Vec<&str> = Vec::new();
    if transparent {
        all.push("transparent");
    }
    all.extend_from_slice(colors);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for c in all {
            if chip(ui, c, value.eq_ignore_ascii_case(c)).clicked() {
                picked = Some(c.to_string());
            }
        }
    });
    picked
}

/// A palette plus a "custom" chip that opens a full colour picker in place, as in Figma's fill
/// panel. Returns the colour picked (continuously while the picker is dragged).
pub fn palette(ui: &mut Ui, colors: &[&str], value: &str, transparent: bool) -> Option<String> {
    let t = theme(ui.ctx());
    let open_id = ui.id().with(("palette-custom", colors.first().copied()));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let custom = value != "transparent" && !value.is_empty() && !colors.iter().any(|c| c.eq_ignore_ascii_case(value));
    let mut picked = None;
    let mut all: Vec<&str> = Vec::new();
    if transparent {
        all.push("transparent");
    }
    all.extend_from_slice(colors);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for c in all {
            if chip(ui, c, value.eq_ignore_ascii_case(c)).clicked() {
                picked = Some(c.to_string());
            }
        }
        // The custom chip: a colour wheel, or the custom colour itself once chosen.
        let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
        let h = motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
        let c = rect.center();
        if custom {
            ui.painter().circle_filled(c, 8.0 + h * 0.5, crate::model::parse_color(value).unwrap_or(Color32::GRAY));
            ui.painter().circle_stroke(c, 8.0 + h * 0.5, Stroke::new(1.0, black(0.15)));
        } else {
            wheel(ui, c, 8.0 + h * 0.5);
        }
        if custom || open {
            ui.painter().rect_stroke(rect, 5, Stroke::new(if custom { 2.0 } else { 1.0 }, if custom { t.brand } else { t.border_strong }), egui::StrokeKind::Inside);
        }
        if tip(resp, "Colore personalizzato", None).clicked() {
            open = !open;
        }
    });
    if open {
        let mut c = crate::model::parse_color(value).filter(|c| c.a() > 0).unwrap_or(Color32::from_rgb(0x0D, 0x99, 0xFF));
        if color_picker(ui, open_id.with("picker"), &mut c, ui.available_width().clamp(160.0, 240.0)) {
            picked = Some(crate::model::hex(c));
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    picked
}

/// A little hue wheel (the "custom colour" chip).
fn wheel(ui: &Ui, c: egui::Pos2, r: f32) {
    let n = 36;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, Color32::WHITE);
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(c + vec2(a.cos(), a.sin()) * r, hsv(i as f32 / n as f32, 1.0, 1.0));
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    ui.painter().add(mesh);
    ui.painter().circle_stroke(c, r, Stroke::new(1.0, black(0.12)));
}

/// HSV (all 0–1) to a colour.
pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, u) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 {
        0 => (v, u, p),
        1 => (q, v, p),
        2 => (p, v, u),
        3 => (p, q, v),
        4 => (u, p, v),
        _ => (v, p, q),
    };
    let to = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(to(r), to(g), to(b))
}

/// A colour to HSV (all 0–1).
pub fn to_hsv(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, if max <= 0.0 { 0.0 } else { d / max }, max)
}

/// Figma's colour picker in the colours of the interface: saturation and brightness in a square,
/// the hue on a bar and the hex code in a field. Returns true while the colour changes.
pub fn color_picker(ui: &mut Ui, id: Id, c: &mut Color32, width: f32) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    // Hue and saturation are kept, so they don't jump on greys and black.
    let (mut h, mut sat, mut val) = match ui.data(|d| d.get_temp::<(f32, f32, f32, Color32)>(id)) {
        Some((h, s, v, was)) if was == *c => (h, s, v),
        Some((h0, s0, _, _)) => {
            let (h, s, v) = to_hsv(*c);
            (if s < 0.01 || v < 0.01 { h0 } else { h }, if v < 0.01 { s0 } else { s }, v)
        }
        None => to_hsv(*c),
    };
    let mut changed = false;
    let width = width.max(140.0);
    // Saturation (across) and brightness (up).
    let (sq, resp) = ui.allocate_exact_size(vec2(width, (width * 0.62).round()), Sense::click_and_drag());
    let (nx, ny) = (12, 8);
    let mut mesh = egui::Mesh::default();
    for j in 0..=ny {
        for i in 0..=nx {
            let (u, v) = (i as f32 / nx as f32, j as f32 / ny as f32);
            mesh.colored_vertex(pos2(sq.min.x + u * sq.width(), sq.min.y + v * sq.height()), hsv(h, u, 1.0 - v));
        }
    }
    for j in 0..ny {
        for i in 0..nx {
            let a = (j * (nx + 1) + i) as u32;
            let b = a + 1;
            let d = a + nx as u32 + 1;
            mesh.add_triangle(a, b, d + 1);
            mesh.add_triangle(a, d + 1, d);
        }
    }
    ui.painter().with_clip_rect(sq).add(mesh);
    ui.painter().rect_stroke(sq, 4, Stroke::new(1.0, black(0.1)), egui::StrokeKind::Inside);
    if let Some(p) = resp.interact_pointer_pos() {
        sat = ((p.x - sq.min.x) / sq.width()).clamp(0.0, 1.0);
        val = 1.0 - ((p.y - sq.min.y) / sq.height()).clamp(0.0, 1.0);
        changed = true;
    }
    let knob = pos2(sq.min.x + sat * sq.width(), sq.min.y + (1.0 - val) * sq.height());
    ui.painter().circle_stroke(knob, 6.0, Stroke::new(2.5, Color32::WHITE));
    ui.painter().circle_stroke(knob, 7.5, Stroke::new(1.0, black(0.3)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Saturazione e luminosità"));
    ui.add_space(6.0);
    // Hue.
    let (bar, hresp) = ui.allocate_exact_size(vec2(width, 12.0), Sense::click_and_drag());
    let n = 24;
    let mut mesh = egui::Mesh::default();
    for i in 0..=n {
        let u = i as f32 / n as f32;
        let x = bar.min.x + u * bar.width();
        mesh.colored_vertex(pos2(x, bar.min.y), hsv(u, 1.0, 1.0));
        mesh.colored_vertex(pos2(x, bar.max.y), hsv(u, 1.0, 1.0));
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k + 1);
            mesh.add_triangle(k - 2, k + 1, k);
        }
    }
    ui.painter().with_clip_rect(bar).add(mesh);
    if let Some(p) = hresp.interact_pointer_pos() {
        h = ((p.x - bar.min.x) / bar.width()).clamp(0.0, 0.999);
        changed = true;
    }
    let hx = bar.min.x + h * bar.width();
    ui.painter().circle(pos2(hx.clamp(bar.min.x + 6.0, bar.max.x - 6.0), bar.center().y), 6.0, hsv(h, 1.0, 1.0), Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(pos2(hx.clamp(bar.min.x + 6.0, bar.max.x - 6.0), bar.center().y), 7.0, Stroke::new(1.0, black(0.3)));
    hresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Tinta"));
    ui.add_space(6.0);
    if changed {
        *c = hsv(h, sat, val);
    }
    // Hex code, with a sample of the colour.
    row(ui, 24.0, |ui| {
        let (sample, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
        ui.painter().rect_filled(sample.shrink(2.0), 4, *c);
        ui.painter().rect_stroke(sample.shrink(2.0), 4, Stroke::new(1.0, black(0.12)), egui::StrokeKind::Inside);
        let fid = id.with("hex");
        let shown = crate::model::hex(*c).trim_start_matches('#').to_string();
        let mut text = ui.data(|d| d.get_temp::<String>(fid)).unwrap_or_else(|| shown.clone());
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        ui.painter().rect_filled(rect, RADIUS, t.bg2);
        ui.painter().text(pos2(rect.min.x + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, "#", FontId::proportional(s.fs), t.text2);
        let field = Rect::from_min_max(pos2(rect.min.x + 20.0, rect.min.y), rect.max);
        let edit = egui::TextEdit::singleline(&mut text).id(fid.with("edit")).frame(egui::Frame::NONE).font(FontId::monospace(s.fs)).text_color(t.text).vertical_align(egui::Align::Center).char_limit(7).desired_width(field.width() - 4.0);
        let r = ui.put(field, edit);
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(fid, text.clone()));
            ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.0, t.brand), egui::StrokeKind::Inside);
        } else {
            ui.data_mut(|d| d.remove::<String>(fid));
        }
        if r.lost_focus() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            let clean: String = text.trim().trim_start_matches('#').chars().filter(|ch| ch.is_ascii_hexdigit()).collect();
            if let Some(v) = crate::model::parse_color(&format!("#{clean}")).filter(|_| clean.len() == 3 || clean.len() == 6) {
                *c = Color32::from_rgb(v.r(), v.g(), v.b());
                let (nh, ns, nv) = to_hsv(*c);
                (h, sat, val) = (if ns < 0.01 { h } else { nh }, ns, nv);
                changed = true;
            }
            ui.data_mut(|d| d.remove::<String>(fid));
        }
        tip(r, "Codice esadecimale del colore", None);
    });
    ui.data_mut(|d| d.insert_temp(id, (h, sat, val, *c)));
    changed
}

/// A colour well that opens the picker in a light popover. Returns the colour while it changes.
pub fn color_well(ui: &mut Ui, label: &str, value: Color32) -> Option<Color32> {
    let t = theme(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let h = motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
    ui.painter().circle_filled(rect.center(), 10.0 + h * 0.5, value);
    ui.painter().circle_stroke(rect.center(), 10.0 + h * 0.5, Stroke::new(1.0, black(0.15)));
    let resp = tip(resp, label, None);
    let mut out = None;
    egui::Popup::from_toggle_button_response(&resp).frame(float_frame(&t).inner_margin(egui::Margin::same(12))).gap(8.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(220.0);
        heading(ui, label);
        ui.add_space(2.0);
        let mut c = value;
        if color_picker(ui, resp.id.with("picker"), &mut c, 220.0) {
            out = Some(c);
        }
    });
    out
}

/// A labelled slider with the value on the right; `log` spreads small values out.
#[allow(clippy::too_many_arguments)]
pub fn slider(ui: &mut Ui, label: &str, value: &mut f64, min: f64, max: f64, step: f64, log: bool, format: impl Fn(f64) -> String, width: f32) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let mut changed = false;
    ui.allocate_ui(vec2(width, 34.0), |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let (head, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
            ui.painter().text(head.left_center(), egui::Align2::LEFT_CENTER, label, FontId::proportional(s.fs), t.text2);
            ui.painter().text(head.right_center(), egui::Align2::RIGHT_CENTER, format(*value), FontId::proportional(s.fs), t.text);
            changed = track(ui, label, value, min, max, step, log, width);
        });
    });
    changed
}

/// Label, track and value on one line, 28 px high: the toolbar's sliders.
#[allow(clippy::too_many_arguments)]
pub fn slider_inline(ui: &mut Ui, label: &str, value: &mut f64, min: f64, max: f64, step: f64, log: bool, format: impl Fn(f64) -> String, width: f32) -> bool {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(s.fs), t.text2);
    let value_w = 40.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    ui.painter().galley(pos2(rect.min.x, rect.center().y - g.size().y / 2.0), g.clone(), t.text2);
    ui.painter().text(pos2(rect.max.x, rect.center().y), egui::Align2::RIGHT_CENTER, format(*value), FontId::proportional(s.fs), t.text);
    let left = rect.min.x + g.size().x + 10.0;
    let right = rect.max.x - value_w - 6.0;
    let mut changed = false;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(left, rect.min.y), pos2(right.max(left + 20.0), rect.max.y))));
    child.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        changed = track(ui, label, value, min, max, step, log, (right - left).max(20.0));
    });
    changed
}

/// The draggable track of a slider: click or drag anywhere on it, arrows when focused.
#[allow(clippy::too_many_arguments)]
fn track(ui: &mut Ui, label: &str, value: &mut f64, min: f64, max: f64, step: f64, log: bool, width: f32) -> bool {
    let t = theme(ui.ctx());
    let mut changed = false;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 16.0), Sense::click_and_drag());
    let to_k = |v: f64| if log { (v.ln() - min.ln()) / (max.ln() - min.ln()) } else { (v - min) / (max - min) };
    let from_k = |k: f64| if log { (min.ln() + k * (max.ln() - min.ln())).exp() } else { min + k * (max - min) };
    // The knob sits inside the track's ends.
    let inner = rect.shrink2(vec2(6.0, 0.0));
    if let Some(p) = resp.interact_pointer_pos() {
        let k = ((p.x - inner.min.x) / inner.width()).clamp(0.0, 1.0) as f64;
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
    let bar = egui::Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    ui.painter().rect_filled(bar, 2, t.bg3);
    let x = inner.min.x + inner.width() * k;
    ui.painter().rect_filled(egui::Rect::from_min_max(bar.min, pos2(x, bar.max.y)), 2, t.brand_fill);
    let hk = motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered() || resp.dragged());
    let knob = pos2(x, bar.center().y);
    ui.painter().circle_filled(knob + vec2(0.0, 0.8), 6.6 + hk, black(0.16));
    ui.painter().circle(knob, 6.0 + hk, Color32::WHITE, Stroke::new(1.0, black(0.12)));
    resp.widget_info(|| egui::WidgetInfo::slider(true, *value, label));
    focus_ring(ui, &resp, rect);
    changed
}

/// A number field in the manner of Figma's: a short label (or an icon, "icon:name") in front of
/// the value; drag the label sideways to change it (Shift for ten steps at a time), or use the
/// arrow keys while typing. None shows "Misto". Returns the new value, continuously while dragged.
pub struct Num<'a> {
    id: Id,
    label: &'a str,
    title: &'a str,
    value: Option<f64>,
    decimals: usize,
    width: f32,
    step: f64,
    min: f64,
    max: f64,
    suffix: &'a str,
}

impl<'a> Num<'a> {
    pub fn new(id: Id, label: &'a str, title: &'a str, value: Option<f64>) -> Self {
        Num { id, label, title, value, decimals: 0, width: 80.0, step: 1.0, min: f64::NEG_INFINITY, max: f64::INFINITY, suffix: "" }
    }
    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = d;
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn step(mut self, s: f64) -> Self {
        self.step = s;
        self
    }
    pub fn range(mut self, min: f64, max: f64) -> Self {
        (self.min, self.max) = (min, max);
        self
    }
    pub fn suffix(mut self, s: &'a str) -> Self {
        self.suffix = s;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Option<f64> {
        let t = theme(ui.ctx());
        let s = sizes(ui.ctx());
        let Num { id, label, title, value, decimals, width, step, min, max, suffix } = self;
        let round = |v: f64| {
            let q = 10f64.powi(decimals as i32);
            ((v * q).round() / q).clamp(min, max)
        };
        let shown = value.map(|v| format!("{}{suffix}", fmt_num(v, decimals))).unwrap_or_else(|| "Misto".into());
        let mut out = None;
        let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
        let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        ui.painter().rect_filled(rect, RADIUS, t.bg2);
        // The label: what you drag to change the value.
        let lab_w = if label.starts_with("icon:") { 24.0 } else { (ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(s.fs), t.text2).size().x + 14.0).max(22.0) };
        let lab = Rect::from_min_size(rect.min, vec2(lab_w, rect.height()));
        let scrub = ui.interact(lab, id.with("scrub"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let sid = id.with("scrub-state");
        if scrub.drag_started() {
            ui.data_mut(|d| d.insert_temp(sid, (value.unwrap_or(0.0), 0.0f32)));
        }
        if scrub.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            if let Some((start, mut acc)) = ui.data(|d| d.get_temp::<(f64, f32)>(sid)) {
                let fast = ui.input(|i| i.modifiers.shift);
                acc += scrub.drag_delta().x * if fast { 10.0 } else { 1.0 };
                ui.data_mut(|d| d.insert_temp(sid, (start, acc)));
                let v = round(start + (acc as f64 / 2.0).round() * step);
                if Some(v) != value.map(round) {
                    out = Some(v);
                }
            }
        }
        if scrub.drag_stopped() {
            ui.data_mut(|d| d.remove::<(f64, f32)>(sid));
        }
        let lc = blend(t.text2, t.text, motion::hover(ui.ctx(), id.with("lab-h"), scrub.hovered() || scrub.dragged()));
        if let Some(name) = label.strip_prefix("icon:") {
            icon(ui, name, lab.center(), 14.0, lc);
        } else {
            ui.painter().text(pos2(rect.min.x + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(s.fs), lc);
        }
        let field = Rect::from_min_max(pos2(lab.max.x - 4.0, rect.min.y), rect.max);
        let edit = egui::TextEdit::singleline(&mut text).id(id.with("edit")).frame(egui::Frame::NONE).font(FontId::proportional(s.fs)).text_color(t.text).vertical_align(egui::Align::Center).desired_width(field.width() - 4.0);
        let resp = ui.put(field, edit);
        let resp = tip(resp, title, None);
        let parse = |s: &str| s.trim().trim_end_matches(suffix).trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
        if resp.has_focus() {
            // Up and down step the value while typing, as in Figma.
            let (up, down, fast) = ui.input_mut(|i| {
                let fast = i.modifiers.shift;
                let m = if fast { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
                (i.consume_key(m, egui::Key::ArrowUp), i.consume_key(m, egui::Key::ArrowDown), fast)
            });
            if up || down {
                let base = parse(&text).or(value).unwrap_or(0.0);
                let v = round(base + if up { step } else { -step } * if fast { 10.0 } else { 1.0 });
                text = format!("{}{suffix}", fmt_num(v, decimals));
                out = Some(v);
            }
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
        if resp.lost_focus() || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            if let Some(v) = parse(&text).map(round)
                && Some(v) != value.map(round)
            {
                out = Some(v);
            }
            ui.data_mut(|d| d.remove::<String>(id));
        }
        let hk = motion::hover(ui.ctx(), id.with("h"), (resp.hovered() || scrub.hovered()) && !resp.has_focus());
        if hk > 0.0 {
            ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.0, t.border_strong.gamma_multiply(hk)), egui::StrokeKind::Inside);
        }
        if resp.has_focus() || scrub.dragged() {
            ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.0, t.brand), egui::StrokeKind::Inside);
        }
        out
    }
}

/// A number field with a short label in front (Figma's X, Y, W, H…). None shows "Misto".
pub fn number_field(ui: &mut Ui, id: Id, label: &str, title: &str, value: Option<f64>, decimals: usize, width: f32) -> Option<f64> {
    Num::new(id, label, title, value).decimals(decimals).width(width).step(1.0 / 10f64.powi(decimals.min(1) as i32)).show(ui)
}

pub fn fmt_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if decimals > 0 { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// Width a menu item asks for: icon gutter, label, shortcut.
fn menu_item_width(ui: &Ui, label: &str, kbd: Option<&str>) -> f32 {
    let s = sizes(ui.ctx());
    let w = |text: &str| ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(s.fs), Color32::PLACEHOLDER).size().x;
    (28.0 + w(label) + kbd.map_or(0.0, |k| w(k) + 28.0) + 10.0).ceil()
}

/// A row of a dark menu: icon, label, shortcut. Returns the click. Menus are as wide as their
/// longest row, never wider.
pub fn menu_item(ui: &mut Ui, icon_name: Option<&str>, label: &str, kbd: Option<&str>, danger: bool) -> Response {
    let t = theme(ui.ctx());
    let s = sizes(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(s.fs), Color32::PLACEHOLDER);
    let kbd_g = kbd.map(|k| ui.painter().layout_no_wrap(k.to_string(), FontId::proportional(s.fs), Color32::PLACEHOLDER));
    let want = menu_item_width(ui, label, kbd).max(140.0);
    // While the menu measures itself it asks only for its own width.
    let w = if ui.is_sizing_pass() { want } else { want.max(ui.available_width()) };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    let hovered = resp.hovered() || resp.has_focus();
    if hovered {
        ui.painter().rect_filled(rect, RADIUS, if danger { hex(0xC8331A) } else { t.brand_fill });
    }
    let fg = if danger && !hovered { hex(0xFF7A5E) } else { t.menu_text };
    if let Some(name) = icon_name {
        icon(ui, name, pos2(rect.min.x + 14.0, rect.center().y), 14.0, fg);
    }
    ui.painter().galley(pos2(rect.min.x + 28.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    if let Some(g) = kbd_g {
        ui.painter().galley(pos2(rect.max.x - 10.0 - g.size().x, rect.center().y - g.size().y / 2.0), g, if hovered { t.menu_text } else { t.menu_text2 });
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

/// A small grey title inside a dark menu.
pub fn menu_label(ui: &mut Ui, text: &str) {
    let t = theme(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(if ui.is_sizing_pass() { 60.0 } else { ui.available_width() }, 22.0), Sense::hover());
    ui.painter().text(pos2(rect.min.x + 10.0, rect.center().y), egui::Align2::LEFT_CENTER, text, FontId::proportional(10.0), t.menu_text2);
}

pub fn menu_sep(ui: &mut Ui) {
    let t = theme(ui.ctx());
    let w = if ui.is_sizing_pass() { 40.0 } else { ui.available_width() };
    let (rect, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, t.menu_sep));
}

/// Frame of a floating surface (toolbar, trays, pills): white, 13 px corners, soft shadow.
pub fn float_frame(t: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(t.bg)
        .corner_radius(RADIUS_LG)
        .inner_margin(egui::Margin::same(8))
        .shadow(egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: black(if t.dark { 0.3 } else { 0.08 }) })
        .stroke(Stroke::new(1.0, t.border))
}

/// Figma's elevation for floating cards: a tight contact shadow under a soft wide one.
pub fn card_shadows(t: &Theme) -> [egui::Shadow; 2] {
    let k = if t.dark { 2.5 } else { 1.0 };
    [
        egui::Shadow { offset: [0, 1], blur: 3, spread: 0, color: black(0.06 * k) },
        egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: black(0.07 * k) },
    ]
}

/// Frame of a dark popup menu.
pub fn menu_frame(t: &Theme) -> egui::Frame {
    egui::Frame::new().fill(t.menu).corner_radius(10).inner_margin(egui::Margin::same(6)).shadow(egui::Shadow { offset: [0, 6], blur: 16, spread: 0, color: black(0.18) })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_text_stays_readable_on_any_accent() {
        for accent in [hex(0x0D99FF), hex(0xC8D8F0), hex(0xFFE066), hex(0x14AE5C), Color32::WHITE] {
            let t = Theme::new(false, accent, false);
            let contrast = 1.05 / (luminance(t.brand_fill) + 0.05);
            assert!(contrast >= 4.5, "{accent:?}: {contrast}");
        }
        // The usual blue keeps the fill it always had.
        assert_eq!(Theme::new(false, hex(0x0D99FF), false).brand_fill, mix(hex(0x0D99FF), 0.76, Color32::BLACK));
    }

    #[test]
    fn colours_survive_the_trip_through_hsv() {
        for c in [Color32::from_rgb(0x0D, 0x99, 0xFF), Color32::from_rgb(0xE0, 0x31, 0x31), Color32::from_rgb(0x2F, 0x9E, 0x44), Color32::WHITE, Color32::BLACK, Color32::from_gray(0x75)] {
            let (h, s, v) = to_hsv(c);
            let back = hsv(h, s, v);
            let near = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 1;
            assert!(near(back.r(), c.r()) && near(back.g(), c.g()) && near(back.b(), c.b()), "{c:?} → {back:?}");
        }
    }
}
