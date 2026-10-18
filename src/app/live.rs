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

/// The longest timer: 99 hours, 59 minutes and 59 seconds.
const MAX_MS: f64 = (99.0 * 3600.0 + 59.0 * 60.0 + 59.0) * 1000.0;

/// The time a new timer starts at, as in FigJam.
const DRAFT_MS: f64 = 3.0 * 60_000.0;

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

/// `mm:ss`, as FigJam shows it; `h:mm:ss` from an hour up.
fn clock(ms: f64) -> String {
    let s = (ms / 1000.0).ceil().max(0.0) as i64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// Digits typed into the timer, read like a microwave's from the right: "130" is 1:30,
/// "13000" 1:30:00; minutes or seconds past 59 carry over ("90" is 1:30). Only digits count.
fn digits_ms(d: &str) -> Option<f64> {
    let d: String = d.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.is_empty() || d.len() > 6 {
        return None;
    }
    let n: u64 = d.parse().ok()?;
    let (h, m, sec) = (n / 10_000, n / 100 % 100, n % 100);
    let ms = ((h * 3600 + m * 60 + sec) * 1000) as f64;
    (ms > 0.0).then(|| ms.min(MAX_MS))
}

/// What the display shows while digits are typed: right-aligned into hh:mm:ss.
fn digits_shown(d: &str) -> String {
    let p = format!("{d:0>6}");
    if p.starts_with("00") { format!("{}:{}", &p[2..4], &p[4..6]) } else { format!("{}:{}:{}", &p[0..2], &p[2..4], &p[4..6]) }
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
            crate::audio::chime(ed.prefs.timer_chime, ed.prefs.timer_volume);
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


/// FigJam's timer chip by the share button: a little record (turning while music plays) and the
/// time, grey until a timer runs; it opens the panel of the timer, the music and the vote.
pub fn timer_button(ui: &mut egui::Ui, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    let tm = read_timer(ed);
    let music = read_music(ed);
    let draft: f64 = ui.data(|d| d.get_temp(Id::new("timer-draft"))).unwrap_or(DRAFT_MS);
    let text = tm.map_or_else(|| clock(draft), |tm| clock(tm.remaining()));
    let font = egui::FontId::new(ui::px(12.5), egui::FontFamily::Name(if tm.is_some() { "ui-medium" } else { "sans-italic" }.into()));
    let g = ui.painter().layout_no_wrap(text.clone(), font, Color32::PLACEHOLDER);
    let h = ui::hit(ui.ctx(), ui::ctl(24.0));
    // On a phone, with no countdown going, just the record: the pill must fit beside the other.
    let compact = tm.is_none() && ui::narrow(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(if compact { 28.0 } else { g.size().x + 40.0 }, h), egui::Sense::click());
    let dx = shake(ui.ctx());
    let r = rect.translate(vec2(dx, 0.0));
    let hv = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    let done = tm.is_some_and(|tm| tm.done());
    let fill = if done { t.danger.gamma_multiply(0.12) } else { ui::blend(t.bg, t.hover, hv) };
    ui.painter().rect(r, ui::RADIUS as f32, fill, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let disc = if compact { r.center() } else { pos2(r.min.x + 14.0, r.center().y) };
    record_icon(ui, disc, 7.5, music.map(|m| m.0), &t);
    let color = match tm {
        Some(tm) => time_look(ui.ctx(), &tm, &t),
        None => t.text3,
    };
    if !compact {
        ui.painter().galley(pos2(r.min.x + 27.0, r.center().y - g.size().y / 2.0), g, color);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Timer: {text}")));
    ui::focus_ring(ui, &resp, rect);
    let resp = ui::tip(resp, "Timer, musica e votazione", None);
    egui::Popup::from_toggle_button_response(&resp)
        .frame(ui::float_frame(&t).corner_radius(16).inner_margin(egui::Margin::ZERO))
        .align(egui::RectAlign::BOTTOM_END)
        .gap(8.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| timer_panel(ui, ed));
}

/// A small record: black, a label in the colour of the piece playing (grey when none), and a
/// glint going round while it plays.
fn record_icon(ui: &egui::Ui, c: egui::Pos2, r: f32, playing: Option<Track>, t: &ui::Theme) {
    let p = ui.painter();
    p.circle_filled(c, r, Color32::from_gray(0x1E));
    p.circle_stroke(c, r * 0.72, Stroke::new(0.8, Color32::from_gray(0x3A)));
    let label = playing.map_or(t.text3, cover_color);
    p.circle_filled(c, r * 0.38, label);
    if playing.is_some() && !ui::motion::reduced(ui.ctx()) {
        let a = (crate::platform::now_ms() / 1000.0 * 3.0) as f32;
        let (sa, ca) = a.sin_cos();
        p.line_segment([c + vec2(ca, sa) * r * 0.5, c + vec2(ca, sa) * r * 0.92], Stroke::new(1.2, Color32::from_white_alpha(90)));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// The panel: the volume, the timer on a display, the record player and the vote.
fn timer_panel(ui: &mut egui::Ui, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    let w = 300.0;
    // A popup lays out in last frame's size: give it the room it may take, or it stays cut.
    let room = (ui.ctx().content_rect().height() - 90.0).max(240.0);
    ui.set_max_size(vec2(w, room));
    ui.set_width(w);
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let line = |ui: &mut egui::Ui| {
        let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), egui::Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
    };
    egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 10, top: 12, bottom: 10 }).show(ui, |ui| {
        ui::row(ui, 28.0, |ui| {
            ui.label(RichText::new("Timer, musica e votazione").font(ui::medium(ui::px(13.0))).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                    egui::Popup::close_all(ui.ctx());
                }
            });
        });
    });
    line(ui);
    let max_h = room - 52.0;
    egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| volume_row(ui, ed, &t));
        line(ui);
        egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| timer_card(ui, ed, &t));
        line(ui);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| chime_card(ui, ed, &t));
        line(ui);
        egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| music_card(ui, ed, &t));
        line(ui);
        egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| vote_card(ui, ed, &t));
    });
}

