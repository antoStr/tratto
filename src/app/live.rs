//! Working together live: the shared timer, voting, presenting (everyone follows one view),
//! comment threads and cursor chat.

use egui::{Color32, Id, Rect, RichText, Stroke, pos2, vec2};
use serde_json::{Value, json};
use yrs::Map;

use super::board::BoardUi;
use crate::editor::{Editor, Tool, peer_color, peer_name};
use crate::model::{CommentMsg, Kind, hex, is_dark};
use crate::music::Track;
use crate::ui::{self, Kind as Btn, icon};

fn now() -> f64 {
    crate::platform::now_ms()
}

/* ---------------- timer and music ---------------- */

/// The longest timer, as in FigJam: 99:59.
const MAX_MS: f64 = (99.0 * 60.0 + 59.0) * 1000.0;

/// `end`: when it reaches zero (shared clock), None while paused; `left`: ms left when paused.
#[derive(Clone, Copy, PartialEq)]
struct Timer {
    total: f64,
    end: Option<f64>,
    left: f64,
}

impl Timer {
    fn remaining(&self) -> f64 {
        match self.end {
            Some(e) => (e - shared_now()).max(0.0),
            None => self.left.max(0.0),
        }
    }
    fn done(&self) -> bool {
        self.remaining() <= 0.0
    }
    fn paused(&self) -> bool {
        self.end.is_none() && self.left > 0.0
    }
}

fn shared_now() -> f64 {
    crate::platform::shared_now()
}

fn read_timer(ed: &Editor) -> Option<Timer> {
    let b = &ed.board;
    let total = b.timer_field("total").and_then(|v| v.as_f64()).filter(|t| t.is_finite() && *t > 0.0)?;
    let left = b.timer_field("left").and_then(|v| v.as_f64()).filter(|v| v.is_finite())?;
    let end = b.timer_field("end").and_then(|v| v.as_f64()).filter(|v| v.is_finite());
    Some(Timer { total: total.min(MAX_MS * 2.0), end, left: left.min(MAX_MS * 2.0) })
}

fn set_timer(ed: &mut Editor, fields: &[(&str, Value)]) {
    ed.board.transact_plain(|txn, b| {
        for (k, v) in fields {
            b.timer.insert(txn, *k, crate::doc::json_any(v));
        }
    });
}

fn start_timer(ed: &mut Editor, ms: f64) {
    let ms = ms.clamp(1000.0, MAX_MS);
    set_timer(ed, &[("total", json!(ms)), ("left", json!(ms)), ("end", json!(shared_now() + ms))]);
}

/// Stops the timer for everyone; the music, if any, carries on.
fn stop_timer(ed: &mut Editor) {
    ed.board.transact_plain(|txn, b| {
        for k in ["total", "left", "end"] {
            b.timer.remove(txn, k);
        }
    });
}

/// `mm:ss`, as FigJam shows it.
fn clock(ms: f64) -> String {
    let s = (ms / 1000.0).ceil().max(0.0) as i64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// The piece playing for everyone on this board, and when it started (shared clock).
fn read_music(ed: &Editor) -> Option<(Track, f64)> {
    let id = ed.board.timer_field("music")?;
    let at = ed.board.timer_field("musicAt").and_then(|v| v.as_f64()).filter(|v| v.is_finite())?;
    Some((Track::from_id(id.as_str()?)?, at))
}

fn set_music(ed: &mut Editor, track: Option<Track>) {
    match track {
        Some(t) => set_timer(ed, &[("music", json!(t.id())), ("musicAt", json!(shared_now()))]),
        None => ed.board.transact_plain(|txn, b| {
            b.timer.remove(txn, "music");
            b.timer.remove(txn, "musicAt");
        }),
    }
}

/// Every frame on a board: the music this person hears (at their own volume) and, when the
/// timer runs out while they are there, the chime and the shake.
pub fn sound(ctx: &egui::Context, ed: &Editor) {
    let volume = if ed.prefs.music_muted { 0.0 } else { ed.prefs.music_volume };
    crate::audio::music(read_music(ed), volume);
    let id = Id::new("timer-running");
    let running = read_timer(ed).is_some_and(|t| !t.done());
    let was = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if was && !running && read_timer(ed).is_some_and(|t| t.done()) {
        ctx.data_mut(|d| d.insert_temp(Id::new("timer-done-at"), crate::platform::now_ms()));
        if ed.prefs.timer_sound {
            crate::audio::chime(0.8);
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, running));
}

/// How far the time shakes sideways (points), in the second after it runs out.
fn shake(ctx: &egui::Context) -> f32 {
    let Some(at) = ctx.data(|d| d.get_temp::<f64>(Id::new("timer-done-at"))) else { return 0.0 };
    let t = ((crate::platform::now_ms() - at) / 1000.0) as f32;
    if !(0.0..0.9).contains(&t) || ui::motion::reduced(ctx) {
        return 0.0;
    }
    ctx.request_repaint();
    5.0 * (-t / 0.28).exp() * (t * std::f32::consts::TAU * 9.0).sin()
}

/// The time's colour and opacity: red when it ran out, blinking slowly while paused (as in
/// FigJam, so everyone notices it stopped).
fn time_look(ctx: &egui::Context, tm: &Timer, t: &ui::Theme) -> Color32 {
    if tm.done() {
        return t.danger_text;
    }
    if tm.paused() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        let k = 0.5 + 0.5 * (crate::platform::now_ms() / 1000.0 * std::f64::consts::TAU / 1.2).cos() as f32;
        return t.text.gamma_multiply(0.35 + 0.65 * k);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    t.text
}

/// A ring that empties as time goes by.
fn ring(p: &egui::Painter, c: egui::Pos2, r: f32, k: f32, color: Color32, track: Color32) {
    p.circle_stroke(c, r, Stroke::new(2.0, track));
    let k = k.clamp(0.0, 1.0);
    if k > 0.0 {
        let n = (48.0 * k).ceil().max(2.0) as usize;
        let pts: Vec<egui::Pos2> = (0..=n).map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * k * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        }).collect();
        p.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
    }
}

