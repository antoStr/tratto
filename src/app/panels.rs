//! The side panels (layers and templates on the left, design and sharing on the right), the
//! pills that replace them when closed, the context menu and the text bar.

use std::sync::Arc;

use egui::{Color32, Id, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2, vec2};

use super::board::{BoardScreen, BoardUi, LeftTab};
use super::{Action, Toasts, dialogs::Dialogs};
use crate::editor::{AlignKind, Editor, Order, Tool, peer_color, peer_name};
use crate::geom::{BBox, aabb, frame_box, scale_element, union};
use crate::model::*;
use crate::prefs;
use crate::templates::Template;
use crate::ui::{self, Kind as Btn, Theme, icon, small_icon_button};

/* ---------------- names ---------------- */

pub fn type_label(el: &El) -> &'static str {
    match el.kind {
        Kind::Ink(_) => "Tratto",
        Kind::Highlighter(_) => "Evidenziatura",
        Kind::Shape(_) => "Forma",
        Kind::Line(_) => "Linea",
        Kind::Text(_) => "Testo",
        Kind::Sticky(_) => "Nota adesiva",
        Kind::Image { .. } => "Immagine",
        Kind::Stamp { .. } => "Reazione",
        Kind::Section { .. } => "Sezione",
        Kind::Comment { .. } => "Commento",
    }
}

fn shape_name(k: ShapeKind) -> &'static str {
    match k {
        ShapeKind::Rect => "Rettangolo",
        ShapeKind::Ellipse => "Ellisse",
        ShapeKind::Triangle => "Triangolo",
        ShapeKind::TriangleDown => "Triangolo capovolto",
        ShapeKind::Diamond => "Rombo",
        ShapeKind::Parallelogram => "Parallelogramma",
        ShapeKind::Pentagon => "Pentagono",
        ShapeKind::Hexagon => "Esagono",
        ShapeKind::Octagon => "Ottagono",
        ShapeKind::Star => "Stella",
        ShapeKind::Plus => "Croce",
        ShapeKind::ArrowRight => "Freccia a destra",
        ShapeKind::ArrowLeft => "Freccia a sinistra",
        ShapeKind::Polygon => "Poligono",
    }
}

fn first_line(s: &str) -> String {
    s.trim().lines().next().unwrap_or("").chars().take(40).collect()
}

/// What an element is called in the layers panel and elsewhere.
pub fn element_label(el: &El) -> String {
    if let Some(n) = el.name.as_ref().filter(|n| !n.is_empty()) {
        return n.clone();
    }
    let or = |s: String, d: &str| if s.is_empty() { d.to_string() } else { s };
    match &el.kind {
        Kind::Text(t) => or(first_line(&t.text), "Testo"),
        Kind::Sticky(s) => or(first_line(&s.text), "Nota adesiva"),
        Kind::Comment { thread } => or(thread.first().map(|m| first_line(&m.text)).unwrap_or_default(), "Commento"),
        Kind::Line(l) => (if l.tape {
            "Nastro adesivo"
        } else if l.from.is_some() || l.to.is_some() {
            "Connettore"
        } else if l.arrow_end || l.arrow_start {
            "Freccia"
        } else {
            "Linea"
        })
        .into(),
        Kind::Shape(s) => or(first_line(s.text.as_deref().unwrap_or("")), shape_name(s.shape)),
        Kind::Stamp { emoji } => crate::assets::stamp(emoji).map_or(format!("Reazione {emoji}"), |s| s.name.to_string()),
        _ => type_label(el).into(),
    }
}

fn type_icon(el: &El) -> &'static str {
    match &el.kind {
        Kind::Ink(_) => "pen-line",
        Kind::Highlighter(_) => "highlighter",
        Kind::Shape(s) if s.shape == ShapeKind::Ellipse => "circle",
        Kind::Shape(_) => "square",
        Kind::Line(l) if l.arrow_end || l.arrow_start => "arrow-up-right",
        Kind::Line(_) => "minus",
        Kind::Text(_) => "type",
        Kind::Sticky(_) => "sticky-note",
        Kind::Image { .. } => "image",
        Kind::Stamp { .. } => "smile",
        Kind::Section { .. } => "square-dashed-top-solid",
        Kind::Comment { .. } => "message-circle",
    }
}

/* ---------------- left panel ---------------- */

pub fn left(ui: &mut Ui, b: &mut BoardScreen, toasts: &mut Toasts, dialogs: &mut Dialogs) -> Option<Action> {
    let t = ui::theme(ui.ctx());
    let mut action = None;
    // Header: app menu, name, status.
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    ui.painter().hline(head.x_range(), head.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(8.0, 6.0))), |ui| {
        ui.horizontal_centered(|ui| {
            if let Some(a) = app_menu(ui, b, dialogs) {
                action = Some(a);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                title(ui, b, toasts, 140.0);
                let saved = b.is_saved();
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 12.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.0, if saved { Color32::from_rgb(0x14, 0xAE, 0x5C) } else { t.text3 });
                    let status = match (b.host, saved) {
                        (true, true) => "Tutto salvato",
                        (true, false) => "Salvataggio…",
                        (false, true) => "In diretta",
                        (false, false) => "Mi collego…",
                    };
                    ui.label(RichText::new(status).size(10.0).color(t.text2));
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_icon_button(ui, "panel-left-close", "Nascondi il pannello", false).clicked() {
                    b.editor.prefs.left_panel = false;
                }
            });
        });
    });
    // Tabs.
    let (tabs, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().hline(tabs.x_range(), tabs.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(tabs.shrink2(vec2(8.0, 8.0))), |ui| {
        ui.horizontal_centered(|ui| {
            tab(ui, &mut b.ui.left_tab, LeftTab::Layers, "Livelli");
            if !b.editor.read_only {
                tab(ui, &mut b.ui.left_tab, LeftTab::Templates, "Modelli");
            }
        });
    });
    if b.ui.left_tab == LeftTab::Templates && !b.editor.read_only {
        templates(ui, &mut b.editor, &t);
    } else {
        layers(ui, &mut b.editor, &mut b.ui, &t);
    }
    action
}

fn tab<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, v: T, label: &str) {
    let t = ui::theme(ui.ctx());
    let on = *value == v;
    let font = if on { ui::medium(11.0) } else { egui::FontId::proportional(11.0) };
    let g = ui.painter().layout_no_wrap(label.into(), font, if on { t.text } else { t.text2 });
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 24.0), Sense::click());
    if resp.hovered() && !on {
        ui.painter().rect_filled(r, ui::RADIUS, t.hover);
    }
    ui.painter().galley(r.center() - g.size() / 2.0, g, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, label));
    if resp.clicked() {
        *value = v;
    }
}

/// The board's name; click to rename.
fn title(ui: &mut Ui, b: &mut BoardScreen, toasts: &mut Toasts, width: f32) {
    let t = ui::theme(ui.ctx());
    if let Some(buf) = &mut b.ui.title_edit {
        let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(width).font(ui::medium(11.0)));
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        if resp.lost_focus() {
            let v = buf.trim().to_string();
            b.ui.title_edit = None;
            if !v.is_empty() && v != b.title && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                b.set_title(&v, toasts);
            }
        }
        return;
    }
    let g = ui.painter().layout_no_wrap(b.title.clone(), ui::medium(11.0), t.text);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x.min(width), 18.0), Sense::click());
    ui.painter().with_clip_rect(r).galley(pos2(r.min.x, r.center().y - g.size().y / 2.0), g, t.text);
    if ui::tip(resp, "Rinomina", None).clicked() {
        b.ui.title_edit = Some(b.title.clone());
    }
}

