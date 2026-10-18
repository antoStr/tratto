//! The side panels (layers and templates on the left, design and sharing on the right), the
//! pills that replace them when closed, the context menu and the text bar.

use std::sync::Arc;

use egui::{Color32, Id, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2, vec2};

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
        ShapeKind::ManualInput => "Inserimento manuale",
        ShapeKind::Delay => "Ritardo",
        ShapeKind::OffPage => "Connettore fuori pagina",
        ShapeKind::Documents => "Più documenti",
        ShapeKind::Display => "Display",
        ShapeKind::InternalStorage => "Memoria interna",
        ShapeKind::Junction => "Giunzione",
        ShapeKind::Cloud => "Nuvola",
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

pub(crate) fn type_icon(el: &El) -> &'static str {
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
        Kind::Stamp { .. } => "stamp",
        Kind::Section { .. } => "section",
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
    let action = None;
    // Header: name and status.
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), ui::ctl(48.0)), Sense::hover());
    ui.painter().hline(head.x_range(), head.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(8.0, 6.0))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                title(ui, b, toasts, 150.0);
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
                    ui.label(RichText::new(status).size(ui::px(10.0)).color(t.text2));
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_min_height(ui.available_height());
                ui.spacing_mut().item_spacing.x = 2.0;
                if small_icon_button(ui, "panel-left-close", "Nascondi il pannello", false).clicked() {
                    b.ui.peek_left = false;
                    b.editor.prefs.left_panel = false;
                }
                if !b.host {
                    guest_buttons(ui, b, dialogs);
                }
            });
        });
    });
    // Tabs.
    let (tabs, _) = ui.allocate_exact_size(vec2(ui.available_width(), ui::ctl(40.0)), Sense::hover());
    ui.painter().hline(tabs.x_range(), tabs.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(tabs.shrink2(vec2(8.0, 8.0))), |ui| {
        ui.horizontal_centered(|ui| {
            let options: &[(LeftTab, &str)] = if b.editor.read_only { &[(LeftTab::Layers, "Livelli")] } else { &[(LeftTab::Layers, "Livelli"), (LeftTab::Templates, "Modelli")] };
            tab_strip(ui, &mut b.ui.left_tab, options);
        });
    });
    let k = ui::motion::entering(ui.ctx(), Id::new("left-tab-content"), b.ui.left_tab as u8);
    ui.multiply_opacity(k.clamp(0.0, 1.0));
    let shift = ui::motion::travel(ui.ctx(), k.clamp(0.0, 1.0)) * 12.0;
    ui.with_visual_transform(egui::emath::TSTransform::from_translation(vec2(0.0, shift)), |ui| {
        if b.ui.left_tab == LeftTab::Templates && !b.editor.read_only {
            // On a phone the panel covers the board: it steps aside to show the template added.
            if templates(ui, &mut b.editor, &t) && ui::narrow(ui.ctx()) {
                b.ui.peek_left = false;
            }
        } else {
            layers(ui, &mut b.editor, &mut b.ui, &t);
        }
    });
    action
}

/// Text tabs; a soft pill glides under the chosen one.
fn tab_strip<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    let t = ui::theme(ui.ctx());
    let rects: Vec<Rect> = options
        .iter()
        .map(|(_, label)| {
            let w = ui.painter().layout_no_wrap(label.to_string(), ui::medium(ui::px(11.0)), t.text).size().x;
            ui.allocate_exact_size(vec2(w + 18.0, ui::ctl(24.0)), Sense::hover()).0
        })
        .collect();
    let Some(first) = rects.first().copied() else { return };
    let at = options.iter().position(|o| o.0 == *value).unwrap_or(0);
    let id = ui.id().with("tabs-pill");
    let x = ui::motion::spring(ui.ctx(), id.with("x"), rects[at].min.x - first.min.x, 0.3, 0.88);
    let w = ui::motion::spring(ui.ctx(), id.with("w"), rects[at].width(), 0.3, 0.88);
    ui.painter().rect_filled(Rect::from_min_size(pos2(first.min.x + x, first.min.y), vec2(w, first.height())), ui::RADIUS, t.bg2);
    for (i, ((v, label), r)) in options.iter().zip(&rects).enumerate() {
        let resp = ui.interact(*r, ui.id().with(("tab", i)), Sense::click());
        let on = i == at;
        let k = ui::motion::hover(ui.ctx(), resp.id.with("h"), on || resp.hovered());
        let font = if on { ui::medium(ui::px(11.0)) } else { egui::FontId::proportional(ui::px(11.0)) };
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
        let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(width).font(ui::medium(ui::px(11.0))));
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
    let g = ui.painter().layout_no_wrap(b.title.clone(), ui::medium(ui::px(11.0)), t.text);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x.min(width), ui::ctl(18.0)), Sense::click());
    ui.painter().with_clip_rect((r).intersect(ui.clip_rect())).galley(pos2(r.min.x, r.center().y - g.size().y / 2.0), g, t.text);
    if ui::tip(resp, "Rinomina", None).clicked() {
        b.ui.title_edit = Some(b.title.clone());
    }
}

/// A guest's way to the settings and to zen mode (the desktop app has them in its tab bar).
fn guest_buttons(ui: &mut Ui, b: &mut BoardScreen, dialogs: &mut Dialogs) {
    if small_icon_button(ui, "scan", "Modalità zen", false).clicked() {
        b.editor.toggle_zen();
    }
    if small_icon_button(ui, "settings", "Impostazioni (Ctrl+,)", false).clicked() {
        dialogs.settings = true;
    }
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
    El(Arc<El>, u8, bool, Parent),
    /// `auto`: strokes written one after the other in the same spot, shown together.
    Folder { id: String, name: String, members: Vec<Arc<El>>, auto: bool },
    Piece { el: Arc<El>, piece: Piece, label: String, depth: u8 },
}

/// What a row is listed under.
#[derive(Clone, Debug, PartialEq)]
pub enum Parent {
    Top,
    /// A folder made by the person.
    Folder(String),
    /// Strokes shown together ("Scrittura"): on the board they are loose, as at the top.
    Run,
    /// The section the element lies in.
    Section(String),
}

fn row_ids(r: &Row) -> Vec<String> {
    match r {
        Row::El(e, ..) | Row::Piece { el: e, .. } => vec![e.id.clone()],
        Row::Folder { members, .. } => members.iter().map(|m| m.id.clone()).collect(),
    }
}

/// What a row is listed under, and how deep (folder rows are always at the top).
fn row_parent(r: &Row) -> (Parent, u8) {
    match r {
        Row::El(_, d, _, p) => (p.clone(), *d),
        Row::Folder { .. } => (Parent::Top, 0),
        Row::Piece { el, depth, .. } => (Parent::Section(format!("piece:{}", el.id)), *depth),
    }
}

/// The row's highest and lowest element in the stack (a folder's top and bottom members).
fn row_ends(r: &Row) -> Option<(String, String)> {
    match r {
        Row::El(e, ..) => Some((e.id.clone(), e.id.clone())),
        Row::Folder { members, .. } => Some((members.first()?.id.clone(), members.last()?.id.clone())),
        Row::Piece { .. } => None,
    }
}

/// Whether `r` is the row that holds what is listed under `p` (a folder, a section).
fn holds(r: &Row, p: &Parent) -> bool {
    match (r, p) {
        (Row::Folder { id, auto: false, .. }, Parent::Folder(f)) => id == f,
        (Row::Folder { auto: true, .. }, Parent::Run) => true,
        (Row::El(e, ..), Parent::Section(s)) => e.id == *s,
        _ => false,
    }
}

/// Rows being dragged in the layers panel: what moves, where it may go, and its label.
#[derive(Clone, Debug)]
pub struct LayerDrag {
    ids: Vec<String>,
    /// Where they are listed now: they move among the rows of the top (folders included) or of
    /// their own section (a section's elements are those lying in it, so they stay there).
    home: Parent,
    /// A whole folder or a section: it stays at the top, never inside another.
    whole: bool,
    label: String,
    icon: &'static str,
    /// The frame it was last seen in: a drag the list stopped showing (the panel closed, another
    /// tab) is dropped, never carried out later.
    frame: u64,
}

