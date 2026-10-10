//! The list of boards: your boards with their pictures first, then the templates; import and
//! settings in the sidebar.

use std::collections::HashMap;

use egui::{Color32, Id, Rect, RichText, ScrollArea, Sense, Stroke, TextureHandle, Ui, pos2, vec2};

use super::{Action, Toasts, dialogs::Dialogs};
use crate::prefs::Prefs;
use crate::store::{BoardRow, Store};
use crate::templates::{TEMPLATES, Template};
use crate::ui::{self, Kind as Btn, icon};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Updated,
    Created,
    Title,
}

pub struct Home {
    boards: Vec<BoardRow>,
    query: String,
    sort: Sort,
    renaming: Option<String>,
    rename_buf: String,
    deleting: Option<BoardRow>,
    /// The delete question is up (false while it fades away).
    delete_open: bool,
    thumbs: HashMap<String, (i64, Option<TextureHandle>)>,
    /// Where the sidebar asked the list to go: 0 your boards, 1 templates.
    jump: Option<u8>,
    /// The section last chosen in the sidebar.
    picked: u8,
}

/// "2 minuti fa", "ieri"…
pub fn time_ago(ms: i64) -> String {
    let s = ((crate::platform::now_ms() as i64 - ms) / 1000).max(0);
    if s < 45 {
        return "adesso".into();
    }
    let m = (s as f64 / 60.0).round() as i64;
    if m < 60 {
        return if m == 1 { "1 minuto fa".into() } else { format!("{m} minuti fa") };
    }
    let h = (m as f64 / 60.0).round() as i64;
    if h < 24 {
        return if h == 1 { "1 ora fa".into() } else { format!("{h} ore fa") };
    }
    let d = (h as f64 / 24.0).round() as i64;
    if d == 1 { "ieri".into() } else { format!("{d} giorni fa") }
}

const CARD_W: f32 = 240.0;
const THUMB_H: f32 = 150.0;
const TPL_W: f32 = 184.0;
const TPL_ART_H: f32 = 116.0;

impl Home {
    pub fn new(store: &Store) -> Home {
        Home { boards: store.boards().unwrap_or_default(), query: String::new(), sort: Sort::Updated, renaming: None, rename_buf: String::new(), deleting: None, delete_open: false, thumbs: HashMap::new(), jump: None, picked: 0 }
    }

    fn reload(&mut self, store: &Store) {
        self.boards = store.boards().unwrap_or_default();
    }

    fn thumb(&mut self, ctx: &egui::Context, store: &Store, b: &BoardRow) -> Option<TextureHandle> {
        if let Some((at, tex)) = self.thumbs.get(&b.id)
            && *at == b.updated_at
        {
            return tex.clone();
        }
        let tex = store.thumbnail(&b.id).ok().flatten().and_then(|bytes| crate::paint::decode_image(&bytes)).map(|img| ctx.load_texture(format!("thumb:{}", b.id), img, egui::TextureOptions::LINEAR));
        self.thumbs.insert(b.id.clone(), (b.updated_at, tex.clone()));
        tex
    }