fn app_menu(ui: &mut Ui, b: &mut BoardScreen, dialogs: &mut Dialogs) -> Option<Action> {
    let t = ui::theme(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 32.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, ui::RADIUS, t.hover);
    }
    ui::icons::logo(ui, Rect::from_center_size(r.center() - vec2(6.0, 0.0), vec2(20.0, 20.0)));
    icon(ui, "chevron-down", r.center() + vec2(13.0, 0.0), 12.0, t.icon2);
    let resp = ui::tip(resp, "Menu principale", None);
    let mut action = None;
    egui::Popup::menu(&resp).frame(ui::menu_frame(&t)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if b.host {
            if ui::menu_item(ui, Some("arrow-left"), "Torna alle lavagne", None, false).clicked() {
                action = Some(Action::Home);
            }
            if ui::menu_item(ui, Some("plus"), "Nuova lavagna", None, false).clicked() {
                action = Some(Action::New { template: None });
            }
            ui::menu_sep(ui);
        }
        if !b.editor.read_only && ui::menu_item(ui, Some("image-plus"), "Inserisci immagine…", Some("I"), false).clicked() {
            b.editor.requests.push(crate::editor::Request::InsertImage);
        }
        if ui::menu_item(ui, Some("download"), "Esporta…", Some("Ctrl+Maiusc+E"), false).clicked() {
            b.ui.export = Some(false);
        }
        ui::menu_sep(ui);
        ui.label(RichText::new("Tema").size(10.0).color(t.menu_text2));
        for (v, label, ic) in [(prefs::Theme::System, "Come il sistema", "monitor"), (prefs::Theme::Light, "Chiaro", "sun"), (prefs::Theme::Dark, "Scuro", "moon")] {
            let on = b.editor.prefs.theme == v;
            if ui::menu_item(ui, Some(if on { "check" } else { ic }), label, None, false).clicked() {
                b.editor.prefs.theme = v;
            }
        }
        ui::menu_sep(ui);
        if ui::menu_item(ui, Some("settings-2"), "Impostazioni…", Some("Ctrl+,"), false).clicked() {
            dialogs.settings = true;
        }
        let focus = b.editor.prefs.focus;
        if ui::menu_item(ui, Some("panels-top-left"), if focus { "Mostra i pannelli" } else { "Nascondi i pannelli" }, Some("Ctrl+\\"), false).clicked() {
            b.editor.toggle_focus();
        }
        if ui::menu_item(ui, Some("keyboard"), "Scorciatoie da tastiera", Some("?"), false).clicked() {
            b.ui.shortcuts = true;
        }
    });
    action
}

/* ---------- layers ---------- */

enum Row {
    El(Arc<El>, u8),
    /// `auto`: strokes written one after the other in the same spot, shown together.
    Folder { id: String, name: String, members: Vec<Arc<El>>, auto: bool },
}

fn row_ids(r: &Row) -> Vec<String> {
    match r {
        Row::El(e, _) => vec![e.id.clone()],
        Row::Folder { members, .. } => members.iter().map(|m| m.id.clone()).collect(),
    }
}

fn row_key(r: &Row) -> String {
    match r {
        Row::El(e, _) => e.id.clone(),
        Row::Folder { id, .. } => format!("folder:{id}"),
    }
}

/// Strokes farther apart than this (board units) start a new "Scrittura" row.
const RUN_GAP: f64 = 300.0;

fn near(a: &BBox, b: &BBox) -> bool {
    a.x - RUN_GAP <= b.right() && b.x - RUN_GAP <= a.right() && a.y - RUN_GAP <= b.bottom() && b.y - RUN_GAP <= a.bottom()
}

fn push_container(rows: &mut Vec<Row>, open: &std::collections::HashSet<String>, id: String, name: String, members: Vec<Arc<El>>, auto: bool) {
    let is_open = open.contains(&id);
    let shown = if is_open { members.clone() } else { Vec::new() };
    rows.push(Row::Folder { id, name, members, auto });
    rows.extend(shown.into_iter().map(|m| Row::El(m, 1)));
}

fn flush_run(rows: &mut Vec<Row>, open: &std::collections::HashSet<String>, run: &mut Vec<Arc<El>>) {
    if run.len() == 1 {
        rows.push(Row::El(run[0].clone(), 0));
    } else if run.len() > 1 {
        // Keyed by the oldest stroke, so the row keeps its state while more strokes come.
        let name = if run.iter().all(|e| matches!(e.kind, Kind::Highlighter(_))) { "Evidenziature" } else { "Scrittura" };
        let id = format!("ink:{}", run[run.len() - 1].id);
        push_container(rows, open, id, name.into(), std::mem::take(run), true);
    }
    run.clear();
}

/// Layers top to bottom; each folder sits where its top element is, members indented below it
/// when open. Consecutive nearby strokes outside folders collapse into one row.
fn build_rows(ed: &mut Editor, open: &std::collections::HashSet<String>) -> Vec<Row> {
    let items: Vec<Arc<El>> = ed.board.all().iter().rev().cloned().collect();
    let mut by_folder: std::collections::HashMap<String, Vec<Arc<El>>> = Default::default();
    for el in &items {
        if let Some(g) = &el.group_id {
            by_folder.entry(g.clone()).or_default().push(el.clone());
        }
    }
    let mut rows = Vec::new();
    let mut run: Vec<Arc<El>> = Vec::new();
    let mut run_box: Option<BBox> = None;
    for el in &items {
        if el.group_id.is_none() && el.is_ink() {
            let b = aabb(el);
            if run_box.is_some_and(|r| !near(&r, &b)) {
                flush_run(&mut rows, open, &mut run);
                run_box = None;
            }
            run.push(el.clone());
            run_box = Some(run_box.map_or(b, |r| union([r, b]).unwrap()));
            continue;
        }
        flush_run(&mut rows, open, &mut run);
        run_box = None;
        match &el.group_id {
            None => rows.push(Row::El(el.clone(), 0)),
            Some(g) if by_folder[g][0].id == el.id => {
                let name = ed.board.group_name(g);
                push_container(&mut rows, open, g.clone(), name, by_folder[g].clone(), false);
            }
            _ => {}
        }
    }
    flush_run(&mut rows, open, &mut run);
    rows
}

