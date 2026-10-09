//! Working together live: the shared timer, voting, presenting (everyone follows one view),
//! comment threads and cursor chat.

use egui::{Color32, Id, Rect, RichText, Sense, Stroke, pos2, vec2};
use serde_json::{Value, json};
use yrs::Map;

use super::board::BoardUi;
use crate::editor::{Editor, Tool, peer_color, peer_name};
use crate::model::{CommentMsg, Kind, hex, is_dark};
use crate::ui::{self, Kind as Btn, icon};

fn now() -> f64 {
    crate::platform::now_ms()
}

/* ---------------- timer ---------------- */

/// `end`: when it reaches zero (setter's clock), None while paused; `left`: ms left when paused.
struct Timer {
    total: f64,
    end: Option<f64>,
    left: f64,
}

fn read_timer(ed: &Editor) -> Option<Timer> {
    let b = &ed.board;
    let total = b.timer_field("total").and_then(|v| v.as_f64()).filter(|t| *t > 0.0)?;
    let left = b.timer_field("left").and_then(|v| v.as_f64())?;
    Some(Timer { total: total.min(24.0 * 3_600_000.0), end: b.timer_field("end").and_then(|v| v.as_f64()), left })
}

fn set_timer(ed: &mut Editor, fields: &[(&str, Value)]) {
    ed.board.transact_plain(|txn, b| {
        for (k, v) in fields {
            b.timer.insert(txn, *k, crate::doc::json_any(v));
        }
    });
}