/// Where a drop would put the rows: next to which element, and in which folder.
#[derive(Clone, Debug, PartialEq)]
struct Drop {
    to: crate::doc::Anchor,
    group: Option<Option<String>>,
    /// The line between rows (its y and left end), or the folder row lit up.
    line: Option<(f32, f32)>,
    into: Option<Rect>,
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
    fn element(&self, rows: &mut Vec<Row>, el: &Arc<El>, depth: u8, parent: Parent) {
        let kids = self.children.get(&el.id);
        let parts = pieces(el);
        rows.push(Row::El(el.clone(), depth, kids.is_some() || !parts.is_empty(), parent));
        if !self.open.contains(&open_key(&el.id)) {
            return;
        }
        if let Some(kids) = kids {
            self.level(rows, kids, depth + 1, Parent::Section(el.id.clone()));
        }
        rows.extend(parts.into_iter().map(|(piece, label)| Row::Piece { el: el.clone(), piece, label, depth: depth + 1 }));
    }

    fn container(&self, rows: &mut Vec<Row>, id: String, name: String, members: Vec<Arc<El>>, auto: bool) {
        let shown = if self.open.contains(&id) { members.clone() } else { Vec::new() };
        let parent = if auto { Parent::Run } else { Parent::Folder(id.clone()) };
        rows.push(Row::Folder { id, name, members, auto });
        for m in shown {
            self.element(rows, &m, 1, parent.clone());
        }
    }

    fn flush_run(&self, rows: &mut Vec<Row>, run: &mut Vec<Arc<El>>, depth: u8, parent: &Parent) {
        if run.len() == 1 {
            self.element(rows, &run[0], depth, parent.clone());
        } else if run.len() > 1 && depth == 0 {
            // Keyed by the oldest stroke, so the row keeps its state while more strokes come.
            let name = if run.iter().all(|e| matches!(e.kind, Kind::Highlighter(_))) { "Evidenziature" } else { "Scrittura" };
            let id = format!("ink:{}", run[run.len() - 1].id);
            self.container(rows, id, name.into(), std::mem::take(run), true);
        } else {
            for el in run.iter() {
                self.element(rows, el, depth, parent.clone());
            }
        }
        run.clear();
    }

    /// One level of the list, top to bottom; each folder sits where its top element is.
    /// Consecutive nearby strokes outside folders collapse into one row.
    fn level(&self, rows: &mut Vec<Row>, items: &[Arc<El>], depth: u8, parent: Parent) {
        let mut run: Vec<Arc<El>> = Vec::new();
        let mut run_box: Option<BBox> = None;
        for el in items {
            if el.group_id.is_none() && el.is_ink() {
                let b = aabb(el);
                if run_box.is_some_and(|r| !near(&r, &b)) {
                    self.flush_run(rows, &mut run, depth, &parent);
                    run_box = None;
                }
                run.push(el.clone());
                run_box = Some(run_box.map_or(b, |r| union([r, b]).unwrap()));
                continue;
            }
            self.flush_run(rows, &mut run, depth, &parent);
            run_box = None;
            match &el.group_id {
                None => self.element(rows, el, depth, parent.clone()),
                Some(g) if self.folders.get(g).is_some_and(|m| m[0].id == el.id) => {
                    let name = String::new();
                    self.container(rows, g.clone(), name, self.folders[g].clone(), false);
                }
                _ => {}
            }
        }
        self.flush_run(rows, &mut run, depth, &parent);
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
    tree.level(&mut rows, &top, 0, Parent::Top);
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
    let frame = ui.ctx().cumulative_frame_nr();
    if rows.is_empty() || ed.read_only || st.layer_drag.as_ref().is_some_and(|d| d.frame + 1 < frame) {
        st.layer_drag = None;
    }
    if let Some(d) = &mut st.layer_drag {
        d.frame = frame;
    }
    if rows.is_empty() {
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui::hint(ui, "Nessun elemento");
            });
        });
        return;
    }
    let selected: std::collections::HashSet<String> = ed.selection.iter().cloned().collect();
    let ro = ed.read_only;
    let row_h = ui::ctl(32.0);
    // Rows being dragged (to another place in the stack, or into a folder), and where the
    // pointer would drop them.
    let dragging: std::collections::HashSet<String> = st.layer_drag.as_ref().map(|d| d.ids.iter().cloned().collect()).unwrap_or_default();
    let pointer = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
    let mut drop: Option<Drop> = None;
    let mut start: Option<usize> = None;
    // A little air between the tabs and the first row (its highlight would touch their line).
    ui.add_space(6.0);
    ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut shown: Vec<(usize, Rect)> = Vec::new();
        for (index, row) in rows.iter().enumerate().skip(range.start).take(range.end - range.start) {
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
                Row::El(e, d, ..) => (element_label(e), *d, false),
                Row::Folder { name, members, .. } => (format!("{name}  {}", members.len()), 0, true),
                Row::Piece { label, depth, .. } => (label.clone(), *depth, false),
            };
            // Rows move by dragging them (the pieces of a table or a widget don't).
            let movable = !ro && piece.is_none() && st.renaming.as_deref() != Some(key.as_str());
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), if movable { Sense::click_and_drag() } else { Sense::click() });
            shown.push((index, rect));
            if movable && resp.drag_started() && st.layer_drag.is_none() {
                start = Some(index);
            }
            let r = rect.shrink2(vec2(8.0, 2.0));
            let sel_k = ui::motion::hover(ui.ctx(), Id::new(("layer-sel", &key)), is_sel);
            if sel_k > 0.0 {
                ui.painter().rect_filled(r, ui::RADIUS, t.selected.gamma_multiply(sel_k));
            }
            ui::hover_fill(ui, Id::new(("layer-h", &key)), r, resp.hovered() && !is_sel && dragging.is_empty(), ui::RADIUS as f32, t.hover);
            let mut x = r.min.x + 8.0 + depth as f32 * 16.0;
            // Opens and closes folders, sections, tables and widgets.
            let toggle = match row {
                Row::Folder { id, .. } => Some(id.clone()),
                Row::El(e, _, true, _) => Some(open_key(&e.id)),
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
                let edit = ui.put(Rect::from_min_max(pos2(x, r.min.y + 2.0), pos2(r.max.x - 8.0, r.max.y - 2.0)), egui::TextEdit::singleline(&mut st.rename_buf).font(egui::FontId::proportional(ui::px(11.0))));
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
                let font = if folder { ui::medium(ui::px(11.0)) } else { egui::FontId::proportional(ui::px(11.0)) };
                ui.painter().with_clip_rect((Rect::from_min_max(pos2(x, r.min.y), pos2(r.max.x - 52.0, r.max.y))).intersect(ui.clip_rect())).text(pos2(x, r.center().y), egui::Align2::LEFT_CENTER, &label, font, color);
            }
            // Lock and hide, shown while the pointer is on the row (also on the buttons
            // themselves, which take the hover from the row) or when on.
            let over = ui.rect_contains_pointer(rect) && dragging.is_empty();
            if !ro && piece.is_none() && (over || locked || hidden) && !renaming {
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
            // The rows on the move stay where they were, faded.
            if !dragging.is_empty() && !ids.is_empty() && ids.iter().all(|i| dragging.contains(i)) {
                ui.painter().rect_filled(rect, 0.0, t.bg.gamma_multiply(0.6));
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
        // Where the rows are, for tests that drag them.
        #[cfg(test)]
        ui.ctx().data_mut(|m| m.insert_temp(Id::new("layer-rows"), shown.clone()));
        // Dragging: where the rows would land, shown as Figma does (a line between rows, or the
        // folder they would go into lit up); near the top or bottom the list scrolls by itself.
        if let (Some(d), Some(p)) = (&st.layer_drag, pointer) {
            let clip = ui.clip_rect();
            if clip.x_range().contains(p.x) {
                drop = drop_target(&rows, &shown, d, p);
                let edge = 28.0;
                let push = if p.y < clip.min.y + edge { clip.min.y + edge - p.y } else if p.y > clip.max.y - edge { -(p.y - (clip.max.y - edge)) } else { 0.0 };
                if push != 0.0 {
                    ui.scroll_with_delta(vec2(0.0, push.clamp(-edge, edge) * 0.6));
                    ui.ctx().request_repaint();
                }
            }
            if let Some(dr) = &drop {
                let painter = ui.painter().with_clip_rect(clip);
                if let Some((y, x0)) = dr.line {
                    painter.line_segment([pos2(x0, y), pos2(clip.max.x - 10.0, y)], Stroke::new(2.0, t.brand));
                    painter.circle(pos2(x0, y), 3.5, t.bg, Stroke::new(2.0, t.brand));
                }
                if let Some(r) = dr.into {
                    painter.rect(r.shrink2(vec2(8.0, 2.0)), ui::RADIUS, t.selected, Stroke::new(1.5, t.brand), egui::StrokeKind::Inside);
                }
            }
        }
    });
    if let Some(i) = start
        && let Some(d) = drag_of(&rows[i], ed)
    {
        st.layer_drag = Some(LayerDrag { frame, ..d });
    }
    if let Some(d) = st.layer_drag.clone() {
        let ctx = ui.ctx().clone();
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(p) = pointer {
            // What is being moved, under the pointer.
            egui::Area::new(Id::new("layer-drag")).order(egui::Order::Tooltip).fixed_pos(p + vec2(14.0, 8.0)).interactable(false).show(&ctx, |ui| {
                ui.set_opacity(0.92);
                ui::float_frame(t).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        icon(ui, d.icon, r.center(), 14.0, t.icon2);
                        ui.label(RichText::new(&d.label).size(ui::px(11.0)).color(t.text));
                    });
                });
            });
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            st.layer_drag = None;
        } else if ui.input(|i| !i.pointer.primary_down()) {
            st.layer_drag = None;
            if let Some(dr) = drop {
                ed.board.move_in_stack(&d.ids, dr.to, dr.group);
            }
        }
        ctx.request_repaint();
    }
}