fn layers(ui: &mut Ui, ed: &mut Editor, st: &mut BoardUi, t: &Theme) {
    let rows = build_rows(ed, &st.open_folders);
    if rows.is_empty() {
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui::hint(ui, "Ancora niente qui. Tutto quello che disegni o aggiungi comparirà in questo elenco.");
            });
        });
        return;
    }
    let selected: std::collections::HashSet<String> = ed.selection.iter().cloned().collect();
    let ro = ed.read_only;
    ScrollArea::vertical().auto_shrink(false).show_rows(ui, 32.0, rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for row in &rows[range] {
            let key = row_key(row);
            let ids = row_ids(row);
            let els: Vec<Arc<El>> = match row {
                Row::El(e, _) => vec![e.clone()],
                Row::Folder { members, .. } => members.clone(),
            };
            let locked = els.iter().all(|e| e.locked);
            let hidden = els.iter().all(|e| e.hidden);
            let is_sel = ids.iter().all(|i| selected.contains(i));
            let (label, depth, folder) = match row {
                Row::El(e, d) => (element_label(e), *d, false),
                Row::Folder { name, members, .. } => (format!("{name}  {}", members.len()), 0, true),
            };
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let r = rect.shrink2(vec2(8.0, 2.0));
            if is_sel {
                ui.painter().rect_filled(r, ui::RADIUS, t.selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, ui::RADIUS, t.hover);
            }
            let mut x = r.min.x + 8.0 + depth as f32 * 16.0;
            if let Row::Folder { id, auto, .. } = row {
                let chev = Rect::from_center_size(pos2(x + 6.0, r.center().y), vec2(16.0, 24.0));
                let c = ui.interact(chev, Id::new(("chev", &key)), Sense::click());
                let open = st.open_folders.contains(id);
                icon(ui, if open { "chevron-down" } else { "chevron-right" }, chev.center(), 12.0, t.icon2);
                if c.clicked() {
                    if open {
                        st.open_folders.remove(id);
                    } else {
                        st.open_folders.insert(id.clone());
                    }
                }
                x += 16.0;
                icon(ui, if *auto { "pen-line" } else { "folder" }, pos2(x + 7.0, r.center().y), 14.0, t.icon2);
            } else if let Row::El(e, _) = row {
                icon(ui, type_icon(e), pos2(x + 7.0, r.center().y), 14.0, t.icon2);
            }
            x += 22.0;
            let renaming = st.renaming.as_deref() == Some(key.as_str());
            if renaming {
                let edit = ui.put(Rect::from_min_max(pos2(x, r.min.y + 2.0), pos2(r.max.x - 8.0, r.max.y - 2.0)), egui::TextEdit::singleline(&mut st.rename_buf).font(egui::FontId::proportional(11.0)));
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if edit.lost_focus() {
                    let v = std::mem::take(&mut st.rename_buf);
                    st.renaming = None;
                    if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        match row {
                            Row::Folder { id, auto: false, .. } => ed.board.rename_group(id, &v),
                            Row::El(e, _) => {
                                let name: String = v.trim().chars().take(80).collect();
                                ed.board.update(&[e.id.clone()], |el| el.name = (!name.is_empty()).then(|| name.clone()));
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                let color = if hidden { t.text3 } else { t.text };
                let font = if folder { ui::medium(11.0) } else { egui::FontId::proportional(11.0) };
                ui.painter().with_clip_rect(Rect::from_min_max(pos2(x, r.min.y), pos2(r.max.x - 52.0, r.max.y))).text(pos2(x, r.center().y), egui::Align2::LEFT_CENTER, &label, font, color);
            }
            // Lock and hide, shown on hover or when on.
            if !ro && (resp.hovered() || locked || hidden) && !renaming {
                for (k, (name, tip, on)) in [("eye", if hidden { "Mostra" } else { "Nascondi" }, hidden), ("lock", if locked { "Sblocca" } else { "Blocca" }, locked)].into_iter().enumerate() {
                    let br = Rect::from_center_size(pos2(r.max.x - 12.0 - k as f32 * 22.0, r.center().y), vec2(20.0, 20.0));
                    let bresp = ui.interact(br, Id::new((name, &key)), Sense::click());
                    if bresp.hovered() {
                        ui.painter().rect_filled(br, 4.0, t.press);
                    }
                    let shown = match (name, on) {
                        ("eye", true) => "eye-off",
                        ("lock", false) => "unlock",
                        (n, _) => n,
                    };
                    icon(ui, shown, br.center(), 12.0, t.icon2);
                    if ui::tip(bresp, tip, None).clicked() {
                        ed.board.stop_capturing();
                        if name == "eye" {
                            ed.board.update(&ids, |e| e.hidden = !hidden);
                        } else {
                            ed.board.update(&ids, |e| e.locked = !locked);
                        }
                    }
                }
            }
            if resp.double_clicked() && !ro && !matches!(row, Row::Folder { auto: true, .. }) {
                st.rename_buf = match row {
                    Row::El(e, _) => element_label(e),
                    Row::Folder { name, .. } => name.clone(),
                };
                st.renaming = Some(key.clone());
            } else if resp.clicked() {
                let m = ui.input(|i| i.modifiers);
                if m.command || m.ctrl {
                    if ids.iter().all(|i| selected.contains(i)) {
                        ed.selection.retain(|i| !ids.contains(i));
                    } else {
                        for i in &ids {
                            if !ed.selection.contains(i) {
                                ed.selection.push(i.clone());
                            }
                        }
                    }
                } else {
                    ed.selection = ids.clone();
                    ed.reveal(&ids);
                }
                ed.tool = Tool::Select;
            }
            if resp.secondary_clicked() && !ro {
                if !ids.iter().all(|i| selected.contains(i)) {
                    ed.selection = ids.clone();
                }
                st.menu = resp.interact_pointer_pos();
            }
        }
    });
}

/* ---------- templates ---------- */

fn templates(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(8.0);
        for tpl in crate::templates::TEMPLATES.iter() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
            let r = rect.shrink2(vec2(8.0, 4.0));
            if resp.hovered() {
                ui.painter().rect_filled(r, ui::RADIUS, t.hover);
            }
            let art = Rect::from_min_size(r.min + vec2(6.0, 6.0), vec2(80.0, 52.0));
            template_art(ui, Some(tpl), art);
            let tx = art.max.x + 10.0;
            ui.painter().text(pos2(tx, r.min.y + 18.0), egui::Align2::LEFT_CENTER, tpl.name, ui::medium(11.0), t.text);
            let g = ui.painter().layout(tpl.description.to_string(), egui::FontId::proportional(10.0), t.text2, r.max.x - tx - 6.0);
            ui.painter().galley(pos2(tx, r.min.y + 28.0), g, t.text2);
            if resp.clicked() {
                ed.insert((tpl.build)(), None, true, true);
            }
        }
    });
}

/* ---------------- right panel ---------------- */

pub fn right(ui: &mut Ui, b: &mut BoardScreen, toasts: &mut Toasts) {
    let t = ui::theme(ui.ctx());
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    ui.painter().hline(head.x_range(), head.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 8.0))), |ui| {
        ui.horizontal_centered(|ui| {
            participants(ui, &mut b.editor);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| share_button(ui, b));
        });
    });
    let (sub, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().hline(sub.x_range(), sub.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(sub.shrink2(vec2(8.0, 8.0))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(4.0);
            ui.label(RichText::new("Design").font(ui::medium(11.0)).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_icon_button(ui, "panel-right-close", "Nascondi il pannello", false).clicked() {
                    b.editor.prefs.right_panel = false;
                }
                zoom_menu(ui, &mut b.editor, &t);
                super::live::spotlight_button(ui, &mut b.editor);
                super::live::vote_button(ui, &mut b.editor);
                super::live::timer_button(ui, &mut b.editor);
            });
        });
    });
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if b.editor.read_only {
            section(ui, Some("Solo visione"), |ui| ui::hint(ui, "Puoi guardare la lavagna, spostarti, fare zoom e usare il puntatore laser. Chi l'ha condivisa può darti il permesso di modificarla."));
        } else if b.editor.selected().is_empty() {
            board_props(ui, b);
        } else {
            selection_props(ui, &mut b.editor, &t);
        }
    });
    let _ = toasts;
}