/// Three little bars dancing while music plays.
fn eq_bars(ui: &egui::Ui, c: egui::Pos2, h: f32, color: Color32) {
    let t = crate::platform::now_ms() / 1000.0;
    let still = ui::motion::reduced(ui.ctx());
    for i in 0..3 {
        let k = if still { 0.6 } else { 0.35 + 0.65 * ((t * (5.0 + i as f64 * 1.7) + i as f64).sin().abs() as f32) };
        let x = c.x + (i as f32 - 1.0) * 4.0;
        let bh = h * k;
        ui.painter().rect_filled(Rect::from_min_max(pos2(x - 1.25, c.y + h / 2.0 - bh), pos2(x + 1.25, c.y + h / 2.0)), 1.0, color);
    }
    if !still {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
    }
}

/// FigJam's "Timer, voting and music" button: the countdown with its ring while a timer runs,
/// dancing bars while music plays, or the timer's icon; it opens the timer and music panel.
/// Everyone sees it, also who can only look (they change only their own volume).
pub fn timer_button(ui: &mut egui::Ui, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    let tm = read_timer(ed);
    let music = read_music(ed);
    let resp = match tm {
        Some(tm) => {
            let font = ui::medium(ui::px(12.0));
            let text = clock(tm.remaining());
            let g = ui.painter().layout_no_wrap(text.clone(), font, Color32::PLACEHOLDER);
            let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 34.0 + if music.is_some() { 14.0 } else { 0.0 }, ui::hit(ui.ctx(), 24.0)), egui::Sense::click());
            let dx = shake(ui.ctx());
            let r = rect.translate(vec2(dx, 0.0));
            let bg = if tm.done() { t.danger.gamma_multiply(0.12) } else { t.selected };
            ui.painter().rect_filled(r, 12.0, ui::blend(bg, t.hover, ui::motion::hover(ui.ctx(), resp.id, resp.hovered()) * 0.5));
            ring(ui.painter(), pos2(r.min.x + 13.0, r.center().y), 6.0, (tm.remaining() / tm.total) as f32, if tm.done() { t.danger } else { t.brand }, t.bg3);
            let color = time_look(ui.ctx(), &tm, &t);
            ui.painter().galley(pos2(r.min.x + 25.0, r.center().y - g.size().y / 2.0), g, color);
            if music.is_some() {
                eq_bars(ui, pos2(r.max.x - 12.0, r.center().y), 10.0, t.brand);
            }
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Timer: {text}")));
            ui::focus_ring(ui, &resp, rect);
            ui::tip(resp, "Timer e musica", None)
        }
        None => {
            let s = ui::hit(ui.ctx(), 24.0);
            let r = ui::icon_button(ui, if music.is_some() { "" } else { "timer" }, "Timer e musica", None, vec2(s, s), 16.0, music.is_some(), false);
            if music.is_some() {
                eq_bars(ui, r.rect.center(), 11.0, t.brand);
            }
            r
        }
    };
    egui::Popup::from_toggle_button_response(&resp)
        .frame(ui::float_frame(&t).inner_margin(egui::Margin::same(14)))
        .align(egui::RectAlign::BOTTOM_END)
        .gap(8.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| timer_panel(ui, ed));
}