/// What dragging this row moves: the row's elements, or the whole selection when the row is
/// part of it. None when the selection lies in different places (two sections) and can't move
/// as one.
fn drag_of(row: &Row, ed: &mut Editor) -> Option<LayerDrag> {
    let own = row_ids(row);
    let selected = own.iter().all(|i| ed.selection.contains(i)) && ed.selection.len() > own.len();
    let ids = if selected { ed.selection.clone() } else { own };
    let items: Vec<Arc<El>> = ed.board.all().to_vec();
    let sections = section_parents(&items);
    let homes: std::collections::HashSet<Option<&String>> = ids.iter().map(|i| sections.get(i)).collect();
    if homes.len() != 1 {
        return None;
    }
    let home = match homes.into_iter().next().flatten() {
        Some(s) => Parent::Section(s.clone()),
        None => Parent::Top,
    };
    // A whole folder, or a section: it stays at the top level.
    let whole = matches!(row, Row::Folder { auto: false, .. }) && !selected || ids.iter().any(|i| ed.board.get(i).is_some_and(|e| e.is_section()));
    let (label, icon) = if selected {
        (format!("{} livelli", ids.len()), "copy")
    } else {
        match row {
            Row::El(e, ..) => (element_label(e), type_icon(e)),
            Row::Folder { name, auto, .. } => (name.clone(), if *auto { "pen-line" } else { "folder" }),
            Row::Piece { .. } => return None,
        }
    };
    Some(LayerDrag { ids, home, whole, label, icon, frame: 0 })
}

/// Where the rows being dragged would land with the pointer at `p`: between which rows (and so
/// next to which element in the stack, and in which folder), or into a folder's row.
fn drop_target(rows: &[Row], shown: &[(usize, Rect)], d: &LayerDrag, p: Pos2) -> Option<Drop> {
    let &(first, first_rect) = shown.first()?;
    let &(i, r) = shown.iter().find(|(_, r)| p.y < r.max.y).unwrap_or(shown.last()?);
    let (i, r) = if p.y < first_rect.min.y { (first, first_rect) } else { (i, r) };
    let f = ((p.y - r.min.y) / r.height()).clamp(0.0, 1.0);
    let moving = |k: usize| {
        let ids = row_ids(&rows[k]);
        !ids.is_empty() && ids.iter().all(|id| d.ids.contains(id))
    };
    // Into a folder: the middle of its row.
    if let Row::Folder { id, members, auto: false, .. } = &rows[i]
        && (0.28..0.72).contains(&f)
        && d.home == Parent::Top
        && !d.whole
        && !moving(i)
    {
        return Some(Drop { to: crate::doc::Anchor::Above(members.first()?.id.clone()), group: Some(Some(id.clone())), line: None, into: Some(r) });
    }
    // Otherwise the gap above or below it; the neighbours are the nearest rows that stay put
    // (the pieces of an opened table or widget don't count).
    let gap = if f < 0.5 { i } else { i + 1 };
    let y = if f < 0.5 { r.min.y } else { r.max.y };
    let usable = |k: usize| !matches!(rows[k], Row::Piece { .. }) && !moving(k);
    let a = (0..gap).rev().find(|&k| usable(k));
    let b = (gap..rows.len()).find(|&k| usable(k));
    let indent = |depth: u8| r.min.x + 8.0 + 16.0 + depth as f32 * 16.0;
    let (pa, da) = a.map_or((Parent::Top, 0), |k| row_parent(&rows[k]));
    let (pb, db) = b.map_or((Parent::Top, 0), |k| row_parent(&rows[k]));
    // Under the row above when it ends a folder or a section and the pointer is indented as it
    // is; otherwise with the row below.
    let ctx = if a.is_some() && da > 0 && (b.is_none() || db < da) && p.x > indent(da) {
        pa
    } else if b.is_some() {
        pb
    } else {
        Parent::Top
    };
    let fits = match (&ctx, &d.home) {
        (Parent::Section(s), Parent::Section(h)) => s == h,
        (Parent::Section(_), _) | (_, Parent::Section(_)) => false,
        (Parent::Folder(_), _) => !d.whole,
        _ => true,
    };
    if !fits {
        return None;
    }
    let in_ctx = |k: usize| row_parent(&rows[k]).0 == ctx;
    let to = if let Some(k) = a.filter(|&k| in_ctx(k)) {
        crate::doc::Anchor::Below(row_ends(&rows[k])?.1)
    } else if let Some(k) = b.filter(|&k| in_ctx(k)) {
        crate::doc::Anchor::Above(row_ends(&rows[k])?.0)
    } else if let Some(k) = a.filter(|&k| holds(&rows[k], &ctx)) {
        // Right under a folder or section with nothing else left in it.
        crate::doc::Anchor::Below(row_ends(&rows[k])?.1)
    } else {
        crate::doc::Anchor::Bottom
    };
    let group = match &ctx {
        Parent::Folder(f) => Some(Some(f.clone())),
        Parent::Section(_) => None,
        _ if d.whole => None,
        _ => Some(None),
    };
    let depth = match (a.filter(|&k| in_ctx(k)), b.filter(|&k| in_ctx(k))) {
        (_, Some(k)) | (Some(k), None) => row_parent(&rows[k]).1,
        (None, None) => a.map_or(0, |k| row_parent(&rows[k]).1 + 1),
    };
    Some(Drop { to, group, line: Some((y, indent(depth))), into: None })
}

/* ---------- templates ---------- */