fn zoom_menu(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    let label = format!("{}%", (ed.cam.z * 100.0).round());
    let g = ui.painter().layout_no_wrap(label.clone(), egui::FontId::proportional(11.0), t.text);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 26.0, 24.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, ui::RADIUS, t.hover);
    }
    ui.painter().galley(pos2(r.min.x + 6.0, r.center().y - g.size().y / 2.0), g, t.text);
    icon(ui, "chevron-down", pos2(r.max.x - 10.0, r.center().y), 12.0, t.icon2);
    let resp = ui::tip(resp, &format!("Zoom {label}"), None);
    egui::Popup::menu(&resp).frame(ui::menu_frame(t)).align(egui::RectAlign::BOTTOM_END).show(|ui| super::toolbar::zoom_items(ui, ed));
}

/// Avatars of the people on the board (click to follow someone's view).
pub fn participants(ui: &mut Ui, ed: &mut Editor) {
    let me = serde_json::Value::Object(ed.presence.local.clone());
    let mut list: Vec<(Option<u64>, String, Color32)> = vec![(None, format!("{} (tu)", peer_name(&me)), peer_color(&me))];
    list.extend(ed.presence.peers.iter().filter(|(_, s)| s["user"].is_object()).map(|(id, s)| (Some(*id), peer_name(s), peer_color(s))));
    ui.spacing_mut().item_spacing.x = 2.0;
    for (id, name, color) in list.iter().take(4) {
        let following = id.is_some() && ed.following == *id;
        let resp = ui::avatar(ui, name.trim_end_matches(" (tu)"), *color, 24.0);
        if following {
            ui.painter().circle_stroke(resp.rect.center(), 13.5, Stroke::new(2.0, *color));
        }
        let tip = match id {
            None => name.clone(),
            Some(_) if following => format!("Smetti di seguire {name}"),
            Some(_) => format!("Segui {name}"),
        };
        if ui::tip(resp, &tip, None).clicked()
            && let Some(id) = id
        {
            ed.follow(if following { None } else { Some(*id) });
        }
    }
    if list.len() > 4 {
        ui.label(RichText::new(format!("+{}", list.len() - 4)).size(10.0));
    }
}

fn share_button(ui: &mut Ui, b: &mut BoardScreen) {
    if !b.host {
        let t = ui::theme(ui.ctx());
        let text = if b.editor.read_only { "Solo visione" } else { "Puoi modificare" };
        ui.label(RichText::new(text).size(10.0).color(t.text2));
        return;
    }
    let sharing = b.sharing();
    if ui::button(ui, if sharing { "Condivisa" } else { "Condividi" }, if sharing { Btn::Secondary } else { Btn::Primary }, Some("share-2"), false, true).clicked() {
        b.ui.share = true;
    }
}

fn section(ui: &mut Ui, title: Option<&str>, body: impl FnOnce(&mut Ui)) {
    let t = ui::theme(ui.ctx());
    egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 14 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        if let Some(title) = title {
            ui.label(RichText::new(title).font(ui::medium(11.0)).color(t.text));
        }
        body(ui);
    });
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
}

/* ---------- nothing selected: the board ---------- */

fn board_props(ui: &mut Ui, b: &mut BoardScreen) {
    let mut meta = b.editor.board.meta();
    let before = meta.clone();
    section(ui, Some("Sfondo"), |ui| background_fields(ui, &mut meta));
    if meta != before {
        b.editor.board.set_meta(&meta);
    }
    section(ui, Some("Esporta"), |ui| {
        if ui::button(ui, "Esporta lavagna…", Btn::Secondary, Some("download"), false, true).clicked() {
            b.ui.export = Some(false);
        }
    });
    section(ui, None, |ui| ui::hint(ui, "Seleziona qualcosa sulla lavagna per modificarne colore, dimensioni e posizione."));
}

fn pattern_label(p: Pattern) -> &'static str {
    match p {
        Pattern::None => "Nessuno",
        Pattern::Dots => "Puntini",
        Pattern::Grid => "Griglia",
        Pattern::Lines => "Righe",
        Pattern::Graph => "Millimetrata",
        Pattern::Isometric => "Isometrica",
    }
}

/// Board colour, pattern and spacing. Also the default of new boards in Settings.
pub fn background_fields(ui: &mut Ui, meta: &mut BoardMeta) {
    let t = ui::theme(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (name, value) in BACKGROUNDS {
            let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            ui.painter().circle_filled(r.center(), 11.0, parse_color(value).unwrap());
            ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(1.0, Color32::from_black_alpha(40)));
            if meta.background.eq_ignore_ascii_case(value) {
                ui.painter().circle_stroke(r.center(), 12.5, Stroke::new(2.0, t.brand));
            }
            if ui::tip(resp, name, None).clicked() {
                meta.background = value.into();
            }
        }
        let mut c = parse_color(&meta.background).unwrap_or(Color32::WHITE);
        let before = c;
        let resp = ui.color_edit_button_srgba(&mut c);
        ui::tip(resp, "Colore personalizzato", None);
        if c != before {
            meta.background = hex(c);
        }
    });
    egui::Grid::new(ui.id().with("patterns")).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, p) in PATTERNS.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(62.0, 50.0), Sense::click());
            let art = Rect::from_min_size(r.min, vec2(62.0, 34.0));
            let mut shapes = Vec::new();
            let m = BoardMeta { pattern: *p, grid_size: 14.0, ..meta.clone() };
            crate::paint::background(&mut shapes, art, &crate::geom::Camera { x: 7.0, y: 9.0, z: 1.0 }, &m, ui.ctx().pixels_per_point());
            ui.painter().with_clip_rect(art).extend(shapes);
            let on = meta.pattern == *p;
            ui.painter().rect_stroke(art, ui::RADIUS, Stroke::new(if on { 2.0 } else { 1.0 }, if on { t.brand } else { t.border }), egui::StrokeKind::Inside);
            ui.painter().text(pos2(r.center().x, r.max.y - 7.0), egui::Align2::CENTER_CENTER, pattern_label(*p), egui::FontId::proportional(10.0), t.text2);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, pattern_label(*p)));
            if resp.clicked() {
                meta.pattern = *p;
            }
            if i % 3 == 2 {
                ui.end_row();
            }
        }
    });
    if meta.pattern != Pattern::None {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Passo").color(t.text2));
            let mut g = meta.grid_size as i64;
            if ui::segmented(ui, &mut g, &[(12, "Fitto"), (24, "Medio"), (48, "Ampio")], 160.0) {
                meta.grid_size = g as f64;
            }
        });
    }
}

/* ---------- selection ---------- */

/// The value shared by every element, or None when they differ.
fn same<T: PartialEq>(els: &[Arc<El>], get: impl Fn(&El) -> Option<T>) -> Option<T> {
    let mut vals = els.iter().filter_map(|e| get(e));
    let first = vals.next()?;
    vals.all(|v| v == first).then_some(first)
}