/// The panel: the timer (time to set, or the countdown and its controls) and the music.
fn timer_panel(ui: &mut egui::Ui, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    ui.set_width(292.0);
    ui.spacing_mut().item_spacing.y = 10.0;
    let ro = ed.read_only;
    ui::heading(ui, "Timer");
    match read_timer(ed) {
        None => {
            // The time to start with: big, with − and + (Maiusc: 5 minutes), the arrow keys
            // too, and Invio to start; or one of the quick picks.
            let draft_id = Id::new("timer-draft");
            let mut draft: f64 = ui.data(|d| d.get_temp(draft_id)).unwrap_or(5.0 * 60_000.0);
            let step = if ui.input(|i| i.modifiers.shift) { 5.0 * 60_000.0 } else { 60_000.0 };
            let typing = ui.ctx().egui_wants_keyboard_input();
            if !ro && !typing {
                let (up, down, enter) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Enter)));
                if up {
                    draft += step;
                }
                if down {
                    draft -= step;
                }
                if enter {
                    start_timer(ed, draft);
                    egui::Popup::close_all(ui.ctx());
                }
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let w = ui.available_width();
                ui.add_space((w - 228.0).max(0.0) / 2.0);
                if ui::icon_button(ui, "minus", "Un minuto in meno (Maiusc: cinque)", Some("↓"), vec2(36.0, 36.0), 18.0, false, false).clicked() && !ro {
                    draft -= step;
                }
                let edit_id = Id::new("timer-draft-edit");
                let mut text: String = ui.data(|d| d.get_temp(edit_id)).unwrap_or_else(|| clock(draft));
                let resp = ui.add_sized(vec2(140.0, 44.0), egui::TextEdit::singleline(&mut text).font(ui::medium(ui::px(32.0))).horizontal_align(egui::Align::Center).frame(egui::Frame::NONE).interactive(!ro).id(edit_id));
                if resp.has_focus() {
                    ui.data_mut(|d| d.insert_temp(edit_id, text.clone()));
                } else {
                    ui.data_mut(|d| d.remove::<String>(edit_id));
                }
                if resp.lost_focus() {
                    if let Some(ms) = parse_time(&text) {
                        draft = ms;
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ro {
                            start_timer(ed, draft);
                            egui::Popup::close_all(ui.ctx());
                        }
                    }
                    ui.data_mut(|d| d.remove::<String>(edit_id));
                }
                if ui::icon_button(ui, "plus", "Un minuto in più (Maiusc: cinque)", Some("↑"), vec2(36.0, 36.0), 18.0, false, false).clicked() && !ro {
                    draft += step;
                }
            });
            draft = draft.clamp(60_000.0, MAX_MS);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for m in [1, 5, 10, 15, 25] {
                    let on = (draft - m as f64 * 60_000.0).abs() < 1.0;
                    if chip(ui, &format!("{m} min"), on, &t).clicked() && !ro {
                        draft = m as f64 * 60_000.0;
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(draft_id, draft));
            if ro {
                ui::hint(ui, "Solo chi modifica può avviarlo.");
            } else if ui::full_button(ui, "Avvia per tutti", Btn::Primary, Some("play")).clicked() {
                start_timer(ed, draft);
                egui::Popup::close_all(ui.ctx());
            }
        }
        Some(tm) => {
            let left = tm.remaining();
            let color = time_look(ui.ctx(), &tm, &t);
            let dx = shake(ui.ctx());
            ui.vertical_centered(|ui| {
                let g = ui.painter().layout_no_wrap(clock(left), ui::medium(ui::px(36.0)), color);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y + 4.0), egui::Sense::hover());
                ui.painter().galley(pos2(r.center().x - g.size().x / 2.0 + dx, r.min.y), g, color);
                let label = if tm.done() { "Tempo scaduto" } else if tm.paused() { "In pausa" } else { "Per tutti sulla lavagna" };
                ui.label(RichText::new(label).size(ui::px(11.0)).color(if tm.done() { t.danger_text } else { t.text2 }));
            });
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), egui::Sense::hover());
            ui.painter().rect_filled(bar, 2.0, t.bg3);
            ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * (left / tm.total).clamp(0.0, 1.0) as f32, 4.0)), 2.0, if tm.done() { t.danger } else { t.brand_fill });
            if !ro {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if !tm.done() {
                        match tm.end {
                            Some(end) => {
                                if ui::button(ui, "Pausa", Btn::Secondary, Some("pause"), false, true).clicked() {
                                    set_timer(ed, &[("left", json!((end - shared_now()).max(0.0))), ("end", Value::Null)]);
                                }
                            }
                            None => {
                                if ui::button(ui, "Riprendi", Btn::Primary, Some("play"), false, true).clicked() {
                                    set_timer(ed, &[("end", json!(shared_now() + tm.left))]);
                                }
                            }
                        }
                    }
                    // Time can be added, not taken away (as in FigJam), up to 99:59.
                    if ui::button(ui, "+1 min", Btn::Secondary, None, false, true).clicked() {
                        add_minute(ed, &tm);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui::button(ui, "Ferma", Btn::DangerText, Some("square"), false, true).clicked() {
                            stop_timer(ed);
                        }
                    });
                });
            }
        }
    }
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
    music_section(ui, ed, &t);
}

fn add_minute(ed: &mut Editor, tm: &Timer) {
    let room = (MAX_MS - tm.remaining()).clamp(0.0, 60_000.0);
    if room <= 0.0 {
        return;
    }
    match tm.end {
        Some(end) => set_timer(ed, &[("total", json!(tm.total + room)), ("end", json!(end.max(shared_now()) + room))]),
        None => set_timer(ed, &[("total", json!(tm.total + room)), ("left", json!(tm.left + room))]),
    }
}

/// "5", "5:30", "05:30" or "90s" as a time.
fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches("min").trim();
    let ms = if let Some(sec) = s.strip_suffix('s') {
        sec.trim().parse::<f64>().ok()? * 1000.0
    } else if let Some((m, sec)) = s.split_once([':', '.', ',']) {
        (m.trim().parse::<f64>().ok()? * 60.0 + sec.trim().parse::<f64>().ok()?) * 1000.0
    } else {
        s.parse::<f64>().ok()? * 60_000.0
    };
    (ms.is_finite() && ms > 0.0).then(|| ms.min(MAX_MS))
}

