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
        Kind::Table(_) => "Tabella",
        Kind::Code(_) => "Blocco di codice",
        Kind::Widget(ref w) => crate::widgets::kind_name(w),
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
        ShapeKind::Pill => "Inizio e fine",
        ShapeKind::Cylinder => "Database",
        ShapeKind::Document => "Documento",
        ShapeKind::Speech => "Fumetto",
        ShapeKind::Chevron => "Gallone",
        ShapeKind::Trapezoid => "Trapezio",
        ShapeKind::Process => "Processo predefinito",
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
        Kind::Code(c) => or(first_line(&c.code), "Blocco di codice"),
        Kind::Widget(crate::model::Widget::Poll { question: t, .. } | crate::model::Widget::Checklist { title: t, .. } | crate::model::Widget::Counter { label: t, .. }) => or(first_line(t), type_label(el)),
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
        Kind::Table(_) => "table",
        Kind::Code(_) => "code",
        Kind::Widget(crate::model::Widget::Poll { .. }) => "chart-column",
        Kind::Widget(crate::model::Widget::Checklist { .. }) => "list-checks",
        Kind::Widget(crate::model::Widget::Counter { .. }) => "hash",
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
            let options: &[(LeftTab, &str)] = if b.editor.read_only { &[(LeftTab::Layers, "Livelli")] } else { &[(LeftTab::Layers, "Livelli"), (LeftTab::Templates, "Modelli")] };
            tab_strip(ui, &mut b.ui.left_tab, options);
        });
    });
    if b.ui.left_tab == LeftTab::Templates && !b.editor.read_only {
        templates(ui, &mut b.editor, &t);
    } else {
        layers(ui, &mut b.editor, &mut b.ui, &t);
    }
    action
}

/// Text tabs; a soft pill glides under the chosen one.
fn tab_strip<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    let t = ui::theme(ui.ctx());
    let rects: Vec<Rect> = options
        .iter()
        .map(|(_, label)| {
            let w = ui.painter().layout_no_wrap(label.to_string(), ui::medium(11.0), t.text).size().x;
            ui.allocate_exact_size(vec2(w + 18.0, 24.0), Sense::hover()).0
        })
        .collect();
    let Some(first) = rects.first().copied() else { return };
    let at = options.iter().position(|o| o.0 == *value).unwrap_or(0);
    let id = ui.id().with("tabs-pill");
    let x = ui::motion::spring(ui.ctx(), id.with("x"), rects[at].min.x - first.min.x, 0.3, 0.88);
    let w = ui::motion::spring(ui.ctx(), id.with("w"), rects[at].width(), 0.3, 0.88);
    ui.painter().rect_filled(Rect::from_min_size(pos2(first.min.x + x, first.min.y), vec2(w, 24.0)), ui::RADIUS, t.bg2);
    for (i, ((v, label), r)) in options.iter().zip(&rects).enumerate() {
        let resp = ui.interact(*r, ui.id().with(("tab", i)), Sense::click());
        let on = i == at;
        let k = ui::motion::hover(ui.ctx(), resp.id.with("h"), on || resp.hovered());
        let font = if on { ui::medium(11.0) } else { egui::FontId::proportional(11.0) };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *label, font, ui::blend(t.text2, t.text, k));
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *label));
        ui::focus_ring(ui, &resp, *r);
        if resp.clicked() {
            *value = *v;
        }
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
    ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
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

/// Something inside an element, listed under it when it is opened: a table's cell, a poll's
/// option or a checklist's item.
#[derive(Clone, Copy, PartialEq)]
enum Piece {
    Cell(usize, usize),
    Item(usize),
}

enum Row {
    /// An element; `bool`: it can be opened (a section with things in it, a table, a widget).
    El(Arc<El>, u8, bool),
    /// `auto`: strokes written one after the other in the same spot, shown together.
    Folder { id: String, name: String, members: Vec<Arc<El>>, auto: bool },
    Piece { el: Arc<El>, piece: Piece, label: String, depth: u8 },
}

fn row_ids(r: &Row) -> Vec<String> {
    match r {
        Row::El(e, ..) | Row::Piece { el: e, .. } => vec![e.id.clone()],
        Row::Folder { members, .. } => members.iter().map(|m| m.id.clone()).collect(),
    }
}

fn row_key(r: &Row) -> String {
    match r {
        Row::El(e, ..) => e.id.clone(),
        Row::Folder { id, .. } => format!("folder:{id}"),
        Row::Piece { el, piece: Piece::Cell(r, c), .. } => format!("{}/{r}/{c}", el.id),
        Row::Piece { el, piece: Piece::Item(i), .. } => format!("{}/{i}", el.id),
    }
}

/// Key of an element's open state in the layers panel.
fn open_key(id: &str) -> String {
    format!("el:{id}")
}

/// What an element shows inside it when opened.
fn pieces(el: &El) -> Vec<(Piece, String)> {
    let or = |s: String, d: String| if s.is_empty() { d } else { s };
    match &el.kind {
        Kind::Table(t) => t
            .cells
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.iter().enumerate().map(move |(c, cell)| (Piece::Cell(r, c), format!("{}{}   {}", col_name(c), r + 1, or(first_line(&cell.text), "—".into())))))
            .collect(),
        Kind::Widget(Widget::Poll { options, .. }) => options.iter().enumerate().map(|(i, o)| (Piece::Item(i), or(first_line(&o.text), format!("Opzione {}", i + 1)))).collect(),
        Kind::Widget(Widget::Checklist { items, .. }) => items.iter().enumerate().map(|(i, it)| (Piece::Item(i), format!("{} {}", if it.done { "✓" } else { "○" }, or(first_line(&it.text), "Cosa da fare".into())))).collect(),
        _ => Vec::new(),
    }
}