fn clock(ms: f64) -> String {
    let s = (ms / 1000.0).ceil() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn timer_button(ui: &mut egui::Ui, ed: &mut Editor) {
    if ed.read_only {
        return;
    }
    let t = ui::theme(ui.ctx());
    let on = read_timer(ed).is_some();
    let r = ui::icon_button(ui, "timer", "Timer", None, vec2(24.0, 24.0), 16.0, on, false);
    egui::Popup::from_toggle_button_response(&r).frame(ui::float_frame(&t).inner_margin(egui::Margin::same(12))).align(egui::RectAlign::BOTTOM_END).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(220.0);
        ui::heading(ui, "Timer per tutti");
        egui::Grid::new("timer-presets").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
            for (i, m) in [1, 3, 5, 10, 15, 30].into_iter().enumerate() {
                if ui::button(ui, &format!("{m} min"), Btn::Secondary, None, false, true).clicked() {
                    let ms = m as f64 * 60_000.0;
                    set_timer(ed, &[("total", json!(ms)), ("left", json!(ms)), ("end", json!(now() + ms))]);
                    egui::Popup::close_all(ui.ctx());
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui::hint(ui, "Il conto alla rovescia compare in alto per chiunque sia sulla lavagna.");
    });
}

/* ---------------- voting ---------------- */

pub fn vote_button(ui: &mut egui::Ui, ed: &mut Editor) {
    if ed.read_only {
        return;
    }
    let t = ui::theme(ui.ctx());
    let voting = ed.board.voting();
    let r = ui::icon_button(ui, "vote", "Votazione", None, vec2(24.0, 24.0), 16.0, voting.is_some(), false);
    egui::Popup::from_toggle_button_response(&r).frame(ui::float_frame(&t).inner_margin(egui::Margin::same(12))).align(egui::RectAlign::BOTTOM_END).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(240.0);
        ui::heading(ui, "Votazione");
        ui::hint(ui, "Ognuno clicca sulle note, forme o immagini che preferisce. I voti degli altri si vedono alla fine.");
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
    let label = if on { "Smetti di presentare" } else { "Presenta: tutti seguono la tua vista" };
    if ui::icon_button(ui, "presentation", label, None, vec2(24.0, 24.0), 16.0, on, false).clicked() {
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
        let g = painter.layout_no_wrap(label, ui::medium(11.0), on);
        let r = Rect::from_center_size(pos2(stage.center().x, stage.min.y + 12.0), g.size() + vec2(16.0, 6.0));
        painter.rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }, color);
        painter.galley(r.center() - g.size() / 2.0, g, on);
    }

    let timer = read_timer(ed);
    let voting = ed.board.voting();
    if timer.is_none() && voting.is_none() && !mine && presenter.is_none() {
        ctx.data_mut(|d| d.insert_temp(fid, f));
        return;
    }
    egui::Area::new(Id::new("live-top")).pivot(egui::Align2::CENTER_TOP).fixed_pos(pos2(stage.center().x, stage.min.y + 32.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            if let Some(tm) = &timer {
                let left = match tm.end {
                    Some(end) => (end - now()).max(0.0),
                    None => tm.left,
                };
                let done = left <= 0.0;
                if tm.end.is_some() && !done {
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                }
                pill(ui, &t, |ui| {
                    icon(ui, "timer", ui.cursor().min + vec2(7.0, 12.0), 14.0, t.icon);
                    ui.add_space(18.0);
                    ui.label(RichText::new(if done { "Tempo scaduto".to_string() } else { clock(left) }).font(ui::medium(13.0)).color(if done { t.danger_text } else { t.text }));
                    let (r, _) = ui.allocate_exact_size(vec2(80.0, 4.0), Sense::hover());
                    ui.painter().rect_filled(r, 2.0, t.bg3);
                    ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * (left / tm.total).clamp(0.0, 1.0) as f32, 4.0)), 2.0, t.brand_fill);
                    if !ed.read_only {
                        if !done {
                            if let Some(end) = tm.end {
                                if ui::small_icon_button(ui, "pause", "Pausa", false).clicked() {
                                    set_timer(ed, &[("left", json!((end - now()).max(0.0))), ("end", Value::Null)]);
                                }
                            } else if ui::small_icon_button(ui, "play", "Riprendi", false).clicked() {
                                set_timer(ed, &[("end", json!(now() + tm.left))]);
                            }
                        }
                        if ui::button(ui, "+1 min", Btn::Ghost, None, false, true).clicked() {
                            match tm.end {
                                Some(end) => set_timer(ed, &[("total", json!(tm.total + 60_000.0)), ("end", json!(end.max(now()) + 60_000.0))]),
                                None => set_timer(ed, &[("total", json!(tm.total + 60_000.0)), ("left", json!(tm.left + 60_000.0))]),
                            }
                        }
                        if ui::small_icon_button(ui, "square", "Ferma il timer", false).clicked() {
                            ed.board.transact_plain(|txn, b| b.timer.clear(txn));
                        }
                    }
                });
                ui.add_space(6.0);
            }
            if let Some((per, ended)) = voting {
                pill(ui, &t, |ui| {
                    icon(ui, "vote", ui.cursor().min + vec2(7.0, 12.0), 14.0, t.icon);
                    ui.add_space(18.0);
                    if !ended {
                        let used: u32 = ed.board.tally(Some(&ed.voter)).values().sum();
                        let text = if ed.read_only { "Votazione: solo chi può modificare vota".to_string() } else { format!("Votazione: ti restano {} voti su {per}. Clicca per votare, Maiusc+clic per togliere", per.saturating_sub(used)) };
                        ui.label(RichText::new(text).color(t.text));
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
                    ui.label(RichText::new("Stai presentando: tutti seguono la tua vista").color(t.text));
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
    let was: Option<String> = ctx.data(|d| d.get_temp(open_id));
    if was != ed.comment {
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
    egui::Area::new(Id::new("comment-thread")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(268.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Commento").font(ui::medium(12.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                        close = true;
                    }
                    if !ed.read_only && !thread.is_empty() && ui::small_icon_button(ui, "check", "Risolvi: togli il commento", false).clicked() {
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
                                ui.label(RichText::new(&m.author).font(ui::medium(11.0)).color(t.text));
                                ui.label(RichText::new(when(m.t)).size(10.0).color(t.text2));
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
    if close {
        ed.comment = None;
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
    egui::Area::new(Id::new("cursor-chat")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
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
        if keep {
            st.chat_clear = Some(now() + 5000.0);
            ctx.request_repaint_after(std::time::Duration::from_millis(5100));
        } else {
            ed.presence.set("chat", Value::Null);
        }
    }
}