/// The templates; true when one was added.
fn templates(ui: &mut Ui, ed: &mut Editor, t: &Theme) -> bool {
    let mut added = false;
    ui.add_space(8.0);
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for tpl in crate::templates::TEMPLATES.iter() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click_and_drag());
            let r = rect.shrink2(vec2(8.0, 4.0));
            ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
            let art = Rect::from_min_size(r.min + vec2(6.0, 6.0), vec2(80.0, 52.0));
            template_art(ui, Some(tpl), art);
            let tx = art.max.x + 10.0;
            ui.painter().text(pos2(tx, r.min.y + 18.0), egui::Align2::LEFT_CENTER, tpl.name, ui::medium(ui::px(11.0)), t.text);
            let g = ui.painter().layout(tpl.description.to_string(), egui::FontId::proportional(ui::px(10.0)), t.text2, r.max.x - tx - 6.0);
            ui.painter().galley(pos2(tx, r.min.y + 28.0), g, t.text2);
            if resp.drag_started() {
                super::toolbar::start_drag(ui.ctx(), super::toolbar::Payload::Template(tpl.id), tpl.name, "layout-template");
            } else if resp.clicked() {
                ed.insert((tpl.build)(), None, true, true);
                added = true;
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
    });
    added
}

/* ---------------- right panel ---------------- */

pub fn right(ui: &mut Ui, b: &mut BoardScreen, toasts: &mut Toasts) {
    let t = ui::theme(ui.ctx());
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), ui::ctl(48.0)), Sense::hover());
    ui.painter().hline(head.x_range(), head.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 8.0))), |ui| {
        ui.horizontal_centered(|ui| {
            participants(ui, &mut b.editor);
            // As in FigJam: the timer (and music) beside Share.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_min_height(ui.available_height());
                share_button(ui, b);
                ui.add_space(4.0);
                super::live::timer_button(ui, &mut b.editor);
            });
        });
    });
    let (sub, _) = ui.allocate_exact_size(vec2(ui.available_width(), ui::ctl(40.0)), Sense::hover());
    ui.painter().hline(sub.x_range(), sub.max.y - 0.5, Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(sub.shrink2(vec2(8.0, 8.0))), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(4.0);
            let title = match b.editor.tool {
                _ if b.editor.read_only => "Design",
                Tool::Pen => "Penna",
                Tool::Highlighter => "Evidenziatore",
                Tool::Eraser => "Gomma",
                Tool::Tape => "Nastro adesivo",
                _ => "Design",
            };
            ui.label(RichText::new(title).font(ui::medium(ui::px(11.0))).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_min_height(ui.available_height());
                if small_icon_button(ui, "panel-right-close", "Nascondi il pannello", false).clicked() {
                    b.ui.peek_right = false;
                    b.editor.prefs.right_panel = false;
                }
                zoom_menu(ui, &mut b.editor, &t);
                super::live::spotlight_button(ui, &mut b.editor);
                super::live::vote_button(ui, &mut b.editor);
            });
        });
    });
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if b.editor.read_only {
            section(ui, Some("Solo visione"), |ui| ui::hint(ui, "Sola lettura"));
        } else if inked(b.editor.tool) {
            tool_props(ui, &mut b.editor, &t);
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
    let g = ui.painter().layout_no_wrap(label.clone(), egui::FontId::proportional(ui::px(11.0)), t.text);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 26.0, ui::hit(ui.ctx(), ui::ctl(24.0))), Sense::click());
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
        let h = ui::motion::hover(ui.ctx(), resp.id.with("ring"), resp.hovered());
        if following || h > 0.0 {
            ui.painter().circle_stroke(resp.rect.center(), 13.5, Stroke::new(2.0, color.gamma_multiply(if following { 1.0 } else { h })));
        }
        let tip = match id {
            None => format!("{name}: il tuo profilo"),
            Some(_) if following => format!("Smetti di seguire {name}"),
            Some(_) => format!("Segui {name}"),
        };
        let resp = ui::tip(resp, &tip, None);
        match id {
            Some(id) => {
                if resp.clicked() {
                    ed.follow(if following { None } else { Some(*id) });
                }
            }
            None => profile_popup(ui, &resp, ed),
        }
    }
    if list.len() > 4 {
        ui.label(RichText::new(format!("+{}", list.len() - 4)).size(ui::px(10.0)));
    }
}

/// The little arrow by the avatars: who is here (a click follows them), presenting, emotes.
fn people_menu(ui: &mut Ui, ed: &mut Editor, alone: bool) {
    let t = ui::theme(ui.ctx());
    // Alone (a phone, where the avatars don't fit): the people's icon with the arrow, so the
    // arrow doesn't hang there pointing at nothing.
    let w = if alone { 34.0 } else { 16.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, ui::hit(ui.ctx(), ui::ctl(24.0))), Sense::click());
    ui::hover_fill(ui, resp.id, r, resp.hovered(), 4.0, t.hover);
    if alone {
        ui::icon(ui, "users", pos2(r.min.x + 11.0, r.center().y), 16.0, t.icon);
        ui::icon(ui, "chevron-down", pos2(r.max.x - 8.0, r.center().y), 12.0, t.icon2);
    } else {
        ui::icon(ui, "chevron-down", r.center(), 12.0, t.icon2);
    }
    let resp = ui::tip(resp, "Persone, presentazione ed emote", None);
    let edit_id = Id::new("people-menu-edit-profile");
    egui::Popup::from_toggle_button_response(&resp).frame(ui::menu_frame(&t)).align(egui::RectAlign::BOTTOM_END).gap(8.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| ui::menu_body(ui, |ui| {
        ui.set_min_width(240.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let me = serde_json::Value::Object(ed.presence.local.clone());
        let mut editing = ui.data(|d| d.get_temp::<bool>(edit_id)).unwrap_or(false);
        if ui::menu_item(ui, Some("user"), &format!("{} (tu)", peer_name(&me)), None, editing).clicked() {
            editing = !editing;
            ui.data_mut(|d| d.insert_temp(edit_id, editing));
        }
        if editing {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(220.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                profile_fields(ui, ed);
            });
            ui::menu_sep(ui);
        }
        let people: Vec<(u64, String)> = ed.presence.peers.iter().filter(|(_, s)| s["user"].is_object()).map(|(id, s)| (*id, peer_name(s))).collect();
        for (id, name) in people {
            let following = ed.following == Some(id);
            if ui::menu_item(ui, Some(if following { "eye" } else { "user" }), &if following { format!("Smetti di seguire {name}") } else { format!("Segui {name}") }, None, false).clicked() {
                ed.follow(if following { None } else { Some(id) });
            }
        }
        ui::menu_sep(ui);
        let on = ed.presence.local.get("spotlight").and_then(|v| v.as_f64()).is_some();
        if ui::menu_item(ui, Some("presentation"), if on { "Smetti di presentare" } else { "Presenta: tutti seguono te" }, None, false).clicked() {
            ed.presence.set("spotlight", if on { serde_json::Value::Null } else { serde_json::json!(crate::platform::now_ms()) });
        }
        ui::menu_sep(ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            for e in crate::editor::EMOTES {
                let (r, b) = ui.allocate_exact_size(vec2(28.0, 32.0), Sense::click());
                let h = ui::motion::hover(ui.ctx(), b.id, b.hovered());
                ui.painter().rect_filled(r, 6.0, t.brand_fill.gamma_multiply(h * 0.6));
                if let Some(Ok(tex)) = ed.painter.images.texture(ui.ctx(), &crate::prims::ImageKey::Stamp(e.to_string())) {
                    let s = 22.0 + 3.0 * h;
                    ui.painter().image(tex, Rect::from_center_size(r.center(), vec2(s, s)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                if b.on_hover_text("Emote (o tieni premuto E)").clicked() {
                    let at = ed.viewport_center();
                    ed.send_emote(e, at);
                }
            }
        });
        ui.add_space(4.0);
    }));
}

/// Colours a person can pick for themselves.
pub const PERSON_COLORS: [&str; 10] = ["#0D99FF", "#9747FF", "#F24822", "#14AE5C", "#FFA629", "#E84393", "#00B5CE", "#7B61FF", "#FF7262", "#1E1E1E"];

fn profile_fields(ui: &mut Ui, ed: &mut Editor) {
    ui::field_label(ui, "Nome");
    ui::text_field(ui, &mut ed.prefs.name, "Come ti vedranno gli altri", 40);
    ui::field_label(ui, "Il tuo colore");
    let cur = ed.prefs.own_color().unwrap_or("").to_string();
    if let Some(c) = ui::color_dots(ui, &PERSON_COLORS, &cur, "Il tuo colore") {
        ed.prefs.color = c;
    }
    ui::field_label(ui, "Puntatore laser");
    if let Some(c) = ui::color_dots(ui, &super::toolbar::LASER_COLORS, &ed.prefs.laser_color, "Colore del puntatore laser") {
        ed.prefs.laser_color = c;
    }
}

/// The profile, under your avatar: the name and colour the others see (cursor, selection,
/// comments), and the laser's colour.
fn profile_popup(ui: &Ui, resp: &egui::Response, ed: &mut Editor) {
    let t = ui::theme(ui.ctx());
    let align = if resp.rect.center().x > ui.ctx().content_rect().center().x { egui::RectAlign::BOTTOM_END } else { egui::RectAlign::BOTTOM_START };
    egui::Popup::from_toggle_button_response(resp).frame(ui::float_frame(&t).inner_margin(egui::Margin::same(14))).align(align).gap(8.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        let max_h = (ui.ctx().content_rect().height() - 96.0).max(160.0);
        ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
            ui.set_width(244.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui::heading(ui, "Il tuo profilo");
            profile_fields(ui, ed);
            ui.add_space(2.0);
            if ui::full_button(ui, "Salva", Btn::Primary, Some("check")).clicked() {
                egui::Popup::close_all(ui.ctx());
            }
        });
    });
}