/// Strokes farther apart than this (board units) start a new "Scrittura" row.
const RUN_GAP: f64 = 300.0;

fn near(a: &BBox, b: &BBox) -> bool {
    a.x - RUN_GAP <= b.right() && b.x - RUN_GAP <= a.right() && a.y - RUN_GAP <= b.bottom() && b.y - RUN_GAP <= a.bottom()
}

/// The section each element lies in (the smallest that holds it), as in Figma's frames. Elements
/// in folders stay in their folder.
fn section_parents(items: &[Arc<El>]) -> std::collections::HashMap<String, String> {
    let sections: Vec<(&Arc<El>, BBox)> = items.iter().filter(|e| e.is_section()).map(|e| (e, frame_box(e))).collect();
    let mut parent = std::collections::HashMap::new();
    if sections.is_empty() {
        return parent;
    }
    for el in items.iter().filter(|e| e.group_id.is_none() && !e.is_comment()) {
        let f = frame_box(el);
        let area = f.w * f.h;
        let best = sections.iter().filter(|(s, b)| s.id != el.id && b.holds(&f) && b.w * b.h > area).min_by(|a, b| (a.1.w * a.1.h).total_cmp(&(b.1.w * b.1.h)));
        if let Some((s, _)) = best {
            parent.insert(el.id.clone(), s.id.clone());
        }
    }
    parent
}

struct Tree<'a> {
    open: &'a std::collections::HashSet<String>,
    children: std::collections::HashMap<String, Vec<Arc<El>>>,
    folders: std::collections::HashMap<String, Vec<Arc<El>>>,
}

impl Tree<'_> {
    fn element(&self, rows: &mut Vec<Row>, el: &Arc<El>, depth: u8) {
        let kids = self.children.get(&el.id);
        let parts = pieces(el);
        rows.push(Row::El(el.clone(), depth, kids.is_some() || !parts.is_empty()));
        if !self.open.contains(&open_key(&el.id)) {
            return;
        }
        if let Some(kids) = kids {
            self.level(rows, kids, depth + 1);
        }
        rows.extend(parts.into_iter().map(|(piece, label)| Row::Piece { el: el.clone(), piece, label, depth: depth + 1 }));
    }

    fn container(&self, rows: &mut Vec<Row>, id: String, name: String, members: Vec<Arc<El>>, auto: bool) {
        let shown = if self.open.contains(&id) { members.clone() } else { Vec::new() };
        rows.push(Row::Folder { id, name, members, auto });
        for m in shown {
            self.element(rows, &m, 1);
        }
    }

    fn flush_run(&self, rows: &mut Vec<Row>, run: &mut Vec<Arc<El>>, depth: u8) {
        if run.len() == 1 {
            self.element(rows, &run[0], depth);
        } else if run.len() > 1 && depth == 0 {
            // Keyed by the oldest stroke, so the row keeps its state while more strokes come.
            let name = if run.iter().all(|e| matches!(e.kind, Kind::Highlighter(_))) { "Evidenziature" } else { "Scrittura" };
            let id = format!("ink:{}", run[run.len() - 1].id);
            self.container(rows, id, name.into(), std::mem::take(run), true);
        } else {
            for el in run.iter() {
                self.element(rows, el, depth);
            }
        }
        run.clear();
    }

    /// One level of the list, top to bottom; each folder sits where its top element is.
    /// Consecutive nearby strokes outside folders collapse into one row.
    fn level(&self, rows: &mut Vec<Row>, items: &[Arc<El>], depth: u8) {
        let mut run: Vec<Arc<El>> = Vec::new();
        let mut run_box: Option<BBox> = None;
        for el in items {
            if el.group_id.is_none() && el.is_ink() {
                let b = aabb(el);
                if run_box.is_some_and(|r| !near(&r, &b)) {
                    self.flush_run(rows, &mut run, depth);
                    run_box = None;
                }
                run.push(el.clone());
                run_box = Some(run_box.map_or(b, |r| union([r, b]).unwrap()));
                continue;
            }
            self.flush_run(rows, &mut run, depth);
            run_box = None;
            match &el.group_id {
                None => self.element(rows, el, depth),
                Some(g) if self.folders.get(g).is_some_and(|m| m[0].id == el.id) => {
                    let name = String::new();
                    self.container(rows, g.clone(), name, self.folders[g].clone(), false);
                }
                _ => {}
            }
        }
        self.flush_run(rows, &mut run, depth);
    }
}

fn build_rows(ed: &mut Editor, open: &std::collections::HashSet<String>) -> Vec<Row> {
    let items: Vec<Arc<El>> = ed.board.all().iter().rev().cloned().collect();
    let parent = section_parents(&items);
    let mut tree = Tree { open, children: Default::default(), folders: Default::default() };
    for el in &items {
        if let Some(p) = parent.get(&el.id) {
            tree.children.entry(p.clone()).or_default().push(el.clone());
        }
        if let Some(g) = &el.group_id {
            tree.folders.entry(g.clone()).or_default().push(el.clone());
        }
    }
    let top: Vec<Arc<El>> = items.iter().filter(|e| !parent.contains_key(&e.id)).cloned().collect();
    let mut rows = Vec::new();
    tree.level(&mut rows, &top, 0);
    // Folder names come from the board.
    for r in &mut rows {
        if let Row::Folder { id, name, auto: false, .. } = r
            && name.is_empty()
        {
            *name = ed.board.group_name(id);
        }
    }
    rows
}