    fn import(&mut self, store: &Store, toasts: &mut Toasts) -> Option<Action> {
        let path = rfd::FileDialog::new().set_title("Importa una lavagna").add_filter("Lavagna di Tratto", &["tratto", "json"]).pick_file()?;
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                toasts.error("Questo file non si può leggere.");
                return None;
            }
        };
        match crate::export::read_tratto(&text).and_then(|(title, state, files)| store.import(&title, &state, &files)) {
            Ok(id) => {
                toasts.info("Lavagna importata.");
                Some(Action::Open { id, template: None })
            }
            Err(e) => {
                toasts.error(e);
                None
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, store: &Store, prefs: &mut Prefs, toasts: &mut Toasts, dialogs: &mut Dialogs) -> Option<Action> {
        let ctx = ui.ctx().clone();
        let t = ui::theme(&ctx);
        let mut action = None;
        let _ = prefs;
        // Which part of the list is in view, for the sidebar (computed by the list below).
        let section_id = Id::new("home-section");
        let section: u8 = ctx.data(|d| d.get_temp(section_id)).unwrap_or(0);
        let side = egui::Panel::left(Id::new("home-side")).exact_size(232.0).resizable(false).frame(egui::Frame::new().fill(t.bg).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 12 })).show(ui, |ui| {
            ui::row(ui, 28.0, |ui| {
                ui.add_space(4.0);
                ui::logo(ui, 22.0);
                ui.label(RichText::new("Tratto").font(ui::medium(14.0)).color(t.text));
            });
            ui.add_space(14.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let items = [("clock", "Recenti"), ("layout-template", "Modelli")];
            let rects: Vec<Rect> = items.iter().map(|_| ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover()).0).collect();
            // The highlight glides to the part of the list you are looking at.
            let y = ui::motion::spring(&ctx, Id::new("home-side-y"), rects[section as usize].min.y - rects[0].min.y, 0.3, 0.88);
            ui.painter().rect_filled(rects[0].translate(vec2(0.0, y)), ui::RADIUS, t.selected);
            for (i, ((name, label), r)) in items.iter().zip(&rects).enumerate() {
                if side_item(ui, *r, name, label, section as usize == i).clicked() {
                    self.jump = Some(i as u8);
                    self.picked = i as u8;
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                if side_button(ui, "settings-2", "Impostazioni").clicked() {
                    dialogs.settings = true;
                }
                if let crate::updates::State::Available { version, url } = crate::updates::current()
                    && side_button(ui, "download", &format!("Scarica Tratto {version}")).clicked()
                {
                    let _ = webbrowser::open(&url);
                }
                if side_button(ui, "file-up", "Importa una lavagna").clicked() {
                    action = self.import(store, toasts);
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.bg).inner_margin(egui::Margin { left: 32, right: 32, top: 18, bottom: 0 })).show(ui, |ui| {
            ui::row(ui, 32.0, |ui| {
                ui.label(RichText::new("Recenti").font(ui::medium(15.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui::button(ui, "Nuova lavagna", Btn::Primary, Some("plus"), false, true).clicked() {
                        action = Some(Action::New { template: None });
                    }
                    self.sort_menu(ui, &t);
                    search_field(ui, &mut self.query, &t);
                });
            });
            ui.add_space(12.0);
            let jump = self.jump.take();
            let out = ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let view = ui.clip_rect();
                // Your boards.
                let mine = ui.label(RichText::new("Le tue lavagne").font(ui::medium(13.0)).color(t.text));
                if jump == Some(0) {
                    mine.scroll_to_me(Some(egui::Align::TOP));
                }
                ui.add_space(10.0);
                let q = self.query.trim().to_lowercase();
                let mut shown: Vec<BoardRow> = self.boards.iter().filter(|b| q.is_empty() || b.title.to_lowercase().contains(&q)).cloned().collect();
                match self.sort {
                    Sort::Updated => shown.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
                    Sort::Created => shown.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
                    Sort::Title => shown.sort_by_key(|b| b.title.to_lowercase()),
                }
                if self.boards.is_empty() {
                    empty_state(ui, &t, &mut action);
                } else if shown.is_empty() {
                    ui::hint(ui, &format!("Nessuna lavagna contiene «{}».", self.query.trim()));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(20.0, 20.0);
                        for b in &shown {
                            if let Some(a) = self.card(ui, store, b, toasts) {
                                action = Some(a);
                            }
                        }
                    });
                }
                ui.add_space(36.0);
                // Templates.
                let head = ui.label(RichText::new("Modelli").font(ui::medium(13.0)).color(t.text));
                if jump == Some(1) {
                    head.scroll_to_me(Some(egui::Align::TOP));
                }
                ui::hint(ui, "Parti da una struttura pronta: la trovi già sulla lavagna nuova, da cambiare come vuoi.");
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(20.0, 20.0);
                    if template_card(ui, None, "Lavagna vuota", "Spazio infinito, tutto da scrivere").clicked() {
                        action = Some(Action::New { template: None });
                    }
                    for tpl in TEMPLATES.iter() {
                        if template_card(ui, Some(tpl), tpl.name, tpl.description).clicked() {
                            action = Some(Action::New { template: Some(tpl.id.into()) });
                        }
                    }
                });
                ui.add_space(32.0);
                // The templates count as "in view" once their title is near the top.
                head.rect.min.y < view.min.y + 120.0
            });
            // At the end of the list (or when it all fits) the sidebar keeps what was chosen.
            let end = out.state.offset.y >= (out.content_size.y - out.inner_rect.height()).max(0.0) - 1.0;
            let section = if out.inner || (end && self.picked == 1) { 1u8 } else { 0 };
            ctx.data_mut(|d| d.insert_temp(section_id, section));
        });
        // Only a hairline between the sidebar and the list, as in Figma.
        let sr = side.response.rect;
        ctx.layer_painter(egui::LayerId::background()).vline(sr.max.x - 0.5, sr.y_range(), Stroke::new(1.0, t.border));
        // Deleting asks first.
        if let Some(b) = self.deleting.clone() {
            let mut confirm = false;
            let mut cancel = false;
            let mut open = self.delete_open;
            let on_screen = super::dialogs::dialog(&ctx, "delete", "Eliminare la lavagna?", 400.0, &mut open, |ui| {
                ui.label(RichText::new(format!("«{}» e le sue immagini verranno cancellate da questo PC. Non si può annullare.", b.title)).color(t.text));
                ui.add_space(8.0);
                ui::row(ui, 24.0, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui::button(ui, "Elimina", Btn::Danger, None, false, true).clicked() {
                            confirm = true;
                        }
                        if ui::button(ui, "Annulla", Btn::Secondary, None, false, true).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
            self.delete_open = open && !confirm && !cancel;
            if confirm {
                match store.delete(&b.id) {
                    Ok(()) => toasts.info("Lavagna eliminata."),
                    Err(e) => toasts.error(e),
                }
                self.reload(store);
            }
            if !on_screen && !self.delete_open {
                self.deleting = None;
            }
        }
        // A .tratto file dropped on the window is imported.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Ok(text) = std::fs::read_to_string(f.path()) {
                match crate::export::read_tratto(&text).and_then(|(title, state, files)| store.import(&title, &state, &files)) {
                    Ok(id) => action = Some(Action::Open { id, template: None }),
                    Err(e) => toasts.error(e),
                }
            }
        }
        action
    }

    fn sort_menu(&mut self, ui: &mut Ui, t: &ui::Theme) {
        let label = match self.sort {
            Sort::Updated => "Ultima modifica",
            Sort::Created => "Data di creazione",
            Sort::Title => "Nome",
        };
        let s = ui::sizes(ui.ctx());
        let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(s.fs), t.text);
        let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 34.0, 28.0), Sense::click());
        ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
        ui.painter().galley(pos2(r.min.x + 10.0, r.center().y - g.size().y / 2.0), g, t.text);
        icon(ui, "chevron-down", pos2(r.max.x - 13.0, r.center().y), 12.0, t.icon2);
        let resp = ui::tip(resp, "Ordina le lavagne", None);
        egui::Popup::menu(&resp).frame(ui::menu_frame(t)).gap(4.0).show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (s, l) in [(Sort::Updated, "Ultima modifica"), (Sort::Created, "Data di creazione"), (Sort::Title, "Nome")] {
                if ui::menu_item(ui, (self.sort == s).then_some("check"), l, None, false).clicked() {
                    self.sort = s;
                }
            }
        });
    }

    fn card(&mut self, ui: &mut Ui, store: &Store, b: &BoardRow, toasts: &mut Toasts) -> Option<Action> {
        let t = ui::theme(ui.ctx());
        let ctx = ui.ctx().clone();
        let mut action = None;
        let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, THUMB_H + 50.0), Sense::hover());
        let over = ui.rect_contains_pointer(rect);
        let thumb_rect = Rect::from_min_size(rect.min, vec2(CARD_W, THUMB_H));
        let thumb = ui.interact(thumb_rect, Id::new(("open", &b.id)), Sense::click());
        // Figma's file browser: a firmer outline and a faint veil, nothing that lifts or glows.
        let k = ui::motion::hover(&ctx, thumb.id.with("h"), thumb.hovered());
        let down = ui::motion::hover(&ctx, thumb.id.with("down"), thumb.is_pointer_button_down_on());
        ui.painter().rect_filled(thumb_rect, 8.0, t.bg2);
        match self.thumb(&ctx, store, b) {
            Some(tex) => {
                ui.painter().add(egui::epaint::RectShape::filled(thumb_rect.shrink(1.0), 7.0, Color32::WHITE).with_texture(tex.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
            }
            None => icon(ui, "pen-line", thumb_rect.center(), 22.0, t.text3),
        }
        let veil = 0.035 * k + 0.05 * down;
        if veil > 0.0 {
            ui.painter().rect_filled(thumb_rect, 8.0, if t.dark { Color32::from_white_alpha((veil * 255.0) as u8) } else { Color32::from_black_alpha((veil * 255.0) as u8) });
        }
        ui.painter().rect_stroke(thumb_rect, 8.0, Stroke::new(1.0, ui::blend(t.border, t.border_strong, k)), egui::StrokeKind::Inside);
        ui::focus_ring(ui, &thumb, thumb_rect);
        thumb.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Apri {}", b.title)));
        if thumb.clicked() {
            action = Some(Action::Open { id: b.id.clone(), template: None });
        }
        let meta = Rect::from_min_max(pos2(rect.min.x, thumb_rect.max.y + 10.0), rect.max);
        let renaming = self.renaming.as_deref() == Some(b.id.as_str());
        if renaming {
            let r = ui.put(Rect::from_min_size(meta.min + vec2(0.0, -2.0), vec2(CARD_W - 34.0, 22.0)), egui::TextEdit::singleline(&mut self.rename_buf));
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            if r.lost_focus() {
                let v = self.rename_buf.trim().to_string();
                self.renaming = None;
                if !v.is_empty() && v != b.title && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if let Err(e) = store.rename(&b.id, &v) {
                        toasts.error(e);
                    }
                    self.reload(store);
                }
            }
        } else {
            ui.painter().with_clip_rect(Rect::from_min_size(meta.min, vec2(CARD_W - 34.0, 40.0))).text(meta.min + vec2(2.0, 9.0), egui::Align2::LEFT_CENTER, &b.title, ui::medium(11.0), t.text);
        }
        ui.painter().text(meta.min + vec2(2.0, 27.0), egui::Align2::LEFT_CENTER, format!("Modificata {}", time_ago(b.updated_at)), egui::FontId::proportional(10.0), t.text2);
        // "More" shows while the pointer is on the card (or its menu is open).
        let more_rect = Rect::from_min_size(pos2(meta.max.x - 26.0, meta.min.y - 3.0), vec2(26.0, 24.0));
        let more = ui.interact(more_rect, Id::new(("more", &b.id)), Sense::click());
        let menu_open = egui::Popup::is_id_open(&ctx, egui::Popup::default_response_id(&more));
        let a = ui::motion::hover(&ctx, more.id.with("show"), over || menu_open || more.has_focus());
        ui::hover_fill(ui, more.id, more_rect, more.hovered() || menu_open, ui::RADIUS as f32, t.hover);
        if a > 0.0 {
            icon(ui, "more-horizontal", more_rect.center(), 16.0, t.icon.gamma_multiply(a));
        }
        let more = ui::tip(more, "Altre azioni", None);
        egui::Popup::menu(&more).frame(ui::menu_frame(&t)).gap(4.0).show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if ui::menu_item(ui, Some("pencil"), "Rinomina", None, false).clicked() {
                self.rename_buf = b.title.clone();
                self.renaming = Some(b.id.clone());
            }
            if ui::menu_item(ui, Some("copy"), "Duplica", None, false).clicked() {
                match store.duplicate(&b.id, None) {
                    Ok(_) => toasts.info("Lavagna duplicata."),
                    Err(e) => toasts.error(e),
                }
                self.reload(store);
            }
            if ui::menu_item(ui, Some("download"), "Esporta file .tratto", None, false).clicked() {
                let state = store.ydoc(&b.id).ok().flatten().unwrap_or_default();
                match crate::export::tratto(store, &b.id, &b.title, &state) {
                    Ok(json) => {
                        if let Some(path) = rfd::FileDialog::new().set_file_name(format!("{}.tratto", crate::export::safe_filename(&b.title))).save_file()
                            && let Err(e) = std::fs::write(path, json)
                        {
                            toasts.error(format!("Non riesco a salvare il file: {e}"));
                        }
                    }
                    Err(e) => toasts.error(e),
                }
            }
            ui::menu_sep(ui);
            if ui::menu_item(ui, Some("trash-2"), "Elimina", None, true).clicked() {
                self.deleting = Some(b.clone());
                self.delete_open = true;
            }
        });
        action
    }
}