fn chip(ui: &mut egui::Ui, label: &str, on: bool, t: &ui::Theme) -> egui::Response {
    let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(ui::px(11.0)), Color32::PLACEHOLDER);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 26.0), egui::Sense::click());
    let h = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    ui.painter().rect_filled(r, 13.0, if on { t.selected } else { ui::blend(t.bg2, t.hover, h) });
    if on {
        ui.painter().rect_stroke(r, 13.0, Stroke::new(1.0, t.brand), egui::StrokeKind::Inside);
    }
    ui.painter().galley(r.center() - g.size() / 2.0, g, if on { t.text } else { t.text2 });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label));
    ui::focus_ring(ui, &resp, r);
    resp
}

/// A button as wide as the panel.
/// The music: one piece for everyone (anyone who can edit picks or stops it) and this
/// person's own volume.
fn music_section(ui: &mut egui::Ui, ed: &mut Editor, t: &ui::Theme) {
    let playing = read_music(ed).map(|m| m.0);
    ui.horizontal(|ui| {
        ui::heading(ui, "Musica");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("la sentono tutti").size(ui::px(10.0)).color(t.text2));
        });
    });
    ui.spacing_mut().item_spacing.y = 2.0;
    for (track, _, name, about, icon_name) in crate::music::TRACKS {
        let on = playing == Some(track);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), egui::Sense::click());
        let h = ui::motion::hover(ui.ctx(), resp.id, resp.hovered() && !ed.read_only);
        ui.painter().rect_filled(r, 8.0, if on { t.selected } else { t.hover.gamma_multiply(h) });
        let tile = Rect::from_min_size(r.min + vec2(6.0, 6.0), vec2(32.0, 32.0));
        ui.painter().rect_filled(tile, 8.0, if on { t.brand_fill } else { t.bg2 });
        icon(ui, icon_name, tile.center(), 16.0, if on { Color32::WHITE } else { t.icon2 });
        ui.painter().text(pos2(tile.max.x + 10.0, r.center().y - 7.0), egui::Align2::LEFT_CENTER, name, ui::medium(ui::px(12.0)), t.text);
        ui.painter().text(pos2(tile.max.x + 10.0, r.center().y + 8.0), egui::Align2::LEFT_CENTER, about, egui::FontId::proportional(ui::px(10.5)), t.text2);
        if on {
            eq_bars(ui, pos2(r.max.x - 18.0, r.center().y), 12.0, t.brand);
        } else if resp.hovered() && !ed.read_only {
            icon(ui, "play", pos2(r.max.x - 18.0, r.center().y), 14.0, t.icon2);
        }
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, !ed.read_only, on, name));
        ui::focus_ring(ui, &resp, r);
        if resp.clicked() && !ed.read_only {
            set_music(ed, if on { None } else { Some(track) });
        }
    }
    ui.spacing_mut().item_spacing.y = 10.0;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let p = &mut ed.prefs;
        let muted = p.music_muted || p.music_volume <= 0.001;
        if ui::icon_button(ui, if muted { "volume-x" } else { "volume-2" }, if muted { "Riattiva l'audio" } else { "Silenzia (solo per te)" }, None, vec2(28.0, 28.0), 16.0, muted, false).clicked() {
            p.music_muted = !p.music_muted;
            if !p.music_muted && p.music_volume <= 0.001 {
                p.music_volume = 0.6;
            }
        }
        let mut v = p.music_volume as f64;
        if ui::slider_inline(ui, "Volume", &mut v, 0.0, 1.0, 0.05, false, |v| format!("{}%", (v * 100.0).round()), ui.available_width()) {
            p.music_volume = v as f32;
            p.music_muted = false;
        }
    });
    if playing.is_some() && !ed.read_only && ui::full_button(ui, "Ferma la musica", Btn::Secondary, Some("square")).clicked() {
        set_music(ed, None);
    }
}

/// The countdown as a banner at the top of the board, for everyone (Settings ▸ Personalizzazione).
fn timer_banner(ui: &mut egui::Ui, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    let Some(tm) = read_timer(ed) else { return };
    let left = tm.remaining();
    let color = time_look(ui.ctx(), &tm, &t);
    let dx = shake(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::hover());
    ring(ui.painter(), r.center() + vec2(dx, 0.0), 8.0, (left / tm.total) as f32, if tm.done() { t.danger } else { t.brand }, t.bg3);
    let g = ui.painter().layout_no_wrap(clock(left), ui::medium(ui::px(15.0)), color);
    let (r, _) = ui.allocate_exact_size(vec2(g.size().x, 24.0), egui::Sense::hover());
    ui.painter().galley(pos2(r.min.x + dx, r.center().y - g.size().y / 2.0), g, color);
    if !ed.read_only {
        if !tm.done() {
            if let Some(end) = tm.end {
                if ui::small_icon_button(ui, "pause", "Pausa", false).clicked() {
                    set_timer(ed, &[("left", json!((end - shared_now()).max(0.0))), ("end", Value::Null)]);
                }
            } else if ui::small_icon_button(ui, "play", "Riprendi", false).clicked() {
                set_timer(ed, &[("end", json!(shared_now() + tm.left))]);
            }
        }
        if ui::button(ui, "+1 min", Btn::Ghost, None, false, true).clicked() {
            add_minute(ed, &tm);
        }
        if ui::small_icon_button(ui, "square", "Ferma il timer", false).clicked() {
            stop_timer(ed);
        }
    }
}