fn selection_props(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    let els = ed.selected();
    let single = (els.len() == 1).then(|| els[0].clone());
    let ids: Vec<String> = els.iter().filter(|e| !e.locked).map(|e| e.id.clone()).collect();
    let types: std::collections::HashSet<&str> = els.iter().map(|e| e.type_name()).collect();
    let only = |k: &str| types.len() == 1 && types.contains(k);
    let apply = |ed: &mut Editor, f: &mut dyn FnMut(&mut El)| {
        ed.board.update(&ids, |el| f(el));
    };
    section(ui, None, |ui| {
        ui.horizontal(|ui| match &single {
            Some(e) => {
                let label = element_label(e);
                ui.label(RichText::new(&label).font(ui::medium(11.0)).color(t.text));
                if label != type_label(e) {
                    ui.label(RichText::new(type_label(e)).color(t.text2));
                }
            }
            None => {
                ui.label(RichText::new(format!("{} elementi", els.len())).font(ui::medium(11.0)).color(t.text));
            }
        });
    });
    if els.len() > 1 {
        section(ui, Some("Allinea"), |ui| {
            ui.horizontal(|ui| {
                for (k, name, label) in [
                    (AlignKind::Left, "align-start-vertical", "Allinea a sinistra"),
                    (AlignKind::HCenter, "align-center-vertical", "Centra orizzontalmente"),
                    (AlignKind::Right, "align-end-vertical", "Allinea a destra"),
                    (AlignKind::Top, "align-start-horizontal", "Allinea in alto"),
                    (AlignKind::VCenter, "align-center-horizontal", "Centra verticalmente"),
                    (AlignKind::Bottom, "align-end-horizontal", "Allinea in basso"),
                ] {
                    if small_icon_button(ui, name, label, false).clicked() {
                        ed.align(k);
                    }
                }
            });
            if els.len() > 2 {
                ui.horizontal(|ui| {
                    if small_icon_button(ui, "align-horizontal-distribute-center", "Distribuisci in orizzontale", false).clicked() {
                        ed.distribute(false);
                    }
                    if small_icon_button(ui, "align-vertical-distribute-center", "Distribuisci in verticale", false).clicked() {
                        ed.distribute(true);
                    }
                });
            }
        });
    }
    // Position and size.
    let frame = union(els.iter().map(|e| frame_box(e))).unwrap_or_default();
    section(ui, Some("Posizione e dimensioni"), |ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        let one = single.as_ref().filter(|e| !e.is_line()).cloned();
        let (x, y) = one.as_ref().map_or((frame.x, frame.y), |e| (e.x, e.y));
        let (bw, bh) = one.as_ref().map_or((frame.w, frame.h), |e| (e.w, e.h));
        let mut set: Option<(Option<f64>, Option<f64>, Option<f64>, Option<f64>)> = None;
        ui.horizontal(|ui| {
            if let Some(v) = ui::number_field(ui, Id::new("px"), "X", "Posizione orizzontale", Some(x), 0, w) {
                set = Some((Some(v), None, None, None));
            }
            if let Some(v) = ui::number_field(ui, Id::new("py"), "Y", "Posizione verticale", Some(y), 0, w) {
                set = Some((None, Some(v), None, None));
            }
        });
        ui.horizontal(|ui| {
            if let Some(v) = ui::number_field(ui, Id::new("pw"), "L", "Larghezza", Some(bw), 0, w) {
                set = Some((None, None, Some(v.max(1.0)), None));
            }
            if let Some(v) = ui::number_field(ui, Id::new("ph"), "A", "Altezza", Some(bh), 0, w) {
                set = Some((None, None, None, Some(v.max(1.0))));
            }
        });
        if let Some(e) = &one {
            let deg = (e.rotation.to_degrees() % 360.0 + 360.0) % 360.0;
            if let Some(v) = ui::number_field(ui, Id::new("prot"), "°", "Rotazione in gradi", Some(deg), 0, w) {
                apply(ed, &mut |el| el.rotation = v.to_radians());
            }
        }
        if let Some((nx, ny, nw, nh)) = set {
            match &one {
                Some(e) if nx.is_some() || ny.is_some() => {
                    let (tx, ty) = (nx.unwrap_or(e.x), ny.unwrap_or(e.y));
                    apply(ed, &mut |el| (el.x, el.y) = (tx, ty));
                }
                Some(e) => {
                    let from = BBox { x: e.x, y: e.y, w: e.w, h: e.h };
                    let to = BBox { w: nw.unwrap_or(e.w), h: nh.unwrap_or(e.h), ..from };
                    let mut next = scale_element(&El { rotation: 0.0, ..(**e).clone() }, from, to);
                    // Keep the top-left corner where it was.
                    (next.x, next.y, next.rotation) = (e.x, e.y, e.rotation);
                    crate::text::fit_text(&mut next);
                    ed.board.put([next]);
                }
                None => {
                    if nx.is_some() || ny.is_some() {
                        let (dx, dy) = (nx.map_or(0.0, |v| v - frame.x), ny.map_or(0.0, |v| v - frame.y));
                        apply(ed, &mut |el| (el.x, el.y) = (el.x + dx, el.y + dy));
                    } else {
                        let to = BBox { w: nw.unwrap_or(frame.w), h: nh.unwrap_or(frame.h), ..frame };
                        let next: Vec<El> = els.iter().filter(|e| !e.locked).map(|e| scale_element(e, frame, to)).collect();
                        ed.board.put(next);
                    }
                }
            }
        }
    });
    if only("section") {
        section(ui, Some("Sezione"), |ui| {
            if let Some(e) = &single {
                let mut name = e.name.clone().unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut name).hint_text("Sezione").desired_width(f32::INFINITY)).changed() {
                    let n: String = name.chars().take(80).collect();
                    apply(ed, &mut |el| el.name = Some(n.clone()));
                }
            }
            let cur = same(&els, |e| if let Kind::Section { fill } = &e.kind { Some(fill.clone()) } else { None });
            if let Some(c) = ui::swatches(ui, &SECTION_COLORS, cur.as_deref().unwrap_or(""), false) {
                apply(ed, &mut |el| {
                    if let Kind::Section { fill } = &mut el.kind {
                        *fill = c.clone();
                    }
                });
            }
            ui::hint(ui, "Quello che metti dentro la sezione si sposta, si copia e si duplica insieme a lei.");
        });
    }
    if only("text") || only("sticky") {
        text_props(ui, ed, &els, &ids);
    }
    if only("shape") {
        section(ui, Some("Testo nella forma"), |ui| {
            let mut f = same(&els, |e| e.shape().map(|s| s.font.unwrap_or_default())).unwrap_or_default();
            if super::toolbar::font_picker(ui, &mut f, ui.available_width(), t) {
                apply(ed, &mut |el| {
                    if let Kind::Shape(s) = &mut el.kind {
                        s.font = Some(f);
                    }
                });
            }
            ui::hint(ui, "Doppio clic sulla forma, o Invio, per scriverci dentro.");
        });
    }
    if only("sticky") {
        if els.iter().any(|e| e.sticky().is_some_and(|s| s.author.is_some())) {
            section(ui, None, |ui| {
                let mut on = els.iter().all(|e| e.sticky().is_none_or(|s| !s.hide_author));
                if ui::switch(ui, &mut on, "Mostra chi l'ha scritta") {
                    apply(ed, &mut |el| {
                        if let Kind::Sticky(s) = &mut el.kind {
                            s.hide_author = !on;
                        }
                    });
                }
            });
        }
        section(ui, Some("Colore della nota"), |ui| {
            let cur = same(&els, |e| e.sticky().map(|s| s.color.clone()));
            if let Some(c) = ui::swatches(ui, &STICKY_COLORS, cur.as_deref().unwrap_or(""), false) {
                apply(ed, &mut |el| {
                    if let Kind::Sticky(s) = &mut el.kind {
                        s.color = c.clone();
                    }
                });
            }
        });
    }
    if only("shape") {
        section(ui, Some("Riempimento"), |ui| {
            let cur = same(&els, |e| e.shape().map(|s| s.fill.clone()));
            if let Some(c) = ui::swatches(ui, &INK_COLORS, cur.as_deref().unwrap_or(""), true) {
                apply(ed, &mut |el| {
                    if let Kind::Shape(s) = &mut el.kind {
                        s.fill = c.clone();
                    }
                });
            }
        });
    }
    let colorable = !types.is_empty() && types.iter().all(|k| matches!(*k, "ink" | "highlighter" | "text" | "shape" | "line"));
    if colorable {
        let title = if only("shape") || only("line") { "Contorno" } else { "Colore" };
        section(ui, Some(title), |ui| {
            let main = same(&els, |e| match &e.kind {
                Kind::Ink(i) | Kind::Highlighter(i) => Some(i.color.clone()),
                Kind::Text(x) => Some(x.color.clone()),
                Kind::Shape(s) => Some(s.stroke.clone()),
                Kind::Line(l) => Some(l.stroke.clone()),
                _ => None,
            });
            if let Some(c) = ui::swatches(ui, &INK_COLORS, main.as_deref().unwrap_or(""), false) {
                apply(ed, &mut |el| match &mut el.kind {
                    Kind::Ink(i) | Kind::Highlighter(i) => i.color = c.clone(),
                    Kind::Text(x) => x.color = c.clone(),
                    Kind::Shape(s) => s.stroke = c.clone(),
                    Kind::Line(l) => l.stroke = c.clone(),
                    _ => {}
                });
            }
            let widths: Vec<Option<f64>> = els
                .iter()
                .map(|e| match &e.kind {
                    Kind::Ink(i) | Kind::Highlighter(i) => Some(i.size),
                    Kind::Shape(s) => Some(s.stroke_width),
                    Kind::Line(l) => Some(l.stroke_width),
                    _ => None,
                })
                .collect();
            if widths.iter().all(Option::is_some) {
                let v = widths.windows(2).all(|w| w[0] == w[1]).then(|| widths[0].unwrap());
                if let Some(w) = ui::number_field(ui, Id::new("pstroke"), "Sp", "Spessore", v, 1, ui.available_width() / 2.0) {
                    let w = w.clamp(0.1, 200.0);
                    apply(ed, &mut |el| match &mut el.kind {
                        Kind::Ink(i) | Kind::Highlighter(i) => i.size = w,
                        Kind::Shape(s) => s.stroke_width = w,
                        Kind::Line(l) => l.stroke_width = w,
                        _ => {}
                    });
                }
            }
            if types.iter().all(|k| *k == "shape" || *k == "line") {
                let mut dash = same(&els, |e| match &e.kind {
                    Kind::Shape(s) => Some(s.dash),
                    Kind::Line(l) => Some(l.dash),
                    _ => None,
                });
                let before = dash;
                ui::segmented(ui, &mut dash, &[(Some(false), "Continuo"), (Some(true), "Tratteggiato")], ui.available_width());
                if dash != before
                    && let Some(d) = dash
                {
                    apply(ed, &mut |el| match &mut el.kind {
                        Kind::Shape(s) => s.dash = d,
                        Kind::Line(l) => l.dash = d,
                        _ => {}
                    });
                }
            }
        });
    }
    if only("shape") {
        section(ui, Some("Forma"), |ui| {
            let cur = same(&els, |e| e.shape().map(|s| s.shape));
            egui::Grid::new("insp-shapes").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
                let mut i = 0;
                for (k, label, _) in super::toolbar::SHAPES.iter() {
                    let prefs::ShapeTool::Kind(kind) = k else { continue };
                    let r = ui::icon_button(ui, "", label, None, vec2(28.0, 28.0), 16.0, cur == Some(*kind), false);
                    super::toolbar::shape_icon(ui, *k, r.rect.center(), 16.0, t.icon);
                    if r.clicked() {
                        let kind = *kind;
                        apply(ed, &mut |el| {
                            if let Kind::Shape(s) = &mut el.kind {
                                s.shape = kind;
                            }
                        });
                    }
                    i += 1;
                    if i % 7 == 0 {
                        ui.end_row();
                    }
                }
            });
            if cur == Some(ShapeKind::Rect) {
                let r = same(&els, |e| e.shape().map(|s| s.radius));
                if let Some(v) = ui::number_field(ui, Id::new("pradius"), "◜", "Raggio degli angoli", r, 0, ui.available_width() / 2.0) {
                    apply(ed, &mut |el| {
                        if let Kind::Shape(s) = &mut el.kind {
                            s.radius = v.max(0.0);
                        }
                    });
                }
            }
        });
    }
    if only("comment") {
        section(ui, Some("Commento"), |ui| {
            if ui::button(ui, "Apri la discussione", Btn::Secondary, None, false, true).clicked() {
                ed.comment = Some(els[0].id.clone());
            }
        });
    }
    if only("line") && els.iter().all(|e| e.line().is_some_and(|l| !l.tape)) {
        section(ui, Some("Frecce"), |ui| {
            ui.horizontal(|ui| {
                let start = els.iter().all(|e| e.line().is_some_and(|l| l.arrow_start));
                let end = els.iter().all(|e| e.line().is_some_and(|l| l.arrow_end));
                if small_icon_button(ui, "move-left", "Freccia all'inizio", start).clicked() {
                    apply(ed, &mut |el| {
                        if let Kind::Line(l) = &mut el.kind {
                            l.arrow_start = !start;
                        }
                    });
                }
                if small_icon_button(ui, "move-right", "Freccia alla fine", end).clicked() {
                    apply(ed, &mut |el| {
                        if let Kind::Line(l) = &mut el.kind {
                            l.arrow_end = !end;
                        }
                    });
                }
            });
        });
    }
    if only("stamp") {
        section(ui, Some("Reazione"), |ui| {
            let cur = same(&els, |e| if let Kind::Stamp { emoji } = &e.kind { Some(emoji.clone()) } else { None });
            if let Some(e) = super::toolbar::stamp_picker(ui, cur.as_deref()) {
                apply(ed, &mut |el| {
                    if let Kind::Stamp { emoji } = &mut el.kind {
                        *emoji = e.clone();
                    }
                });
            }
        });
    }
    if els.iter().any(|e| !e.erase.is_empty()) {
        section(ui, None, |ui| {
            if ui::button(ui, "Ripristina le parti cancellate", Btn::Secondary, Some("eraser"), false, true).clicked() {
                apply(ed, &mut |el| el.erase.clear());
            }
        });
    }
    section(ui, Some("Livello"), |ui| {
        ui.horizontal(|ui| {
            let op = same(&els, |e| Some((e.opacity * 100.0).round()));
            if let Some(v) = ui::number_field(ui, Id::new("popacity"), "%", "Opacità", op, 0, 64.0) {
                let v = (v / 100.0).clamp(0.0, 1.0);
                apply(ed, &mut |el| el.opacity = v);
            }
            let locked = els.iter().all(|e| e.locked);
            if small_icon_button(ui, if locked { "lock" } else { "unlock" }, if locked { "Sblocca" } else { "Blocca" }, locked).clicked() {
                ed.toggle_lock();
            }
            let hidden = els.iter().all(|e| e.hidden);
            if small_icon_button(ui, if hidden { "eye-off" } else { "eye" }, "Nascondi", hidden).clicked() {
                ed.toggle_hide();
            }
            if small_icon_button(ui, "arrow-up-to-line", "Porta in primo piano", false).clicked() {
                ed.order(Order::Front);
            }
            if small_icon_button(ui, "arrow-down-to-line", "Porta in fondo", false).clicked() {
                ed.order(Order::Back);
            }
        });
    });
    section(ui, None, |ui| {
        ui.horizontal(|ui| {
            if ui::button(ui, "Duplica", Btn::Secondary, Some("copy-plus"), false, true).clicked() {
                ed.duplicate();
            }
            if ui::button(ui, "Elimina", Btn::DangerText, Some("trash-2"), false, true).clicked() {
                ed.remove();
            }
        });
    });
}