/// This person's volume: the speaker silences, the slider sets it.
fn volume_row(ui: &mut egui::Ui, ed: &mut Editor, _t: &ui::Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let p = &mut ed.prefs;
        let muted = p.music_muted || p.music_volume <= 0.001;
        if ui::icon_button(ui, if muted { "volume-x" } else { "volume-2" }, if muted { "Riattiva l'audio" } else { "Silenzia (solo per te)" }, None, vec2(28.0, 28.0), 16.0, false, false).clicked() {
            p.music_muted = !p.music_muted;
            if !p.music_muted && p.music_volume <= 0.001 {
                p.music_volume = 0.6;
            }
        }
        let mut v = if muted { 0.0 } else { p.music_volume as f64 };
        if ui::slider_inline(ui, "", &mut v, 0.0, 1.0, 0.05, false, |_| String::new(), ui.available_width()) {
            p.music_volume = v as f32;
            p.music_muted = false;
        }
    });
}

/// A round button in the brand colour (play, pause): FigJam's.
fn round_button(ui: &mut egui::Ui, name: &str, label: &str, t: &ui::Theme) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::click());
    let h = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    ui.painter().circle_filled(r.center(), 18.0, ui::blend(t.brand_fill, t.brand_fill_hover, h));
    icon(ui, name, r.center() + if name == "play" { vec2(1.0, 0.0) } else { vec2(0.0, 0.0) }, 16.0, Color32::WHITE);
    ui::focus_ring(ui, &resp, r);
    ui::tip(resp, label, None)
}