/* ---------------- emotes ---------------- */

/// A 3D emoji picture in the interface.
fn emoji(ui: &egui::Ui, ed: &mut Editor, e: &str, rect: Rect) {
    if let Some(Ok(tex)) = ed.painter.images.texture(ui.ctx(), &crate::prims::ImageKey::Stamp(e.to_string())) {
        ui.painter().image(tex, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
}

/// Emotes without a keyboard (with one: hold E): a feeling that floats up for everyone from
/// the middle of the view.
pub fn emote_button(ui: &mut egui::Ui, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    let s = ui::hit(ui.ctx(), 24.0);
    let (r, resp) = ui.allocate_exact_size(vec2(s, s), egui::Sense::click());
    ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
    emoji(ui, ed, "❤️", Rect::from_center_size(r.center(), vec2(17.0, 17.0)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Emote"));
    ui::focus_ring(ui, &resp, r);
    let resp = ui::tip(resp, "Emote", Some("tieni premuto E"));
    egui::Popup::from_toggle_button_response(&resp)
        .frame(ui::float_frame(&t).inner_margin(egui::Margin::same(6)))
        .align(egui::RectAlign::BOTTOM_END)
        .gap(8.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for e in crate::editor::EMOTES {
                    let (r, b) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::click());
                    let h = ui::motion::hover(ui.ctx(), b.id, b.hovered());
                    ui.painter().rect_filled(r, 8.0, t.hover.gamma_multiply(h));
                    emoji(ui, ed, e, Rect::from_center_size(r.center(), vec2(26.0 + 4.0 * h, 26.0 + 4.0 * h)));
                    b.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::assets::stamp(e).map_or(e, |s| s.name)));
                    if b.clicked() {
                        let at = ed.viewport_center();
                        ed.send_emote(e, at);
                    }
                }
            });
        });
}

/* ---------------- voting ---------------- */

pub fn vote_button(ui: &mut egui::Ui, ed: &mut Editor) {
    if ed.read_only {
        return;
    }
    let t = ui::theme(ui.ctx());
    let voting = ed.board.voting();
    let r = ui::icon_button(ui, "vote", "Votazione", None, vec2(ui::hit(ui.ctx(), 24.0), ui::hit(ui.ctx(), 24.0)), 16.0, voting.is_some(), false);
    egui::Popup::from_toggle_button_response(&r).frame(ui::float_frame(&t).inner_margin(egui::Margin::same(12))).align(egui::RectAlign::BOTTOM_END).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(240.0);
        ui::heading(ui, "Votazione");
        ui::hint(ui, "I voti restano nascosti fino alla fine.");
        ui::heading(ui, "Voti per persona");
        let id = Id::new("vote-per");
        let mut per: u32 = ui.data(|d| d.get_temp(id)).unwrap_or(3);
        ui::segmented(ui, &mut per, &[(1, "1"), (3, "3"), (5, "5"), (10, "10")], 216.0);
        ui.data_mut(|d| d.insert_temp(id, per));
        if ui::button(ui, if voting.is_some() { "Ricomincia la votazione" } else { "Inizia la votazione" }, Btn::Primary, None, false, true).clicked() {
            ed.board.start_voting(per);
            ed.set_tool(Tool::Select);
            egui::Popup::close_all(ui.ctx());
        }
    });
}

/* ---------------- presenting ---------------- */

fn spotlight_of(s: &Value) -> Option<f64> {
    s["spotlight"].as_f64().filter(|v| v.is_finite())
}

pub fn spotlight_button(ui: &mut egui::Ui, ed: &mut Editor) {
    let on = ed.presence.local.get("spotlight").and_then(|v| v.as_f64()).is_some();
    let label = if on { "Smetti di presentare" } else { "Presenta" };
    if ui::icon_button(ui, "presentation", label, None, vec2(ui::hit(ui.ctx(), 24.0), ui::hit(ui.ctx(), 24.0)), 16.0, on, false).clicked() {
        ed.presence.set("spotlight", if on { Value::Null } else { json!(now()) });
    }
}

#[derive(Clone, Default)]
struct Follow {
    /// Presentation the person stopped following on purpose.
    dismissed: Option<String>,
    /// Presenter we started following for the presentation.
    auto: Option<u64>,
}