fn share_button(ui: &mut Ui, b: &mut BoardScreen) {
    if !b.host {
        // A guest's permission: written out where there is room, an icon on a phone.
        let t = ui::theme(ui.ctx());
        let text = if b.editor.read_only { "Solo visione" } else { "Puoi modificare" };
        if ui::narrow(ui.ctx()) {
            let s = ui::hit(ui.ctx(), ui::ctl(24.0));
            let (r, resp) = ui.allocate_exact_size(vec2(s, s), Sense::hover());
            icon(ui, if b.editor.read_only { "eye" } else { "pencil" }, r.center(), 15.0, t.icon2);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
            ui::tip(resp, text, None);
        } else {
            ui.label(RichText::new(text).size(ui::px(10.0)).color(t.text2));
        }
        return;
    }
    let sharing = b.sharing();
    let label = if sharing { "Condivisa" } else { "Condividi" };
    // On a phone just its icon: the words pushed the pill over the one on the left.
    if ui::narrow(ui.ctx()) {
        let t = ui::theme(ui.ctx());
        let s = ui::hit(ui.ctx(), ui::ctl(24.0));
        let (r, resp) = ui.allocate_exact_size(vec2(s, s), Sense::click());
        let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
        let (fill, fg) = if sharing { (ui::blend(t.bg, t.hover, k), t.icon) } else { (ui::blend(t.brand_fill, t.brand_fill_hover, k), Color32::WHITE) };
        ui.painter().rect_filled(r, ui::RADIUS, fill);
        if sharing {
            ui.painter().rect_stroke(r, ui::RADIUS, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        }
        icon(ui, "share-2", r.center(), 15.0, fg);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        ui::focus_ring(ui, &resp, r);
        if ui::tip(resp, label, None).clicked() {
            b.ui.share = true;
        }
        return;
    }
    if ui::button(ui, label, if sharing { Btn::Secondary } else { Btn::Primary }, Some("share-2"), false, true).clicked() {
        b.ui.share = true;
    }
}

fn section(ui: &mut Ui, title: Option<&str>, body: impl FnOnce(&mut Ui)) {
    let t = ui::theme(ui.ctx());
    egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 16 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        if let Some(title) = title {
            ui.label(RichText::new(title).font(ui::medium(ui::px(11.0))).color(t.text));
            ui.add_space(2.0);
        }
        body(ui);
    });
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
}

/* ---------- a drawing tool in hand: how it draws ---------- */

/// The tools whose settings the right panel shows while they are in hand.
fn inked(tool: Tool) -> bool {
    matches!(tool, Tool::Pen | Tool::Highlighter | Tool::Eraser | Tool::Tape)
}