/// The timer: the time, large (click it and type the digits, as on a microwave), a minute less
/// or more, a few lengths ready and play; while it runs, a minute more, pause or resume and stop.
fn timer_card(ui: &mut egui::Ui, ed: &mut Editor, t: &ui::Theme) {
    let ro = ed.read_only;
    let tm = read_timer(ed);
    let draft_id = Id::new("timer-draft");
    let mut draft: f64 = ui.data(|d| d.get_temp(draft_id)).unwrap_or(DRAFT_MS);
    let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), 76.0), egui::Sense::hover());
    ui.painter().rect_filled(card, 12.0, t.bg2);
    if tm.is_none() && !ro {
        let side = |x: f32| Rect::from_center_size(pos2(x, card.center().y), vec2(32.0, 32.0));
        let minus = ui.put(side(card.min.x + 26.0), |ui: &mut egui::Ui| ui::icon_button(ui, "minus", "Un minuto in meno", None, vec2(32.0, 32.0), 16.0, false, false));
        if minus.clicked() {
            draft = (draft - 60_000.0).max(60_000.0f64.min(draft));
        }
        let plus = ui.put(side(card.max.x - 26.0), |ui: &mut egui::Ui| ui::icon_button(ui, "plus", "Un minuto in più", None, vec2(32.0, 32.0), 16.0, false, false));
        if plus.clicked() {
            draft = (draft + 60_000.0).min(MAX_MS);
        }
    }
    let digits_id = Id::new("timer-digits");
    let mut digits: String = ui.data(|d| d.get_temp(digits_id)).unwrap_or_default();
    let mut editing = false;
    let shown = match tm {
        Some(tm) => clock(tm.remaining()),
        None if !ro => {
            let over = Rect::from_center_size(card.center(), vec2(card.width() - 112.0, card.height() - 12.0));
            let field = ui.scope(|ui| {
                ui.visuals_mut().text_cursor.stroke = Stroke::NONE;
                ui.put(over, egui::TextEdit::singleline(&mut digits).id(Id::new("timer-field")).frame(egui::Frame::NONE).text_color(Color32::TRANSPARENT).char_limit(6).desired_width(over.width()).font(egui::FontId::proportional(1.0)))
            }).inner;
            digits.retain(|c| c.is_ascii_digit());
            editing = field.has_focus();
            if field.gained_focus() {
                digits.clear();
            }
            if field.lost_focus() {
                if let Some(ms) = digits_ms(&digits) {
                    draft = ms;
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        start_timer(ed, draft);
                    }
                }
                digits.clear();
            }
            if editing && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(ms) = digits_ms(&digits) {
                    draft = ms;
                    start_timer(ed, draft);
                    digits.clear();
                    field.surrender_focus();
                }
            }
            if editing {
                if digits.is_empty() {
                    "00:00".to_string()
                } else {
                    digits_shown(&digits)
                }
            } else {
                clock(draft)
            }
        }
        None => clock(draft),
    };
    ui.data_mut(|d| d.insert_temp(digits_id, digits.clone()));
    let color = match &tm {
        Some(tm) => time_look(ui.ctx(), tm, t),
        None if editing && digits.is_empty() => t.text3,
        None => t.text,
    };
    let dx = shake(ui.ctx());
    let time = big_time(ui.painter(), card.center() + vec2(dx, if editing { -6.0 } else { 0.0 }), &shown, 40.0, color);
    if editing {
        if (crate::platform::now_ms() / 530.0) as i64 % 2 == 0 {
            ui.painter().rect_filled(Rect::from_min_size(pos2(time.max.x + 3.0, time.min.y + 4.0), vec2(2.0, time.height() - 8.0)), 1.0, t.brand);
        }
        ui.painter().text(pos2(card.center().x, card.max.y - 12.0), egui::Align2::CENTER_CENTER, "Digita i minuti/secondi • Invio per avviare", egui::FontId::proportional(ui::px(10.0)), t.text2);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    // While it runs, a thin bar along the bottom empties.
    if let Some(tm) = &tm {
        let k = (tm.remaining() / tm.total).clamp(0.0, 1.0) as f32;
        let bar = Rect::from_min_size(pos2(card.min.x + 16.0, card.max.y - 10.0), vec2(card.width() - 32.0, 3.0));
        ui.painter().rect_filled(bar, 1.5, t.bg3);
        ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * k, bar.height())), 1.5, if tm.done() { t.danger } else { t.brand });
    }
    ui.add_space(12.0);
    if ro {
        ui.data_mut(|d| d.insert_temp(draft_id, draft));
        return;
    }
    // One row as tall as the round button, everything centred on it.
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        match &tm {
            None => {
                let mut secs = (draft / 1000.0).round() as i64;
                if ui::segmented(ui, &mut secs, &[(60, "1 min"), (180, "3 min"), (300, "5 min"), (600, "10 min")], ui.available_width() - 44.0) {
                    draft = secs as f64 * 1000.0;
                }
            }
            Some(tm) => {
                if ui::button(ui, "1 min", Btn::Secondary, Some("plus"), true, true).clicked() {
                    add_minute(ed, tm);
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match &tm {
                None => {
                    if round_button(ui, "play", "Avvia per tutti", t).clicked() {
                        start_timer(ed, draft);
                    }
                }
                Some(tm) if tm.done() => {
                    if round_button(ui, "rotate-ccw", "Di nuovo", t).clicked() {
                        start_timer(ed, tm.total);
                    }
                    if ui::icon_button(ui, "square", "Ferma", None, vec2(32.0, 32.0), 14.0, false, false).clicked() {
                        stop_timer(ed);
                    }
                }
                Some(tm) => {
                    let pause = tm.end;
                    if round_button(ui, if pause.is_some() { "pause" } else { "play" }, if pause.is_some() { "Pausa" } else { "Riprendi" }, t).clicked() {
                        match pause {
                            Some(end) => set_timer(ed, &[("left", json!((end - shared_now()).max(0.0))), ("end", Value::Null)]),
                            None => set_timer(ed, &[("end", json!(shared_now() + tm.left))]),
                        }
                    }
                    if ui::icon_button(ui, "square", "Ferma", None, vec2(32.0, 32.0), 14.0, false, false).clicked() {
                        stop_timer(ed);
                    }
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(draft_id, draft));
}

/// The chime sound setting: soft chime choices, preview button, and volume slider.
fn chime_card(ui: &mut egui::Ui, ed: &mut Editor, t: &ui::Theme) {
    ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
    let p = &mut ed.prefs;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let on = p.timer_sound;
        if ui::icon_button(ui, if on { "bell" } else { "bell-off" }, if on { "Suoneria attiva al termine" } else { "Suoneria disattivata" }, None, vec2(28.0, 28.0), 16.0, on, false).clicked() {
            p.timer_sound = !on;
        }
        ui.label(RichText::new("Suoneria a fine timer").font(ui::medium(ui::px(12.0))).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui::small_icon_button(ui, "play", "Ascolta anteprima suoneria", false).clicked() {
                crate::audio::chime(p.timer_chime, p.timer_volume);
            }
        });
    });
    if p.timer_sound {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let current = p.timer_chime;
            let label = current.name();
            let (r, resp) = ui.allocate_exact_size(vec2(145.0, 26.0), egui::Sense::click());
            ui.painter().rect_filled(r, 6.0, t.bg2);
            let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
            if k > 0.0 {
                ui.painter().rect_stroke(r, 6.0, Stroke::new(1.0, t.border_strong.gamma_multiply(k)), egui::StrokeKind::Inside);
            }
            ui.painter().text(pos2(r.min.x + 8.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(ui::px(11.0)), t.text);
            icon(ui, "chevron-down", pos2(r.max.x - 12.0, r.center().y), 12.0, t.icon2);
            let resp = ui::tip(resp, "Scegli il suono rilassante", None);
            egui::Popup::menu(&resp).frame(ui::menu_frame(t)).show(|ui| ui::menu_body(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for sound in crate::prefs::TIMER_SOUNDS {
                    let on = sound == current;
                    let (ir, item) = ui.allocate_exact_size(vec2(160.0, 26.0), egui::Sense::click());
                    if item.hovered() {
                        ui.painter().rect_filled(ir, ui::RADIUS, t.hover);
                    }
                    if on {
                        icon(ui, "check", pos2(ir.min.x + 10.0, ir.center().y), 12.0, t.brand);
                    }
                    ui.painter().text(pos2(ir.min.x + 24.0, ir.center().y), egui::Align2::LEFT_CENTER, sound.name(), egui::FontId::proportional(ui::px(11.0)), if on { t.brand } else { t.text });
                    if item.clicked() {
                        p.timer_chime = sound;
                        crate::audio::chime(sound, p.timer_volume);
                    }
                }
            }));
            let mut v = p.timer_volume as f64;
            if ui::slider_inline(ui, "", &mut v, 0.05, 1.0, 0.05, false, |_| String::new(), ui.available_width()) {
                p.timer_volume = v as f32;
            }
        });
    }
}