/// Timer, voting and presenting bars at the top of the board; follows whoever presents.
pub fn top_bars(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let t = ui::theme(ctx);
    let fid = Id::new("follow-state");
    let mut f: Follow = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    let mine = ed.presence.local.get("spotlight").and_then(|v| v.as_f64()).is_some();
    let presenter = ed.presence.peers.iter().filter_map(|(id, s)| spotlight_of(s).map(|at| (*id, at, peer_name(s)))).max_by(|a, b| a.1.total_cmp(&b.1));
    match &presenter {
        Some((id, at, _)) if !mine => {
            let key = format!("{id}:{at}");
            if f.dismissed.as_deref() != Some(key.as_str()) {
                if ed.following != Some(*id) && f.auto == Some(*id) {
                    // They moved away themselves: leave them be until the next presentation.
                    f.dismissed = Some(key);
                    f.auto = None;
                } else if f.auto != Some(*id) {
                    f.auto = Some(*id);
                    ed.follow(Some(*id));
                }
            }
        }
        _ => {
            if f.auto.is_some() && ed.following == f.auto {
                ed.follow(None);
            }
            f.auto = None;
        }
    }
    if ed.following.is_some() {
        ed.apply_follow();
    }

    // A coloured frame around the board while following someone.
    if let Some(id) = ed.following
        && let Some(s) = ed.presence.peer(id)
    {
        let color = peer_color(s);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("follow-frame")));
        painter.rect_stroke(stage.shrink(1.5), 0.0, Stroke::new(3.0, color), egui::StrokeKind::Inside);
        let label = format!("Stai seguendo {}", peer_name(s));
        let on = if is_dark(&hex(color)) { Color32::WHITE } else { crate::model::DARK };
        let g = painter.layout_no_wrap(label, ui::medium(ui::px(11.0)), on);
        let r = Rect::from_center_size(pos2(stage.center().x, stage.min.y + 12.0), g.size() + vec2(16.0, 6.0));
        painter.rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }, color);
        painter.galley(r.center() - g.size() / 2.0, g, on);
    }

    let timer = read_timer(ed);
    let timer_floating = timer.is_some() && ed.prefs.timer_banner;
    let voting = ed.board.voting();
    let show_any = timer_floating || voting.is_some() || mine || presenter.is_some();
    let k = ui::motion::presence(ctx, Id::new("live-top-anim"), show_any);
    if k <= 0.01 && !show_any {
        ui::motion::reset(ctx, Id::new("live-top-anim"));
        ctx.data_mut(|d| d.insert_temp(fid, f));
        return;
    }
    let is_mobile = stage.width() < 640.0;
    let travel = ui::motion::travel(ctx, k);
    // Where the bars go: the first place that covers nothing else on screen (the pills, the
    // minimap, the zoom controls, the toolbar and its options): at the top in the middle, at
    // the top beside the minimap, below the pills, or at the bottom above the toolbar.
    let size = ctx.data(|d| d.get_temp::<egui::Vec2>(Id::new("live-top-size"))).unwrap_or(vec2(320.0, 40.0));
    let obstacles: Vec<Rect> = ["left-pill-rect", "right-pill-rect", "minimap-rect", "view-controls-rect", "bottom-bars", "toolbar-rect", "ctx-bar-rect"].iter().filter_map(|k| ctx.data(|d| d.get_temp::<Rect>(Id::new(*k)))).collect();
    let fits = |r: Rect| stage.shrink(4.0).contains_rect(r) && !obstacles.iter().any(|o| r.intersects(o.expand(6.0)));
    let top = if is_mobile { stage.min.y + 54.0 } else { stage.min.y + 32.0 };
    let below_pills = obstacles.iter().filter(|o| o.min.y < stage.min.y + 80.0).map(|o| o.max.y + 10.0).fold(top, f32::max);
    let beside_minimap = ctx.data(|d| d.get_temp::<Rect>(Id::new("minimap-rect"))).map(|m| (stage.min.x + 12.0 + (m.min.x - 24.0 - stage.min.x) / 2.0, top));
    let bottom = ctx.data(|d| d.get_temp::<Rect>(Id::new("bottom-bars"))).map_or(stage.max.y - 80.0, |b| b.min.y - 10.0) - size.y;
    let mut candidates = vec![(stage.center().x, top)];
    candidates.extend(beside_minimap);
    candidates.extend([(stage.center().x, below_pills), (stage.center().x, bottom)]);
    let (cx, base_y) = candidates.iter().copied().find(|&(x, y)| fits(Rect::from_min_size(pos2(x - size.x / 2.0, y), size))).unwrap_or((stage.center().x, bottom));
    let y = base_y - travel * 16.0;
    let resp = egui::Area::new(Id::new("live-top")).pivot(egui::Align2::CENTER_TOP).fixed_pos(pos2(cx, y)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui.vertical_centered(|ui| {
            if timer_floating {
                pill(ui, &t, |ui| {
                    timer_banner(ui, ed);
                });
                ui.add_space(6.0);
            }
            if let Some((per, ended)) = voting {
                pill(ui, &t, |ui| {
                    icon(ui, "vote", ui.cursor().min + vec2(7.0, 12.0), 14.0, t.icon);
                    ui.add_space(18.0);
                    if !ended {
                        let used: u32 = ed.board.tally(Some(&ed.voter)).values().sum();
                        let text = if ed.read_only { "Votazione".to_string() } else { format!("Votazione · ti restano {} voti su {per}", per.saturating_sub(used)) };
                        ui.label(RichText::new(text).color(t.text)).on_hover_text("Clic per votare · Maiusc+clic per togliere");
                        if !ed.read_only && ui::button(ui, "Termina", Btn::Secondary, None, false, true).clicked() {
                            ed.board.end_voting();
                        }
                    } else {
                        let mut results: Vec<(String, u32)> = ed.board.tally(None).into_iter().collect();
                        results.sort_by(|a, b| b.1.cmp(&a.1));
                        ui.label(RichText::new(if results.is_empty() { "Nessun voto." } else { "Più votati:" }).color(t.text));
                        for (id, n) in results.into_iter().take(3) {
                            if let Some(el) = ed.board.get(&id).cloned() {
                                let label: String = super::panels::element_label(&el).chars().take(18).collect();
                                if ui::button(ui, &format!("{label} · {n}"), Btn::Ghost, None, false, true).clicked() {
                                    ed.reveal(&[id]);
                                }
                            }
                        }
                        if !ed.read_only && ui::button(ui, "Chiudi", Btn::Secondary, None, false, true).clicked() {
                            ed.board.clear_voting();
                        }
                    }
                });
                ui.add_space(6.0);
            }
            if mine {
                pill(ui, &t, |ui| {
                    icon(ui, "presentation", ui.cursor().min + vec2(7.0, 12.0), 14.0, t.icon);
                    ui.add_space(18.0);
                    ui.label(RichText::new("Stai presentando").color(t.text));
                    if ui::button(ui, "Interrompi", Btn::Secondary, None, false, true).clicked() {
                        ed.presence.set("spotlight", Value::Null);
                    }
                });
            } else if let Some((id, at, name)) = &presenter {
                pill(ui, &t, |ui| {
                    icon(ui, "presentation", ui.cursor().min + vec2(7.0, 12.0), 14.0, t.icon);
                    ui.add_space(18.0);
                    ui.label(RichText::new(format!("{name} sta presentando")).color(t.text));
                    if ed.following == Some(*id) {
                        if ui::button(ui, "Smetti di seguire", Btn::Secondary, None, false, true).clicked() {
                            ed.follow(None);
                            f.dismissed = Some(format!("{id}:{at}"));
                            f.auto = None;
                        }
                    } else if ui::button(ui, "Segui", Btn::Secondary, None, false, true).clicked() {
                        f.dismissed = None;
                        f.auto = Some(*id);
                        ed.follow(Some(*id));
                    }
                });
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("live-top-size"), resp.response.rect.size()));
    ctx.data_mut(|d| d.insert_temp(fid, f));
}

fn pill(ui: &mut egui::Ui, t: &ui::Theme, body: impl FnOnce(&mut egui::Ui)) {
    ui::float_frame(t).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            body(ui);
        });
    });
}