fn layers(ui: &mut Ui, ed: &mut Editor, st: &mut BoardUi, t: &Theme) {
    // A new selection inside closed sections opens them, so it can be seen in the list.
    let seen = ui.id().with("layers-selection");
    if ui.data(|d| d.get_temp::<Vec<String>>(seen)).as_ref() != Some(&ed.selection) {
        ui.data_mut(|d| d.insert_temp(seen, ed.selection.clone()));
        let items: Vec<Arc<El>> = ed.board.all().to_vec();
        let parent = section_parents(&items);
        for id in &ed.selection {
            let mut at = parent.get(id);
            while let Some(p) = at {
                st.open_folders.insert(open_key(p));
                at = parent.get(p);
            }
        }
    }
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
                Row::El(e, ..) | Row::Piece { el: e, .. } => vec![e.clone()],
                Row::Folder { members, .. } => members.clone(),
            };
            let piece = match row {
                Row::Piece { piece, .. } => Some(*piece),
                _ => None,
            };
            let locked = els.iter().all(|e| e.locked);
            let hidden = els.iter().all(|e| e.hidden);
            let is_sel = match (row, piece) {
                // A cell is chosen while it is being written in.
                (Row::Piece { el, .. }, Some(Piece::Cell(r, c))) => ed.editing.as_deref() == Some(el.id.as_str()) && ed.editing_cell == Some((r, c)),
                (_, Some(_)) => false,
                _ => ids.iter().all(|i| selected.contains(i)),
            };
            let (label, depth, folder) = match row {
                Row::El(e, d, _) => (element_label(e), *d, false),
                Row::Folder { name, members, .. } => (format!("{name}  {}", members.len()), 0, true),
                Row::Piece { label, depth, .. } => (label.clone(), *depth, false),
            };
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let r = rect.shrink2(vec2(8.0, 2.0));
            let sel_k = ui::motion::hover(ui.ctx(), Id::new(("layer-sel", &key)), is_sel);
            if sel_k > 0.0 {
                ui.painter().rect_filled(r, ui::RADIUS, t.selected.gamma_multiply(sel_k));
            }
            ui::hover_fill(ui, Id::new(("layer-h", &key)), r, resp.hovered() && !is_sel, ui::RADIUS as f32, t.hover);
            let mut x = r.min.x + 8.0 + depth as f32 * 16.0;
            // Opens and closes folders, sections, tables and widgets.
            let toggle = match row {
                Row::Folder { id, .. } => Some(id.clone()),
                Row::El(e, _, true) => Some(open_key(&e.id)),
                _ => None,
            };
            if let Some(tk) = &toggle {
                let chev = Rect::from_center_size(pos2(x + 6.0, r.center().y), vec2(16.0, 24.0));
                let c = ui.interact(chev, Id::new(("chev", &key)), Sense::click());
                let open = st.open_folders.contains(tk);
                icon(ui, if open { "chevron-down" } else { "chevron-right" }, chev.center(), 12.0, if c.hovered() { t.icon } else { t.icon2 });
                if c.clicked() {
                    if open {
                        st.open_folders.remove(tk);
                    } else {
                        st.open_folders.insert(tk.clone());
                    }
                }
            }
            x += 16.0;
            match row {
                Row::Folder { auto, .. } => icon(ui, if *auto { "pen-line" } else { "folder" }, pos2(x + 7.0, r.center().y), 14.0, t.icon2),
                Row::El(e, ..) => icon(ui, type_icon(e), pos2(x + 7.0, r.center().y), 14.0, t.icon2),
                Row::Piece { piece: Piece::Cell(..), .. } => icon(ui, "rectangle-horizontal", pos2(x + 7.0, r.center().y), 12.0, t.text3),
                Row::Piece { el, .. } => icon(ui, if matches!(el.widget(), Some(Widget::Poll { .. })) { "chart-column" } else { "list-checks" }, pos2(x + 7.0, r.center().y), 12.0, t.text3),
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
                            Row::El(e, ..) => {
                                let name: String = v.trim().chars().take(80).collect();
                                ed.board.update(&[e.id.clone()], |el| el.name = (!name.is_empty()).then(|| name.clone()));
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                let color = if hidden { t.text3 } else if piece.is_some() { t.text2 } else { t.text };
                let font = if folder { ui::medium(11.0) } else { egui::FontId::proportional(11.0) };
                ui.painter().with_clip_rect(Rect::from_min_max(pos2(x, r.min.y), pos2(r.max.x - 52.0, r.max.y))).text(pos2(x, r.center().y), egui::Align2::LEFT_CENTER, &label, font, color);
            }
            // Lock and hide, shown on hover or when on.
            if !ro && piece.is_none() && (resp.hovered() || locked || hidden) && !renaming {
                for (k, (name, tip, on)) in [("eye", if hidden { "Mostra" } else { "Nascondi" }, hidden), ("lock", if locked { "Sblocca" } else { "Blocca" }, locked)].into_iter().enumerate() {
                    let br = Rect::from_center_size(pos2(r.max.x - 12.0 - k as f32 * 22.0, r.center().y), vec2(20.0, 20.0));
                    let bresp = ui.interact(br, Id::new((name, &key)), Sense::click());
                    ui::hover_fill(ui, bresp.id, br, bresp.hovered(), 4.0, t.press);
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
            if let (Some(p), Row::Piece { el, .. }) = (piece, row) {
                if resp.clicked() && !ro && !el.locked {
                    ed.set_tool(Tool::Select);
                    ed.selection = vec![el.id.clone()];
                    ed.reveal(&ids);
                    match p {
                        // A cell: straight to writing in it.
                        Piece::Cell(r, c) => {
                            ed.editing = Some(el.id.clone());
                            ed.editing_cell = Some((r, c));
                        }
                        // An option or item: written in the side panel.
                        Piece::Item(_) => {
                            ed.prefs.right_panel = true;
                            ed.prefs.focus = false;
                        }
                    }
                }
                continue;
            }
            if resp.double_clicked() && !ro && !matches!(row, Row::Folder { auto: true, .. }) {
                st.rename_buf = match row {
                    Row::El(e, ..) => element_label(e),
                    Row::Folder { name, .. } => name.clone(),
                    Row::Piece { .. } => String::new(),
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
            ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
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
    ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
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
        if let Some(c) = ui::color_well(ui, "Colore personalizzato dello sfondo", parse_color(&meta.background).unwrap_or(Color32::WHITE)) {
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
                ui.spacing_mut().item_spacing.x = 2.0;
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
                if els.len() > 2 {
                    if small_icon_button(ui, "align-horizontal-distribute-center", "Distribuisci in orizzontale", false).clicked() {
                        ed.distribute(false);
                    }
                    if small_icon_button(ui, "align-vertical-distribute-center", "Distribuisci in verticale", false).clicked() {
                        ed.distribute(true);
                    }
                }
            });
        });
    }
    // Position and size, rotation and corners: every number can be dragged by its label.
    let frame = union(els.iter().map(|e| frame_box(e))).unwrap_or_default();
    section(ui, Some("Posizione e dimensioni"), |ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        let one = single.as_ref().filter(|e| !e.is_line()).cloned();
        let (x, y) = one.as_ref().map_or((frame.x, frame.y), |e| (e.x, e.y));
        let (bw, bh) = one.as_ref().map_or((frame.w, frame.h), |e| (e.w, e.h));
        let mut set: Option<(Option<f64>, Option<f64>, Option<f64>, Option<f64>)> = None;
        ui.horizontal(|ui| {
            if let Some(v) = ui::Num::new(Id::new("px"), "X", "Posizione orizzontale: trascina la X o scrivi", Some(x)).width(w).show(ui) {
                set = Some((Some(v), None, None, None));
            }
            if let Some(v) = ui::Num::new(Id::new("py"), "Y", "Posizione verticale: trascina la Y o scrivi", Some(y)).width(w).show(ui) {
                set = Some((None, Some(v), None, None));
            }
        });
        ui.horizontal(|ui| {
            if let Some(v) = ui::Num::new(Id::new("pw"), "L", "Larghezza: trascina la L o scrivi", Some(bw)).width(w).range(1.0, f64::INFINITY).show(ui) {
                set = Some((None, None, Some(v), None));
            }
            if let Some(v) = ui::Num::new(Id::new("ph"), "A", "Altezza: trascina la A o scrivi", Some(bh)).width(w).range(1.0, f64::INFINITY).show(ui) {
                set = Some((None, None, None, Some(v)));
            }
        });
        let radii: Vec<f64> = els.iter().filter_map(|e| crate::style::radius(e)).collect();
        let round = !radii.is_empty() && radii.len() == els.len();
        ui.horizontal(|ui| {
            if let Some(e) = &one {
                let deg = (e.rotation.to_degrees() % 360.0 + 360.0) % 360.0;
                if let Some(v) = ui::Num::new(Id::new("prot"), "icon:rotate-cw", "Rotazione in gradi: trascina l'icona o scrivi", Some(deg)).width(w).suffix("°").show(ui) {
                    let v = v.rem_euclid(360.0);
                    apply(ed, &mut |el| el.rotation = v.to_radians());
                }
            }
            if round {
                let cur = radii.windows(2).all(|p| (p[0] - p[1]).abs() < 0.01).then(|| radii[0]);
                let max = els.iter().map(|e| crate::style::max_radius(e)).fold(f64::INFINITY, f64::min);
                if let Some(v) = ui::Num::new(Id::new("pradius"), "icon:corner-radius", "Raggio degli angoli: trascina l'icona o scrivi", cur.map(|r| r.min(max).round())).width(w).range(0.0, max.round()).show(ui) {
                    apply(ed, &mut |el| crate::style::set_radius(el, v));
                }
            }
        });
        if round {
            // The same radius on a slider, from square to fully round.
            let max = els.iter().map(|e| crate::style::max_radius(e)).fold(f64::INFINITY, f64::min);
            let mut r = radii[0].min(max);
            if ui::slider_inline(ui, "Angoli", &mut r, 0.0, max.max(1.0), 1.0, false, |v| format!("{}", v.round()), ui.available_width()) {
                apply(ed, &mut |el| crate::style::set_radius(el, r));
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
    if let Some(e) = single.as_ref().filter(|e| !e.locked) {
        match &e.kind {
            Kind::Widget(_) => widget_props(ui, ed, e),
            Kind::Code(c) => {
                let c = c.clone();
                section(ui, Some("Codice"), |ui| {
                    if let Some(l) = language_picker(ui, &c.language, t) {
                        ed.prefs.code_language = l.clone();
                        apply(ed, &mut |el| {
                            if let Kind::Code(x) = &mut el.kind {
                                x.language = l.clone();
                            }
                        });
                    }
                    let mut light = c.light;
                    if ui::segmented(ui, &mut light, &[(false, "Scuro"), (true, "Chiaro")], ui.available_width()) {
                        apply(ed, &mut |el| {
                            if let Kind::Code(x) = &mut el.kind {
                                x.light = light;
                            }
                        });
                    }
                });
            }
            Kind::Table(tb) => table_props(ui, ed, e, tb),
            _ => {}
        }
    }
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
            if let Some(c) = ui::palette(ui, &SECTION_COLORS, cur.as_deref().unwrap_or(""), false) {
                apply(ed, &mut |el| {
                    if let Kind::Section { fill } = &mut el.kind {
                        *fill = c.clone();
                    }
                });
            }
            ui::hint(ui, "Quello che metti dentro la sezione si sposta, si copia e si duplica insieme a lei.");
        });
    }
    if els.iter().all(|e| crate::style::look(e).is_some()) {
        text_props(ui, ed, &els, &ids);
    }
    if only("sticky") {
        section(ui, Some("Colore della nota"), |ui| {
            let cur = same(&els, |e| e.sticky().map(|s| s.color.clone()));
            if let Some(c) = ui::palette(ui, &STICKY_COLORS, cur.as_deref().unwrap_or(""), false) {
                apply(ed, &mut |el| {
                    if let Kind::Sticky(s) = &mut el.kind {
                        s.color = c.clone();
                    }
                });
            }
            if els.iter().any(|e| e.sticky().is_some_and(|s| s.author.is_some())) {
                let mut on = els.iter().all(|e| e.sticky().is_none_or(|s| !s.hide_author));
                if ui::switch(ui, &mut on, "Mostra chi l'ha scritta") {
                    apply(ed, &mut |el| {
                        if let Kind::Sticky(s) = &mut el.kind {
                            s.hide_author = !on;
                        }
                    });
                }
            }
        });
    }
    if only("shape") {
        section(ui, Some("Riempimento"), |ui| {
            let cur = same(&els, |e| e.shape().map(|s| s.fill.clone()));
            if let Some(c) = ui::palette(ui, &INK_COLORS, cur.as_deref().unwrap_or(""), true) {
                apply(ed, &mut |el| {
                    if let Kind::Shape(s) = &mut el.kind {
                        s.fill = c.clone();
                    }
                });
            }
        });
    }
    let colorable = !types.is_empty() && types.iter().all(|k| matches!(*k, "ink" | "highlighter" | "shape" | "line"));
    if colorable {
        let title = if only("shape") || only("line") { "Contorno" } else { "Colore" };
        section(ui, Some(title), |ui| {
            let main = same(&els, |e| match &e.kind {
                Kind::Ink(i) | Kind::Highlighter(i) => Some(i.color.clone()),
                Kind::Shape(s) => Some(s.stroke.clone()),
                Kind::Line(l) => Some(l.stroke.clone()),
                _ => None,
            });
            if let Some(c) = ui::palette(ui, &INK_COLORS, main.as_deref().unwrap_or(""), false) {
                apply(ed, &mut |el| match &mut el.kind {
                    Kind::Ink(i) | Kind::Highlighter(i) => i.color = c.clone(),
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
            ui.horizontal(|ui| {
                if widths.iter().all(Option::is_some) {
                    let v = widths.windows(2).all(|w| w[0] == w[1]).then(|| widths[0].unwrap());
                    if let Some(w) = ui::Num::new(Id::new("pstroke"), "Sp", "Spessore: trascina «Sp» o scrivi", v).decimals(1).step(0.5).range(0.1, 200.0).width(96.0).show(ui) {
                        apply(ed, &mut |el| match &mut el.kind {
                            Kind::Ink(i) | Kind::Highlighter(i) => i.size = w,
                            Kind::Shape(s) => s.stroke_width = w,
                            Kind::Line(l) => l.stroke_width = w,
                            _ => {}
                        });
                    }
                }
            });
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
                for (r, name, label) in super::toolbar::ROUTES {
                    let route = same(&els, |e| e.line().map(|l| l.route));
                    if small_icon_button(ui, name, label, route == Some(r)).clicked() {
                        ed.prefs.route = r;
                        apply(ed, &mut |el| {
                            if let Kind::Line(l) = &mut el.kind {
                                l.route = r;
                            }
                        });
                    }
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
        ui::row(ui, 24.0, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let op = same(&els, |e| Some((e.opacity * 100.0).round()));
            if let Some(v) = ui::Num::new(Id::new("popacity"), "icon:droplet", "Opacità: trascina la goccia o scrivi", op).suffix("%").range(0.0, 100.0).width(84.0).show(ui) {
                let v = (v / 100.0).clamp(0.0, 1.0);
                apply(ed, &mut |el| el.opacity = v);
            }
            ui.add_space(6.0);
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

/// The text of whatever is selected (text boxes, notes, shapes, tables, code): font, size,
/// weight, alignment and colour, changed for all of them at once.
fn text_props(ui: &mut Ui, ed: &mut Editor, els: &[Arc<El>], ids: &[String]) {
    let t = ui::theme(ui.ctx());
    let looks: Vec<crate::style::Look> = els.iter().filter_map(|e| crate::style::look(e)).collect();
    if looks.is_empty() {
        return;
    }
    let mut change: Option<crate::style::Change> = None;
    section(ui, Some("Testo"), |ui| {
        use crate::style::Change;
        if looks.iter().all(|l| l.font.is_some()) {
            let mut f = same(els, |e| crate::style::look(e).and_then(|l| l.font)).unwrap_or_default();
            if super::toolbar::font_picker(ui, &mut f, ui.available_width(), &t) {
                change = Some(Change::Font(f));
            }
        }
        ui::row(ui, 24.0, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let auto = looks.iter().all(|l| l.auto && l.size.is_none());
            let size = same(els, |e| crate::style::shown_size(e).map(|v| (v * 10.0).round() / 10.0));
            let title = if auto { "Dimensione del testo (adesso si adatta da sola): trascina o scrivi per fissarla" } else { "Dimensione del testo: trascina l'icona o scrivi" };
            if let Some(v) = ui::Num::new(Id::new("pfont"), "icon:text-size", title, size).decimals(1).range(1.0, 2000.0).width(88.0).show(ui) {
                change = Some(Change::Size(Some(v)));
            }
            if looks.iter().all(|l| l.auto) {
                let r = ui::icon_button(ui, "maximize", if auto { "La dimensione si adatta alla forma" } else { "Fai adattare la dimensione alla forma" }, None, vec2(24.0, 24.0), 14.0, auto, false);
                if r.clicked() && !auto {
                    change = Some(Change::Size(None));
                }
            }
            ui.add_space(6.0);
            let all = |f: fn(&crate::style::Look) -> Option<bool>| looks.iter().all(|l| f(l) == Some(true));
            if looks.iter().all(|l| l.bold.is_some()) {
                let on = all(|l| l.bold);
                if small_icon_button(ui, "bold", "Grassetto", on).clicked() {
                    change = Some(Change::Bold(!on));
                }
                let on = all(|l| l.italic);
                if small_icon_button(ui, "italic", "Corsivo", on).clicked() {
                    change = Some(Change::Italic(!on));
                }
            }
            if looks.iter().all(|l| l.strike.is_some()) {
                let on = all(|l| l.strike);
                if small_icon_button(ui, "strikethrough", "Barrato", on).clicked() {
                    change = Some(Change::Strike(!on));
                }
            }
        });
        if looks.iter().all(|l| l.align.is_some()) {
            let mut align = same(els, |e| crate::style::look(e).and_then(|l| l.align));
            let before = align;
            ui::segmented(ui, &mut align, &[(Some(Align::Left), "icon:text-align-start"), (Some(Align::Center), "icon:text-align-center"), (Some(Align::Right), "icon:text-align-end")], ui.available_width());
            if align != before
                && let Some(a) = align
            {
                change = Some(Change::Align(a));
            }
        }
        if looks.iter().all(|l| l.color.is_some()) {
            let cur = same(els, |e| crate::style::look(e).and_then(|l| l.color.flatten()));
            if let Some(c) = ui::palette(ui, &INK_COLORS, cur.as_deref().unwrap_or(""), false) {
                change = Some(Change::Color(c));
            }
        }
    });
    if let Some(c) = change {
        ed.board.update(ids, |el| crate::style::apply(el, &c));
        // The next text box starts with the same font.
        if let (crate::style::Change::Font(f), true) = (&c, els.iter().all(|e| e.text().is_some())) {
            ed.prefs.text.font = *f;
        }
    }
}

/// A table: its size, its first row, and the cell being written in.
fn table_props(ui: &mut Ui, ed: &mut Editor, el: &El, tb: &Table) {
    let id = el.id.clone();
    let cell = if ed.editing.as_deref() == Some(id.as_str()) { ed.editing_cell } else { None };
    let mut next: Option<Table> = None;
    section(ui, Some("Tabella"), |ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        ui.horizontal(|ui| {
            if let Some(v) = ui::Num::new(Id::new("trows"), "Righe", "Righe: trascina o scrivi", Some(tb.rows.len() as f64)).range(1.0, 200.0).width(w).show(ui) {
                let mut t = tb.clone();
                let n = (v as usize).clamp(1, 200);
                while t.rows.len() < n {
                    t.insert_row(t.rows.len());
                }
                while t.rows.len() > n {
                    t.remove_row(t.rows.len() - 1);
                }
                next = Some(t);
            }
            if let Some(v) = ui::Num::new(Id::new("tcols"), "Colonne", "Colonne: trascina o scrivi", Some(tb.cols.len() as f64)).range(1.0, 50.0).width(w).show(ui) {
                let mut t = tb.clone();
                let n = (v as usize).clamp(1, 50);
                while t.cols.len() < n {
                    t.insert_col(t.cols.len());
                }
                while t.cols.len() > n {
                    t.remove_col(t.cols.len() - 1);
                }
                next = Some(t);
            }
        });
        let mut header = tb.header;
        if ui::switch(ui, &mut header, "Prima riga come intestazione") {
            next = Some(Table { header, ..tb.clone() });
        }
        match cell {
            Some((r, c)) if r < tb.rows.len() && c < tb.cols.len() => {
                ui::heading(ui, &format!("Cella {}{}", col_name(c), r + 1));
                let cur = tb.cells[r][c].fill.clone().unwrap_or_else(|| "transparent".into());
                if let Some(f) = ui::palette(ui, &CELL_COLORS, &cur, true) {
                    let mut t = tb.clone();
                    t.cells[r][c].fill = (f != "transparent").then_some(f);
                    next = Some(t);
                }
            }
            _ => ui::hint(ui, "Un clic su una cella per scriverci; Tab passa alla cella dopo (nell'ultima aggiunge una riga). Il colore di una cella si sceglie mentre ci scrivi."),
        }
    });
    if let Some(t) = next {
        ed.board.update(&[id], |e| {
            e.kind = Kind::Table(t.clone());
            crate::text::fit_text(e);
        });
    }
}

/// Spreadsheet name of a column: A, B, … Z, AA…
pub fn col_name(c: usize) -> String {
    let mut n = c + 1;
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

/// Light fills for table cells.
const CELL_COLORS: [&str; 8] = ["#F5F5F5", "#FFF3A3", "#C9F2C7", "#C7E5FF", "#FFD1E3", "#E2D4FF", "#FFDDB8", "#1E1E1E"];

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
    // Beside the minimap when it sits in the top right corner.
    let mini = if b.editor.prefs.minimap { ctx.data(|d| d.get_temp::<Rect>(Id::new("minimap-rect"))) } else { None };
    let x = match mini {
        Some(m) if m.min.y < stage.min.y + 64.0 && m.max.x > stage.max.x - 120.0 => m.min.x - 8.0,
        _ => stage.max.x - 12.0,
    };
    let x = ui::motion::spring(ctx, Id::new("right-pill-x"), x, 0.3, 0.9);
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

/// While a cell is written in: its colour, and rows and columns added or taken away around it,
/// above the table.
fn table_tools(ctx: &egui::Context, stage: Rect, ed: &mut Editor, el: &El, tb: &Table) {
    let t = ui::theme(ctx);
    let Some((r, c)) = ed.editing_cell.filter(|(r, c)| *r < tb.rows.len() && *c < tb.cols.len()) else { return };
    let b = frame_box(el);
    let a = ed.to_screen(b.x, b.y) + stage.min.to_vec2();
    let below = ed.to_screen(b.x, b.bottom()) + stage.min.to_vec2();
    let y = if a.y - 44.0 >= stage.min.y + 8.0 { a.y - 44.0 } else { below.y + 8.0 };
    let x = a.x.clamp(stage.min.x + 8.0, (stage.max.x - 320.0).max(stage.min.x + 8.0));
    let id = el.id.clone();
    let mut next: Option<(Table, (usize, usize))> = None;
    egui::Area::new(Id::new("table-tools")).fixed_pos(pos2(x, y)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui::row(ui, 28.0, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{}{}", col_name(c), r + 1)).font(ui::medium(11.0)).color(t.text2));
                ui.add_space(4.0);
                // The cell's colour.
                let cur = tb.cells[r][c].fill.clone().unwrap_or_else(|| "transparent".into());
                let (cr, chip) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                ui::hover_fill(ui, chip.id, cr, chip.hovered(), ui::RADIUS as f32, t.hover);
                let fill = parse_color(&cur).filter(|c| c.a() > 0);
                match fill {
                    Some(f) => {
                        ui.painter().circle_filled(cr.center(), 8.0, f);
                    }
                    None => {
                        ui.painter().circle_filled(cr.center(), 8.0, Color32::WHITE);
                        ui.painter().line_segment([cr.center() + vec2(-5.5, 5.5), cr.center() + vec2(5.5, -5.5)], Stroke::new(1.5, t.danger));
                    }
                }
                ui.painter().circle_stroke(cr.center(), 8.0, Stroke::new(1.0, Color32::from_black_alpha(40)));
                let chip = ui::tip(chip, "Colore della cella", None);
                egui::Popup::from_toggle_button_response(&chip).frame(ui::float_frame(&t).inner_margin(egui::Margin::same(10))).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.set_width(196.0);
                    ui::heading(ui, "Colore della cella");
                    if let Some(f) = ui::palette(ui, &CELL_COLORS, &cur, true) {
                        let mut tt = tb.clone();
                        tt.cells[r][c].fill = (f != "transparent").then_some(f);
                        next = Some((tt, (r, c)));
                    }
                });
                let (sr, _) = ui.allocate_exact_size(vec2(9.0, 28.0), Sense::hover());
                ui.painter().vline(sr.center().x, (sr.center().y - 8.0)..=(sr.center().y + 8.0), Stroke::new(1.0, t.border));
                let mut op = |ui: &mut Ui, name: &str, label: &str, f: &dyn Fn(&mut Table) -> (usize, usize)| {
                    if ui::icon_button(ui, name, label, None, vec2(28.0, 28.0), 16.0, false, false).clicked() {
                        let mut tt = tb.clone();
                        let at = f(&mut tt);
                        next = Some((tt, at));
                    }
                };
                op(ui, "row-plus", "Aggiungi una riga sotto", &|t| {
                    t.insert_row(r + 1);
                    (r + 1, c)
                });
                op(ui, "col-plus", "Aggiungi una colonna a destra", &|t| {
                    t.insert_col(c + 1);
                    (r, c + 1)
                });
                if tb.rows.len() > 1 {
                    op(ui, "row-minus", "Elimina questa riga", &|t| {
                        t.remove_row(r);
                        (r.min(t.rows.len() - 1), c)
                    });
                }
                if tb.cols.len() > 1 {
                    op(ui, "col-minus", "Elimina questa colonna", &|t| {
                        t.remove_col(c);
                        (r, c.min(t.cols.len() - 1))
                    });
                }
                // More: before this row or column.
                let more = ui::icon_button(ui, "more-horizontal", "Altro", None, vec2(28.0, 28.0), 16.0, false, false);
                egui::Popup::menu(&more).frame(ui::menu_frame(&t)).show(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if ui::menu_item(ui, Some("row-plus"), "Aggiungi una riga sopra", None, false).clicked() {
                        let mut tt = tb.clone();
                        tt.insert_row(r);
                        next = Some((tt, (r + 1, c)));
                    }
                    if ui::menu_item(ui, Some("col-plus"), "Aggiungi una colonna a sinistra", None, false).clicked() {
                        let mut tt = tb.clone();
                        tt.insert_col(c);
                        next = Some((tt, (r, c + 1)));
                    }
                    ui::menu_sep(ui);
                    let mut header = tb.header;
                    if ui::menu_item(ui, header.then_some("check"), "Prima riga come intestazione", None, false).clicked() {
                        header = !header;
                        next = Some((Table { header, ..tb.clone() }, (r, c)));
                    }
                });
            });
        });
    });
    if let Some((tt, cell)) = next {
        ed.board.update(std::slice::from_ref(&id), |e| {
            e.kind = Kind::Table(tt.clone());
            crate::text::fit_text(e);
        });
        ed.editing_cell = Some(cell);
        ctx.memory_mut(|m| m.request_focus(Id::new("board-text")));
    }
}

/// While code is typed: its language and its theme, above the block.
fn code_tools(ctx: &egui::Context, stage: Rect, ed: &mut Editor, el: &El, code: &crate::model::Code) {
    let t = ui::theme(ctx);
    let a = ed.to_screen(el.x, el.y) + stage.min.to_vec2();
    let y = if a.y - 40.0 >= stage.min.y + 8.0 { a.y - 40.0 } else { a.y + 8.0 };
    let id = el.id.clone();
    egui::Area::new(Id::new("code-tools")).fixed_pos(pos2(a.x.max(stage.min.x + 8.0), y)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui::row(ui, 24.0, |ui| {
                if let Some(lang) = language_picker(ui, &code.language, &t) {
                    ed.prefs.code_language = lang.clone();
                    ed.board.update(std::slice::from_ref(&id), |el| {
                        if let Kind::Code(c) = &mut el.kind {
                            c.language = lang.clone();
                        }
                    });
                    ui.memory_mut(|m| m.request_focus(Id::new("board-text")));
                }
                let light = code.light;
                if small_icon_button(ui, if light { "moon" } else { "sun" }, if light { "Tema scuro" } else { "Tema chiaro" }, false).clicked() {
                    ed.board.update(std::slice::from_ref(&id), |el| {
                        if let Kind::Code(c) = &mut el.kind {
                            c.light = !light;
                        }
                    });
                    ui.memory_mut(|m| m.request_focus(Id::new("board-text")));
                }
            });
        });
    });
}