/// The time in large figures, centred on `c`, each digit in a cell of the same width so the
/// time doesn't shift as it counts; where it went.
fn big_time(p: &egui::Painter, c: egui::Pos2, text: &str, size: f32, color: Color32) -> Rect {
    let font = ui::medium(size);
    let glyph = |ch: char| p.layout_no_wrap(ch.to_string(), font.clone(), color);
    let cell = ('0'..='9').map(|d| glyph(d).size().x).fold(0.0, f32::max);
    let width = |ch: char| if ch == ':' { glyph(':').size().x + size * 0.06 } else { cell };
    let w: f32 = text.chars().map(width).sum();
    let h = glyph('0').size().y;
    let mut x = c.x - w / 2.0;
    for ch in text.chars() {
        let g = glyph(ch);
        // The colon up level with the middle of the digits, as on a clock.
        let lift = if ch == ':' { size * 0.07 } else { 0.0 };
        p.galley(pos2(x + (width(ch) - g.size().x) / 2.0, c.y - h / 2.0 - lift), g, color);
        x += width(ch);
    }
    Rect::from_center_size(c, vec2(w, h * 0.72))
}

/// The colour of a piece's cover.
fn cover_color(track: Track) -> Color32 {
    crate::music::TRACKS.iter().find(|x| x.0 == track).and_then(|x| crate::model::parse_color(x.3)).unwrap_or(Color32::GRAY)
}

