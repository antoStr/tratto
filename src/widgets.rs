//! FigJam-style widgets: a poll, a checklist and a counter. One layout drives both the drawing
//! and the clickable parts, so what you see is exactly what you can press.

use egui::Color32;

use crate::model::{Align, El, FontKind, WIDGET_W, Widget};
use crate::prims::{Path, Prim, with_alpha};
use crate::text::{self, LINE_HEIGHT, layout_text, place_lines};

/// Something a click on a widget does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hot {
    /// Vote for (or take back the vote on) an option.
    Vote(usize),
    /// Tick or untick an item.
    Check(usize),
    Add(i64),
}

pub struct Part {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub hot: Hot,
}

pub struct Layout {
    pub title_top: f64,
    pub title_lines: Vec<String>,
    pub parts: Vec<Part>,
    pub height: f64,
}

const PAD: f64 = 20.0;
const TITLE: f64 = 18.0;
const BODY: f64 = 15.0;
const KICKER: f64 = 11.0;

fn title_of(w: &Widget) -> &str {
    match w {
        Widget::Poll { question, .. } => question,
        Widget::Checklist { title, .. } => title,
        Widget::Counter { label, .. } => label,
    }
}

pub fn kind_name(w: &Widget) -> &'static str {
    match w {
        Widget::Poll { .. } => "Sondaggio",
        Widget::Checklist { .. } => "Lista di cose da fare",
        Widget::Counter { .. } => "Contatore",
    }
}

pub fn layout(el: &El, w: &Widget) -> Layout {
    let width = el.w.max(160.0);
    let face = text::font(FontKind::Sans, true, false).face;
    let title_top = PAD + KICKER * LINE_HEIGHT + 4.0;
    let title_lines = layout_text(title_of(w), face, TITLE, Some(width - PAD * 2.0)).lines;
    let mut y = title_top + title_lines.len().max(1) as f64 * TITLE * LINE_HEIGHT + 12.0;
    let mut parts = Vec::new();
    match w {
        Widget::Poll { options, .. } => {
            for i in 0..options.len() {
                parts.push(Part { x: PAD, y, w: width - PAD * 2.0, h: 40.0, hot: Hot::Vote(i) });
                y += 48.0;
            }
            y += 20.0;
        }
        Widget::Checklist { items, .. } => {
            for i in 0..items.len() {
                parts.push(Part { x: PAD - 6.0, y, w: width - PAD * 2.0 + 12.0, h: 34.0, hot: Hot::Check(i) });
                y += 36.0;
            }
            y += 26.0;
        }
        Widget::Counter { .. } => {
            y += 64.0;
            let b = 44.0;
            parts.push(Part { x: width / 2.0 - b - 12.0, y, w: b, h: b, hot: Hot::Add(-1) });
            parts.push(Part { x: width / 2.0 + 12.0, y, w: b, h: b, hot: Hot::Add(1) });
            y += b;
        }
    }
    Layout { title_top, title_lines, parts, height: y + PAD }
}

pub fn fit(el: &mut El) {
    if el.w < 160.0 {
        el.w = WIDGET_W;
    }
    let Some(w) = el.widget() else { return };
    el.h = layout(el, w).height;
}

/// The clickable part under a point in the widget's own coordinates.
pub fn hot_at(el: &El, x: f64, y: f64) -> Option<Hot> {
    let w = el.widget()?;
    layout(el, w).parts.into_iter().find(|p| x >= p.x && y >= p.y && x <= p.x + p.w && y <= p.y + p.h).map(|p| p.hot)
}

/// Applies a click; `voter` is this device. Returns false when nothing changed.
pub fn press(w: &mut Widget, hot: Hot, voter: &str) -> bool {
    match (w, hot) {
        (Widget::Poll { options, .. }, Hot::Vote(i)) if i < options.len() => {
            // One vote each: voting again takes it back, voting elsewhere moves it.
            let had = options[i].votes.iter().any(|v| v == voter);
            for o in options.iter_mut() {
                o.votes.retain(|v| v != voter);
            }
            if !had {
                options[i].votes.push(voter.to_string());
            }
            true
        }
        (Widget::Checklist { items, .. }, Hot::Check(i)) if i < items.len() => {
            items[i].done = !items[i].done;
            true
        }
        (Widget::Counter { value, .. }, Hot::Add(d)) => {
            *value = value.saturating_add(d).clamp(-999_999, 999_999);
            true
        }
        _ => false,
    }
}

const BRAND: Color32 = Color32::from_rgb(0x0D, 0x99, 0xFF);
const INK: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
const MUTED: Color32 = Color32::from_rgb(0x75, 0x75, 0x75);
const TRACK: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF2);

#[allow(clippy::too_many_arguments)]
fn one_line(s: &str, size: f64, bold: bool, align: Align, width: f64, top: f64, left: f64, color: Color32, alpha: f64, out: &mut Vec<Prim>) {
    out.push(Prim::Text { placed: place_lines(&[s.to_string()], text::font(FontKind::Sans, bold, false), size, align, width, top, left), color: with_alpha(color, alpha) });
}