/// The language of a code block, as a dropdown.
pub fn language_picker(ui: &mut Ui, current: &str, t: &Theme) -> Option<String> {
    let name = crate::code::language_name(current);
    let g = ui.painter().layout_no_wrap(name.to_string(), ui::medium(11.0), t.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 24.0), Sense::click());
    let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    ui.painter().rect_filled(rect, ui::RADIUS, t.hover.gamma_multiply(k.max(0.5)));
    ui.painter().galley(pos2(rect.min.x + 8.0, rect.center().y - g.size().y / 2.0), g, t.text);
    ui::icon(ui, "chevron-down", rect.right_center() - vec2(10.0, 0.0), 12.0, t.icon2);
    let resp = ui::tip(resp, "Linguaggio", None);
    let mut picked = None;
    egui::Popup::menu(&resp).frame(ui::menu_frame(t)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (id, label) in crate::code::LANGUAGES {
                if ui::menu_item(ui, (id == current).then_some("check"), label, None, false).clicked() {
                    picked = Some(id.to_string());
                }
            }
        });
    });
    picked
}

/// What a widget says: its question or title and its options or items, edited in the panel.
fn widget_props(ui: &mut Ui, ed: &mut Editor, el: &El) {
    let Some(mut w) = el.widget().cloned() else { return };
    let mut changed = false;
    let field = |ui: &mut Ui, text: &mut String, hint: &str, width: f32| ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width)).changed();
    match &mut w {
        Widget::Poll { question, options } => {
            section(ui, Some("Domanda"), |ui| {
                changed |= ui.add(egui::TextEdit::multiline(question).hint_text("Fai una domanda").desired_rows(2).desired_width(f32::INFINITY)).changed();
            });
            section(ui, Some("Opzioni"), |ui| {
                let mut remove = None;
                for (i, o) in options.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= field(ui, &mut o.text, &format!("Opzione {}", i + 1), ui.available_width() - 30.0);
                        if small_icon_button(ui, "x", "Togli l'opzione", false).clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    options.remove(i);
                    changed = true;
                }
                if options.len() < 50 && ui::button(ui, "Aggiungi un'opzione", ui::Kind::Secondary, Some("plus"), false, true).clicked() {
                    options.push(PollOption { text: String::new(), votes: Vec::new() });
                    changed = true;
                }
            });
        }
        Widget::Checklist { title, items } => {
            section(ui, Some("Titolo"), |ui| {
                changed |= field(ui, title, "Titolo della lista", f32::INFINITY);
            });
            section(ui, Some("Cose da fare"), |ui| {
                let mut remove = None;
                for (i, it) in items.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if small_icon_button(ui, "check", if it.done { "Togli la spunta" } else { "Spunta" }, it.done).clicked() {
                            it.done = !it.done;
                            changed = true;
                        }
                        changed |= field(ui, &mut it.text, "Cosa da fare", ui.available_width() - 30.0);
                        if small_icon_button(ui, "x", "Togli", false).clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    items.remove(i);
                    changed = true;
                }
                if items.len() < 200 && ui::button(ui, "Aggiungi", ui::Kind::Secondary, Some("plus"), false, true).clicked() {
                    items.push(CheckItem { text: String::new(), done: false });
                    changed = true;
                }
            });
        }
        Widget::Counter { label, value } => {
            section(ui, Some("Contatore"), |ui| {
                changed |= field(ui, label, "Cosa contiamo?", f32::INFINITY);
                if let Some(v) = ui::number_field(ui, Id::new("pcounter"), "#", "Valore", Some(*value as f64), 0, 120.0) {
                    *value = v.round().clamp(-999_999.0, 999_999.0) as i64;
                    changed = true;
                }
            });
        }
    }
    if changed {
        ed.board.update(std::slice::from_ref(&el.id), |e| {
            e.kind = Kind::Widget(w.clone());
            crate::text::fit_text(e);
        });
    }
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
    if let Some(tb) = el.table() {
        return table_tools(ctx, stage, ed, el, tb);
    }
    if el.is_line() {
        return;
    }
    if let Some(code) = el.code() {
        return code_tools(ctx, stage, ed, el, code);
    }
    let b = frame_box(el);
    let a = ed.to_screen(b.x, b.y) + stage.min.to_vec2();
    let below = ed.to_screen(b.x, b.bottom()) + stage.min.to_vec2();
    let width = if el.text().is_some() { 232.0 } else { 176.0 };
    let y = if a.y - 40.0 >= stage.min.y + 8.0 { a.y - 40.0 } else { below.y + 8.0 };
    let x = a.x.clamp(stage.min.x + 8.0, (stage.max.x - width - 8.0).max(stage.min.x + 8.0));
    let id = el.id.clone();
    egui::Area::new(Id::new("text-tools")).fixed_pos(pos2(x, y)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui::row(ui, 24.0, |ui| {
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
        ui.painter().rect(rect, 8.0, t.bg2, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
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
        ui.painter().add(egui::epaint::RectShape::filled(rect, 8.0, Color32::WHITE).with_texture(tex.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
    }
    ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
}