/// A piece's cover: its colour and a little line drawing (a bird, a face with headphones, a
/// burst, a swirl, a sunset over a grid, a sailboat).
fn cover(p: &egui::Painter, r: Rect, track: Track) {
    p.rect_filled(r, r.width() * 0.12, cover_color(track));
    let k = r.width() / 24.0;
    let at = |x: f32, y: f32| r.min + vec2(x * k, y * k);
    let ink = Color32::from_gray(0x1A);
    let st = Stroke::new(1.3 * k.max(0.7), ink);
    let curve = |pts: [egui::Pos2; 4]| egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, st));
    match track {
        Track::Acoustic => {
            p.add(curve([at(4.0, 13.0), at(8.0, 17.0), at(15.0, 17.0), at(19.0, 11.0)]));
            p.add(curve([at(4.0, 13.0), at(8.0, 9.0), at(12.0, 9.0), at(14.0, 12.0)]));
            p.add(curve([at(10.0, 11.0), at(11.0, 6.0), at(15.0, 5.0), at(18.0, 6.0)]));
            p.line_segment([at(19.0, 11.0), at(21.0, 10.0)], st);
            p.circle_filled(at(17.0, 11.5), 0.9 * k, ink);
        }
        Track::Lofi => {
            p.circle_stroke(at(12.0, 13.0), 6.0 * k, st);
            p.add(curve([at(5.5, 13.0), at(5.0, 4.0), at(19.0, 4.0), at(18.5, 13.0)]));
            p.rect_filled(Rect::from_center_size(at(5.5, 14.0), vec2(2.4 * k, 4.0 * k)), k, ink);
            p.rect_filled(Rect::from_center_size(at(18.5, 14.0), vec2(2.4 * k, 4.0 * k)), k, ink);
            p.circle_filled(at(10.0, 12.5), 0.8 * k, ink);
            p.circle_filled(at(14.0, 12.5), 0.8 * k, ink);
            p.add(curve([at(10.0, 15.5), at(11.0, 16.8), at(13.0, 16.8), at(14.0, 15.5)]));
        }
        Track::Chillhop => {
            p.rect(Rect::from_min_max(at(7.0, 9.0), at(17.0, 19.0)), egui::CornerRadius { nw: 2, ne: 2, sw: 5, se: 5 }, ink, Stroke::NONE, egui::StrokeKind::Inside);
            p.add(curve([at(17.0, 11.0), at(21.0, 11.0), at(21.0, 17.0), at(17.0, 17.0)]));
            p.add(curve([at(9.0, 7.5), at(8.0, 5.5), at(10.0, 4.0), at(9.0, 2.5)]));
            p.add(curve([at(14.0, 7.5), at(13.0, 5.5), at(15.0, 4.0), at(14.0, 2.5)]));
        }
        Track::Zen => {
            let stone_a = Rect::from_center_size(at(12.0, 18.0), vec2(15.0 * k, 5.0 * k));
            let stone_b = Rect::from_center_size(at(12.0, 13.5), vec2(11.0 * k, 4.2 * k));
            let stone_c = Rect::from_center_size(at(12.0, 9.5), vec2(7.5 * k, 3.8 * k));
            p.rect_filled(stone_a, 2.5 * k, ink);
            p.rect_filled(stone_b, 2.1 * k, ink);
            p.rect_filled(stone_c, 1.8 * k, ink);
            p.circle_stroke(at(12.0, 5.0), 1.5 * k, Stroke::new(1.0, ink));
        }
        Track::Smash => {
            let pts: Vec<egui::Pos2> = (0..24).map(|i| {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                let rr = if i % 2 == 0 { 8.5 } else { 4.5 };
                at(12.0 + a.cos() * rr, 12.0 + a.sin() * rr)
            }).collect();
            p.add(egui::Shape::closed_line(pts.clone(), Stroke::new(0.5, ink)));
            for w in 0..12 {
                p.add(egui::Shape::convex_polygon(vec![at(12.0, 12.0), pts[w * 2 + 1], pts[(w * 2 + 2) % 24], pts[(w * 2 + 3) % 24]], ink, Stroke::NONE));
            }
        }
        Track::Friday => {
            let spiral: Vec<egui::Pos2> = (0..60).map(|i| {
                let a = i as f32 * 0.32;
                let rr = 1.0 + i as f32 * 0.13;
                at(12.0 + a.cos() * rr, 12.0 + a.sin() * rr)
            }).collect();
            p.add(egui::Shape::line(spiral, st));
            p.circle_filled(at(19.5, 5.5), 1.2 * k, ink);
            p.circle_filled(at(5.0, 19.0), 0.9 * k, ink);
        }
        Track::Synthwave => {
            let sun = at(12.0, 10.0);
            p.circle_filled(sun, 5.5 * k, Color32::from_rgb(0xFF, 0xD0, 0x5A));
            for (i, y) in [10.5f32, 12.5, 14.0].iter().enumerate() {
                p.rect_filled(Rect::from_min_max(at(5.0, *y), at(19.0, *y + 0.6 + i as f32 * 0.3)), 0.0, cover_color(track));
            }
            p.line_segment([at(3.0, 16.0), at(21.0, 16.0)], st);
            for x in [3.0f32, 8.0, 12.0, 16.0, 21.0] {
                p.line_segment([at(12.0 + (x - 12.0) * 0.4, 16.0), at(x, 21.0)], st);
            }
            p.line_segment([at(2.0, 18.5), at(22.0, 18.5)], st);
        }
        Track::Hero => {
            p.add(egui::Shape::closed_line(vec![at(11.5, 4.0), at(11.5, 15.0), at(5.5, 15.0)], st));
            p.add(egui::Shape::closed_line(vec![at(12.5, 6.0), at(12.5, 15.0), at(18.0, 15.0)], st));
            p.add(curve([at(4.0, 16.5), at(7.0, 20.0), at(17.0, 20.0), at(20.0, 16.5)]));
            p.line_segment([at(4.0, 16.5), at(20.0, 16.5)], st);
        }
    }
}