/// Shortens a label with an ellipsis to fit `width`.
fn clip(s: &str, size: f64, bold: bool, width: f64) -> String {
    let face = text::font(FontKind::Sans, bold, false).face;
    if text::measure(face, s, size) <= width {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && text::measure(face, &format!("{out}…"), size) > width {
        out.pop();
    }
    format!("{}…", out.trim_end())
}

pub fn prims(el: &El, w: &Widget, alpha: f64, out: &mut Vec<Prim>) {
    let (ew, eh) = (el.w as f32, el.h as f32);
    let l = layout(el, w);
    out.push(Prim::Shadow { x: 0.0, y: 0.0, w: ew, h: eh, radius: 16.0, blur: 8.0, dy: 2.0, color: with_alpha(Color32::from_black_alpha(26), alpha) });
    out.push(Prim::Fill { path: Path::round_rect(0.0, 0.0, ew, eh, [16.0; 4]), color: with_alpha(Color32::WHITE, alpha) });
    out.push(Prim::Stroke { path: Path::round_rect(0.5, 0.5, ew - 1.0, eh - 1.0, [16.0; 4]), width: 1.0, color: with_alpha(Color32::from_rgb(0xE6, 0xE6, 0xE6), alpha), round: true, dash: None });
    one_line(&kind_name(w).to_uppercase(), KICKER, true, Align::Left, 0.0, PAD, PAD, BRAND, alpha, out);
    let empty = title_of(w).is_empty();
    let title = if empty { vec![placeholder(w).to_string()] } else { l.title_lines.clone() };
    out.push(Prim::Text { placed: place_lines(&title, text::font(FontKind::Sans, true, false), TITLE, Align::Left, el.w - PAD * 2.0, l.title_top, PAD), color: with_alpha(if empty { MUTED } else { INK }, alpha) });
    match w {
        Widget::Poll { options, .. } => {
            let total: usize = options.iter().map(|o| o.votes.len()).sum();
            let most = options.iter().map(|o| o.votes.len()).max().unwrap_or(0);
            for (p, o) in l.parts.iter().zip(options) {
                let (x, y, pw, ph) = (p.x as f32, p.y as f32, p.w as f32, p.h as f32);
                out.push(Prim::Fill { path: Path::round_rect(x, y, pw, ph, [10.0; 4]), color: with_alpha(TRACK, alpha) });
                let n = o.votes.len();
                if n > 0 {
                    let k = n as f32 / total.max(1) as f32;
                    let fill = if n == most { Color32::from_rgb(0xBD, 0xE3, 0xFF) } else { Color32::from_rgb(0xDD, 0xEE, 0xFB) };
                    out.push(Prim::Fill { path: Path::round_rect(x, y, (pw * k).max(20.0), ph, [10.0; 4]), color: with_alpha(fill, alpha) });
                }
                let count = if total > 0 { format!("{n} · {}%", (100.0 * n as f64 / total as f64).round()) } else { "0".into() };
                let label = clip(if o.text.is_empty() { "Opzione" } else { &o.text }, BODY, false, p.w - 96.0);
                one_line(&label, BODY, false, Align::Left, 0.0, p.y + (p.h - BODY * LINE_HEIGHT) / 2.0, p.x + 14.0, if o.text.is_empty() { MUTED } else { INK }, alpha, out);
                one_line(&count, 13.0, true, Align::Right, 0.0, p.y + (p.h - 13.0 * LINE_HEIGHT) / 2.0, p.x + p.w - 14.0, MUTED, alpha, out);
            }
            let votes = match total {
                0 => "Nessun voto: clicca un'opzione per votare".to_string(),
                1 => "1 voto".to_string(),
                n => format!("{n} voti"),
            };
            one_line(&votes, 12.0, false, Align::Left, 0.0, el.h - PAD - 12.0 * LINE_HEIGHT, PAD, MUTED, alpha, out);
        }
        Widget::Checklist { items, .. } => {
            for (p, it) in l.parts.iter().zip(items) {
                let bx = (p.x + 6.0) as f32;
                let by = (p.y + (p.h - 20.0) / 2.0) as f32;
                if it.done {
                    out.push(Prim::Fill { path: Path::round_rect(bx, by, 20.0, 20.0, [6.0; 4]), color: with_alpha(BRAND, alpha) });
                    let mut tick = Path::default();
                    tick.move_to(bx + 5.0, by + 10.5);
                    tick.line_to(bx + 8.5, by + 14.0);
                    tick.line_to(bx + 15.0, by + 6.5);
                    out.push(Prim::Stroke { path: tick, width: 2.2, color: with_alpha(Color32::WHITE, alpha), round: true, dash: None });
                } else {
                    out.push(Prim::Stroke { path: Path::round_rect(bx + 0.75, by + 0.75, 18.5, 18.5, [5.5; 4]), width: 1.5, color: with_alpha(Color32::from_rgb(0xB3, 0xB3, 0xB3), alpha), round: true, dash: None });
                }
                let left = p.x + 6.0 + 32.0;
                let label = clip(if it.text.is_empty() { "Cosa da fare" } else { &it.text }, BODY, false, p.w - 44.0);
                let color = if it.done || it.text.is_empty() { MUTED } else { INK };
                one_line(&label, BODY, false, Align::Left, 0.0, p.y + (p.h - BODY * LINE_HEIGHT) / 2.0, left, color, alpha, out);
                if it.done {
                    let face = text::font(FontKind::Sans, false, false).face;
                    let lw = text::measure(face, &label, BODY);
                    let cy = (p.y + p.h / 2.0) as f32;
                    let mut strike = Path::default();
                    strike.move_to(left as f32, cy);
                    strike.line_to((left + lw) as f32, cy);
                    out.push(Prim::Stroke { path: strike, width: 1.4, color: with_alpha(MUTED, alpha), round: true, dash: None });
                }
            }
            let done = items.iter().filter(|i| i.done).count();
            let (bx, by, bw) = (PAD as f32, (el.h - PAD - 6.0) as f32, (el.w - PAD * 2.0) as f32);
            out.push(Prim::Fill { path: Path::round_rect(bx, by, bw, 6.0, [3.0; 4]), color: with_alpha(TRACK, alpha) });
            if done > 0 {
                out.push(Prim::Fill { path: Path::round_rect(bx, by, (bw * done as f32 / items.len().max(1) as f32).max(6.0), 6.0, [3.0; 4]), color: with_alpha(Color32::from_rgb(0x14, 0xAE, 0x5C), alpha) });
            }
            one_line(&format!("{done} di {} fatte", items.len()), 12.0, false, Align::Right, 0.0, el.h - PAD - 12.0 - 12.0 * LINE_HEIGHT, el.w - PAD, MUTED, alpha, out);
        }
        Widget::Counter { value, .. } => {
            let top = l.title_top + l.title_lines.len().max(1) as f64 * TITLE * LINE_HEIGHT + 8.0;
            one_line(&value.to_string(), 44.0, true, Align::Center, el.w, top, 0.0, INK, alpha, out);
            for p in &l.parts {
                let (cx, cy) = ((p.x + p.w / 2.0) as f32, (p.y + p.h / 2.0) as f32);
                let r = (p.w / 2.0) as f32;
                out.push(Prim::Fill { path: Path::ellipse(cx, cy, r, r), color: with_alpha(TRACK, alpha) });
                let mut sign = Path::default();
                sign.move_to(cx - 7.0, cy);
                sign.line_to(cx + 7.0, cy);
                if p.hot == Hot::Add(1) {
                    sign.move_to(cx, cy - 7.0);
                    sign.line_to(cx, cy + 7.0);
                }
                out.push(Prim::Stroke { path: sign, width: 2.4, color: with_alpha(INK, alpha), round: true, dash: None });
            }
        }
    }
}

fn placeholder(w: &Widget) -> &'static str {
    match w {
        Widget::Poll { .. } => "Fai una domanda",
        Widget::Checklist { .. } => "Titolo della lista",
        Widget::Counter { .. } => "Cosa contiamo?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckItem, Kind, PollOption};

    #[test]
    fn one_vote_each_and_clicks_land_where_drawn() {
        let mut w = Widget::Poll { question: "Pizza o sushi?".into(), options: vec![PollOption { text: "Pizza".into(), votes: vec![] }, PollOption { text: "Sushi".into(), votes: vec![] }] };
        assert!(press(&mut w, Hot::Vote(0), "a"));
        assert!(press(&mut w, Hot::Vote(1), "a"), "voting elsewhere moves the vote");
        press(&mut w, Hot::Vote(1), "b");
        let Widget::Poll { options, .. } = &w else { unreachable!() };
        assert_eq!((options[0].votes.len(), options[1].votes.len()), (0, 2));
        press(&mut w, Hot::Vote(1), "a");
        let Widget::Poll { options, .. } = &w else { unreachable!() };
        assert_eq!(options[1].votes, vec!["b".to_string()], "voting again takes it back");

        let mut el = El::new(Kind::Widget(Widget::Checklist { title: "Spesa".into(), items: vec![CheckItem { text: "Pane".into(), done: false }, CheckItem { text: "Latte".into(), done: false }] }));
        el.w = WIDGET_W;
        fit(&mut el);
        let l = layout(&el, el.widget().unwrap());
        let second = &l.parts[1];
        assert_eq!(hot_at(&el, second.x + 10.0, second.y + second.h / 2.0), Some(Hot::Check(1)));
        assert_eq!(hot_at(&el, 2.0, 2.0), None, "the margin is for dragging");
        assert!(el.h > second.y + second.h, "the card holds every item");
    }
}