/* ---------------- comments ---------------- */

fn when(t: f64) -> String {
    let s = (now() - t) / 1000.0;
    if s < 60.0 {
        "adesso".into()
    } else if s < 3600.0 {
        let m = (s / 60.0).round();
        if m == 1.0 { "1 minuto fa".into() } else { format!("{m} minuti fa") }
    } else if s < 86_400.0 {
        let h = (s / 3600.0).round();
        if h == 1.0 { "1 ora fa".into() } else { format!("{h} ore fa") }
    } else {
        let d = (s / 86_400.0).round();
        if d == 1.0 { "ieri".into() } else { format!("{d} giorni fa") }
    }
}

/// The open comment's thread, beside its pin: messages, reply, resolve.
pub fn comment_thread(ctx: &egui::Context, ed: &mut Editor, st: &mut BoardUi) {
    let t = ui::theme(ctx);
    let open_id = Id::new("comment-open");
    // Stored as Option<String> (no comment open is remembered too), read back as the same type.
    let was: Option<String> = ctx.data(|d| d.get_temp::<Option<String>>(open_id)).flatten();
    let fresh = was != ed.comment;
    if fresh {
        // Closing a new comment without writing anything takes the pin away again.
        if let Some(old) = &was
            && ed.board.get(old).is_some_and(|e| matches!(&e.kind, Kind::Comment { thread } if thread.is_empty()))
        {
            ed.board.remove([old.clone()]);
        }
        st.comment_draft.clear();
        ctx.data_mut(|d| d.insert_temp(open_id, ed.comment.clone()));
    }
    let Some(id) = ed.comment.clone() else { return };
    let Some(el) = ed.board.get(&id).cloned() else {
        ed.comment = None;
        return;
    };
    let Kind::Comment { thread } = &el.kind else { return };
    let p = ed.to_screen(el.x, el.y) + ed.origin.to_vec2();
    let pin = (crate::prims::COMMENT_PIN) as f32;
    let screen = ctx.content_rect();
    let at = pos2((p.x + pin + 10.0).clamp(8.0, screen.max.x - 300.0), (p.y - pin).clamp(8.0, screen.max.y - 320.0));
    let mut close = false;
    let k = ui::motion::appear(ctx, Id::new("comment-thread-anim"), true);
    let lift = ui::motion::travel(ctx, k) * 8.0;
    let shown = egui::Area::new(Id::new("comment-thread")).fixed_pos(at + vec2(0.0, lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        // Room to grow as replies come (an area lays out in last frame's size).
        ui.set_max_size(ctx.content_rect().size());
        ui::float_frame(&t).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(268.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Commento").font(ui::medium(ui::px(12.0))).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                        close = true;
                    }
                    if !ed.read_only && !thread.is_empty() && ui::small_icon_button(ui, "check", "Risolvi", false).clicked() {
                        ed.board.remove([id.clone()]);
                        close = true;
                    }
                });
            });
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for (i, m) in thread.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        ui::avatar(ui, &m.author, crate::model::parse_color(&m.color).unwrap_or(Color32::from_rgb(0x97, 0x47, 0xFF)), 24.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&m.author).font(ui::medium(ui::px(11.0))).color(t.text));
                                ui.label(RichText::new(when(m.t)).size(ui::px(10.0)).color(t.text2));
                                if !ed.read_only && i > 0 && ui::small_icon_button(ui, "trash-2", "Elimina la risposta", false).clicked() {
                                    let mut next = thread.clone();
                                    next.remove(i);
                                    ed.board.update(std::slice::from_ref(&id), |el| el.kind = Kind::Comment { thread: next.clone() });
                                }
                            });
                            ui.label(RichText::new(&m.text).color(t.text));
                        });
                    });
                    ui.add_space(4.0);
                }
            });
            if !ed.read_only {
                let resp = ui.add(egui::TextEdit::multiline(&mut st.comment_draft).desired_rows(2).desired_width(f32::INFINITY).hint_text(if thread.is_empty() { "Scrivi un commento…" } else { "Rispondi…" }).char_limit(2000));
                if ctx.memory(|m| m.focused().is_none()) {
                    resp.request_focus();
                }
                let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                let send = ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui::button(ui, "Invia", Btn::Primary, None, false, !st.comment_draft.trim().is_empty()).clicked()).inner
                });
                if (send.inner || enter) && !st.comment_draft.trim().is_empty() {
                    let me = Value::Object(ed.presence.local.clone());
                    let msg = CommentMsg { author: peer_name(&me), color: hex(peer_color(&me)), text: st.comment_draft.trim().chars().take(2000).collect(), t: now() };
                    let mut next = thread.clone();
                    next.push(msg);
                    ed.board.update(std::slice::from_ref(&id), |el| el.kind = Kind::Comment { thread: next.clone() });
                    st.comment_draft.clear();
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });
    });
    // A press anywhere else (the board, a menu, a panel) closes it, like a menu.
    let away = ctx.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| !shown.response.rect.contains(p)));
    if close || (away && !fresh) {
        ed.comment = None;
        ui::motion::reset(ctx, Id::new("comment-thread-anim"));
    }
}