/// The record player: the record turning under its arm while the piece plays, the speaker
/// beside it; under it the piece to pick and play or stop it (for everyone).
fn music_card(ui: &mut egui::Ui, ed: &mut Editor, t: &ui::Theme) {
    let playing = read_music(ed).map(|m| m.0);
    let pick_id = Id::new("music-pick");
    let mut pick: Track = playing.or_else(|| ui.data(|d| d.get_temp(pick_id))).unwrap_or(Track::Acoustic);
    let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(card, 12.0, t.bg2);
    let still = ui::motion::reduced(ui.ctx());
    let spin = ui::motion::spring(ui.ctx(), Id::new("record-spin"), if playing.is_some() { 1.0 } else { 0.0 }, 0.2, 0.95);
    let c = pos2(card.min.x + 62.0, card.center().y);
    let r = 50.0;
    p.circle_filled(c + vec2(0.0, 2.0), r + 1.0, Color32::from_black_alpha(30));
    p.circle_filled(c, r, Color32::from_gray(0x16));
    for k in [0.9f32, 0.78, 0.66, 0.55] {
        p.circle_stroke(c, r * k, Stroke::new(0.8, Color32::from_gray(0x2C)));
    }
    let angle = if still { 0.0 } else { (crate::platform::now_ms() / 1000.0 * 2.2) as f32 * spin };
    for off in [0.0f32, std::f32::consts::PI] {
        let a = angle + off;
        let pts: Vec<egui::Pos2> = (0..10).map(|i| {
            let b = a + i as f32 * 0.06;
            c + vec2(b.cos(), b.sin()) * r * 0.82
        }).collect();
        p.add(egui::Shape::line(pts, Stroke::new(2.0, Color32::from_white_alpha(28))));
    }
    let label = Rect::from_center_size(c, vec2(r * 0.62, r * 0.62));
    p.circle_filled(c, r * 0.36, cover_color(playing.unwrap_or(pick)));
    cover(p, label.shrink(r * 0.09), playing.unwrap_or(pick));
    p.circle_filled(c, 2.2, Color32::from_gray(0xE0));
    if playing.is_some() && !still {
        ui.ctx().request_repaint();
    }
    // The arm: off the record at rest, on its edge while it plays.
    let pivot = pos2(card.min.x + 140.0, card.min.y + 20.0);
    let rest = 0.18f32;
    let play = 0.62f32;
    let a = rest + (play - rest) * spin;
    let dir = vec2(-a.sin(), a.cos());
    let head = pivot + dir * 72.0;
    p.line_segment([pivot, head], Stroke::new(3.0, Color32::from_gray(0xC8)));
    p.line_segment([pivot, head], Stroke::new(1.5, Color32::from_gray(0xF4)));
    p.circle_filled(pivot, 7.0, Color32::from_gray(0xD4));
    p.circle_filled(pivot, 3.0, Color32::from_gray(0x9A));
    p.add(egui::Shape::convex_polygon(vec![head + vec2(-4.0, -2.0), head + vec2(4.0, -2.0), head + vec2(5.0, 6.0), head + vec2(-3.0, 6.0)], Color32::from_gray(0xB0), Stroke::NONE));
    // The speaker.
    let sc = pos2(card.max.x - 52.0, card.center().y);
    for i in -6..=6 {
        for j in -6..=6 {
            let q = sc + vec2(i as f32 * 5.0, j as f32 * 5.0);
            if q.distance(sc) <= 31.0 {
                p.circle_filled(q, 1.5, t.text.gamma_multiply(0.35));
            }
        }
    }
    ui.add_space(12.0);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let name = crate::music::TRACKS.iter().find(|x| x.0 == pick).map_or("", |x| x.2);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 44.0, 36.0), egui::Sense::click());
        let h = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
        ui.painter().rect(r, 8.0, ui::blend(t.bg, t.hover, h), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        cover(ui.painter(), Rect::from_center_size(pos2(r.min.x + 18.0, r.center().y), vec2(20.0, 20.0)), pick);
        ui.painter().text(pos2(r.min.x + 36.0, r.center().y), egui::Align2::LEFT_CENTER, name, ui::medium(ui::px(12.0)), t.text);
        icon(ui, "chevron-down", pos2(r.max.x - 14.0, r.center().y), 14.0, t.icon2);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, format!("Brano: {name}")));
        // Open on its own (egui keeps one menu open at a time: this one would close the panel).
        let open_id = Id::new("music-list-open");
        let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        if resp.clicked() {
            open = !open;
        }
        egui::Popup::from_response(&resp).open_bool(&mut open).close_behavior(egui::PopupCloseBehavior::CloseOnClick).frame(ui::menu_frame(t).inner_margin(egui::Margin::same(6))).width(r.width()).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (track, _, title, _) in crate::music::TRACKS {
                let (row, rr) = ui.allocate_exact_size(vec2(r.width() - 12.0, 40.0), egui::Sense::click());
                let on = track == pick;
                let hv = ui::motion::hover(ui.ctx(), rr.id, rr.hovered());
                if hv > 0.0 {
                    ui.painter().rect_filled(row, 6.0, t.brand_fill.gamma_multiply(hv));
                }
                if on {
                    icon(ui, "check", pos2(row.min.x + 12.0, row.center().y), 14.0, t.menu_text);
                }
                cover(ui.painter(), Rect::from_center_size(pos2(row.min.x + 40.0, row.center().y), vec2(30.0, 30.0)), track);
                ui.painter().text(pos2(row.min.x + 64.0, row.center().y), egui::Align2::LEFT_CENTER, title, ui::medium(ui::px(12.5)), t.menu_text);
                if rr.clicked() {
                    pick = track;
                    if playing.is_some() && !ed.read_only {
                        set_music(ed, Some(track));
                    }
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(open_id, open));
        if !ed.read_only {
            let on = playing.is_some();
            if round_button(ui, if on { "pause" } else { "play" }, if on { "Ferma la musica" } else { "Fai partire la musica per tutti" }, t).clicked() {
                set_music(ed, if on { None } else { Some(pick) });
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(pick_id, pick));
}

/// The vote: a little picture and the button that sets one up (votes each, then start); while
/// one runs, how it is going.
fn vote_card(ui: &mut egui::Ui, ed: &mut Editor, t: &ui::Theme) {
    let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(card, 12.0, t.bg2);
    // A note with stickers on it.
    let n = Rect::from_center_size(card.center() + vec2(-34.0, 6.0), vec2(70.0, 70.0));
    let (sn, cs) = (-0.12f32).sin_cos();
    let rot = |q: egui::Pos2| n.center() + vec2((q.x - n.center().x) * cs - (q.y - n.center().y) * sn, (q.x - n.center().x) * sn + (q.y - n.center().y) * cs);
    let yellow = Color32::from_rgb(0xFF, 0xD9, 0x66);
    p.add(egui::Shape::convex_polygon(vec![rot(n.left_top()), rot(n.right_top()), rot(n.right_bottom() - vec2(0.0, 14.0)), rot(n.right_bottom() - vec2(14.0, 0.0)), rot(n.left_bottom())], yellow, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![rot(n.right_bottom() - vec2(0.0, 14.0)), rot(n.right_bottom() - vec2(14.0, 0.0)), rot(n.right_bottom() - vec2(14.0, 14.0))], ui::mix(yellow, 0.82, Color32::BLACK), Stroke::NONE));
    for (e, at) in [("❤️", card.center() + vec2(26.0, -18.0)), ("👍", card.center() + vec2(46.0, 16.0)), ("💯", card.center() + vec2(8.0, 22.0))] {
        emoji(ui, ed, e, Rect::from_center_size(at, vec2(30.0, 30.0)));
    }
    ui.add_space(12.0);
    if ed.read_only {
        return;
    }
    let setup_id = Id::new("vote-setup");
    let mut setup: bool = ui.data(|d| d.get_temp(setup_id)).unwrap_or(false);
    match ed.board.voting() {
        Some((per, false)) => {
            let used: u32 = ed.board.tally(Some(&ed.voter)).values().sum();
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(format!("Votazione in corso · ti restano {} voti su {per}", per.saturating_sub(used))).size(ui::px(12.0)).color(t.text2));
            });
        }
        Some((_, true)) => {
            ui.vertical_centered(|ui| {
                if ui::button(ui, "Risultati", Btn::Secondary, Some("vote"), false, true).clicked() {
                    ui.ctx().data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));
                    egui::Popup::close_all(ui.ctx());
                }
            });
        }
        None if setup => {
            let id = Id::new("vote-per");
            let mut per: u32 = ui.data(|d| d.get_temp(id)).unwrap_or(3);
            ui.label(RichText::new("Voti per persona").font(ui::medium(ui::px(12.0))).color(t.text));
            ui.add_space(6.0);
            ui::segmented(ui, &mut per, &[(1, "1"), (3, "3"), (5, "5"), (10, "10")], ui.available_width());
            ui.data_mut(|d| d.insert_temp(id, per));
            ui.add_space(8.0);
            if ui::full_button(ui, "Inizia la votazione", Btn::Primary, None).clicked() {
                let me = ed.voter.clone();
                ed.board.start_voting(per, &me);
                ed.set_tool(Tool::Select);
                setup = false;
                egui::Popup::close_all(ui.ctx());
            }
        }
        None => {
            ui.vertical_centered(|ui| {
                if ui::button(ui, "Nuova votazione", Btn::Primary, None, false, true).clicked() {
                    setup = true;
                }
            });
        }
    }
    ui.data_mut(|d| d.insert_temp(setup_id, setup));
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
            let me = ed.voter.clone();
            ed.board.start_voting(per, &me);
            ui.ctx().data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));
            ed.set_tool(Tool::Select);
            egui::Popup::close_all(ui.ctx());
        }
    });
}