/// A rounded search box with its magnifier, like Figma's.
fn search_field(ui: &mut Ui, query: &mut String, t: &ui::Theme) {
    let s = ui::sizes(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(220.0, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 6.0, t.bg2);
    icon(ui, "search", pos2(rect.min.x + 14.0, rect.center().y), 14.0, t.icon2);
    let field = Rect::from_min_max(pos2(rect.min.x + 28.0, rect.min.y), pos2(rect.max.x - 6.0, rect.max.y));
    let edit = egui::TextEdit::singleline(query).id(Id::new("home-search")).frame(egui::Frame::NONE).hint_text(RichText::new("Cerca").color(t.text3)).font(egui::FontId::proportional(s.fs)).text_color(t.text).vertical_align(egui::Align::Center).desired_width(field.width());
    let resp = ui.put(field, edit);
    let k = ui::motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered() && !resp.has_focus());
    if resp.has_focus() {
        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, t.brand), egui::StrokeKind::Inside);
    } else if k > 0.0 {
        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, t.border_strong.gamma_multiply(k)), egui::StrokeKind::Inside);
    }
}

fn empty_state(ui: &mut Ui, t: &ui::Theme, action: &mut Option<Action>) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 168.0), Sense::hover());
    ui.painter().rect(rect, 10.0, t.bg2, Stroke::NONE, egui::StrokeKind::Inside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(24.0)).layout(egui::Layout::top_down(egui::Align::Center)));
    child.add_space(8.0);
    icon(&child, "pen-line", child.cursor().min + vec2(child.available_width() / 2.0, 12.0), 22.0, t.text3);
    child.add_space(30.0);
    child.label(RichText::new("Nessuna lavagna, per ora").font(ui::medium(13.0)).color(t.text));
    ui::hint(&mut child, "Crea una lavagna vuota o parti da uno dei modelli qui sotto. Tutto viene salvato su questo PC mentre lavori.");
    child.add_space(4.0);
    if ui::button(&mut child, "Nuova lavagna", Btn::Primary, Some("plus"), true, true).clicked() {
        *action = Some(Action::New { template: None });
    }
}