fn text_props(ui: &mut Ui, ed: &mut Editor, els: &[Arc<El>], ids: &[String]) {
    let t = ui::theme(ui.ctx());
    let is_text = els.iter().all(|e| e.text().is_some());
    section(ui, Some("Testo"), |ui| {
        let mut f = same(els, |e| e.text().map(|x| x.font).or_else(|| e.sticky().map(|s| s.font))).unwrap_or_default();
        if super::toolbar::font_picker(ui, &mut f, ui.available_width(), &t) {
            ed.board.update(ids, |el| {
                match &mut el.kind {
                    Kind::Text(x) => x.font = f,
                    Kind::Sticky(s) => s.font = f,
                    _ => {}
                }
                crate::text::fit_text(el);
            });
        }
        if is_text {
            ui.horizontal(|ui| {
                let size = same(els, |e| e.text().map(|x| (x.font_size * 10.0).round() / 10.0));
                if let Some(v) = ui::number_field(ui, Id::new("pfont"), "Aa", "Dimensione del testo", size, 1, 90.0) {
                    let v = v.clamp(0.1, 2000.0);
                    ed.board.update(ids, |el| {
                        if let Kind::Text(x) = &mut el.kind {
                            x.font_size = v;
                        }
                        crate::text::fit_text(el);
                    });
                }
                let bold = els.iter().all(|e| e.text().is_some_and(|x| x.bold));
                let italic = els.iter().all(|e| e.text().is_some_and(|x| x.italic));
                if small_icon_button(ui, "bold", "Grassetto", bold).clicked() {
                    ed.board.update(ids, |el| {
                        if let Kind::Text(x) = &mut el.kind {
                            x.bold = !bold;
                        }
                        crate::text::fit_text(el);
                    });
                }
                if small_icon_button(ui, "italic", "Corsivo", italic).clicked() {
                    ed.board.update(ids, |el| {
                        if let Kind::Text(x) = &mut el.kind {
                            x.italic = !italic;
                        }
                        crate::text::fit_text(el);
                    });
                }
            });
        }
        let mut align = same(els, |e| e.text().map(|x| x.align).or_else(|| e.sticky().map(|s| s.align))).unwrap_or_default();
        if ui::segmented(ui, &mut align, &[(Align::Left, "icon:text-align-start"), (Align::Center, "icon:text-align-center"), (Align::Right, "icon:text-align-end")], ui.available_width()) {
            ed.board.update(ids, |el| match &mut el.kind {
                Kind::Text(x) => x.align = align,
                Kind::Sticky(s) => s.align = align,
                _ => {}
            });
        }
    });
}