/// Whether this person runs the vote: the host, or whoever started it.
fn can_run_vote(ctx: &egui::Context, ed: &Editor) -> bool {
    let host = ctx.data(|d| d.get_temp::<bool>(Id::new("board-host"))).unwrap_or(false);
    !ed.read_only && (host || ed.board.voting_by().as_deref() == Some(ed.voter.as_str()))
}

/// The results of a vote once it has ended, on the right: votes and voters, the ranking with
/// a bar each (a click goes to the thing voted), and for whoever runs it a new vote or the vote
/// taken off the board for everyone. Each person can close it for themselves.
pub fn vote_results(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let hidden_id = Id::new("vote-results-hidden");
    let ended = ed.board.voting().map(|v| v.1);
    if ended != Some(true) {
        ctx.data_mut(|d| d.remove::<bool>(hidden_id));
    }
    let show = ended == Some(true) && !ctx.data(|d| d.get_temp::<bool>(hidden_id)).unwrap_or(false);
    let k = ui::motion::presence(ctx, Id::new("vote-results"), show);
    if k <= 0.001 || ended.is_none() {
        return;
    }
    let t = ui::theme(ctx);
    let runs = can_run_vote(ctx, ed);
    let per = ed.board.voting().map_or(3, |v| v.0);
    let mut results: Vec<(String, u32)> = ed.board.tally(None).into_iter().filter(|(id, _)| ed.board.get(id).is_some()).collect();
    results.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: u32 = results.iter().map(|r| r.1).sum();
    let voters = ed.board.voter_count();
    let top = results.first().map_or(1, |r| r.1).max(1);
    let narrow = ui::narrow(ctx);
    let w = if narrow { stage.width() - 16.0 } else { 300.0 };
    let at = pos2(stage.max.x - w - if narrow { 8.0 } else { 12.0 }, stage.min.y + if narrow { 8.0 } else { 68.0 });
    let mut close = false;
    egui::Area::new(Id::new("vote-results")).fixed_pos(at + vec2(12.0 * (1.0 - k), 0.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui::float_frame(&t).corner_radius(14).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            ui::row(ui, 24.0, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::hover());
                icon(ui, "vote", r.center(), 16.0, t.icon);
                ui.label(RichText::new("Risultati").font(ui::medium(ui::px(13.0))).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                        close = true;
                    }
                });
            });
            ui.label(RichText::new(format!("{total} {} · {voters} {}", if total == 1 { "voto" } else { "voti" }, if voters == 1 { "persona" } else { "persone" })).size(ui::px(12.0)).color(t.text2));
            ui.add_space(6.0);
            if results.is_empty() {
                ui.label(RichText::new("Nessun voto").color(t.text2));
            }
            egui::ScrollArea::vertical().max_height((stage.height() - 260.0).clamp(160.0, 420.0)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, (id, n)) in results.iter().enumerate().take(30) {
                    let Some(el) = ed.board.get(id).cloned() else { continue };
                    let (row, resp) = ui.allocate_exact_size(vec2(w - 32.0, 46.0), egui::Sense::click());
                    ui::hover_fill(ui, resp.id, row, resp.hovered(), 8.0, t.hover);
                    // The first three get a medal; the others their place.
                    let c = pos2(row.min.x + 16.0, row.center().y);
                    let medal = [Color32::from_rgb(0xF5, 0xB7, 0x00), Color32::from_rgb(0xA9, 0xB4, 0xC0), Color32::from_rgb(0xD0, 0x87, 0x4C)].get(i).copied();
                    ui.painter().circle_filled(c, 11.0, medal.unwrap_or(t.bg2));
                    ui.painter().text(c, egui::Align2::CENTER_CENTER, (i + 1).to_string(), ui::medium(ui::px(11.0)), if medal.is_some() { Color32::WHITE } else { t.text2 });
                    let x = row.min.x + 36.0;
                    icon(ui, super::panels::type_icon(&el), pos2(x + 7.0, row.min.y + 15.0), 14.0, t.icon2);
                    let label: String = super::panels::element_label(&el).chars().take(40).collect();
                    let room = row.max.x - x - 56.0;
                    let g = ui.painter().layout(label, egui::FontId::proportional(ui::px(12.0)), t.text, room);
                    let first = g.rows.first().map(|r| r.text()).unwrap_or_default();
                    ui.painter().text(pos2(x + 20.0, row.min.y + 15.0), egui::Align2::LEFT_CENTER, if g.rows.len() > 1 { format!("{}…", first.trim_end()) } else { first }, egui::FontId::proportional(ui::px(12.0)), t.text);
                    ui.painter().text(pos2(row.max.x - 8.0, row.min.y + 15.0), egui::Align2::RIGHT_CENTER, n.to_string(), ui::medium(ui::px(12.0)), t.text);
                    let bar = Rect::from_min_size(pos2(x, row.min.y + 29.0), vec2(row.max.x - 8.0 - x, 6.0));
                    ui.painter().rect_filled(bar, 3.0, t.bg2);
                    let kk = ui::motion::spring(ctx, Id::new(("vote-bar", id.as_str())), *n as f32 / top as f32, 0.25, 0.9).clamp(0.0, 1.0);
                    ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * kk, bar.height())), 3.0, if i == 0 { t.brand } else { t.brand.gamma_multiply(0.55) });
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        ed.reveal(std::slice::from_ref(id));
                    }
                }
            });
            if runs {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui::button(ui, "Nuova votazione", Btn::Secondary, Some("rotate-ccw"), false, true).clicked() {
                        let me = ed.voter.clone();
                        ed.board.start_voting(per, &me);
                    }
                    if ui::button(ui, "Chiudi per tutti", Btn::Ghost, None, false, true).clicked() {
                        ed.board.clear_voting();
                    }
                });
            }
        });
    });
    if close {
        ctx.data_mut(|d| d.insert_temp(hidden_id, true));
    }
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
                    let runs = can_run_vote(ctx, ed);
                    if !ended {
                        let used: u32 = ed.board.tally(Some(&ed.voter)).values().sum();
                        let text = if ed.read_only { "Votazione".to_string() } else { format!("Votazione · ti restano {} voti su {per}", per.saturating_sub(used)) };
                        ui.label(RichText::new(text).color(t.text)).on_hover_text("Clic per votare · Maiusc+clic per togliere");
                        if runs && ui::button(ui, "Termina", Btn::Secondary, None, false, true).clicked() {
                            ed.board.end_voting();
                            ctx.data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));
                        }
                    } else {
                        ui.label(RichText::new("Votazione terminata").color(t.text));
                        if ctx.data(|d| d.get_temp::<bool>(Id::new("vote-results-hidden"))).unwrap_or(false) && ui::button(ui, "Risultati", Btn::Secondary, None, false, true).clicked() {
                            ctx.data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));
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
        assert_eq!(super::clock(90.0 * 60_000.0), "1:30:00", "hours from an hour up");
        // Digits typed like a microwave's: from the right, carrying over past 59.
        assert_eq!(super::digits_ms("5"), Some(5_000.0));
        assert_eq!(super::digits_ms("130"), Some(90_000.0));
        assert_eq!(super::digits_ms("90"), Some(90_000.0));
        assert_eq!(super::digits_ms("13000"), Some(5_400_000.0));
        assert_eq!(super::digits_ms("999999"), Some(super::MAX_MS));
        assert_eq!((super::digits_ms("0"), super::digits_ms("")), (None, None));
        assert_eq!(super::digits_shown("130"), "01:30");
        assert_eq!(super::digits_shown("13000"), "01:30:00");
    }
}