/// A section of the list in the sidebar (its highlight is drawn by the caller, gliding).
fn side_item(ui: &mut Ui, r: Rect, name: &str, label: &str, current: bool) -> egui::Response {
    let t = ui::theme(ui.ctx());
    let resp = ui.interact(r, Id::new(("home-side", label)), Sense::click());
    ui::hover_fill(ui, resp.id, r, resp.hovered() && !current, ui::RADIUS as f32, t.hover);
    icon(ui, name, pos2(r.min.x + 16.0, r.center().y), 16.0, t.icon);
    ui.painter().text(pos2(r.min.x + 34.0, r.center().y), egui::Align2::LEFT_CENTER, label, if current { ui::medium(11.0) } else { egui::FontId::proportional(11.0) }, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, current, label));
    ui::focus_ring(ui, &resp, r);
    resp
}

/// An action at the bottom of the sidebar.
fn side_button(ui: &mut Ui, name: &str, label: &str) -> egui::Response {
    let t = ui::theme(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
    icon(ui, name, pos2(r.min.x + 16.0, r.center().y), 16.0, t.icon);
    ui.painter().text(pos2(r.min.x + 34.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(11.0), t.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    ui::focus_ring(ui, &resp, r);
    resp
}

fn template_card(ui: &mut Ui, tpl: Option<&Template>, name: &str, desc: &str) -> egui::Response {
    let t = ui::theme(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(TPL_W, TPL_ART_H + 58.0), Sense::click());
    let art = Rect::from_min_size(rect.min, vec2(TPL_W, TPL_ART_H));
    let k = ui::motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
    let down = ui::motion::hover(ui.ctx(), resp.id.with("down"), resp.is_pointer_button_down_on());
    super::panels::template_art(ui, tpl, art);
    let veil = 0.035 * k + 0.05 * down;
    if veil > 0.0 {
        ui.painter().rect_filled(art, 8.0, if t.dark { Color32::from_white_alpha((veil * 255.0) as u8) } else { Color32::from_black_alpha((veil * 255.0) as u8) });
    }
    ui.painter().rect_stroke(art, 8.0, Stroke::new(1.0, ui::blend(t.border, t.border_strong, k)), egui::StrokeKind::Inside);
    ui.painter().text(rect.min + vec2(2.0, TPL_ART_H + 15.0), egui::Align2::LEFT_CENTER, name, ui::medium(11.0), t.text);
    let g = ui.painter().layout(desc.to_string(), egui::FontId::proportional(10.0), t.text2, TPL_W - 4.0);
    ui.painter().galley(rect.min + vec2(2.0, TPL_ART_H + 26.0), g, t.text2);
    ui::focus_ring(ui, &resp, art);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    resp
}