/* ---------------- pills ---------------- */

/// Left panel closed (or focus mode): this pill stays at the top left to bring it back.
pub fn focus_pill(ctx: &egui::Context, stage: Rect, b: &mut BoardScreen, dialogs: &mut Dialogs) -> Option<Action> {
    let t = ui::theme(ctx);
    let mut action = None;
    egui::Area::new(Id::new("focus-pill")).fixed_pos(stage.min + vec2(12.0, 12.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                action = app_menu(ui, b, dialogs);
                ui.label(RichText::new(&b.title).font(ui::medium(11.0)).color(t.text));
                let focus = b.editor.prefs.focus;
                if ui::icon_button(ui, "panel-left-open", if focus { "Mostra i pannelli" } else { "Mostra il pannello dei livelli" }, focus.then_some("Ctrl+\\"), vec2(24.0, 24.0), 16.0, false, false).clicked() {
                    if focus {
                        b.editor.toggle_focus();
                    } else {
                        b.editor.prefs.left_panel = true;
                    }
                }
            });
        });
    });
    action
}

/// Right panel closed: people, share and the way back, floating at the top right.
pub fn right_pill(ctx: &egui::Context, stage: Rect, b: &mut BoardScreen) {
    let t = ui::theme(ctx);
    let x = if b.editor.prefs.minimap { stage.max.x - 12.0 - 200.0 - 8.0 } else { stage.max.x - 12.0 };
    egui::Area::new(Id::new("right-pill")).pivot(egui::Align2::RIGHT_TOP).fixed_pos(pos2(x, stage.min.y + 12.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                participants(ui, &mut b.editor);
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.x = 2.0;
                super::live::timer_button(ui, &mut b.editor);
                super::live::vote_button(ui, &mut b.editor);
                super::live::spotlight_button(ui, &mut b.editor);
                share_button(ui, b);
                if small_icon_button(ui, "panel-right-open", "Mostra il pannello", false).clicked() {
                    b.editor.prefs.right_panel = true;
                    b.editor.prefs.focus = false;
                }
            });
        });
    });
}

/* ---------------- context menu ---------------- */