/* ---------------- cursor chat ---------------- */

/// Press / and type: the others see it by your cursor.
pub fn cursor_chat(ctx: &egui::Context, ed: &mut Editor, st: &mut BoardUi) {
    if st.chat_clear.is_some_and(|t| now() >= t) {
        st.chat_clear = None;
        ed.presence.set("chat", Value::Null);
    }
    let Some(text) = &mut st.chat else { return };
    let t = ui::theme(ctx);
    let at = ctx.input(|i| i.pointer.hover_pos()).unwrap_or(ctx.content_rect().center()) + vec2(14.0, 18.0);
    let me = Value::Object(ed.presence.local.clone());
    let color = peer_color(&me);
    let on = if is_dark(&hex(color)) { Color32::WHITE } else { crate::model::DARK };
    let mut close: Option<bool> = None;
    let k = ui::motion::appear(ctx, Id::new("cursor-chat-anim"), true);
    let lift = ui::motion::travel(ctx, k) * 6.0;
    egui::Area::new(Id::new("cursor-chat")).fixed_pos(at + vec2(0.0, lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        egui::Frame::new().fill(color).corner_radius(egui::CornerRadius { nw: 4, ne: 14, sw: 14, se: 14 }).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            let resp = ui.add(egui::TextEdit::singleline(text).frame(egui::Frame::NONE).text_color(on).hint_text(RichText::new("Scrivi un messaggio…").color(ui::mix(on, 0.6, color))).desired_width(220.0).char_limit(80).id(Id::new("chat-input")));
            resp.request_focus();
            if resp.changed() {
                let v = if text.trim().is_empty() { Value::Null } else { json!(text.clone()) };
                ed.presence.set("chat", v);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = Some(false);
            } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) || resp.lost_focus() {
                close = Some(true);
            }
        });
    });
    let _ = t;
    // The message stays a few seconds after you finish, then fades for everyone.
    if let Some(keep) = close {
        st.chat = None;
        ui::motion::reset(ctx, Id::new("cursor-chat-anim"));
        if keep {
            st.chat_clear = Some(now() + 5000.0);
            ctx.request_repaint_after(std::time::Duration::from_millis(5100));
        } else {
            ed.presence.set("chat", Value::Null);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn times_are_typed_and_shown_as_in_figjam() {
        assert_eq!(super::clock(5.0 * 60_000.0), "05:00");
        assert_eq!(super::clock(61_001.0), "01:02", "a second just begun still counts");
        assert_eq!(super::parse_time("5"), Some(300_000.0));
        assert_eq!(super::parse_time("2:30"), Some(150_000.0));
        assert_eq!(super::parse_time(" 90s "), Some(90_000.0));
        assert_eq!(super::parse_time("200"), Some(super::MAX_MS), "at most 99:59");
        assert_eq!(super::parse_time("dieci"), None);
        assert_eq!(super::parse_time("0"), None);
    }
}