/// A setting with its name on the left and its control on the right, on one line.
fn setting_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let t = ui::theme(ui.ctx());
    ui::row(ui, ui::ctl(28.0), |ui| {
        ui.label(RichText::new(label).size(ui::px(11.0)).color(t.text2));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
}

fn tool_props(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    use super::toolbar::{PCT, PX};
    let w = ui.available_width() - 32.0;
    match ed.tool {
        Tool::Pen => pen_props(ui, ed, t),
        Tool::Highlighter => {
            let p = &mut ed.prefs.highlighter;
            section(ui, Some("Colore"), |ui| {
                if let Some(c) = ui::swatches(ui, &HIGHLIGHT_COLORS, &p.color, false) {
                    p.color = c;
                }
            });
            section(ui, Some("Tratto"), |ui| {
                ui::slider(ui, "Spessore", &mut p.size, 4.0, 96.0, 0.5, true, PX, w);
                ui::slider(ui, "Opacità", &mut p.opacity, 0.1, 1.0, 0.05, false, PCT, w);
            });
        }
        Tool::Tape => {
            let p = &mut ed.prefs.tape;
            section(ui, Some("Colore"), |ui| {
                if let Some(c) = ui::swatches(ui, &TAPE_COLORS, &p.color, false) {
                    p.color = c;
                }
            });
            section(ui, Some("Nastro"), |ui| {
                ui::slider(ui, "Larghezza", &mut p.size, 8.0, 120.0, 0.5, true, PX, w);
            });
        }
        Tool::Eraser => {
            let p = &mut ed.prefs.eraser;
            section(ui, Some("Cancella"), |ui| {
                ui::segmented(ui, &mut p.mode, &[(prefs::EraserMode::Pixel, "Dove passa"), (prefs::EraserMode::Stroke, "Tratto intero")], w);
                ui::slider(ui, "Dimensione", &mut p.size, 2.0, 240.0, 0.5, true, PX, w);
                if p.mode == prefs::EraserMode::Pixel {
                    ui::slider(ui, "Forza", &mut p.strength, 0.1, 1.0, 0.05, false, PCT, w);
                }
            });
        }
        _ => {}
    }
    section(ui, None, |ui| {
        ui::hint(ui, "Alt + trascina: spessore  ·  col tasto destro: opacità  ·  Alt + clic: prendi un colore  ·  [ e ]: spessore");
    });
}

/// The pen in use: how it looks, its brush, colour and stroke, and how it follows the hand.
fn pen_props(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    use super::toolbar::{PCT, PX, brush_sample, brush_sizes, set_brush, size_range};
    if ed.prefs.pens.is_empty() {
        return;
    }
    let i = ed.pen.min(ed.prefs.pens.len() - 1);
    let w = ui.available_width() - 32.0;
    // In the pen's colour, unless it would hardly show on the panel.
    let shown_color = |c: &str, op: f64| {
        let c = color_or(c, DARK);
        let luma = (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0;
        let c = if (t.dark && luma < 0.2) || (!t.dark && luma > 0.85) { t.text2 } else { c };
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (op.clamp(0.1, 1.0) * 255.0) as u8)
    };
    // The stroke as it will come out, large.
    section(ui, None, |ui| {
        let pen = &ed.prefs.pens[i];
        let (r, _) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
        ui.painter().rect_filled(r, 10.0, t.bg2);
        brush_sample(ui, r.shrink2(vec2(10.0, 6.0)), pen.brush, shown_color(&pen.color, pen.opacity), pen.size.clamp(1.0, 26.0));
        ui.painter().text(r.left_top() + vec2(10.0, 9.0), egui::Align2::LEFT_TOP, format!("{} {}", pen.brush.name(), i + 1), ui::medium(ui::px(10.0)), t.text2);
    });
    section(ui, Some("Pennello"), |ui| {
        let pen = &mut ed.prefs.pens[i];
        let gap = 6.0;
        let tile = vec2(((w - gap) / 2.0).floor(), 50.0);
        egui::Grid::new("pen-brushes").spacing(vec2(gap, gap)).show(ui, |ui| {
            for (n, (brush, name)) in BRUSHES.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(tile, Sense::click());
                let on = pen.brush == *brush;
                let sel = ui::motion::hover(ui.ctx(), resp.id.with("sel"), on);
                ui.painter().rect_filled(r, 8.0, ui::blend(t.bg2, t.selected, sel));
                ui::hover_fill(ui, resp.id, r, resp.hovered() && !on, 8.0, t.hover);
                if on {
                    ui.painter().rect_stroke(r, 8.0, Stroke::new(1.5, t.brand), egui::StrokeKind::Inside);
                }
                let (_, _, start) = brush_sizes(*brush);
                let sample = Rect::from_min_max(r.min + vec2(4.0, 2.0), pos2(r.max.x - 4.0, r.max.y - 16.0));
                brush_sample(ui, sample, *brush, shown_color(&pen.color, 1.0), if *brush == Brush::Airbrush { 12.0 } else { start.min(7.0) });
                ui.painter().text(pos2(r.center().x, r.max.y - 9.0), egui::Align2::CENTER_CENTER, *name, egui::FontId::proportional(ui::px(10.0)), if on { t.text } else { t.text2 });
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, *name));
                if ui::tip(resp, name, None).clicked() {
                    set_brush(pen, *brush);
                }
                if n % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    });
    section(ui, Some("Colore"), |ui| {
        let pen = &mut ed.prefs.pens[i];
        if let Some(c) = ui::swatches(ui, &INK_COLORS, &pen.color, false) {
            pen.color = c;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(c) = ui::color_well(ui, "Colore personalizzato", color_or(&pen.color, DARK)) {
                pen.color = hex(c);
            }
            let on = ed.eyedropper;
            if ui::button(ui, "Prendi dalla lavagna", if on { Btn::Primary } else { Btn::Secondary }, Some("droplet"), false, true).clicked() {
                ed.eyedropper = !on;
            }
        });
    });
    let p = &mut ed.prefs;
    section(ui, Some("Tratto"), |ui| {
        let pen = &mut p.pens[i];
        let (lo, hi) = size_range(pen.brush);
        ui::slider(ui, "Spessore", &mut pen.size, lo, hi, 0.5, true, PX, w);
        ui::slider(ui, "Opacità", &mut pen.opacity, 0.1, 1.0, 0.05, false, PCT, w);
        ui::slider(ui, "Stabilizzatore", &mut p.streamline, 0.0, 1.0, 0.05, false, PCT, w);
    });
    section(ui, Some("Pressione e inclinazione"), |ui| {
        ui::switch(ui, &mut p.pressure, "Lo spessore segue la pressione");
        if p.pressure {
            setting_row(ui, "Sensibilità", |ui| {
                ui::segmented(ui, &mut p.sensitivity, &[(prefs::Sensitivity::Light, "Leggera"), (prefs::Sensitivity::Normal, "Normale"), (prefs::Sensitivity::Firm, "Decisa")], 150.0);
            });
        }
        ui::switch(ui, &mut p.tilt_shading, "Inclinata, la penna sfuma più largo");
    });
    section(ui, Some("Aiuti"), |ui| {
        ui::switch(ui, &mut p.quick_shape, "Forma perfetta tenendo ferma la penna");
        ui::switch(ui, &mut p.ink_to_shape, "Trasforma i disegni in forme modificabili");
        ui::switch(ui, &mut p.finger_draw, "Le dita disegnano (non spostano la lavagna)");
        ui.add_space(2.0);
        ui.label(RichText::new("Simmetria").size(ui::px(11.0)).color(t.text2));
        ui::segmented(ui, &mut p.symmetry, &[(prefs::Symmetry::Off, "No"), (prefs::Symmetry::Vertical, "Vert."), (prefs::Symmetry::Horizontal, "Oriz."), (prefs::Symmetry::Quad, "4"), (prefs::Symmetry::Radial, "8")], w);
    });
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
        ui::hint(ui, "Tutta la lavagna come immagine PNG, SVG o PDF.");
        if ui::full_button(ui, "Esporta lavagna…", Btn::Secondary, Some("download")).clicked() {
            b.ui.export = Some(false);
        }
    });
}