/// Right-click menu of the board and of the layers panel; returns true once something was chosen.
pub fn selection_menu(ui: &mut Ui, ed: &mut Editor, st: &mut BoardUi) -> bool {
    let mut done = false;
    let mut item = |ui: &mut Ui, icon: Option<&str>, label: &str, kbd: Option<&str>, danger: bool| {
        let c = ui::menu_item(ui, icon, label, kbd, danger).clicked();
        done |= c;
        c
    };
    if ed.read_only {
        if item(ui, Some("maximize"), "Adatta alla lavagna", Some("Maiusc+1"), false) {
            ed.fit();
        }
        return done;
    }
    if ed.selection.is_empty() {
        if item(ui, Some("clipboard-paste"), "Incolla", Some("Ctrl+V"), false) {
            ed.paste();
        }
        if item(ui, Some("mouse-pointer-square-dashed"), "Seleziona tutto", Some("Ctrl+A"), false) {
            ed.select_all();
        }
        ui::menu_sep(ui);
        if item(ui, Some("maximize"), "Adatta alla lavagna", Some("Maiusc+1"), false) {
            ed.fit();
        }
        let r = ed.ruler.visible;
        if item(ui, Some("ruler"), if r { "Nascondi righello" } else { "Mostra righello" }, Some("U"), false) {
            ed.toggle_ruler();
        }
        if item(ui, Some("unlock"), "Sblocca e mostra tutto", None, false) {
            ed.unlock_all();
        }
        return done;
    }
    if item(ui, Some("copy"), "Copia", Some("Ctrl+C"), false)
        && let Some(text) = ed.copy()
    {
        ui.ctx().copy_text(text);
    }
    if item(ui, Some("scissors"), "Taglia", Some("Ctrl+X"), false)
        && let Some(text) = ed.cut()
    {
        ui.ctx().copy_text(text);
    }
    if item(ui, Some("clipboard-paste"), "Incolla", Some("Ctrl+V"), false) {
        ed.paste();
    }
    if item(ui, Some("copy-plus"), "Duplica", Some("Ctrl+D"), false) {
        ed.duplicate();
    }
    ui::menu_sep(ui);
    for (o, icon, label, kbd) in [
        (Order::Front, Some("arrow-up-to-line"), "Porta in primo piano", "Ctrl+Maiusc+]"),
        (Order::Forward, None, "Porta avanti", "Ctrl+]"),
        (Order::Backward, None, "Porta indietro", "Ctrl+["),
        (Order::Back, Some("arrow-down-to-line"), "Porta in fondo", "Ctrl+Maiusc+["),
    ] {
        if item(ui, icon, label, Some(kbd), false) {
            ed.order(o);
        }
    }
    ui::menu_sep(ui);
    // Folders.
    let els = ed.selected();
    let mut in_folder: Vec<String> = els.iter().filter_map(|e| e.group_id.clone()).collect();
    in_folder.sort();
    in_folder.dedup();
    if item(ui, Some("folder-plus"), "Metti in una nuova cartella", Some("Ctrl+G"), false) {
        ed.group();
    }
    let others: Vec<(String, String)> = ed.board.group_list().into_iter().filter(|(g, _)| !(in_folder.len() == 1 && in_folder[0] == *g && els.iter().all(|e| e.group_id.as_deref() == Some(g)))).collect();
    for (g, name) in others.iter().take(8) {
        if item(ui, Some("folder-input"), &format!("Sposta in «{name}»"), None, false) {
            ed.board.stop_capturing();
            let sel = ed.selection.clone();
            ed.board.set_group(&sel, Some(g));
        }
    }
    if in_folder.len() == 1 {
        let whole = ed.board.members(&in_folder[0]);
        if whole.len() > els.len() && item(ui, Some("mouse-pointer-square-dashed"), "Seleziona tutta la cartella", None, false) {
            ed.selection = whole.iter().map(|e| e.id.clone()).collect();
        }
    }
    if !in_folder.is_empty() && item(ui, Some("folder-output"), "Togli dalla cartella", Some("Ctrl+Maiusc+G"), false) {
        ed.ungroup();
    }
    ui::menu_sep(ui);
    if item(ui, Some("lock"), "Blocca", Some("Ctrl+Maiusc+L"), false) {
        ed.toggle_lock();
    }
    if item(ui, Some("eye-off"), "Nascondi", Some("Ctrl+Maiusc+H"), false) {
        ed.toggle_hide();
    }
    if item(ui, Some("maximize"), "Zoom sulla selezione", Some("Maiusc+2"), false) {
        ed.fit_selection();
    }
    if item(ui, None, "Esporta selezione…", None, false) {
        st.export = Some(true);
    }
    ui::menu_sep(ui);
    if item(ui, Some("trash-2"), "Elimina", Some("Canc"), true) {
        ed.remove();
    }
    done
}

/* ---------------- text bar ---------------- */

/// Bar over the text being typed: font, plus bold and italic for text boxes.
pub fn text_tools(ctx: &egui::Context, stage: Rect, ed: &mut Editor, el: &El) {
    let t = ui::theme(ctx);
    let b = frame_box(el);
    let a = ed.to_screen(b.x, b.y) + stage.min.to_vec2();
    let below = ed.to_screen(b.x, b.bottom()) + stage.min.to_vec2();
    let width = if el.text().is_some() { 232.0 } else { 176.0 };
    let y = if a.y - 40.0 >= stage.min.y + 8.0 { a.y - 40.0 } else { below.y + 8.0 };
    let x = a.x.clamp(stage.min.x + 8.0, (stage.max.x - width - 8.0).max(stage.min.x + 8.0));
    let id = el.id.clone();
    egui::Area::new(Id::new("text-tools")).fixed_pos(pos2(x, y)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut f = match &el.kind {
                    Kind::Text(x) => x.font,
                    Kind::Sticky(s) => s.font,
                    Kind::Shape(s) => s.font.unwrap_or_default(),
                    _ => FontKind::Sans,
                };
                let mut refocus = false;
                if super::toolbar::font_picker(ui, &mut f, 160.0, &t) {
                    ed.board.update(std::slice::from_ref(&id), |el| {
                        match &mut el.kind {
                            Kind::Text(x) => x.font = f,
                            Kind::Sticky(s) => s.font = f,
                            Kind::Shape(s) => s.font = Some(f),
                            _ => {}
                        }
                        crate::text::fit_text(el);
                    });
                    // The next text box starts with the same font.
                    if el.text().is_some() {
                        ed.prefs.text.font = f;
                    }
                    refocus = true;
                }
                if let Some(x) = el.text() {
                    let (bold, italic) = (x.bold, x.italic);
                    if small_icon_button(ui, "bold", "Grassetto", bold).clicked() {
                        ed.board.update(std::slice::from_ref(&id), |el| {
                            if let Kind::Text(x) = &mut el.kind {
                                x.bold = !bold;
                            }
                            crate::text::fit_text(el);
                        });
                        refocus = true;
                    }
                    if small_icon_button(ui, "italic", "Corsivo", italic).clicked() {
                        ed.board.update(std::slice::from_ref(&id), |el| {
                            if let Kind::Text(x) = &mut el.kind {
                                x.italic = !italic;
                            }
                            crate::text::fit_text(el);
                        });
                        refocus = true;
                    }
                }
                if refocus {
                    ui.memory_mut(|m| m.request_focus(Id::new("board-text")));
                }
            });
        });
    });
}

/// Small picture of a template, drawn once with the board renderer.
pub fn template_art(ui: &Ui, tpl: Option<&Template>, rect: Rect) {
    let t = ui::theme(ui.ctx());
    let Some(tpl) = tpl else {
        ui.painter().rect_filled(rect, ui::RADIUS, t.bg2);
        icon(ui, "plus", rect.center(), 20.0, t.text3);
        return;
    };
    let id = Id::new(("tpl-art", tpl.id));
    let tex: Option<egui::TextureHandle> = ui.data(|d| d.get_temp(id)).or_else(|| {
        let els: Vec<std::sync::Arc<crate::model::El>> = (tpl.build)().into_iter().map(std::sync::Arc::new).collect();
        let b = crate::geom::union(els.iter().map(|e| crate::geom::aabb(e)))?;
        let (w, h) = (320.0, 200.0);
        let k = ((w - 24.0) / b.w).min((h - 24.0) / b.h);
        let view = crate::geom::BBox { x: b.x + b.w / 2.0 - w / 2.0 / k, y: b.y + b.h / 2.0 - h / 2.0 / k, w: w / k, h: h / k };
        let pm = crate::raster::render(&els, Some(Color32::from_gray(0xF5)), w as u32, h as u32, view, &crate::prims::Env::default(), &|_| None)?;
        let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
        let tex = ui.ctx().load_texture(format!("tpl:{}", tpl.id), img, egui::TextureOptions::LINEAR);
        ui.data_mut(|d| d.insert_temp(id, tex.clone()));
        Some(tex)
    });
    if let Some(tex) = tex {
        ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    ui.painter().rect_stroke(rect, ui::RADIUS, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
}
