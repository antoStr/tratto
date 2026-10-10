//! The list of boards: recent boards with their pictures, templates, import and settings.

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
    thumbs: HashMap<String, (i64, Option<TextureHandle>)>,
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

impl Home {
    pub fn new(store: &Store) -> Home {
        Home { boards: store.boards().unwrap_or_default(), query: String::new(), sort: Sort::Updated, renaming: None, rename_buf: String::new(), deleting: None, thumbs: HashMap::new() }
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
        egui::Panel::left(Id::new("home-side")).exact_size(240.0).resizable(false).frame(egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.border)).inner_margin(egui::Margin::same(12))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui::logo(ui, 24.0);
                ui.label(RichText::new("Tratto").font(ui::medium(15.0)).color(t.text));
            });
            ui.add_space(16.0);
            side_item(ui, "clock", "Recenti", true);
            if side_item(ui, "layout-template", "Modelli", false).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(Id::new("scroll-templates"), true));
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                if side_item(ui, "settings-2", "Impostazioni", false).clicked() {
                    dialogs.settings = true;
                }
                if let crate::updates::State::Available { version, url } = crate::updates::current()
                    && side_item(ui, "download", &format!("Scarica Tratto {version}"), false).clicked()
                {
                    let _ = webbrowser::open(&url);
                }
                if side_item(ui, "file-up", "Importa una lavagna", false).clicked() {
                    action = self.import(store, toasts);
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.bg).inner_margin(egui::Margin::symmetric(32, 20))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recenti").font(ui::medium(15.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::button(ui, "Nuova lavagna", Btn::Primary, Some("plus"), false, true).clicked() {
                        action = Some(Action::New { template: None });
                    }
                    let label = match self.sort {
                        Sort::Updated => "Ultima modifica",
                        Sort::Created => "Data di creazione",
                        Sort::Title => "Nome",
                    };
                    let r = ui::button(ui, label, Btn::Ghost, None, false, true);
                    egui::Popup::menu(&r).frame(ui::menu_frame(&t)).show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (s, l) in [(Sort::Updated, "Ultima modifica"), (Sort::Created, "Data di creazione"), (Sort::Title, "Nome")] {
                            if ui::menu_item(ui, (self.sort == s).then_some("check"), l, None, false).clicked() {
                                self.sort = s;
                            }
                        }
                    });
                    ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Cerca").desired_width(200.0));
                });
            });
            ui.add_space(12.0);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                // Templates.
                let resp = ui.label(RichText::new("Inizia da un modello").font(ui::medium(13.0)).color(t.text));
                if ui.ctx().data_mut(|d| d.remove_temp::<bool>(Id::new("scroll-templates"))).is_some() {
                    resp.scroll_to_me(Some(egui::Align::TOP));
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    if template_card(ui, None, "Lavagna vuota", "Spazio infinito, tutto da scrivere").clicked() {
                        action = Some(Action::New { template: None });
                    }
                    for tpl in TEMPLATES.iter() {
                        if template_card(ui, Some(tpl), tpl.name, tpl.description).clicked() {
                            action = Some(Action::New { template: Some(tpl.id.into()) });
                        }
                    }
                });
                ui.add_space(24.0);
                ui.label(RichText::new("Le tue lavagne").font(ui::medium(13.0)).color(t.text));
                ui.add_space(8.0);
                let q = self.query.trim().to_lowercase();
                let mut shown: Vec<BoardRow> = self.boards.iter().filter(|b| q.is_empty() || b.title.to_lowercase().contains(&q)).cloned().collect();
                match self.sort {
                    Sort::Updated => shown.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
                    Sort::Created => shown.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
                    Sort::Title => shown.sort_by_key(|b| b.title.to_lowercase()),
                }
                if self.boards.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.label(RichText::new("Nessuna lavagna, per ora").font(ui::medium(13.0)).color(t.text));
                        ui::hint(ui, "Crea una lavagna vuota o parti da un modello. Tutto viene salvato su questo PC mentre lavori.");
                        if ui::button(ui, "Nuova lavagna", Btn::Primary, Some("plus"), true, true).clicked() {
                            action = Some(Action::New { template: None });
                        }
                    });
                } else if shown.is_empty() {
                    ui::hint(ui, &format!("Nessuna lavagna contiene «{}».", self.query.trim()));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
                        for b in &shown {
                            if let Some(a) = self.card(ui, store, b, toasts) {
                                action = Some(a);
                            }
                        }
                    });
                }
            });
        });
        // Deleting asks first.
        if let Some(b) = self.deleting.clone() {
            let mut confirm = false;
            let open = super::dialogs::dialog(&ctx, "delete", "Eliminare la lavagna?", 400.0, |ui| {
                ui.label(RichText::new(format!("«{}» e le sue immagini verranno cancellate da questo PC. Non si può annullare.", b.title)).color(t.text));
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::button(ui, "Elimina", Btn::Danger, None, false, true).clicked() {
                        confirm = true;
                    }
                    if ui::button(ui, "Annulla", Btn::Secondary, None, false, true).clicked() {
                        self.deleting = None;
                    }
                });
            });
            if confirm {
                self.deleting = None;
                match store.delete(&b.id) {
                    Ok(()) => toasts.info("Lavagna eliminata."),
                    Err(e) => toasts.error(e),
                }
                self.reload(store);
            } else if !open {
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

    fn card(&mut self, ui: &mut Ui, store: &Store, b: &BoardRow, toasts: &mut Toasts) -> Option<Action> {
        let t = ui::theme(ui.ctx());
        let mut action = None;
        let (rect, _) = ui.allocate_exact_size(vec2(240.0, 206.0), Sense::hover());
        let thumb_rect = Rect::from_min_size(rect.min, vec2(240.0, 150.0));
        let thumb = ui.interact(thumb_rect, Id::new(("open", &b.id)), Sense::click());
        ui.painter().rect_filled(thumb_rect, ui::RADIUS, t.bg2);
        match self.thumb(ui.ctx(), store, b) {
            Some(tex) => {
                ui.painter().image(tex.id(), thumb_rect.shrink(1.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => icon(ui, "pen-line", thumb_rect.center(), 24.0, t.text3),
        }
        ui.painter().rect_stroke(thumb_rect, ui::RADIUS, Stroke::new(if thumb.hovered() { 2.0 } else { 1.0 }, if thumb.hovered() { t.brand } else { t.border }), egui::StrokeKind::Inside);
        if ui::tip(thumb, &format!("Apri {}", b.title), None).clicked() {
            action = Some(Action::Open { id: b.id.clone(), template: None });
        }
        let meta = Rect::from_min_max(pos2(rect.min.x, thumb_rect.max.y + 8.0), rect.max);
        icon(ui, "pen-line", pos2(meta.min.x + 10.0, meta.min.y + 14.0), 12.0, t.icon2);
        let renaming = self.renaming.as_deref() == Some(b.id.as_str());
        if renaming {
            let r = ui.put(Rect::from_min_size(meta.min + vec2(24.0, 2.0), vec2(180.0, 22.0)), egui::TextEdit::singleline(&mut self.rename_buf));
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
            ui.painter().with_clip_rect(Rect::from_min_size(meta.min, vec2(200.0, 40.0))).text(meta.min + vec2(24.0, 10.0), egui::Align2::LEFT_CENTER, &b.title, ui::medium(11.0), t.text);
        }
        ui.painter().text(meta.min + vec2(24.0, 28.0), egui::Align2::LEFT_CENTER, format!("Modificata {}", time_ago(b.updated_at)), egui::FontId::proportional(10.0), t.text2);
        let more_rect = Rect::from_min_size(pos2(meta.max.x - 26.0, meta.min.y + 2.0), vec2(24.0, 24.0));
        let more = ui.interact(more_rect, Id::new(("more", &b.id)), Sense::click());
        if more.hovered() {
            ui.painter().rect_filled(more_rect, ui::RADIUS, t.hover);
        }
        icon(ui, "more-horizontal", more_rect.center(), 16.0, t.icon);
        let more = ui::tip(more, "Altre azioni", None);
        egui::Popup::menu(&more).frame(ui::menu_frame(&t)).show(|ui| {
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
            }
        });
        action
    }
}

fn side_item(ui: &mut Ui, name: &str, label: &str, current: bool) -> egui::Response {
    let t = ui::theme(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    if current {
        ui.painter().rect_filled(r, ui::RADIUS, t.selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, ui::RADIUS, t.hover);
    }
    icon(ui, name, pos2(r.min.x + 16.0, r.center().y), 16.0, t.icon);
    ui.painter().text(pos2(r.min.x + 32.0, r.center().y), egui::Align2::LEFT_CENTER, label, if current { ui::medium(11.0) } else { egui::FontId::proportional(11.0) }, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, current, label));
    resp
}

fn template_card(ui: &mut Ui, tpl: Option<&Template>, name: &str, desc: &str) -> egui::Response {
    let t = ui::theme(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(180.0, 170.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect.expand(4.0), ui::RADIUS + 2, t.hover);
    }
    super::panels::template_art(ui, tpl, Rect::from_min_size(rect.min, vec2(180.0, 112.0)));
    ui.painter().text(rect.min + vec2(2.0, 124.0), egui::Align2::LEFT_CENTER, name, ui::medium(11.0), t.text);
    let g = ui.painter().layout(desc.to_string(), egui::FontId::proportional(10.0), t.text2, 176.0);
    ui.painter().galley(rect.min + vec2(2.0, 134.0), g, t.text2);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    resp
}