/// A small grey title over a group of controls inside a section.
fn sub_label(ui: &mut Ui, text: &str) {
    let t = ui::theme(ui.ctx());
    ui.label(RichText::new(text).size(ui::px(10.5)).color(t.text2));
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
    sub_label(ui, "Colore");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (name, value) in BACKGROUNDS {
            let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            ui.painter().circle_filled(r.center(), 11.0, parse_color(value).unwrap());
            // An edge that shows on either theme, so dark colours don't vanish into a dark panel.
            ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(1.0, if t.dark { Color32::from_white_alpha(46) } else { Color32::from_black_alpha(40) }));
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
    ui.add_space(4.0);
    sub_label(ui, "Motivo");
    let avail = ui.available_width();
    let cols = 3;
    let spacing = 6.0;
    let tile_w = ((avail - (cols - 1) as f32 * spacing) / cols as f32).floor().max(62.0);
    let tile_h = (tile_w * 0.72).clamp(50.0, 64.0);
    let art_h = tile_h - 16.0;
    egui::Grid::new(ui.id().with("patterns")).spacing(vec2(spacing, spacing)).show(ui, |ui| {
        for (i, p) in PATTERNS.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(tile_w, tile_h), Sense::click());
            let art = Rect::from_min_size(r.min, vec2(tile_w, art_h));
            let mut shapes = Vec::new();
            let m = BoardMeta { pattern: *p, grid_size: 14.0, ..meta.clone() };
            crate::paint::background(&mut shapes, art, &crate::geom::Camera { x: 7.0, y: 9.0, z: 1.0 }, &m, ui.ctx().pixels_per_point());
            ui.painter().with_clip_rect((art).intersect(ui.clip_rect())).extend(shapes);
            // The pattern is drawn square: the panel's colour rounds its corners off.
            ui.painter().rect_stroke(art, ui::RADIUS, Stroke::new(3.0, t.bg), egui::StrokeKind::Outside);
            let on = meta.pattern == *p;
            ui.painter().rect_stroke(art, ui::RADIUS, Stroke::new(if on { 2.0 } else { 1.0 }, if on { t.brand } else { t.border }), egui::StrokeKind::Inside);
            ui.painter().text(pos2(r.center().x, r.max.y - 7.0), egui::Align2::CENTER_CENTER, pattern_label(*p), egui::FontId::proportional(ui::px(10.0)), t.text2);
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
        ui.add_space(4.0);
        sub_label(ui, "Passo del motivo");
        let mut g = meta.grid_size as i64;
        if ui::segmented(ui, &mut g, &[(12, "Fitto"), (24, "Medio"), (48, "Ampio")], ui.available_width()) {
            meta.grid_size = g as f64;
        }
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
                ui.label(RichText::new(&label).font(ui::medium(ui::px(11.0))).color(t.text));
                if label != type_label(e) {
                    ui.label(RichText::new(type_label(e)).color(t.text2));
                }
            }
            None => {
                ui.label(RichText::new(format!("{} elementi", els.len())).font(ui::medium(ui::px(11.0))).color(t.text));
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
            if let Some(v) = ui::Num::new(Id::new("px"), "X", "Posizione X", Some(x)).width(w).show(ui) {
                set = Some((Some(v), None, None, None));
            }
            if let Some(v) = ui::Num::new(Id::new("py"), "Y", "Posizione Y", Some(y)).width(w).show(ui) {
                set = Some((None, Some(v), None, None));
            }
        });
        ui.horizontal(|ui| {
            if let Some(v) = ui::Num::new(Id::new("pw"), "L", "Larghezza", Some(bw)).width(w).range(1.0, f64::INFINITY).show(ui) {
                set = Some((None, None, Some(v), None));
            }
            if let Some(v) = ui::Num::new(Id::new("ph"), "A", "Altezza", Some(bh)).width(w).range(1.0, f64::INFINITY).show(ui) {
                set = Some((None, None, None, Some(v)));
            }
        });
        let radii: Vec<f64> = els.iter().filter_map(|e| crate::style::radius(e)).collect();
        let round = !radii.is_empty() && radii.len() == els.len();
        ui.horizontal(|ui| {
            if let Some(e) = &one {
                let deg = (e.rotation.to_degrees() % 360.0 + 360.0) % 360.0;
                if let Some(v) = ui::Num::new(Id::new("prot"), "icon:rotate-cw", "Rotazione", Some(deg)).width(w).suffix("°").show(ui) {
                    let v = v.rem_euclid(360.0);
                    apply(ed, &mut |el| el.rotation = v.to_radians());
                }
            }
            if round {
                let cur = radii.windows(2).all(|p| (p[0] - p[1]).abs() < 0.01).then(|| radii[0]);
                let max = els.iter().map(|e| crate::style::max_radius(e)).fold(f64::INFINITY, f64::min);
                if let Some(v) = ui::Num::new(Id::new("pradius"), "icon:corner-radius", "Raggio degli angoli", cur.map(|r| r.min(max).round())).width(w).range(0.0, max.round()).show(ui) {
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
                    if let Some(w) = ui::Num::new(Id::new("pstroke"), "Sp", "Spessore", v).decimals(1).step(0.5).range(0.1, 200.0).width(96.0).show(ui) {
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
                ui.spacing_mut().item_spacing.x = 2.0;
                let start = same(&els, |e| e.line().map(|l| l.arrow_start.then_some(l.head_start)));
                let end = same(&els, |e| e.line().map(|l| l.arrow_end.then_some(l.head_end)));
                if let Some(h) = super::toolbar::end_picker(ui, start, true, 24.0, t) {
                    super::toolbar::set_end(ed, &ids, true, h);
                }
                if small_icon_button(ui, "arrow-right-left", "Inverti la direzione", false).clicked() {
                    super::toolbar::swap_ends(ed, &ids);
                }
                if let Some(h) = super::toolbar::end_picker(ui, end, false, 24.0, t) {
                    super::toolbar::set_end(ed, &ids, false, h);
                }
                ui.add_space(8.0);
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
    if els.iter().all(|e| crate::style::can_shade(e)) && !locked_all(&els) {
        shadow_props(ui, ed, &els, &ids);
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
            if let Some(v) = ui::Num::new(Id::new("popacity"), "icon:droplet", "Opacità", op).suffix("%").range(0.0, 100.0).width(84.0).show(ui) {
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
            let title = if auto { "Dimensione del testo (automatica)" } else { "Dimensione del testo" };
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

fn locked_all(els: &[Arc<El>]) -> bool {
    els.iter().all(|e| e.locked)
}

/// The shadow of notes, shapes, tables, code, widgets and pictures: on or off, how dark, how
/// soft and how far below.
fn shadow_props(ui: &mut Ui, ed: &mut Editor, els: &[Arc<El>], ids: &[String]) {
    let shades: Vec<Option<Shade>> = els.iter().map(|e| crate::style::shade(e)).collect();
    let mut on = shades.iter().all(Option::is_some);
    let first = shades.iter().flatten().next().copied().unwrap_or(Shade { opacity: 0.16, blur: 12.0, y: 4.0 });
    let mut change: Option<Shade> = None;
    section(ui, Some("Ombra"), |ui| {
        if ui::switch(ui, &mut on, "Ombra sotto l'elemento") {
            change = Some(if on { Shade { opacity: first.opacity.max(0.05), ..first } } else { Shade { opacity: 0.0, ..first } });
        }
        if on {
            let w = ui.available_width();
            let mut s = first;
            let mut density = s.opacity * 100.0;
            if ui::slider(ui, "Densità", &mut density, 2.0, 100.0, 1.0, false, |v| format!("{}%", v.round()), w) {
                s.opacity = density / 100.0;
                change = Some(s);
            }
            if ui::slider(ui, "Sfumatura", &mut s.blur, 0.0, 80.0, 1.0, false, |v| format!("{}", v.round()), w) {
                change = Some(s);
            }
            if ui::slider(ui, "Distanza", &mut s.y, -20.0, 60.0, 1.0, false, |v| format!("{}", v.round()), w) {
                change = Some(s);
            }
            if els.iter().any(|e| e.shadow.is_some()) && ui::button(ui, "Ombra di base", Btn::Ghost, Some("rotate-ccw"), false, true).clicked() {
                ed.board.update(ids, |el| el.shadow = None);
            }
        }
    });
    if let Some(s) = change {
        ed.board.update(ids, |el| el.shadow = Some(s));
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
            if let Some(v) = ui::Num::new(Id::new("tcols"), "Colonne", "Colonne", Some(tb.cols.len() as f64)).range(1.0, 50.0).width(w).show(ui) {
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
            _ => {}
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
    let is_mobile = stage.width() < 600.0;
    let k = ui::motion::appear(ctx, Id::new("focus-pill-anim"), true);
    let lift = ui::motion::travel(ctx, k) * 6.0;
    let mut action = None;
    let pill = egui::Area::new(Id::new("focus-pill")).fixed_pos(stage.min + vec2(12.0, 12.0 + lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui::float_frame(&t).inner_margin(egui::Margin::symmetric(if is_mobile { 6 } else { 8 }, 6)).show(ui, |ui| {
            let row_h = ui::hit(ui.ctx(), ui::ctl(24.0));
            ui::row(ui, row_h, |ui| {
                ui.spacing_mut().item_spacing.x = if is_mobile { 4.0 } else { 6.0 };
                action = main_menu(ui, b, dialogs);
                if !is_mobile {
                    ui.label(RichText::new(&b.title).font(ui::medium(ui::px(11.0))).color(t.text));
                }
                ui.spacing_mut().item_spacing.x = 2.0;
                // On a phone zen mode and the settings are in the logo's menu: the two pills at
                // the top must fit side by side.
                if !b.host && !is_mobile {
                    guest_buttons(ui, b, dialogs);
                }
                let focus = b.editor.prefs.focus;
                if ui::icon_button(ui, "panel-left-open", if focus { "Mostra i pannelli" } else { "Mostra il pannello dei livelli" }, focus.then_some("Ctrl+\\"), vec2(row_h, row_h), 16.0, false, false).clicked() {
                    if focus {
                        b.editor.toggle_focus();
                    } else {
                        // In a narrow window it shows over the board (a phone has room for one
                        // panel at a time).
                        b.editor.prefs.left_panel = true;
                        b.ui.peek_left = true;
                        if ui::narrow(ui.ctx()) {
                            b.ui.peek_right = false;
                        }
                    }
                }
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("left-pill-rect"), pill.response.rect));
    action
}

/// The logo with its menu, as in FigJam: the board, what to add or export, what to show, and
/// the way to the settings.
fn main_menu(ui: &mut Ui, b: &mut BoardScreen, dialogs: &mut Dialogs) -> Option<Action> {
    let t = ui::theme(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(38.0, ui::hit(ui.ctx(), ui::ctl(24.0))), Sense::click());
    ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
    ui::icons::logo(ui, Rect::from_center_size(r.center() - vec2(6.0, 0.0), vec2(18.0, 18.0)));
    icon(ui, "chevron-down", r.center() + vec2(12.0, 0.0), 10.0, t.icon2);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Menu principale"));
    ui::focus_ring(ui, &resp, r);
    let resp = ui::tip(resp, "Menu principale", None);
    let mut action = None;
    egui::Popup::menu(&resp).frame(ui::menu_frame(&t)).show(|ui| ui::menu_body(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.set_min_width(240.0);
        if b.host {
            if ui::menu_item(ui, Some("home"), "Torna alle lavagne", None, false).clicked() {
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
        // What is on screen now (a narrow window shows a panel only when asked, one at a time on a phone).
        let (left_on, right_on) = ui.ctx().data(|d| d.get_temp::<(bool, bool)>(Id::new("panels-shown"))).unwrap_or((false, false));
        let phone = ui::narrow(ui.ctx());
        let check = |on: bool, icon: &'static str| Some(if on { "check" } else { icon });
        if ui::menu_item(ui, check(left_on, "panel-left-open"), "Pannello dei livelli", None, false).clicked() {
            (b.editor.prefs.left_panel, b.ui.peek_left, b.editor.prefs.focus) = (!left_on, !left_on, false);
            if phone && !left_on {
                b.ui.peek_right = false;
            }
        }
        if ui::menu_item(ui, check(right_on, "panel-right-open"), "Pannello Design", None, false).clicked() {
            (b.editor.prefs.right_panel, b.ui.peek_right, b.editor.prefs.focus) = (!right_on, !right_on, false);
            if phone && !right_on {
                b.ui.peek_left = false;
            }
        }
        let p = &mut b.editor.prefs;
        if ui::menu_item(ui, check(p.minimap, "map"), "Minimappa", None, false).clicked() {
            p.minimap = !p.minimap;
        }
        if ui::menu_item(ui, Some("scan"), "Modalità zen", Some("Alt+Z"), false).clicked() {
            b.editor.toggle_zen();
        }
        ui::menu_sep(ui);
        if ui::menu_item(ui, Some("settings"), "Impostazioni e personalizzazione…", Some("Ctrl+,"), false).clicked() {
            dialogs.settings = true;
        }
        if ui::menu_item(ui, Some("keyboard"), "Scorciatoie da tastiera", Some("?"), false).clicked() {
            b.ui.shortcuts = true;
        }
    }));
    action
}

/// Zen mode's way out: a quiet button in the corner that wakes up under the pointer.
pub fn zen_pill(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let t = ui::theme(ctx);
    let k = ui::motion::appear(ctx, Id::new("zen-pill"), true);
    // Top right, faint until the pointer comes close (on a touch screen, always readable).
    let corner = pos2(stage.max.x - 16.0, stage.min.y + 16.0);
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let near = pointer.is_none_or(|p| p.distance(corner) < 180.0);
    let wake = ui::motion::hover(ctx, Id::new("zen-pill-wake"), near);
    egui::Area::new(Id::new("zen-pill")).pivot(egui::Align2::RIGHT_TOP).fixed_pos(corner - vec2(0.0, ui::motion::travel(ctx, k) * 10.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0) * (0.3 + 0.7 * wake));
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            let r = ui::button(ui, "Esci da zen", Btn::Ghost, Some("minimize-2"), false, true);
            if ui::tip(r, "Esci dalla modalità zen", Some("Esc · Alt+Z")).clicked() {
                ed.prefs.zen = false;
            }
        });
    });
}

/// Right panel closed: people, share and the way back, floating at the top right of `stage`
/// (the board as it will be once the panels have finished moving), beside `mini`, where the
/// minimap rests, when that is the top right corner.
pub fn right_pill(ctx: &egui::Context, stage: Rect, mini: Option<Rect>, b: &mut BoardScreen) {
    let t = ui::theme(ctx);
    let is_mobile = stage.width() < 600.0;
    let x = match mini {
        Some(m) if m.min.y < stage.min.y + 64.0 && m.max.x > stage.max.x - 120.0 => m.min.x - 8.0,
        _ => stage.max.x - 12.0,
    };
    // Coming back, it is where it belongs straight away: it glides only while it stays, to keep
    // beside the minimap as that comes, goes or changes corner.
    let (glide, anim) = (Id::new("right-pill-x"), Id::new("right-pill-anim"));
    if ui::motion::coming_back(ctx, anim) {
        ui::motion::hold(ctx, glide, x);
    }
    let x = ui::motion::spring(ctx, glide, x, 0.3, 0.9);
    let k = ui::motion::appear(ctx, anim, true);
    let lift = ui::motion::travel(ctx, k) * 6.0;
    let pill = egui::Area::new(Id::new("right-pill")).pivot(egui::Align2::RIGHT_TOP).fixed_pos(pos2(x, stage.min.y + 12.0 + lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui::float_frame(&t).inner_margin(egui::Margin::symmetric(if is_mobile { 6 } else { 8 }, 6)).show(ui, |ui| {
            let row_h = ui::hit(ui.ctx(), ui::ctl(24.0));
            ui::row(ui, row_h, |ui| {
                // FigJam's: the people (and their menu), the templates, the timer, Share.
                if !is_mobile {
                    participants(ui, &mut b.editor);
                }
                people_menu(ui, &mut b.editor, is_mobile);
                ui.spacing_mut().item_spacing.x = 4.0;
                if !is_mobile && !b.editor.read_only {
                    // Clicked again, the templates go away: back to the layers if the panel was
                    // already open, the panel closed if it wasn't.
                    let shown = ctx.data(|d| d.get_temp::<(bool, bool)>(Id::new("panels-shown"))).is_some_and(|s| s.0);
                    let open = shown && b.ui.left_tab == LeftTab::Templates;
                    let back = Id::new("templates-back");
                    if small_icon_button(ui, "layout-template", if open { "Chiudi i modelli" } else { "Modelli" }, open).clicked() {
                        if !open {
                            ctx.data_mut(|d| d.insert_temp(back, shown));
                            b.editor.prefs.left_panel = true;
                            b.ui.peek_left = true;
                            b.editor.prefs.focus = false;
                            b.editor.requests.push(crate::editor::Request::Templates);
                        } else if ctx.data(|d| d.get_temp::<bool>(back)).unwrap_or(false) {
                            b.ui.left_tab = LeftTab::Layers;
                        } else {
                            b.editor.prefs.left_panel = false;
                            b.ui.peek_left = false;
                        }
                    }
                }
                super::live::timer_button(ui, &mut b.editor);
                share_button(ui, b);
                ui.add_space(2.0);
                if small_icon_button(ui, "panel-right-open", "Mostra il pannello", false).clicked() {
                    b.ui.peek_right = true;
                    if ui::narrow(ui.ctx()) {
                        b.ui.peek_left = false;
                    }
                    b.editor.prefs.right_panel = true;
                    b.editor.prefs.focus = false;
                }
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("right-pill-rect"), pill.response.rect));
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
    let k = ui::motion::appear(ctx, Id::new("table-tools-anim"), true);
    let lift = ui::motion::travel(ctx, k) * 6.0;
    egui::Area::new(Id::new("table-tools")).fixed_pos(pos2(x, y + lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui::row(ui, 28.0, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{}{}", col_name(c), r + 1)).font(ui::medium(ui::px(11.0))).color(t.text2));
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
                egui::Popup::menu(&more).frame(ui::menu_frame(&t)).show(|ui| ui::menu_body(ui, |ui| {
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
                }));
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
    let k = ui::motion::appear(ctx, Id::new("code-tools-anim"), true);
    let lift = ui::motion::travel(ctx, k) * 6.0;
    egui::Area::new(Id::new("code-tools")).fixed_pos(pos2(a.x.max(stage.min.x + 8.0), y + lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
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
    let g = ui.painter().layout_no_wrap(name.to_string(), ui::medium(ui::px(11.0)), t.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 24.0), Sense::click());
    let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    ui.painter().rect_filled(rect, ui::RADIUS, t.hover.gamma_multiply(k.max(0.5)));
    ui.painter().galley(pos2(rect.min.x + 8.0, rect.center().y - g.size().y / 2.0), g, t.text);
    ui::icon(ui, "chevron-down", rect.right_center() - vec2(10.0, 0.0), 12.0, t.icon2);
    let resp = ui::tip(resp, "Linguaggio", None);
    let mut picked = None;
    egui::Popup::menu(&resp).frame(ui::menu_frame(t)).show(|ui| ui::menu_body(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.set_max_height(320.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (id, label) in crate::code::LANGUAGES {
                if ui::menu_item(ui, (id == current).then_some("check"), label, None, false).clicked() {
                    picked = Some(id.to_string());
                }
            }
        });
    }));
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
    let k = ui::motion::appear(ctx, Id::new("text-tools-anim"), true);
    let lift = ui::motion::travel(ctx, k) * 6.0;
    egui::Area::new(Id::new("text-tools")).fixed_pos(pos2(x, y + lift)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
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
