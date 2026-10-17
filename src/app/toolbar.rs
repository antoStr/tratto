//! The floating toolbar with its contextual tray, the zoom controls and the minimap.

use egui::{Color32, Id, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::board::BoardUi;
use crate::editor::{Editor, Tool};
use crate::geom::{BBox, union};
use crate::model::{FONTS, FontKind, HIGHLIGHT_COLORS, INK_COLORS, STICKY_COLORS, ShapeKind, TAPE_COLORS};
use crate::prefs::{EraserMode, Pen, ShapeTool, ToolbarPos};
use crate::ui::{self, Theme, icon, icon_button};

/// The toolbar's size (Settings ▸ Personalizzazione: compact, normal, large), set each frame.
static BAR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

fn bar() -> f32 {
    f32::from_bits(BAR.load(std::sync::atomic::Ordering::Relaxed))
}

/// A toolbar button's side, and its icon.
fn btn() -> f32 {
    (40.0 * bar()).round()
}

fn icon_px() -> f32 {
    (22.0 * bar()).round()
}

pub const SHAPES: [(ShapeTool, &str, Option<&str>); 29] = [
    (ShapeTool::Kind(ShapeKind::Rect), "Rettangolo", Some("R")),
    (ShapeTool::RoundRect, "Rettangolo arrotondato", None),
    (ShapeTool::Kind(ShapeKind::Ellipse), "Ellisse", Some("O")),
    (ShapeTool::Kind(ShapeKind::Diamond), "Rombo", None),
    (ShapeTool::Kind(ShapeKind::Triangle), "Triangolo", None),
    (ShapeTool::Kind(ShapeKind::TriangleDown), "Triangolo capovolto", None),
    (ShapeTool::Kind(ShapeKind::Parallelogram), "Parallelogramma", None),
    (ShapeTool::Kind(ShapeKind::Pentagon), "Pentagono", None),
    (ShapeTool::Kind(ShapeKind::Hexagon), "Esagono", None),
    (ShapeTool::Kind(ShapeKind::Octagon), "Ottagono", None),
    (ShapeTool::Kind(ShapeKind::Star), "Stella", None),
    (ShapeTool::Kind(ShapeKind::Plus), "Croce", None),
    (ShapeTool::Kind(ShapeKind::ArrowRight), "Freccia a destra", None),
    (ShapeTool::Kind(ShapeKind::ArrowLeft), "Freccia a sinistra", None),
    (ShapeTool::Kind(ShapeKind::Pill), "Inizio e fine", None),
    (ShapeTool::Kind(ShapeKind::Process), "Processo predefinito", None),
    (ShapeTool::Kind(ShapeKind::Document), "Documento", None),
    (ShapeTool::Kind(ShapeKind::Cylinder), "Database", None),
    (ShapeTool::Kind(ShapeKind::Speech), "Fumetto", None),
    (ShapeTool::Kind(ShapeKind::Chevron), "Gallone", None),
    (ShapeTool::Kind(ShapeKind::Trapezoid), "Trapezio", None),
    (ShapeTool::Kind(ShapeKind::ManualInput), "Inserimento manuale", None),
    (ShapeTool::Kind(ShapeKind::Delay), "Ritardo", None),
    (ShapeTool::Kind(ShapeKind::OffPage), "Connettore fuori pagina", None),
    (ShapeTool::Kind(ShapeKind::Documents), "Più documenti", None),
    (ShapeTool::Kind(ShapeKind::Display), "Display", None),
    (ShapeTool::Kind(ShapeKind::InternalStorage), "Memoria interna", None),
    (ShapeTool::Kind(ShapeKind::Junction), "Giunzione", None),
    (ShapeTool::Kind(ShapeKind::Cloud), "Nuvola", None),
];

/// The paths a connector can take, as FigJam offers them.
/// The shapes in the shape tool's row, as in FigJam's; the others are in the shapes panel.
const QUICK_SHAPES: [ShapeTool; 8] = [
    ShapeTool::Kind(ShapeKind::Rect),
    ShapeTool::Kind(ShapeKind::Ellipse),
    ShapeTool::Kind(ShapeKind::Diamond),
    ShapeTool::Kind(ShapeKind::Triangle),
    ShapeTool::Kind(ShapeKind::TriangleDown),
    ShapeTool::Kind(ShapeKind::Pill),
    ShapeTool::Kind(ShapeKind::Cylinder),
    ShapeTool::RoundRect,
];

/// A connector drawn small: elbow, curved or straight with its head; `None`: a plain line.
fn connector_icon(ui: &Ui, route: Option<crate::model::Route>, c: Pos2, color: Color32) {
    use crate::model::Route;
    let p = ui.painter();
    let st = Stroke::new(1.4, color);
    let head = |tip: Pos2, dir: egui::Vec2| {
        let d = dir.normalized();
        let n = vec2(-d.y, d.x);
        p.line_segment([tip, tip - d * 4.5 + n * 3.5], st);
        p.line_segment([tip, tip - d * 4.5 - n * 3.5], st);
    };
    match route {
        Some(Route::Elbow) => {
            let pts = vec![c + vec2(-7.0, 8.0), c + vec2(-1.0, 8.0), c + vec2(-1.0, -7.0), c + vec2(7.0, -7.0)];
            p.add(egui::Shape::line(pts, st));
            head(c + vec2(7.0, -7.0), vec2(1.0, 0.0));
        }
        Some(Route::Curved) => {
            let (a, b) = (c + vec2(-7.0, 8.0), c + vec2(7.0, -7.0));
            p.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke([a, a + vec2(9.0, 0.0), b + vec2(-9.0, 0.0), b], false, Color32::TRANSPARENT, st)));
            head(b, vec2(1.0, 0.0));
        }
        Some(Route::Straight) => {
            p.line_segment([c + vec2(-7.0, 7.0), c + vec2(7.0, -7.0)], st);
            head(c + vec2(7.0, -7.0), vec2(1.0, -1.0));
        }
        None => {
            p.line_segment([c + vec2(-7.0, 7.0), c + vec2(7.0, -7.0)], st);
        }
    }
}

pub const ROUTES: [(crate::model::Route, &str, &str); 3] = [
    (crate::model::Route::Straight, "route-straight", "Dritta"),
    (crate::model::Route::Elbow, "route-elbow", "A gomito"),
    (crate::model::Route::Curved, "route-curved", "Curva"),
];

/// A short line with the end drawn at its right (or left, for the start): none or a head.
pub fn end_icon(ui: &Ui, c: Pos2, head: Option<crate::model::Head>, start: bool, color: Color32) {
    use crate::model::Head;
    let p = ui.painter();
    let dir = if start { -1.0 } else { 1.0 };
    let tip = c + vec2(8.0 * dir, 0.0);
    let tail = c - vec2(8.0 * dir, 0.0);
    let st = Stroke::new(1.6, color);
    let back = match head {
        Some(Head::Triangle) => 4.0,
        Some(Head::Circle) => 5.0,
        Some(Head::Diamond) => 7.0,
        _ => 0.0,
    };
    p.line_segment([tail, tip - vec2(back * dir, 0.0)], st);
    let at = |b: f32, a: f32| tip + vec2(-b * dir, a);
    match head {
        None => {}
        Some(Head::Triangle) => {
            p.add(egui::Shape::convex_polygon(vec![tip, at(6.0, -4.0), at(6.0, 4.0)], color, Stroke::NONE));
        }
        Some(Head::Open) => {
            p.line_segment([at(5.0, -4.0), tip], st);
            p.line_segment([at(5.0, 4.0), tip], st);
        }
        Some(Head::Circle) => {
            p.circle_filled(at(3.0, 0.0), 3.0, color);
        }
        Some(Head::Diamond) => {
            p.add(egui::Shape::convex_polygon(vec![tip, at(3.5, -3.5), at(7.0, 0.0), at(3.5, 3.5)], color, Stroke::NONE));
        }
        Some(Head::Bar) => {
            p.line_segment([at(0.0, -4.5), at(0.0, 4.5)], st);
        }
    }
}

/// The end of a line: a button showing it, opening the choices (none, or one of the heads).
/// Returns the choice made.
pub fn end_picker(ui: &mut Ui, current: Option<Option<crate::model::Head>>, start: bool, size: f32, t: &Theme) -> Option<Option<crate::model::Head>> {
    let label = if start { "Inizio della linea" } else { "Fine della linea" };
    let r = icon_button(ui, "", label, None, vec2(size + 8.0, size), 16.0, false, false);
    end_icon(ui, r.rect.center() - vec2(3.0, 0.0), current.flatten(), start, t.icon);
    icon(ui, "chevron-down", r.rect.right_center() - vec2(7.0, 0.0), 9.0, t.icon2);
    let mut picked = None;
    egui::Popup::menu(&r).frame(ui::menu_frame(t)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut choices: Vec<(Option<crate::model::Head>, &str)> = vec![(None, "Nessuna")];
        choices.extend(crate::model::HEADS.iter().map(|(h, n)| (Some(*h), *n)));
        for (h, name) in choices {
            let resp = ui::menu_item(ui, None, name, None, false);
            end_icon(ui, pos2(resp.rect.min.x + 14.0, resp.rect.center().y), h, start, t.menu_text);
            if current == Some(h) {
                icon(ui, "check", pos2(resp.rect.max.x - 14.0, resp.rect.center().y), 12.0, t.menu_text);
            }
            if resp.clicked() {
                picked = Some(h);
            }
        }
    });
    picked
}

/// Sets one end of the lines in `ids`: no head, or this head.
pub fn set_end(ed: &mut Editor, ids: &[String], start: bool, h: Option<crate::model::Head>) {
    ed.board.update(ids, |el| {
        if let crate::model::Kind::Line(l) = &mut el.kind {
            match (start, h) {
                (true, None) => l.arrow_start = false,
                (false, None) => l.arrow_end = false,
                (true, Some(h)) => (l.arrow_start, l.head_start) = (true, h),
                (false, Some(h)) => (l.arrow_end, l.head_end) = (true, h),
            }
        }
    });
}

/// Turns lines round: the start becomes the end (joins and heads go with them).
pub fn swap_ends(ed: &mut Editor, ids: &[String]) {
    ed.board.update(ids, |el| {
        if let crate::model::Kind::Line(l) = &mut el.kind {
            if l.points.len() >= 4 {
                l.points = vec![l.points[2], l.points[3], l.points[0], l.points[1]];
            }
            std::mem::swap(&mut l.from, &mut l.to);
            std::mem::swap(&mut l.arrow_start, &mut l.arrow_end);
            std::mem::swap(&mut l.head_start, &mut l.head_end);
            if let Some(s) = &mut l.sides {
                s.swap(0, 1);
            }
        }
    });
}

/// The icon of a shape choice.
pub fn shape_icon(ui: &Ui, tool: ShapeTool, c: Pos2, size: f32, color: Color32) {
    let s = size / 24.0;
    let p = |x: f32, y: f32| c + vec2((x - 12.0) * s, (y - 12.0) * s);
    let st = Stroke::new(2.0 * s.max(0.75), color);
    let name = match tool {
        ShapeTool::RoundRect => {
            ui.painter().rect_stroke(Rect::from_min_max(p(3.0, 5.0), p(21.0, 19.0)), 5.0 * s, st, egui::StrokeKind::Middle);
            return;
        }
        ShapeTool::Kind(ShapeKind::TriangleDown) => {
            ui.painter().add(egui::Shape::closed_line(vec![p(3.0, 4.0), p(21.0, 4.0), p(12.0, 20.0)], st));
            return;
        }
        ShapeTool::Kind(ShapeKind::Parallelogram) => {
            ui.painter().add(egui::Shape::closed_line(vec![p(7.0, 5.0), p(22.0, 5.0), p(17.0, 19.0), p(2.0, 19.0)], st));
            return;
        }
        // The newer shapes draw their own outline, scaled into the icon.
        ShapeTool::Kind(
            k @ (ShapeKind::Pill
            | ShapeKind::Process
            | ShapeKind::Document
            | ShapeKind::Cylinder
            | ShapeKind::Speech
            | ShapeKind::Chevron
            | ShapeKind::Trapezoid
            | ShapeKind::ManualInput
            | ShapeKind::Delay
            | ShapeKind::OffPage
            | ShapeKind::Documents
            | ShapeKind::Display
            | ShapeKind::InternalStorage
            | ShapeKind::Junction
            | ShapeKind::Cloud),
        ) => {
            let (w, h) = if matches!(k, ShapeKind::Cylinder) { (16.0, 19.0) } else { (20.0, 15.0) };
            let o = p(12.0 - w as f32 / 2.0, 12.0 - h as f32 / 2.0);
            let at = |q: &[f64]| q.chunks_exact(2).map(|v| o + vec2(v[0] as f32 * s, v[1] as f32 * s)).collect::<Vec<_>>();
            if let Some(poly) = crate::geom::shape_polygon(k, w, h, None) {
                ui.painter().add(egui::Shape::closed_line(at(&poly), st));
            }
            // Bars a little further in than on the board, or the icon reads as a thick frame.
            let details = if k == ShapeKind::Process { vec![vec![4.5, 0.0, 4.5, h], vec![w - 4.5, 0.0, w - 4.5, h]] } else { crate::geom::shape_details(k, w, h) };
            for d in details {
                ui.painter().add(egui::Shape::line(at(&d), st));
            }
            return;
        }
        ShapeTool::Kind(k) => match k {
            ShapeKind::Rect => "square",
            ShapeKind::Ellipse => "circle",
            ShapeKind::Diamond => "diamond",
            ShapeKind::Triangle => "triangle",
            ShapeKind::Pentagon => "pentagon",
            ShapeKind::Hexagon => "hexagon",
            ShapeKind::Octagon => "octagon",
            ShapeKind::Star => "star",
            ShapeKind::Plus => "plus",
            ShapeKind::ArrowRight => "arrow-big-right",
            ShapeKind::ArrowLeft => "arrow-big-left",
            _ => "square",
        },
    };
    icon(ui, name, c, size, color);
}

/// A thin divider between groups, `h` tall (the row's height).
fn sep_h(ui: &mut Ui, t: &Theme, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, h), Sense::hover());
    let half = (h * 0.28).min(10.0);
    ui.painter().vline(r.center().x, (r.center().y - half)..=(r.center().y + half), Stroke::new(1.0, t.border));
}

fn sep(ui: &mut Ui, t: &Theme) {
    sep_h(ui, t, btn());
}

/// Height of the tray's row for a tool: everything in it sits on one centre line.
const TRAY_H: f32 = 32.0;

/// Something taken from the "+" card or the templates, being dragged onto the board.
#[derive(Clone, PartialEq)]
pub enum Payload {
    /// A note torn off the pad, in the colour of the pad.
    Sticky,
    /// A shape from the shapes panel.
    Shape(ShapeTool),
    Code,
    Poll,
    Checklist,
    Counter,
    Template(&'static str),
}

/// What a payload adds, at `at` (or in the middle of the view).
pub fn insert_payload(ed: &mut Editor, what: &Payload, at: Option<crate::geom::Pt>) {
    use crate::model::{CheckItem, PollOption, Widget};
    match what {
        Payload::Code => ed.insert_code(at),
        Payload::Poll => ed.insert_widget(Widget::Poll { question: String::new(), options: (1..=3).map(|n| PollOption { text: format!("Opzione {n}"), votes: Vec::new() }).collect() }, at),
        Payload::Checklist => ed.insert_widget(Widget::Checklist { title: String::new(), items: (0..3).map(|_| CheckItem { text: String::new(), done: false }).collect() }, at),
        Payload::Counter => ed.insert_widget(Widget::Counter { label: String::new(), value: 0 }, at),
        Payload::Sticky => {
            let p = at.unwrap_or_else(|| ed.viewport_center());
            ed.start_sticky(p);
        }
        Payload::Shape(kind) => {
            let p = at.unwrap_or_else(|| ed.viewport_center());
            ed.place_shape(*kind, p);
        }
        Payload::Template(id) => {
            if let Some(t) = crate::templates::TEMPLATES.iter().find(|t| t.id == *id) {
                match at {
                    Some(p) => ed.insert((t.build)(), Some(p), false, false),
                    None => ed.insert((t.build)(), None, true, true),
                }
            }
        }
    }
}

/// Starts dragging something onto the board: it follows the pointer until let go.
pub fn start_drag(ctx: &egui::Context, what: Payload, label: &str, icon_name: &'static str) {
    ctx.data_mut(|d| d.insert_temp(Id::new("insert-drag"), (what, label.to_string(), icon_name)));
    ui::motion::reset(ctx, Id::new("insert-drag-card"));
    egui::Popup::close_all(ctx);
}

/// The thing being dragged onto the board: a small card under the pointer; let go over the
/// board, it lands there (over a panel or the toolbar, nothing happens).
pub fn insert_drag(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let Some((what, label, icon_name)) = ctx.data(|d| d.get_temp::<(Payload, String, &'static str)>(Id::new("insert-drag"))) else { return };
    let t = ui::theme(ctx);
    let (pos, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
    let Some(pos) = pos else { return };
    // Over the board itself, not over a panel, a bar or a card floating on it.
    let over_board = stage.contains(pos) && ctx.layer_id_at(pos).is_none_or(|l| l.order == egui::Order::Background);
    if !down {
        ctx.data_mut(|d| d.remove::<(Payload, String, &'static str)>(Id::new("insert-drag")));
        if over_board && !ed.read_only {
            let at = ed.to_world(ed.screen_pt(pos));
            insert_payload(ed, &what, Some(at));
        }
        return;
    }
    let k = ui::motion::appear(ctx, Id::new("insert-drag-card"), true);
    let lift = ui::motion::spring(ctx, Id::new("insert-drag-lift"), if over_board { 1.0 } else { 0.0 }, 0.25, 0.8);
    // A note torn off the pad: the note itself under the pointer, as big as it will be, swinging
    // a little with the movement.
    if what == Payload::Sticky {
        let color = crate::model::color_or(&ed.prefs.sticky_color, Color32::from_rgb(0xFF, 0xE5, 0x8F));
        let vx = ctx.input(|i| i.pointer.velocity().x);
        // Held up, a little askew, swinging with the movement.
        let swing = ui::motion::spring(ctx, Id::new("sticky-swing"), -0.05 + (vx * 0.0006).clamp(-0.22, 0.22), 0.2, 0.7);
        let size = (220.0 * (0.55 + 0.45 * k.clamp(0.0, 1.0))).min(220.0);
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("sticky-ghost")));
        let c = pos2(pos.x, pos.y + size * 0.3);
        note_shadow(&p, c + vec2(0.0, 10.0), size, swing);
        note_shape(&p, c, size, swing, color, ui::mix(color, 0.8, Color32::BLACK).gamma_multiply(0.4));
        ctx.set_cursor_icon(if over_board { egui::CursorIcon::Grabbing } else { egui::CursorIcon::NoDrop });
        ctx.request_repaint();
        return;
    }
    egui::Area::new(Id::new("insert-drag-card")).fixed_pos(pos + vec2(12.0, 12.0)).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0) * (0.75 + 0.25 * lift));
        ui::float_frame(&t).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui::row(ui, 24.0, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                icon(ui, icon_name, r.center(), 16.0, t.icon);
                ui.label(RichText::new(&label).font(ui::medium(ui::px(11.0))).color(t.text));
            });
        });
    });
    ctx.set_cursor_icon(if over_board { egui::CursorIcon::Grabbing } else { egui::CursorIcon::NoDrop });
    ctx.request_repaint();
}

/// FigJam's "+": code, widgets, pictures and templates, in a light card of big tiles. A tile
/// clicked adds in the middle of the view; dragged, it goes where it is let go.
fn more_tools(ui: &mut Ui, ed: &mut Editor, t: &Theme, top: bool) {
    // FigJam's round "+" at the end of the toolbar.
    let (rect, r) = ui.allocate_exact_size(vec2(btn(), btn()), Sense::click());
    let h = ui::motion::hover(ui.ctx(), r.id.with("h"), r.hovered() || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&r)));
    ui.painter().circle_filled(rect.center(), 15.0 * bar(), ui::blend(t.bg2, t.press, h));
    icon(ui, "plus", rect.center(), icon_px(), t.icon);
    let r = ui::tip(r, "Altri strumenti", None);
    egui::Popup::menu(&r).frame(ui::float_frame(t).inner_margin(egui::Margin::same(8))).align(if top { egui::RectAlign::BOTTOM } else { egui::RectAlign::TOP }).gap(10.0).show(|ui| {
        ui.label(egui::RichText::new("Aggiungi").font(ui::medium(ui::px(11.0))).color(t.text2));
        ui.add_space(4.0);
        let items: [(&str, &str, &str, Option<Payload>); 6] = [
            ("code", "Blocco di codice", "Codice con i colori della sintassi", Some(Payload::Code)),
            ("chart-column", "Sondaggio", "Ognuno vota un'opzione", Some(Payload::Poll)),
            ("list-checks", "Lista di cose da fare", "Caselle da spuntare insieme", Some(Payload::Checklist)),
            ("hash", "Contatore", "Un numero con + e −", Some(Payload::Counter)),
            ("image-plus", "Immagine", "Dal computer", None),
            ("layout-template", "Modelli", "Brainstorming, Kanban e altri", None),
        ];
        egui::Grid::new("more-tools").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
            for (i, (icon_name, label, hint, payload)) in items.into_iter().enumerate() {
                let resp = tile(ui, icon_name, label, hint, t, payload.is_some());
                if let Some(p) = &payload
                    && resp.drag_started()
                {
                    start_drag(ui.ctx(), p.clone(), label, icon_name);
                } else if resp.clicked() {
                    match payload {
                        Some(p) => insert_payload(ed, &p, None),
                        None if i == 4 => ed.requests.push(crate::editor::Request::InsertImage),
                        None => {
                            ed.prefs.left_panel = true;
                            ed.prefs.focus = false;
                            ed.requests.push(crate::editor::Request::Templates);
                        }
                    }
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    });
}

/// A big tile of the "+" card: icon on a tinted square, name and a line about it.
fn tile(ui: &mut Ui, name: &str, label: &str, hint: &str, t: &Theme, draggable: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(216.0, 48.0), if draggable { Sense::click_and_drag() } else { Sense::click() });
    let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    let s = ui::motion::press(ui.ctx(), resp.id, resp.is_pointer_button_down_on());
    let r2 = Rect::from_center_size(r.center(), r.size() * s);
    ui.painter().rect_filled(r2, 10.0, t.hover.gamma_multiply(k));
    let sq = Rect::from_min_size(r2.min + vec2(6.0, 6.0), vec2(36.0, 36.0));
    ui.painter().rect_filled(sq, 9.0, t.selected);
    icon(ui, name, sq.center(), 18.0, if t.dark { Color32::WHITE } else { ui::mix(t.brand, 0.75, Color32::BLACK) });
    ui.painter().text(pos2(sq.max.x + 10.0, r2.center().y - 7.0), egui::Align2::LEFT_CENTER, label, ui::medium(ui::px(12.0)), t.text);
    ui.painter().text(pos2(sq.max.x + 10.0, r2.center().y + 8.0), egui::Align2::LEFT_CENTER, hint, egui::FontId::proportional(ui::px(11.0)), t.text2);
    if draggable && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    ui::focus_ring(ui, &resp, r);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

/// One marker of FigJam's drawer: pen, highlighter or washi tape drawn as the real thing, in the
/// colour it will draw with. It peeks out of the toolbar, rises a little under the pointer and
/// stands up when chosen, on a spring.
fn drawer_slot(ui: &mut Ui, ed: &mut Editor, t: &Theme, tool: Tool) -> egui::Response {
    let slot_w = slot_w();
    let (rect, _) = ui.allocate_exact_size(vec2(slot_w, btn()), Sense::hover());
    // The drawer runs into the toolbar's bottom edge: the tools vanish into it.
    let clip = Rect::from_min_max(pos2(rect.min.x, rect.min.y - 6.0), pos2(rect.max.x, rect.max.y + 5.0));
    let pen = ed.prefs.pens.get(ed.pen).cloned().unwrap_or_default();
    let (label, kbd, color) = match tool {
        Tool::Pen => ("Penna", "P", pen.color.clone()),
        Tool::Highlighter => ("Evidenziatore", "M", ed.prefs.highlighter.color.clone()),
        _ => ("Nastro adesivo", "W", ed.prefs.tape.color.clone()),
    };
    let slot = Rect::from_min_size(pos2(rect.min.x + 1.0, rect.min.y - 6.0), vec2(slot_w - 2.0, btn() + 11.0));
    let resp = ui.interact(slot, ui.id().with(("drawer", format!("{tool:?}"))), Sense::click());
    let on = ed.tool == tool;
    // In hand: drawn out of the drawer, a little less with its options folded away.
    let out = if tool_look(ed, tool).0 { 0.7 } else { 1.0 };
    let lift = ui::motion::spring(ui.ctx(), resp.id.with("lift"), if on { out } else if resp.hovered() { 0.45 } else { 0.0 }, 0.3, 0.7);
    let top = slot.min.y + 18.0 - 14.0 * lift;
    let c = crate::model::parse_color(&color).unwrap_or(crate::model::DARK);
    let p = ui.painter().with_clip_rect((clip).intersect(ui.clip_rect()));
    match tool {
        Tool::Pen => marker(&p, slot.center().x, top, c, t, 14.0 * bar(), false),
        Tool::Highlighter => marker(&p, slot.center().x, top, c, t, 17.0 * bar(), true),
        _ => tape_roll(&p, slot.center().x, top, c, t, bar()),
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
    ui::tip(resp, label, Some(kbd))
}

/// A marker standing in the drawer, as FigJam draws it: a white barrel, a grey cone and the tip in
/// the colour it draws with (a highlighter: a wide cap and a slanted chisel).
fn marker(p: &egui::Painter, x: f32, top: f32, c: Color32, t: &Theme, w: f32, chisel: bool) {
    let (body, edge, cone) = if t.dark { (Color32::from_gray(0x58), Color32::from_gray(0x70), Color32::from_gray(0x48)) } else { (Color32::from_gray(0xFC), Color32::from_gray(0xCF), Color32::from_gray(0xE4)) };
    let h = w / 2.0;
    let neck = top + if chisel { 12.0 } else { 15.0 };
    p.rect(Rect::from_min_max(pos2(x - h, neck), pos2(x + h, top + 70.0)), egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, body, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    // Light down one side of the barrel.
    if !t.dark {
        p.rect_filled(Rect::from_min_max(pos2(x + h - 3.5, neck + 2.0), pos2(x + h - 2.0, top + 70.0)), 1.0, Color32::from_gray(0xEC));
    }
    if chisel {
        p.rect(Rect::from_min_max(pos2(x - h, top + 6.0), pos2(x + h, neck + 1.0)), 1.0, c, Stroke::new(1.0, ui::mix(c, 0.8, Color32::BLACK)), egui::StrokeKind::Inside);
        let tip = vec![pos2(x - h + 2.0, top + 6.5), pos2(x + h - 2.0, top + 6.5), pos2(x + h - 2.0, top + 2.0), pos2(x - h + 4.0, top)];
        p.add(egui::Shape::convex_polygon(tip, ui::mix(c, 0.85, Color32::BLACK), Stroke::NONE));
    } else {
        let n = h * 0.34;
        p.add(egui::Shape::convex_polygon(vec![pos2(x - h + 0.5, neck), pos2(x + h - 0.5, neck), pos2(x + n, top + 5.0), pos2(x - n, top + 5.0)], cone, Stroke::new(1.0, edge)));
        p.add(egui::Shape::convex_polygon(vec![pos2(x - n, top + 5.5), pos2(x + n, top + 5.5), pos2(x + 0.9, top), pos2(x - 0.9, top)], c, Stroke::NONE));
    }
}

/// A roll of washi tape with its loose end standing up.
fn tape_roll(p: &egui::Painter, x: f32, top: f32, c: Color32, t: &Theme, k: f32) {
    let edge = ui::mix(c, 0.7, Color32::BLACK);
    let strip = Rect::from_min_max(pos2(x - 6.0 * k, top), pos2(x + 6.0 * k, top + 16.0 * k));
    p.add(egui::Shape::convex_polygon(vec![strip.left_bottom(), strip.left_top() + vec2(0.0, 2.0), strip.center_top() + vec2(-2.0, 0.0), strip.center_top() + vec2(2.0, 2.5), strip.right_top() + vec2(0.0, 0.5), strip.right_bottom()], c, Stroke::new(1.0, edge)));
    let centre = pos2(x, top + 25.0 * k);
    p.circle(centre, 11.0 * k, c, Stroke::new(1.0, edge));
    p.circle(centre, 4.5 * k, if t.dark { Color32::from_gray(0x3A) } else { Color32::WHITE }, Stroke::new(1.0, edge));
}



/// Tools with options in a tray over the toolbar.
fn has_tray(tool: Tool) -> bool {
    matches!(tool, Tool::Pen | Tool::Highlighter | Tool::Tape | Tool::Eraser | Tool::Lasso | Tool::Shape | Tool::Line | Tool::Arrow | Tool::Text | Tool::Sticky | Tool::Stamp | Tool::Table | Tool::Laser)
}

/// How a tool's button looks, as (tinted, solid): solid while the tool is in hand with its
/// options open, a light tint once they are folded away (so that shows), plain otherwise.
fn tool_look(ed: &Editor, tool: Tool) -> (bool, bool) {
    let held = ed.tool == tool;
    let folded = held && !ed.prefs.tray && has_tray(tool);
    (folded, held && !folded)
}

/// The colour of an icon drawn over a tool button in that look.
fn tool_ink(ed: &Editor, tool: Tool, t: &Theme) -> Color32 {
    match tool_look(ed, tool) {
        (_, true) => Color32::WHITE,
        (true, _) if !t.dark => ui::mix(t.brand, 0.7, Color32::BLACK),
        _ => t.icon,
    }
}

/// Picks a tool from the toolbar; the tool already chosen opens or closes its options instead.
fn pick(ed: &mut Editor, tool: Tool) {
    if ed.tool == tool {
        ed.prefs.tray = !ed.prefs.tray;
    } else {
        ed.set_tool(tool);
        ed.prefs.tray = true;
    }
}

/// One button of the toolbar, by its id (see `prefs::TOOLBAR_ITEMS`).
fn toolbar_item(ui: &mut Ui, ed: &mut Editor, st: &mut BoardUi, t: &Theme, id: &str, top: bool) {
    let simple = |ui: &mut Ui, ed: &mut Editor, tool: Tool, name: &str, label: &str, kbd: Option<&str>| {
        let (tint, solid) = tool_look(ed, tool);
        if icon_button(ui, name, label, kbd, vec2(btn(), btn()), icon_px(), tint, solid).clicked() {
            pick(ed, tool);
        }
    };
    match id {
        "draw" => draw_drawer(ui, ed, t),
        "select" => simple(ui, ed, Tool::Select, "mouse-pointer-2", "Seleziona", Some("V")),
        "hand" => simple(ui, ed, Tool::Hand, "hand", "Mano", Some("H")),
        "pen" | "highlighter" | "tape" => {
            let tool = match id {
                "pen" => Tool::Pen,
                "highlighter" => Tool::Highlighter,
                _ => Tool::Tape,
            };
            if drawer_slot(ui, ed, t, tool).clicked() {
                pick(ed, tool);
            }
        }
        "eraser" => simple(ui, ed, Tool::Eraser, "eraser", "Gomma", Some("E")),
        "lasso" => simple(ui, ed, Tool::Lasso, "lasso", "Lazo", Some("Q")),
        "ruler" => {
            let on = ed.ruler.visible;
            if icon_button(ui, "ruler", if on { "Nascondi righello" } else { "Mostra righello" }, Some("U"), vec2(btn(), btn()), icon_px(), on, false).clicked() {
                ed.toggle_ruler();
            }
        }
        "sticky" => sticky_stack(ui, ed, t),
        "shape" if !ed.prefs.toolbar_hidden.iter().any(|h| h == "draw") => shapes_combo(ui, ed, t),
        "shape" => {
            let shape = ed.prefs.last_shape;
            let (label, kbd) = SHAPES.iter().find(|s| s.0 == shape).map_or(("Forma", None), |s| (s.1, s.2));
            let (tint, solid) = tool_look(ed, Tool::Shape);
            let r = icon_button(ui, "", label, kbd, vec2(btn(), btn()), icon_px(), tint, solid);
            if r.clicked() {
                pick(ed, Tool::Shape);
            }
            shape_icon(ui, shape, r.rect.center(), icon_px(), tool_ink(ed, Tool::Shape, t));
            let (cr, chevron) = ui.allocate_exact_size(vec2(14.0, btn()), Sense::click());
            ui::hover_fill(ui, chevron.id, cr, chevron.hovered(), 4.0, t.hover);
            icon(ui, "chevron-down", cr.center(), 12.0, t.icon2);
            // 29 shapes don't fit above the toolbar of a small window: the list scrolls, opened
            // at the shape in use.
            let screen = ui.ctx().content_rect();
            let room = if top { screen.bottom() - chevron.rect.bottom() } else { chevron.rect.top() - screen.top() } - 40.0;
            let fresh = Id::new("shape-menu-fresh");
            if chevron.clicked() {
                ui.data_mut(|d| d.insert_temp(fresh, true));
            }
            let opened = ui.data_mut(|d| d.remove_temp::<bool>(fresh)).is_some();
            egui::Popup::menu(&chevron).frame(ui::menu_frame(t)).align(if top { egui::RectAlign::BOTTOM } else { egui::RectAlign::TOP }).show(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.spacing_mut().scroll = egui::style::ScrollStyle::floating();
                egui::ScrollArea::vertical().max_height(room.clamp(120.0, 420.0)).show(ui, |ui| {
                    for (k, label, kbd) in SHAPES {
                        let resp = ui::menu_item(ui, None, label, kbd, false);
                        shape_icon(ui, k, pos2(resp.rect.min.x + 12.0, resp.rect.center().y), 14.0, t.menu_text);
                        if opened && k == ed.prefs.last_shape {
                            resp.scroll_to_me(Some(egui::Align::Center));
                        }
                        if resp.clicked() {
                            ed.prefs.last_shape = k;
                            ed.set_tool(Tool::Shape);
                        }
                    }
                });
            });
        }
        "arrow" => simple(ui, ed, Tool::Arrow, "arrow-up-right", "Connettore", Some("X")),
        "line" => simple(ui, ed, Tool::Line, "minus", "Linea", Some("L")),
        "text" => simple(ui, ed, Tool::Text, "type", "Testo", Some("T")),
        "section" => simple(ui, ed, Tool::Section, "section", "Sezione", Some("Maiusc+S")),
        "table" => simple(ui, ed, Tool::Table, "table", "Tabella", None),
        "stamp" => simple(ui, ed, Tool::Stamp, "stamp", "Reazioni", None),
        "comment" => simple(ui, ed, Tool::Comment, "message-circle", "Commento", Some("C")),
        "chat" => {
            let on = st.chat.is_some();
            if icon_button(ui, "message-square-text", "Chat del cursore", Some("/"), vec2(btn(), btn()), icon_px(), on, false).clicked() {
                st.chat = if on { None } else { Some(String::new()) };
            }
        }
        "more" => more_tools(ui, ed, t, top),
        "laser" => laser_button(ui, ed),
        "minimap" => {
            let on = ed.prefs.minimap;
            if icon_button(ui, "map", if on { "Nascondi la minimappa" } else { "Mostra la minimappa" }, None, vec2(btn(), btn()), icon_px(), on, false).clicked() {
                ed.prefs.minimap = !on;
            }
        }
        _ => {}
    }
}

/// Width a toolbar button takes (with the gap after it).
fn item_width(id: &str, ed: &Editor) -> f32 {
    match id {
        "pen" | "highlighter" | "tape" | "draw" => slot_w() + 2.0,
        "sticky" => stack_w() + 2.0,
        "shape" if !ed.prefs.toolbar_hidden.iter().any(|h| h == "draw") => combo_w() + 2.0,
        "shape" => btn() + 14.0 + 4.0,
        _ => btn() + 2.0,
    }
}

/// The width of a marker in the drawer.
fn slot_w() -> f32 {
    (32.0 * bar()).round()
}

fn stack_w() -> f32 {
    (50.0 * bar()).round()
}

fn combo_w() -> f32 {
    (btn() + 8.0).round()
}

/// The tools of the pens' drawer.
fn is_drawing(tool: Tool) -> bool {
    matches!(tool, Tool::Pen | Tool::Highlighter | Tool::Tape | Tool::Eraser | Tool::Lasso)
}

/// The drawing tools, side by side in their options.
const DRAWING: [(Tool, &str, &str, &str); 5] = [
    (Tool::Pen, "pen", "Penna", "P"),
    (Tool::Highlighter, "highlighter", "Evidenziatore", "M"),
    (Tool::Tape, "rectangle-horizontal", "Nastro adesivo", "W"),
    (Tool::Eraser, "eraser", "Gomma", "E"),
    (Tool::Lasso, "lasso", "Lazo", "Q"),
];

/// FigJam's pens: one marker in the toolbar, the one in hand (the pen while another tool is).
/// The toolbar keeps the buttons it was given: the other drawing tools are in the options.
fn draw_drawer(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    let tool = if matches!(ed.tool, Tool::Highlighter | Tool::Tape) { ed.tool } else { Tool::Pen };
    if drawer_slot(ui, ed, t, tool).clicked() {
        pick(ed, tool);
    }
}

/// A sticky note drawn turned by `a`: its colour, a thin edge and the fold at the corner.
fn note_shape(p: &egui::Painter, c: Pos2, s: f32, a: f32, color: Color32, edge: Color32) {
    let (sn, cs) = a.sin_cos();
    let r = |x: f32, y: f32| c + vec2(x * cs - y * sn, x * sn + y * cs);
    let (h, f) = (s / 2.0, s * 0.2);
    p.add(egui::Shape::convex_polygon(vec![r(-h, -h), r(h, -h), r(h, h - f), r(h - f, h), r(-h, h)], color, Stroke::new(1.0, edge)));
    p.add(egui::Shape::convex_polygon(vec![r(h, h - f), r(h - f, h), r(h - f, h - f)], ui::mix(color, 0.84, Color32::BLACK), Stroke::NONE));
}

/// What the shapes panel lists, by group: FigJam's basic shapes, the flowchart's, the systems'.
const SHAPE_GROUPS: [(&str, &[ShapeTool]); 3] = [
    (
        "Base",
        &[
            ShapeTool::Kind(ShapeKind::Rect),
            ShapeTool::Kind(ShapeKind::Ellipse),
            ShapeTool::Kind(ShapeKind::Diamond),
            ShapeTool::Kind(ShapeKind::Triangle),
            ShapeTool::Kind(ShapeKind::TriangleDown),
            ShapeTool::RoundRect,
            ShapeTool::Kind(ShapeKind::Pentagon),
            ShapeTool::Kind(ShapeKind::Hexagon),
            ShapeTool::Kind(ShapeKind::Octagon),
            ShapeTool::Kind(ShapeKind::Plus),
            ShapeTool::Kind(ShapeKind::ArrowLeft),
            ShapeTool::Kind(ShapeKind::ArrowRight),
            ShapeTool::Kind(ShapeKind::Chevron),
            ShapeTool::Kind(ShapeKind::Star),
            ShapeTool::Kind(ShapeKind::Speech),
            ShapeTool::Kind(ShapeKind::Trapezoid),
        ],
    ),
    (
        "Diagrammi di flusso",
        &[
            ShapeTool::Kind(ShapeKind::Pill),
            ShapeTool::Kind(ShapeKind::Process),
            ShapeTool::Kind(ShapeKind::Diamond),
            ShapeTool::Kind(ShapeKind::Parallelogram),
            ShapeTool::Kind(ShapeKind::Document),
            ShapeTool::Kind(ShapeKind::Documents),
            ShapeTool::Kind(ShapeKind::ManualInput),
            ShapeTool::Kind(ShapeKind::Delay),
            ShapeTool::Kind(ShapeKind::OffPage),
            ShapeTool::Kind(ShapeKind::Display),
            ShapeTool::Kind(ShapeKind::InternalStorage),
            ShapeTool::Kind(ShapeKind::Junction),
        ],
    ),
    (
        "Schemi informatici",
        &[
            ShapeTool::Kind(ShapeKind::Cylinder),
            ShapeTool::Kind(ShapeKind::Cloud),
            ShapeTool::Kind(ShapeKind::Display),
            ShapeTool::Kind(ShapeKind::Documents),
            ShapeTool::Kind(ShapeKind::InternalStorage),
            ShapeTool::Kind(ShapeKind::Process),
        ],
    ),
];

/// FigJam's shapes panel ("Altre forme" in the shapes row): every shape, by group, with a search
/// box; a click picks the shape for the shape tool, a drag puts it on the board.
pub fn shapes_panel(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let open_id = Id::new("shapes-panel");
    let open = ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) && !ed.read_only;
    let k = ui::motion::presence(ctx, open_id.with("anim"), open);
    if k <= 0.001 {
        return;
    }
    let t = ui::theme(ctx);
    let narrow = ui::narrow(ctx);
    let w = if narrow { stage.width() - 16.0 } else { 300.0 };
    let at = if narrow { pos2(stage.min.x + 8.0, stage.min.y + 8.0) } else { pos2(stage.min.x + 12.0, stage.min.y + 68.0) };
    // Down to the toolbar and its options, never over them.
    let floor = ctx.data(|d| d.get_temp::<Rect>(Id::new("bottom-bars"))).filter(|b| b.center().y > stage.center().y).map_or(stage.max.y - 12.0, |b| b.min.y - 10.0);
    let h = (floor - at.y).min(680.0).max(160.0);
    let mut close = false;
    let query_id = open_id.with("query");
    let mut query = ctx.data(|d| d.get_temp::<String>(query_id)).unwrap_or_default();
    let folded_id = open_id.with("folded");
    let mut folded: Vec<String> = ctx.data(|d| d.get_temp::<Vec<String>>(folded_id)).unwrap_or_default();
    let mut want: Option<Tool> = None;
    egui::Area::new(open_id).fixed_pos(at + vec2(-12.0 * (1.0 - k), 0.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui.set_max_size(vec2(w, h));
        ui::float_frame(&t).corner_radius(14).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
            ui.set_width(w);
            ui.set_height(h);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 10, top: 12, bottom: 10 }).show(ui, |ui| {
                ui::row(ui, 28.0, |ui| {
                    ui.label(RichText::new("Forme").font(ui::medium(ui::px(13.0))).color(t.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(0.0);
                    ui.set_width(w - 26.0);
                    ui::text_field(ui, &mut query, "Cerca forme", 40);
                });
            });
            let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
            let q = query.trim().to_lowercase();
            let name = |s: ShapeTool| SHAPES.iter().find(|x| x.0 == s).map_or("Forma", |x| x.1);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                // The connectors first, then the shapes, group by group.
                let connectors: [(Option<crate::model::Route>, &str); 4] = [(Some(crate::model::Route::Elbow), "Connettore a gomito"), (Some(crate::model::Route::Curved), "Connettore curvo"), (Some(crate::model::Route::Straight), "Freccia"), (None, "Linea")];
                let mut groups: Vec<(&str, Vec<(String, Cell)>)> = vec![("Connettori", connectors.iter().filter(|c| q.is_empty() || c.1.to_lowercase().contains(&q)).map(|c| (c.1.to_string(), Cell::Connector(c.0))).collect())];
                for (title, list) in SHAPE_GROUPS {
                    groups.push((title, list.iter().filter(|s| q.is_empty() || name(**s).to_lowercase().contains(&q)).map(|s| (name(*s).to_string(), Cell::Shape(*s))).collect()));
                }
                let cols = ((w - 24.0) / 64.0).floor().max(3.0) as usize;
                let cell = vec2((w - 24.0) / cols as f32, 56.0);
                for (title, cells) in groups.into_iter().filter(|g| !g.1.is_empty()) {
                    let is_folded = q.is_empty() && folded.iter().any(|f| f == title);
                    let (hr, hresp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
                    ui::hover_fill(ui, hresp.id, hr.shrink2(vec2(6.0, 2.0)), hresp.hovered(), 6.0, t.hover);
                    ui.painter().text(pos2(hr.min.x + 16.0, hr.center().y), egui::Align2::LEFT_CENTER, title, ui::medium(ui::px(12.0)), t.text);
                    icon(ui, if is_folded { "chevron-down" } else { "chevron-up" }, pos2(hr.max.x - 22.0, hr.center().y), 14.0, t.icon2);
                    if hresp.clicked() {
                        if is_folded {
                            folded.retain(|f| f != title);
                        } else {
                            folded.push(title.to_string());
                        }
                    }
                    if is_folded {
                        continue;
                    }
                    for row in cells.chunks(cols) {
                        let (rr, _) = ui.allocate_exact_size(vec2(w, cell.y), Sense::hover());
                        for (i, (label, c)) in row.iter().enumerate() {
                            let r = Rect::from_min_size(pos2(rr.min.x + 12.0 + i as f32 * cell.x, rr.min.y), cell).shrink(2.0);
                            let resp = ui.interact(r, Id::new(("shape-cell", title, i, label.as_str())), Sense::click_and_drag());
                            let on = match c {
                                Cell::Shape(s) => ed.tool == Tool::Shape && ed.prefs.last_shape == *s,
                                Cell::Connector(Some(route)) => ed.tool == Tool::Arrow && ed.prefs.route == *route,
                                Cell::Connector(None) => ed.tool == Tool::Line,
                            };
                            let hv = ui::motion::hover(ctx, resp.id.with("h"), resp.hovered());
                            if on || hv > 0.0 {
                                ui.painter().rect_filled(r, 8.0, if on { t.selected } else { t.hover.gamma_multiply(hv) });
                            }
                            match c {
                                Cell::Shape(s) => shape_icon(ui, *s, r.center(), 26.0, t.icon),
                                Cell::Connector(route) => connector_icon(ui, *route, r.center(), t.icon),
                            }
                            if resp.drag_started()
                                && let Cell::Shape(s) = c
                            {
                                start_drag(ctx, Payload::Shape(*s), label, "shapes");
                            } else if resp.clicked() {
                                match c {
                                    Cell::Shape(s) => {
                                        ed.prefs.last_shape = *s;
                                        want = Some(Tool::Shape);
                                    }
                                    Cell::Connector(Some(route)) => {
                                        ed.prefs.route = *route;
                                        want = Some(Tool::Arrow);
                                    }
                                    Cell::Connector(None) => want = Some(Tool::Line),
                                }
                            }
                            ui::tip(resp, label, None);
                        }
                    }
                    ui.add_space(6.0);
                }
                ui.add_space(8.0);
            });
        });
    });
    if let Some(tool) = want {
        ed.set_tool(tool);
        ed.prefs.tray = true;
    }
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.data_mut(|d| d.insert_temp(open_id, false));
    }
    ctx.data_mut(|d| {
        d.insert_temp(query_id, query);
        d.insert_temp(folded_id, folded);
    });
}

/// A cell of the shapes panel.
enum Cell {
    Shape(ShapeTool),
    Connector(Option<crate::model::Route>),
}

/// The soft shadow of a note held up off the board.
fn note_shadow(p: &egui::Painter, c: Pos2, s: f32, _a: f32) {
    let r = Rect::from_center_size(c, vec2(s * 0.94, s * 0.94));
    p.add(egui::epaint::Shadow { offset: [0, 10], blur: 34, spread: 0, color: Color32::from_black_alpha(60) }.as_shape(r, 6.0));
}

/// FigJam's pad of sticky notes: a click picks the note tool; dragged, the top note peels off
/// and goes where it is let go, while the next one slides up into its place.
fn sticky_stack(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    let ctx = ui.ctx().clone();
    let (rect, resp) = ui.allocate_exact_size(vec2(stack_w(), btn()), Sense::click_and_drag());
    let color = crate::model::color_or(&ed.prefs.sticky_color, Color32::from_rgb(0xFF, 0xE5, 0x8F));
    let (tint, solid) = tool_look(ed, Tool::Sticky);
    let sel = ui::motion::hover(&ctx, resp.id.with("sel"), tint || solid);
    if sel > 0.0 {
        ui.painter().rect_filled(rect, 8.0, t.selected.gamma_multiply(sel));
    }
    ui::hover_fill(ui, resp.id, rect, resp.hovered() && !(tint || solid), 8.0, t.hover);
    let lift = ui::motion::hover(&ctx, resp.id.with("lift"), resp.hovered() || resp.dragged());
    // The next note coming up after one was torn off.
    let refill = Id::new("sticky-refill");
    let torn = ctx.data(|d| d.get_temp::<f64>(refill));
    let k = torn.map_or(1.0, |t0| (((crate::platform::now_ms() - t0) / 380.0) as f32).clamp(0.0, 1.0));
    if k < 1.0 {
        ctx.request_repaint();
    }
    let k = egui::emath::easing::cubic_out(k);
    let edge = ui::mix(color, 0.72, Color32::BLACK).gamma_multiply(0.55);
    let s = (30.0 * bar()).round();
    let c = rect.center() + vec2(0.0, 3.0);
    let p = ui.painter().with_clip_rect(Rect::from_min_max(pos2(rect.min.x, rect.min.y - 10.0), pos2(rect.max.x, rect.max.y + 4.0)).intersect(ui.clip_rect()));
    note_shape(&p, c + vec2(3.0, 1.5), s, 0.14, ui::mix(color, 0.9, Color32::BLACK), edge);
    note_shape(&p, c + vec2(-2.0, 0.5), s, -0.1, ui::mix(color, 0.95, Color32::BLACK), edge);
    note_shape(&p, c + vec2(0.0, -1.0 - 3.0 * lift + (1.0 - k) * 6.0), s * (0.92 + 0.08 * k), -0.02 * lift, color, edge);
    if resp.drag_started() && !ed.read_only {
        start_drag(&ctx, Payload::Sticky, "Nota adesiva", "sticky-note");
        ctx.data_mut(|d| d.insert_temp(refill, crate::platform::now_ms()));
    } else if resp.clicked() {
        pick(ed, Tool::Sticky);
    }
    if resp.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
    }
    ui::tip(resp, "Note adesive", Some("S"));
}

/// The shapes and connectors, one button: a square, a circle and an arrow between them.
fn shapes_combo(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    let held = matches!(ed.tool, Tool::Shape | Tool::Arrow | Tool::Line);
    let folded = held && !ed.prefs.tray;
    let r = icon_button(ui, "", "Forme e connettori", Some("R"), vec2(combo_w(), btn()), icon_px(), folded, held && !folded);
    if r.clicked() {
        if held {
            ed.prefs.tray = !ed.prefs.tray;
        } else {
            ed.set_tool(Tool::Shape);
            ed.prefs.tray = true;
        }
    }
    let ink = if held && !folded { Color32::WHITE } else if folded && !t.dark { ui::mix(t.brand, 0.7, Color32::BLACK) } else { t.icon };
    let k = bar();
    let c = r.rect.center();
    let p = ui.painter();
    let line = Stroke::new(1.3, ink);
    p.rect_stroke(Rect::from_center_size(c + vec2(-6.0, -5.0) * k, vec2(11.0, 11.0) * k), 1.5, line, egui::StrokeKind::Middle);
    p.circle_stroke(c + vec2(6.0, 6.0) * k, 5.5 * k, line);
    let a = c + vec2(2.0, -4.0) * k;
    let b = c + vec2(9.0, -8.0) * k;
    p.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke([a, a + vec2(0.0, -4.0) * k, b + vec2(-4.0, 0.0) * k, b], false, Color32::TRANSPARENT, line)));
    p.line_segment([b, b + vec2(-3.0, -2.5) * k], line);
    p.line_segment([b, b + vec2(-3.0, 2.5) * k], line);
}

/// A toolbar button that didn't fit, as a row of the "…" menu.
fn overflow_item(ui: &mut Ui, ed: &mut Editor, st: &mut BoardUi, id: &str) {
    let name = crate::prefs::TOOLBAR_ITEMS.iter().find(|i| i.0 == id).map_or(id, |i| i.1);
    let tool = match id {
        "draw" => Some(Tool::Pen),
        "select" => Some(Tool::Select),
        "hand" => Some(Tool::Hand),
        "pen" => Some(Tool::Pen),
        "highlighter" => Some(Tool::Highlighter),
        "tape" => Some(Tool::Tape),
        "eraser" => Some(Tool::Eraser),
        "lasso" => Some(Tool::Lasso),
        "sticky" => Some(Tool::Sticky),
        "shape" => Some(Tool::Shape),
        "arrow" => Some(Tool::Arrow),
        "line" => Some(Tool::Line),
        "text" => Some(Tool::Text),
        "section" => Some(Tool::Section),
        "table" => Some(Tool::Table),
        "stamp" => Some(Tool::Stamp),
        "comment" => Some(Tool::Comment),
        "laser" => Some(Tool::Laser),
        _ => None,
    };
    let icon_name = toolbar_icon(id);
    match (id, tool) {
        (_, Some(tool)) => {
            if ui::menu_item(ui, Some(if ed.tool == tool { "check" } else { icon_name }), name, None, false).clicked() {
                ed.set_tool(tool);
                ed.prefs.tray = true;
            }
        }
        ("ruler", _) => {
            if ui::menu_item(ui, Some(icon_name), if ed.ruler.visible { "Nascondi righello" } else { "Mostra righello" }, Some("U"), false).clicked() {
                ed.toggle_ruler();
            }
        }
        ("chat", _) => {
            if ui::menu_item(ui, Some(icon_name), name, Some("/"), false).clicked() {
                st.chat = Some(String::new());
            }
        }
        ("minimap", _) => {
            let on = ed.prefs.minimap;
            if ui::menu_item(ui, Some(if on { "check" } else { icon_name }), name, None, false).clicked() {
                ed.prefs.minimap = !on;
            }
        }
        _ => {
            // The "+": its additions one by one.
            for (what, label, ic) in [(Payload::Code, "Blocco di codice", "code"), (Payload::Poll, "Sondaggio", "chart-column"), (Payload::Checklist, "Lista di cose da fare", "list-checks"), (Payload::Counter, "Contatore", "hash")] {
                if ui::menu_item(ui, Some(ic), label, None, false).clicked() {
                    insert_payload(ed, &what, None);
                }
            }
            if ui::menu_item(ui, Some("image-plus"), "Immagine", Some("I"), false).clicked() {
                ed.requests.push(crate::editor::Request::InsertImage);
            }
            if ui::menu_item(ui, Some("layout-template"), "Modelli", None, false).clicked() {
                ed.prefs.left_panel = true;
                ed.prefs.focus = false;
                ed.requests.push(crate::editor::Request::Templates);
            }
        }
    }
}

/// The icon of a toolbar button, in lists and menus.
pub fn toolbar_icon(id: &str) -> &'static str {
    match id {
        "draw" => "pen-line",
        "select" => "mouse-pointer-2",
        "hand" => "hand",
        "pen" => "pen",
        "highlighter" => "highlighter",
        "tape" => "rectangle-horizontal",
        "eraser" => "eraser",
        "lasso" => "lasso",
        "ruler" => "ruler",
        "sticky" => "sticky-note",
        "shape" => "shapes",
        "arrow" => "arrow-up-right",
        "line" => "minus",
        "text" => "type",
        "section" => "section",
        "table" => "table",
        "stamp" => "stamp",
        "comment" => "message-circle",
        "chat" => "message-square-text",
        "more" => "plus",
        "minimap" => "map",
        _ => "locate-fixed",
    }
}

pub fn toolbar(ctx: &egui::Context, stage: Rect, ed: &mut Editor, st: &mut BoardUi) {
    let t = ui::theme(ctx);
    BAR.store(ed.prefs.toolbar_size.clamp(0.8, 1.4).to_bits(), std::sync::atomic::Ordering::Relaxed);
    let top = ed.prefs.toolbar_pos == ToolbarPos::Top;
    let anchor = if top { egui::Align2::CENTER_TOP } else { egui::Align2::CENTER_BOTTOM };
    let dy = if top { 16.0 } else { -16.0 };
    let items: Vec<String> = crate::prefs::toolbar_items(&ed.prefs.toolbar_order)
        .into_iter()
        .filter(|id| !ed.prefs.toolbar_hidden.contains(id))
        .filter(|id| !ed.read_only || matches!(id.as_str(), "select" | "hand" | "laser" | "chat" | "minimap"))
        // A phone has no minimap.
        .filter(|id| id != "minimap" || stage.width() >= 600.0)
        .collect();
    let group = |id: &str| crate::prefs::TOOLBAR_ITEMS.iter().find(|i| i.0 == id).map_or(0, |i| i.2);
    // A narrow window: the buttons from the end that don't fit go into a "…" menu (the one in
    // use stays out, so its options keep their place).
    let room = stage.width() - 40.0;
    let width = |list: &[String]| {
        let seps = list.windows(2).filter(|w| group(&w[0]) != group(&w[1])).count() as f32;
        list.iter().map(|id| item_width(id, ed)).sum::<f32>() + seps * 11.0
    };
    let mut shown = items.clone();
    let mut hidden: Vec<String> = Vec::new();
    while shown.len() > 2 && width(&shown) + if hidden.is_empty() { 0.0 } else { btn() + 13.0 } > room {
        let current = |id: &str| match id {
            "draw" => is_drawing(ed.tool),
            "select" => ed.tool == Tool::Select,
            "hand" => ed.tool == Tool::Hand,
            "pen" => ed.tool == Tool::Pen,
            "highlighter" => ed.tool == Tool::Highlighter,
            "tape" => ed.tool == Tool::Tape,
            "eraser" => ed.tool == Tool::Eraser,
            "lasso" => ed.tool == Tool::Lasso,
            "sticky" => ed.tool == Tool::Sticky,
            "shape" => matches!(ed.tool, Tool::Shape | Tool::Arrow | Tool::Line),
            "arrow" => ed.tool == Tool::Arrow,
            "line" => ed.tool == Tool::Line,
            "text" => ed.tool == Tool::Text,
            "section" => ed.tool == Tool::Section,
            "table" => ed.tool == Tool::Table,
            "stamp" => ed.tool == Tool::Stamp,
            "comment" => ed.tool == Tool::Comment,
            "laser" => ed.tool == Tool::Laser,
            _ => false,
        };
        let Some(at) = shown.iter().rposition(|id| !current(id) && id != "select") else { break };
        hidden.insert(0, shown.remove(at));
    }
    let items = shown;
    // Centred on the part of the board the panels leave free; when it doesn't fit there, on the
    // whole window (it floats over the panels).
    let width = ctx.data(|d| d.get_temp::<Rect>(Id::new("toolbar-rect"))).map_or(0.0, |r| r.width());
    let cx = if width > stage.width() - 16.0 { 0.0 } else { stage.center().x - ctx.content_rect().center().x };
    // Zen mode slides it away past the edge (1 in place, 0 gone).
    let dock = ctx.data(|d| d.get_temp::<f32>(Id::new("toolbar-dock"))).unwrap_or(1.0).clamp(0.0, 1.0);
    let away = (1.0 - dock) * 90.0 * if top { -1.0 } else { 1.0 };
    let resp = egui::Area::new(Id::new("toolbar")).anchor(anchor, [cx, dy + away + if top { stage.min.y } else { stage.max.y - ctx.content_rect().max.y }]).interactable(dock > 0.5).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.multiply_opacity(dock);
        // FigJam's toolbar: a big friendly card.
        let [contact, soft] = ui::card_shadows(&t);
        let under = ui.painter().add(egui::Shape::Noop);
        let card = egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.border)).corner_radius(16).shadow(soft).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui::row(ui, btn(), |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                let mut last: Option<u8> = None;
                for id in &items {
                    let g = group(id);
                    if last.is_some_and(|l| l != g) {
                        sep(ui, &t);
                    }
                    last = Some(g);
                    toolbar_item(ui, ed, st, &t, id, top);
                }
                if !hidden.is_empty() {
                    sep(ui, &t);
                    let r = icon_button(ui, "more-horizontal", "Altri strumenti che qui non entrano", None, vec2(btn(), btn()), icon_px(), false, false);
                    egui::Popup::menu(&r).frame(ui::menu_frame(&t)).align(if top { egui::RectAlign::BOTTOM_END } else { egui::RectAlign::TOP_END }).show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for id in &hidden {
                            overflow_item(ui, ed, st, id);
                        }
                    });
                }
            });
        });
        ui.painter().set(under, contact.as_shape(card.response.rect, egui::CornerRadius::same(16)));
    });
    let bar = resp.response.rect;
    ctx.data_mut(|d| d.insert_temp(Id::new("toolbar-rect"), bar));
    let tray_rect = if !ed.read_only && ed.prefs.tray { tray(ctx, bar, stage.width(), top, ed, &t) } else { None };
    // What the zoom controls and the minimap keep clear of.
    let bars = tray_rect.map_or(bar, |r| r.union(bar));
    ctx.data_mut(|d| d.insert_temp(Id::new("bottom-bars"), bars));
    if ed.lost {
        back_to_content(ctx, bars, top, ed, &t);
    }
}

fn laser_button(ui: &mut Ui, ed: &mut Editor) {
    let (tint, solid) = tool_look(ed, Tool::Laser);
    let r = icon_button(ui, "", "Puntatore laser", Some("K"), vec2(btn(), btn()), icon_px(), tint, solid);
    if r.clicked() {
        pick(ed, Tool::Laser);
    }
    let c = r.rect.center();
    let red = crate::model::parse_color(&ed.prefs.laser_color).unwrap_or(Color32::from_rgb(0xFF, 0x3B, 0x30));
    ui.painter().circle_filled(c, 7.5, red.gamma_multiply(0.18));
    ui.painter().circle_filled(c, 4.0, red);
    ui.painter().circle_filled(c, 1.6, Color32::WHITE);
}

fn back_to_content(ctx: &egui::Context, bar: Rect, top: bool, ed: &mut Editor, t: &Theme) {
    let y = if top { bar.max.y + 56.0 } else { bar.min.y - 56.0 - 32.0 };
    egui::Area::new(Id::new("back-to-content")).fixed_pos(pos2(bar.center().x - 80.0, y)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(t).inner_margin(egui::Margin::symmetric(4, 4)).show(ui, |ui| {
            if ui::button(ui, "Torna ai contenuti", ui::Kind::Ghost, Some("locate-fixed"), false, true).clicked() {
                ed.fit();
            }
        });
    });
}

const PX: fn(f64) -> String = |v| format!("{} px", (v * 10.0).round() / 10.0);
const PCT: fn(f64) -> String = |v| format!("{}%", (v * 100.0).round());

/// The contextual tray above the toolbar: what the current tool can be set to.
fn tray(ctx: &egui::Context, bar: Rect, room: f32, top: bool, ed: &mut Editor, t: &Theme) -> Option<Rect> {
    let tool = ed.tool;
    if !has_tray(tool) {
        return None;
    }
    let pivot = if top { egui::Align2::CENTER_TOP } else { egui::Align2::CENTER_BOTTOM };
    // Each tool's options rise out of the toolbar on a spring when the tool is picked.
    let last = ctx.data(|d| d.get_temp::<Tool>(Id::new("tray-tool")));
    if last != Some(tool) {
        ctx.data_mut(|d| d.insert_temp(Id::new("tray-tool"), tool));
        ui::motion::reset(ctx, Id::new("tray"));
    }
    let k = ui::motion::appear(ctx, Id::new("tray"), true);
    let rise = ui::motion::travel(ctx, k) * 10.0;
    let at = if top { pos2(bar.center().x, bar.max.y + 8.0 - rise) } else { pos2(bar.center().x, bar.min.y - 8.0 + rise) };
    let mut close = false;
    // A narrow window: sliders and pickers give up some length, and what still doesn't fit goes
    // on a second row (never a scroll bar).
    let max_tray_w = (room.min(ctx.content_rect().width()) - 24.0).max(200.0);
    let natural_id = Id::new(("tray-natural", format!("{tool:?}")));
    let natural = ctx.data(|d| d.get_temp::<f32>(natural_id));
    // No room for the whole row: each slider becomes a chip that opens it, and what still
    // doesn't fit (a phone) goes on a second row.
    let narrow = natural.is_some_and(|w| w > max_tray_w);
    let sw = move |w: f32| w;
    // Squeezed hard, the rows may wrap: dividers would end up at the start or end of a row.
    let sep_h = move |ui: &mut Ui, t: &Theme, h: f32| if narrow { ui.add_space(6.0) } else { sep_h(ui, t, h) };
    #[allow(clippy::too_many_arguments)]
    let slide = move |ui: &mut Ui, label: &str, v: &mut f64, min: f64, max: f64, step: f64, log: bool, f: fn(f64) -> String, w: f32| {
        if narrow { ui::slider_chip(ui, label, v, min, max, step, log, f, top) } else { ui::slider_inline(ui, label, v, min, max, step, log, f, w) }
    };
    // The tallest thing in the tray sets its height; everything else is centred on it. When
    // the row wraps, each line is as tall as what it holds.
    let h = match tool {
        _ if narrow => ui::ctl(TRAY_H),
        Tool::Stamp => 96.0,
        Tool::Table => 76.0,
        _ => ui::ctl(TRAY_H),
    };
    // A connector or a shape picked in the row: that tool in hand (after the row is drawn).
    let mut want: Option<Tool> = None;
    let dock = ctx.data(|d| d.get_temp::<f32>(Id::new("toolbar-dock"))).unwrap_or(1.0).clamp(0.0, 1.0);
    let shown = egui::Area::new(Id::new(("tray", format!("{tool:?}")))).pivot(pivot).fixed_pos(at).constrain_to(ctx.content_rect().shrink(8.0)).interactable(dock > 0.5).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0) * dock);
        // An area lays out in last frame's size: give the row the width it may take.
        ui.set_max_width(max_tray_w);
        ui::float_frame(t).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            {
                ui::wrap_row(ui, h, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, if narrow { 4.0 } else { 0.0 });
                if is_drawing(tool) {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (k, name, label, kbd) in DRAWING {
                        if icon_button(ui, name, label, Some(kbd), vec2(30.0, 30.0), 16.0, false, tool == k).clicked() {
                            want = Some(k);
                        }
                    }
                    let on = ed.ruler.visible;
                    if icon_button(ui, "ruler", if on { "Nascondi righello" } else { "Mostra righello" }, Some("U"), vec2(30.0, 30.0), 16.0, on, false).clicked() {
                        ed.toggle_ruler();
                    }
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if tool != Tool::Lasso {
                        sep_h(ui, t, h);
                    }
                }
                let p = &mut ed.prefs;
                match tool {
                    Tool::Pen => {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let n = p.pens.len();
                        for i in 0..n {
                            pen_slot(ui, ed, i, t, top);
                        }
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let p = &mut ed.prefs;
                        let i = ed.pen.min(p.pens.len() - 1);
                        sep_h(ui, t, h);
                        let pen = &mut p.pens[i];
                        slide(ui, "Spessore", &mut pen.size, 1.0, 64.0, 0.5, true, PX, sw(196.0));
                        ui.add_space(8.0);
                        slide(ui, "Opacità", &mut pen.opacity, 0.1, 1.0, 0.05, false, PCT, sw(172.0));
                        sep_h(ui, t, h);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icon_button(ui, "shapes", "Riconosci le forme", None, vec2(32.0, 32.0), 18.0, p.ink_to_shape, false).clicked() {
                            p.ink_to_shape = !p.ink_to_shape;
                        }
                        let label = if p.finger_draw { "Le dita disegnano" } else { "Le dita spostano la lavagna" };
                        if icon_button(ui, "hand", label, None, vec2(32.0, 32.0), 18.0, p.finger_draw, false).clicked() {
                            p.finger_draw = !p.finger_draw;
                        }
                    }
                    Tool::Highlighter => {
                        if let Some(c) = ui::swatches(ui, &HIGHLIGHT_COLORS, &p.highlighter.color, false) {
                            p.highlighter.color = c;
                        }
                        sep_h(ui, t, h);
                        slide(ui, "Spessore", &mut p.highlighter.size, 4.0, 96.0, 0.0, true, PX, sw(196.0));
                    }
                    Tool::Tape => {
                        if let Some(c) = ui::swatches(ui, &TAPE_COLORS, &p.tape.color, false) {
                            p.tape.color = c;
                        }
                        sep_h(ui, t, h);
                        slide(ui, "Larghezza", &mut p.tape.size, 8.0, 120.0, 0.0, true, PX, sw(204.0));
                    }
                    Tool::Eraser => {
                        ui::segmented(ui, &mut p.eraser.mode, &[(EraserMode::Pixel, "Pixel"), (EraserMode::Stroke, "Tratto intero")], sw(196.0));
                        sep_h(ui, t, h);
                        let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                        let d = (p.eraser.size as f32).clamp(4.0, 26.0);
                        ui.painter().circle(r.center(), d / 2.0, t.bg2, Stroke::new(1.0, t.border_strong));
                        slide(ui, "Dimensione", &mut p.eraser.size, 2.0, 240.0, 0.0, true, PX, sw(212.0));
                        if p.eraser.mode == EraserMode::Pixel {
                            slide(ui, "Forza", &mut p.eraser.strength, 0.1, 1.0, 0.05, false, PCT, sw(164.0));
                        }
                    }
                    Tool::Shape | Tool::Line | Tool::Arrow => {
                        // FigJam's row: the colour, the connectors, the shapes used most, and
                        // the rest in the shapes panel.
                        if tool == Tool::Shape {
                            color_pop(ui, "Riempimento", &mut p.shape_style.fill, true, false, top, t);
                        } else {
                            color_pop(ui, "Colore", &mut p.shape_style.stroke, false, true, top, t);
                        }
                        sep_h(ui, t, h);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (route, label) in [(crate::model::Route::Elbow, "Connettore a gomito"), (crate::model::Route::Curved, "Connettore curvo"), (crate::model::Route::Straight, "Freccia")] {
                            let on = tool == Tool::Arrow && p.route == route;
                            let r = icon_button(ui, "", label, Some("X"), vec2(30.0, 30.0), 16.0, false, on);
                            connector_icon(ui, Some(route), r.rect.center(), if on { Color32::WHITE } else { t.icon });
                            if r.clicked() {
                                p.route = route;
                                want = Some(Tool::Arrow);
                            }
                        }
                        let on = tool == Tool::Line;
                        let r = icon_button(ui, "", "Linea", Some("L"), vec2(30.0, 30.0), 16.0, false, on);
                        connector_icon(ui, None, r.rect.center(), if on { Color32::WHITE } else { t.icon });
                        if r.clicked() {
                            want = Some(Tool::Line);
                        }
                        ui.spacing_mut().item_spacing.x = 6.0;
                        sep_h(ui, t, h);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for k in QUICK_SHAPES {
                            let on = tool == Tool::Shape && p.last_shape == k;
                            let label = SHAPES.iter().find(|s| s.0 == k).map_or("Forma", |s| s.1);
                            let r = icon_button(ui, "", label, None, vec2(30.0, 30.0), 16.0, false, on);
                            shape_icon(ui, k, r.rect.center(), 18.0, if on { Color32::WHITE } else { t.icon });
                            if r.clicked() {
                                p.last_shape = k;
                                want = Some(Tool::Shape);
                            }
                        }
                        ui.spacing_mut().item_spacing.x = 6.0;
                        sep_h(ui, t, h);
                        if ui::button(ui, "Altre forme", ui::Kind::Secondary, None, false, true).clicked() {
                            ui.ctx().data_mut(|d| d.insert_temp(Id::new("shapes-panel"), true));
                        }
                    }
                    Tool::Text => {
                        font_picker(ui, &mut p.text.font, sw(168.0), t);
                        sep_h(ui, t, h);
                        color_pop(ui, "Colore testo", &mut p.text.color, false, false, top, t);
                        let mut size = p.text.font_size.round() as i64;
                        if ui::segmented(ui, &mut size, &[(16, "S"), (24, "M"), (36, "L"), (56, "XL")], sw(150.0)) {
                            p.text.font_size = size as f64;
                        }
                        if let Some(v) = ui::Num::new(Id::new("tray-font-size"), "icon:text-size", "Dimensione del testo", Some(p.text.font_size)).width(76.0).range(4.0, 400.0).show(ui) {
                            p.text.font_size = v;
                        }
                    }
                    Tool::Sticky => {
                        if let Some(c) = ui::swatches(ui, &STICKY_COLORS, &p.sticky_color, false) {
                            p.sticky_color = c;
                        }
                    }
                    Tool::Stamp => {
                        if let Some(e) = stamp_picker(ui, Some(&p.stamp)) {
                            p.stamp = e;
                        }
                    }
                    Tool::Table => table_size_picker(ui, &mut p.table_size, t),
                    Tool::Laser => {
                        ui.label(RichText::new("Colore del laser").color(t.text2));
                        if let Some(c) = ui::swatches(ui, &LASER_COLORS, &p.laser_color, false) {
                            p.laser_color = c;
                        }
                    }
                    _ => {}
                }
                // Closes the options; the tool stays. Picking the tool again brings them back.
                sep_h(ui, t, h);
                if icon_button(ui, if top { "chevron-up" } else { "chevron-down" }, "Nascondi le opzioni", None, vec2(24.0, 24.0), 14.0, false, false).clicked() {
                    close = true;
                }
            });
            }
        });
    });
    if close {
        ed.prefs.tray = false;
    }
    if let Some(tool) = want {
        ed.set_tool(tool);
        ed.prefs.tray = true;
    }
    // Its natural width, measured whenever it is laid out in full on one row; when it wraps in
    // full, it is known to be too wide.
    if !narrow {
        let one_row = shown.response.rect.height() < h + 20.0;
        let w = if one_row { shown.response.rect.width() } else { max_tray_w + 1.0 };
        ctx.data_mut(|d| d.insert_temp(natural_id, w));
    }
    Some(shown.response.rect)
}

/// The size of the next table: a grid of cells, those up to the pointer lit, as in Word.
fn table_size_picker(ui: &mut Ui, size: &mut [u8; 2], t: &Theme) {
    const COLS: usize = 8;
    const ROWS: usize = 5;
    const CELL: f32 = 12.0;
    const GAP: f32 = 2.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(COLS as f32 * (CELL + GAP) - GAP, ROWS as f32 * (CELL + GAP) - GAP), Sense::click());
    let cell_at = |p: Pos2| (((p.y - rect.min.y) / (CELL + GAP)).floor().clamp(0.0, ROWS as f32 - 1.0) as u8 + 1, ((p.x - rect.min.x) / (CELL + GAP)).floor().clamp(0.0, COLS as f32 - 1.0) as u8 + 1);
    let shown = resp.hover_pos().map(cell_at).unwrap_or((size[0].min(ROWS as u8), size[1].min(COLS as u8)));
    for r in 0..ROWS {
        for c in 0..COLS {
            let cell = Rect::from_min_size(rect.min + vec2(c as f32 * (CELL + GAP), r as f32 * (CELL + GAP)), vec2(CELL, CELL));
            let on = (r as u8) < shown.0 && (c as u8) < shown.1;
            ui.painter().rect(cell, 3.0, if on { t.brand.gamma_multiply(0.18) } else { t.bg2 }, Stroke::new(1.0, if on { t.brand } else { t.border }), egui::StrokeKind::Inside);
        }
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let (r, c) = cell_at(p);
        *size = [r, c];
    }
    ui.add_space(4.0);
    ui.allocate_ui_with_layout(vec2(44.0, rect.height()), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.label(RichText::new(format!("{} × {}", shown.0, shown.1)).font(ui::medium(ui::px(12.0))).color(t.text));
    });
}


/// Colours of the laser pointer.
pub const LASER_COLORS: [&str; 6] = ["#FF3B30", "#FF9500", "#34C759", "#0D99FF", "#AF52DE", "#FF2D92"];

/// A pen in the tray, drawn as a real stroke in its colour, thickness and opacity. The active
/// one opens its colour and stroke settings.
fn pen_slot(ui: &mut Ui, ed: &mut Editor, i: usize, t: &Theme, top: bool) {
    let pen: Pen = ed.prefs.pens[i].clone();
    let active = ed.pen == i;
    let (rect, resp) = ui.allocate_exact_size(vec2(48.0, 32.0), Sense::click());
    let sel = ui::motion::hover(ui.ctx(), resp.id.with("sel"), active);
    if sel > 0.0 {
        ui.painter().rect_filled(rect, ui::RADIUS, t.selected.gamma_multiply(sel));
    }
    ui::hover_fill(ui, resp.id, rect, resp.hovered() && !active, ui::RADIUS as f32, t.hover);
    let pts: Vec<f32> = (0..=24)
        .flat_map(|k| {
            let s = k as f32 / 24.0;
            [4.0 + s * 40.0, 16.0 + (s * std::f32::consts::TAU).sin() * 6.0, 0.35 + (s * std::f32::consts::PI).sin() * 0.55]
        })
        .collect();
    let path = crate::ink::stroke_path(&pts, (pen.size * 1.2).min(12.0), false).translated(rect.min.x, rect.min.y);
    let color = crate::model::color_or(&pen.color, crate::model::DARK);
    let color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (pen.opacity.clamp(0.1, 1.0) * 255.0) as u8);
    ui.painter().add(crate::paint::fill_mesh(&path, color));
    let label = format!("Penna {}", i + 1);
    let resp = ui::tip(resp, if active { "Colore della penna" } else { &label }, Some(&(i + 1).to_string()));
    if resp.clicked() && !active {
        ed.pen = i;
    }
    if active {
        egui::Popup::from_toggle_button_response(&resp).frame(ui::float_frame(t).inner_margin(egui::Margin::same(12))).align(if top { egui::RectAlign::BOTTOM } else { egui::RectAlign::TOP }).gap(12.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(232.0);
            ui::heading(ui, "Colore");
            let pen = &mut ed.prefs.pens[i];
            if let Some(c) = ui::swatches(ui, &INK_COLORS, &pen.color, false) {
                pen.color = c;
            }
            ui.add_space(4.0);
            ui::heading(ui, "Tratto");
            ui::slider(ui, "Spessore", &mut pen.size, 1.0, 64.0, 0.5, true, PX, 208.0);
            ui::slider(ui, "Opacità", &mut pen.opacity, 0.1, 1.0, 0.05, false, PCT, 208.0);
        });
    }
}

/// A colour chip opening a palette (with "none" for fills).
pub fn color_pop(ui: &mut Ui, label: &str, value: &mut String, transparent: bool, ring: bool, top: bool, t: &Theme) {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    ui::hover_fill(ui, resp.id, rect, resp.hovered(), ui::RADIUS as f32, t.hover);
    let chip = rect.shrink(5.0);
    let c = crate::model::parse_color(value).unwrap_or(Color32::TRANSPARENT);
    if value == "transparent" {
        ui.painter().rect_filled(chip, 9.0, Color32::WHITE);
        ui.painter().line_segment([chip.left_bottom(), chip.right_top()], Stroke::new(1.5, t.danger));
    } else if ring {
        ui.painter().circle_stroke(chip.center(), chip.width() / 2.0 - 2.0, Stroke::new(4.0, c));
    } else {
        ui.painter().circle_filled(chip.center(), chip.width() / 2.0, c);
    }
    ui.painter().circle_stroke(chip.center(), chip.width() / 2.0, Stroke::new(1.0, Color32::from_black_alpha(30)));
    let shown = if value == "transparent" { "nessuno".to_string() } else { value.clone() };
    let resp = ui::tip(resp, &format!("{label}: {shown}"), None);
    egui::Popup::from_toggle_button_response(&resp).frame(ui::float_frame(t).inner_margin(egui::Margin::same(12))).align(if top { egui::RectAlign::BOTTOM } else { egui::RectAlign::TOP }).gap(12.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(208.0);
        ui::heading(ui, label);
        if let Some(c) = ui::swatches(ui, &INK_COLORS, value, transparent) {
            *value = c;
        }
    });
}

/// Font menu: each name written in its own font.
pub fn font_picker(ui: &mut Ui, value: &mut FontKind, width: f32, t: &Theme) -> bool {
    let name = FONTS.iter().find(|f| f.0 == *value).map_or("Inter", |f| f.1);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ui::ctl(24.0)), Sense::click());
    ui.painter().rect_filled(rect, ui::RADIUS, t.bg2);
    let k = ui::motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
    if k > 0.0 {
        ui.painter().rect_stroke(rect, ui::RADIUS, Stroke::new(1.0, t.border_strong.gamma_multiply(k)), egui::StrokeKind::Inside);
    }
    let family = egui::FontFamily::Name(crate::text::prefix(*value).into());
    ui.painter().text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::new(ui::px(12.0), family), t.text);
    icon(ui, "chevron-down", rect.right_center() - vec2(12.0, 0.0), 12.0, t.icon2);
    let resp = ui::tip(resp, "Carattere", None);
    let mut changed = false;
    egui::Popup::menu(&resp).frame(ui::menu_frame(t)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut group = "";
        for (k, name, g) in FONTS {
            if g != group {
                group = g;
                ui.add_space(4.0);
                ui.label(RichText::new(g).size(ui::px(10.0)).color(t.menu_text2));
            }
            let (r, item) = ui.allocate_exact_size(vec2(200.0, 26.0), Sense::click());
            if item.hovered() {
                ui.painter().rect_filled(r, ui::RADIUS, t.brand_fill);
            }
            if k == *value {
                icon(ui, "check", r.left_center() + vec2(10.0, 0.0), 12.0, t.menu_text);
            }
            let fam = egui::FontFamily::Name(crate::text::prefix(k).into());
            ui.painter().text(r.left_center() + vec2(24.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::new(13.0, fam), t.menu_text);
            if item.clicked() {
                *value = k;
                changed = true;
            }
        }
    });
    changed
}

/// The reactions, drawn with the same pictures as on the board, eight to a row.
pub fn stamp_picker(ui: &mut Ui, value: Option<&str>) -> Option<String> {
    let t = ui::theme(ui.ctx());
    const CELL: f32 = 32.0;
    let cols = 8;
    let rows = crate::assets::STAMPS.len().div_ceil(cols);
    let (area, _) = ui.allocate_exact_size(vec2(CELL * cols as f32, CELL * rows as f32), Sense::hover());
    let mut picked = None;
    for (i, s) in crate::assets::STAMPS.iter().enumerate() {
        let r = Rect::from_min_size(area.min + vec2((i % cols) as f32 * CELL, (i / cols) as f32 * CELL), vec2(CELL, CELL)).shrink(1.0);
        let resp = ui.interact(r, ui.id().with(("stamp", i)), Sense::click());
        let on = value == Some(s.emoji);
        if on {
            ui.painter().rect_filled(r, ui::RADIUS, t.selected);
        }
        ui::hover_fill(ui, resp.id, r, resp.hovered() && !on, ui::RADIUS as f32, t.hover);
        // A little bigger under the pointer, as if picked up.
        let k = ui::motion::hover(ui.ctx(), resp.id.with("grow"), resp.hovered());
        let size = 24.0 + 3.0 * k;
        ui::icons::bitmap(ui, &format!("stamp:{}", s.emoji), Rect::from_center_size(r.center(), vec2(size, size)), |px| crate::assets::stamp_pixmap(s, px));
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, s.name));
        if ui::tip(resp, s.name, None).clicked() {
            picked = Some(s.emoji.to_string());
        }
    }
    picked
}

/* ---------------- zoom controls ---------------- */

/// Bottom right, Whiteboard style: minimap on/off, zoom in, zoom level (menu), zoom out.
pub fn view_controls(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let t = ui::theme(ctx);
    // In the bottom right corner; when the toolbar, its options or the minimap are there, the
    // first free place nearby: above the minimap, beside it, above the toolbar. It glides there.
    let bar = ctx.data(|d| d.get_temp::<Rect>(Id::new("bottom-bars")));
    let size = ctx.data(|d| d.get_temp::<egui::Vec2>(Id::new("view-controls-size"))).unwrap_or(vec2(40.0, 130.0));
    let corner = stage.max - vec2(16.0, 16.0);
    let mini = if ed.prefs.minimap { ctx.data(|d| d.get_temp::<Rect>(Id::new("minimap-rect"))) } else { None };
    let obstacles: Vec<Rect> = [bar, mini].into_iter().flatten().map(|r| r.expand(8.0)).collect();
    let free = |p: Pos2| {
        let r = Rect::from_min_max(p - size, p);
        r.min.y >= stage.min.y + 8.0 && r.min.x >= stage.min.x + 8.0 && !obstacles.iter().any(|o| o.intersects(r))
    };
    let mut spots = vec![corner];
    if let Some(m) = mini {
        spots.push(pos2(corner.x, m.min.y - 10.0));
        spots.push(pos2(m.min.x - 10.0, corner.y));
    }
    if let Some(b) = bar {
        spots.push(pos2(corner.x, b.min.y - 12.0));
        if let Some(m) = mini {
            spots.push(pos2(corner.x, b.min.y.min(m.min.y) - 12.0));
            spots.push(pos2(m.min.x - 10.0, b.min.y - 12.0));
        }
    }
    let target = spots.iter().copied().find(|p| free(*p)).unwrap_or(*spots.last().unwrap());
    let x = ui::motion::spring(ctx, Id::new("view-controls-x"), target.x, 0.3, 0.9);
    let y = ui::motion::spring(ctx, Id::new("view-controls-y"), target.y, 0.3, 0.9);
    let resp = egui::Area::new(Id::new("view-controls")).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(pos2(x, y)).order(egui::Order::Foreground).show(ctx, |ui| {
        if !ed.prefs.zoom_full {
            // FigJam's: a round-ended pill with − and +; the percentage on hover, the rest of the
            // zoom (fit, 100%, the minimap) in the menu of a right click or a long press.
            ui::float_frame(&t).corner_radius(18).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let pct = format!("{}%", (ed.cam.z * 100.0).round());
                    let pill = |ui: &mut Ui, name: &str, round: egui::CornerRadius| {
                        let (r, resp) = ui.allocate_exact_size(vec2(38.0, 32.0), Sense::click());
                        let h = ui::motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
                        let d = ui::motion::hover(ui.ctx(), resp.id.with("d"), resp.is_pointer_button_down_on());
                        if h > 0.0 {
                            ui.painter().rect_filled(r, round, ui::blend(t.hover.gamma_multiply(h), t.press, d));
                        }
                        icon(ui, name, r.center(), 16.0, t.icon);
                        resp
                    };
                    let minus = pill(ui, "minus", egui::CornerRadius { nw: 16, sw: 16, ne: 0, se: 0 });
                    let (line, _) = ui.allocate_exact_size(vec2(1.0, 32.0), Sense::hover());
                    ui.painter().vline(line.center().x, line.y_range().shrink(7.0), Stroke::new(1.0, t.border));
                    let plus = pill(ui, "plus", egui::CornerRadius { nw: 0, sw: 0, ne: 16, se: 16 });
                    let minus = ui::tip(minus, &format!("Riduci · {pct}"), Some("Ctrl+−"));
                    let plus = ui::tip(plus, &format!("Ingrandisci · {pct}"), Some("Ctrl++"));
                    if minus.clicked() {
                        ed.zoom_by(0.8);
                    }
                    if plus.clicked() {
                        ed.zoom_by(1.25);
                    }
                    let both = minus.union(plus);
                    egui::Popup::context_menu(&both).frame(ui::menu_frame(&t)).show(|ui| zoom_items(ui, ed));
                });
            });
            return;
        }
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                let on = ed.prefs.minimap;
                if icon_button(ui, "map", if on { "Nascondi la minimappa" } else { "Mostra la minimappa" }, None, vec2(32.0, 28.0), 16.0, on, false).clicked() {
                    ed.prefs.minimap = !on;
                }
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 5.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
                if icon_button(ui, "plus", "Ingrandisci", Some("Ctrl++"), vec2(32.0, 28.0), 16.0, false, false).clicked() {
                    ed.zoom_by(1.25);
                }
                let label = format!("{}%", (ed.cam.z * 100.0).round());
                let (r, resp) = ui.allocate_exact_size(vec2(32.0, 24.0), Sense::click());
                ui::hover_fill(ui, resp.id, r, resp.hovered(), ui::RADIUS as f32, t.hover);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, &label, egui::FontId::proportional(ui::px(10.0)), t.text);
                egui::Popup::menu(&resp).frame(ui::menu_frame(&t)).align(egui::RectAlign::LEFT_END).show(|ui| zoom_items(ui, ed));
                if icon_button(ui, "minus", "Riduci", Some("Ctrl+−"), vec2(32.0, 28.0), 16.0, false, false).clicked() {
                    ed.zoom_by(0.8);
                }
            });
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(Id::new("view-controls-size"), resp.response.rect.size());
        d.insert_temp(Id::new("view-controls-rect"), resp.response.rect);
    });
}

/// Zoom menu entries, shared by the Design panel and the zoom controls.
pub fn zoom_items(ui: &mut Ui, ed: &mut Editor) {
    ui.spacing_mut().item_spacing.y = 0.0;
    if ui::menu_item(ui, None, "Ingrandisci", Some("Ctrl++"), false).clicked() {
        ed.zoom_by(1.25);
    }
    if ui::menu_item(ui, None, "Riduci", Some("Ctrl+−"), false).clicked() {
        ed.zoom_by(0.8);
    }
    if ui::menu_item(ui, None, "Adatta alla lavagna", Some("Maiusc+1"), false).clicked() {
        ed.fit();
    }
    if ui::menu_item(ui, None, "Adatta alla selezione", Some("Maiusc+2"), false).clicked() {
        ed.fit_selection();
    }
    ui::menu_sep(ui);
    for (z, label, kbd) in [(0.5, "50%", None), (1.0, "100%", Some("Maiusc+0")), (2.0, "200%", None)] {
        if ui::menu_item(ui, None, label, kbd, false).clicked() {
            ed.zoom_to(z);
        }
    }
    ui::menu_sep(ui);
    let on = ed.prefs.minimap;
    if ui::menu_item(ui, Some("map"), if on { "Nascondi la minimappa" } else { "Mostra la minimappa" }, None, false).clicked() {
        ed.prefs.minimap = !on;
    }
}

/* ---------------- minimap ---------------- */

const MW: f32 = 200.0;
const MH: f32 = 132.0;
/// Card padding: the map's corners (8) sit concentric inside the card's (12).
const PAD: f32 = 4.0;

/// Map pixel = board point · k + (x, y).
#[derive(Clone, Copy, Default, PartialEq)]
struct Fit {
    k: f64,
    x: f64,
    y: f64,
}

fn fit_for(b: BBox, w: f64, h: f64) -> Fit {
    let pad = 0.08 * b.w.max(b.h).max(1.0);
    let k = (w / (b.w + pad * 2.0)).min(h / (b.h + pad * 2.0));
    Fit { k, x: (w - b.w * k) / 2.0 - b.x * k, y: (h - b.h * k) / 2.0 - b.y * k }
}

#[derive(Default)]
pub struct Minimap {
    tex: Option<egui::TextureHandle>,
    /// How the drawn picture maps the board.
    shown: Option<Fit>,
    /// Board version in the picture, the newest one seen and when it was seen.
    drawn: u64,
    seen: u64,
    seen_at: f64,
    /// Pointer offset from the view centre while dragging, in board units.
    grab: Option<(f64, f64)>,
    /// The map's scale while dragging in it: still, or the point under the pointer would run off.
    frozen: Option<Fit>,
    /// Pointer offset from the card's corner while the card itself is being moved.
    moving: Option<egui::Vec2>,
    /// The picture being drawn away from the interface, and how it maps the board.
    job: Option<std::sync::mpsc::Receiver<(Option<egui::ColorImage>, Fit)>>,
}

/// What the minimap's picture frames: the content, but never less than a screenful at 100%
/// around it, so a single dot doesn't fill the map.
fn map_area(content: Option<BBox>, view: BBox, screen: (f64, f64)) -> BBox {
    let c = content.unwrap_or(BBox { x: view.center().x, y: view.center().y, w: 0.0, h: 0.0 });
    let (w, h) = (c.w.max(screen.0 * 0.8), c.h.max(screen.1 * 0.8));
    BBox { x: c.x + c.w / 2.0 - w / 2.0, y: c.y + c.h / 2.0 - h / 2.0, w, h }
}

/// The part of `lo..hi` inside `min..max`, at least `least` long and never outside: when the
/// range is off to one side the marker waits at that edge.
fn clamp_span(lo: f32, hi: f32, min: f32, max: f32, least: f32) -> (f32, f32) {
    let (a, b) = (lo.max(min), hi.min(max));
    if b - a >= least {
        return (a, b);
    }
    let c = ((lo + hi) / 2.0).clamp(min + least / 2.0, max - least / 2.0);
    (c - least / 2.0, c + least / 2.0)
}

/// Overview of the board and of the visible area; click or drag to move there. The card can be
/// put anywhere over the board by its grip (it sticks to the edges), and stays there.
///
/// The picture shows the content and is redrawn only after the board has stayed unchanged for a
/// moment: drawing it costs tens of milliseconds on a big board, and moving around never needs it
/// (the view rectangle is drawn on top, clamped to the edge when you are off the content).
pub fn minimap(ctx: &egui::Context, free: Rect, ed: &mut Editor, m: &mut Minimap) {
    let t = ui::theme(ctx);
    let now = crate::platform::now_ms();
    let version = ed.board.version;
    if m.seen != version {
        m.seen = version;
        m.seen_at = now;
    }
    let (mw, mh) = ((MW - PAD * 2.0) as f64, (MH - PAD * 2.0) as f64);
    let quiet = now - m.seen_at;
    // The picture is drawn on another thread (tens of milliseconds on a big board, which
    // would make typing and drawing stutter), one at a time.
    if m.job.is_none() && (m.tex.is_none() || (m.drawn != version && m.grab.is_none() && quiet > 300.0)) {
        m.drawn = version;
        let content = union(ed.board.all().iter().filter(|e| !e.hidden).map(|e| crate::geom::aabb(e)));
        let fit = fit_for(map_area(content, ed.view(), ed.size), mw, mh);
        let ppp = ctx.pixels_per_point() as f64;
        let (w, h) = ((mw * ppp) as u32, (mh * ppp) as u32);
        let b = BBox { x: -fit.x / fit.k, y: -fit.y / fit.k, w: mw / fit.k, h: mh / fit.k };
        let els = ed.board.paint_order().to_vec();
        let meta = ed.board.meta();
        let dark = crate::model::is_dark(&meta.background);
        let bg = crate::model::color_or(&meta.background, Color32::from_gray(0xF5));
        // The pictures it needs, taken along (the image cache stays here).
        let mut pics = std::collections::HashMap::new();
        for el in &els {
            let key = match &el.kind {
                crate::model::Kind::Image { file_id } => crate::prims::ImageKey::File(file_id.clone()),
                crate::model::Kind::Stamp { emoji } => crate::prims::ImageKey::Stamp(emoji.clone()),
                _ => continue,
            };
            if let Some(p) = ed.painter.images.pixmap(&key) {
                pics.insert(key, p);
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        m.job = Some(rx);
        let c = ctx.clone();
        let draw = move || {
            let env = crate::prims::Env { zoom: fit.k, pixel: Some(1.0 / (fit.k * ppp)), hairline: true, dark_board: dark, ..Default::default() };
            let img = crate::raster::render(&els, Some(bg), w, h, b, &env, &|k| pics.get(k).cloned()).map(|pm| egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()));
            let _ = tx.send((img, fit));
            c.request_repaint();
        };
        // A browser page has no threads to spare: there it is drawn at once.
        #[cfg(target_arch = "wasm32")]
        draw();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(draw);
    } else if m.drawn != version {
        ctx.request_repaint_after(std::time::Duration::from_millis((310.0 - quiet).max(10.0) as u64));
    }
    if let Some(rx) = &m.job {
        match rx.try_recv() {
            Ok((img, fit)) => {
                m.job = None;
                if let Some(img) = img {
                    m.shown = Some(fit);
                    match &mut m.tex {
                        Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                        None => m.tex = Some(ctx.load_texture("minimap", img, egui::TextureOptions::LINEAR)),
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => m.job = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
    let Some(pic) = m.shown else { return };
    let view = ed.view();
    // The map always takes in where you are: zoomed far out (or off the content) it widens
    // around both, the picture drawn smaller inside it.
    let area = BBox { x: -pic.x / pic.k, y: -pic.y / pic.k, w: mw / pic.k, h: mh / pic.k };
    let inside = view.x >= area.x && view.y >= area.y && view.right() <= area.right() && view.bottom() <= area.bottom();
    let f = match m.frozen {
        Some(f) => f,
        None if inside => pic,
        None => fit_for(union([area, view]).unwrap_or(area), mw, mh),
    };
    let k = ui::motion::appear(ctx, Id::new("minimap"), true);
    // Where the card sits: a fraction of the room it can move in, so it keeps its corner when
    // the window changes size. Dragged, it follows the pointer; let go, it glides into place.
    let room = free.shrink(12.0);
    let range = vec2((room.width() - MW).max(0.0), (room.height() - MH).max(0.0));
    let at = ed.prefs.minimap_at.unwrap_or([1.0, 0.0]);
    let target = room.min + vec2(at[0] * range.x, at[1] * range.y);
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let (px, py) = match (m.moving, pointer) {
        (Some(grab), Some(p)) => {
            let p = (p - grab).clamp(room.min, room.min + range);
            ui::motion::hold(ctx, Id::new("minimap-x"), p.x);
            ui::motion::hold(ctx, Id::new("minimap-y"), p.y);
            (p.x, p.y)
        }
        _ => (ui::motion::spring(ctx, Id::new("minimap-x"), target.x, 0.32, 0.86), ui::motion::spring(ctx, Id::new("minimap-y"), target.y, 0.32, 0.86)),
    };
    // Never under the toolbar or its options: up above them when it would be.
    let bars = ctx.data(|d| d.get_temp::<Rect>(Id::new("bottom-bars")));
    let card = Rect::from_min_size(pos2(px, py), vec2(MW, MH));
    let push = match bars {
        Some(b) if m.moving.is_none() && card.expand(6.0).intersects(b) => {
            if b.center().y > free.center().y { (card.max.y - b.min.y + 10.0).max(0.0) } else { -(b.max.y - card.min.y + 10.0).max(0.0) }
        }
        _ => 0.0,
    };
    let push = ui::motion::spring(ctx, Id::new("minimap-push"), push, 0.3, 0.9);
    let pos = pos2(px, py - push - ui::motion::travel(ctx, k) * 8.0);
    let resp = egui::Area::new(Id::new("minimap")).fixed_pos(pos).order(egui::Order::Middle).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        let (card, resp) = ui.allocate_exact_size(vec2(MW, MH), Sense::click_and_drag());
        let radius = egui::CornerRadius::same(12);
        for s in ui::card_shadows(&t) {
            ui.painter().add(s.as_shape(card, radius));
        }
        ui.painter().rect(card, radius, t.bg, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let map = card.shrink(PAD);
        let bg = crate::model::color_or(&ed.board.meta().background, Color32::from_gray(0xF5));
        let to = |x: f64, y: f64| map.min + vec2((x * f.k + f.x) as f32, (y * f.k + f.y) as f32);
        let fill = egui::epaint::RectShape::filled(map, egui::CornerRadius::same(8), if m.tex.is_some() && f == pic { Color32::WHITE } else { bg });
        match &m.tex {
            Some(tex) if f == pic => ui.painter().add(fill.with_texture(tex.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)))),
            Some(tex) => {
                ui.painter().add(fill);
                let r = Rect::from_min_max(to(area.x, area.y), to(area.right(), area.bottom()));
                // Too small to make out: a mark where the drawing is.
                if r.width() < 10.0 {
                    ui.painter().rect_filled(Rect::from_center_size(r.center(), r.size().max(vec2(8.0, 8.0))), 2.0, t.text3.gamma_multiply(0.6));
                }
                ui.painter().with_clip_rect(map).image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE)
            }
            None => ui.painter().add(fill),
        };
        // Where you are: kept inside the map on each axis, so it shows the way back when you
        // are far off and never spills over the card.
        let vr = Rect::from_min_max(to(view.x, view.y), to(view.right(), view.bottom()));
        let inner = map.shrink(1.5);
        let (x0, x1) = clamp_span(vr.min.x, vr.max.x, inner.min.x, inner.max.x, 6.0);
        let (y0, y1) = clamp_span(vr.min.y, vr.max.y, inner.min.y, inner.max.y, 6.0);
        let v = Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
        // Everything in view: no rectangle, the map alone says it.
        if !vr.contains_rect(inner) {
            ui.painter().with_clip_rect((map).intersect(ui.clip_rect())).rect(v, egui::CornerRadius::same(3), t.brand.gamma_multiply(0.06), Stroke::new(1.5, t.brand), egui::StrokeKind::Inside);
        }
        let point = |p: Pos2| ((p.x.clamp(map.min.x, map.max.x) - map.min.x) as f64 - f.x) / f.k;
        let point_y = |p: Pos2| ((p.y.clamp(map.min.y, map.max.y) - map.min.y) as f64 - f.y) / f.k;
        if resp.drag_started() || resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (px, py) = (point(p), point_y(p));
                let inside = view.contains(crate::geom::pt(px, py));
                // Grabbing the rectangle drags it from where it was taken; elsewhere the view centres on the click.
                m.grab = Some(if inside { (px - view.center().x, py - view.center().y) } else { (0.0, 0.0) });
                m.frozen = Some(f);
            }
        }
        if let (Some(g), Some(p)) = (m.grab, resp.interact_pointer_pos()) {
            ed.center_on(crate::geom::pt(point(p) - g.0, point_y(p) - g.1));
            // Zoomed out far beyond the drawing, a click goes there close enough to see it.
            if resp.clicked() && f != pic {
                let z = (ed.size.0 / area.w).min(ed.size.1 / area.h) * 0.9;
                let (cx, cy) = (ed.size.0 / 2.0, ed.size.1 / 2.0);
                let (x, y) = (point(p), point_y(p));
                ed.fly_to(crate::geom::Camera { x: cx - x * z, y: cy - y * z, z });
            }
        }
        if resp.drag_stopped() || resp.clicked() {
            m.grab = None;
            m.frozen = None;
        }
        let over = ui.rect_contains_pointer(card);
        ui::tip(resp, "Minimappa", None);
        // The close button shows while the pointer is over the map, like Figma's hover controls.
        let close = Rect::from_center_size(map.right_top() + vec2(-13.0, 13.0), vec2(20.0, 20.0));
        let c = ui.interact(close, Id::new("minimap-close"), Sense::click());
        let a = ui::motion::hover(ctx, Id::new("minimap-close-show"), over || c.has_focus());
        if a > 0.01 {
            let h = ui::motion::hover(ctx, c.id, c.hovered());
            ui.painter().circle(close.center(), 10.0, ui::mix(t.hover, h, t.bg).gamma_multiply(a), Stroke::new(1.0, t.border.gamma_multiply(a)));
            icon(ui, "x", close.center(), 12.0, t.icon.gamma_multiply(a));
        }
        ui::focus_ring(ui, &c, close);
        if ui::tip(c, "Chiudi la minimappa", None).clicked() {
            ed.prefs.minimap = false;
            ui::motion::reset(ctx, Id::new("minimap"));
        }
        // The grip, top left: drag it to put the minimap anywhere over the board.
        let grip = Rect::from_center_size(map.left_top() + vec2(13.0, 13.0), vec2(20.0, 20.0));
        let g = ui.interact(grip, Id::new("minimap-grip"), Sense::drag()).on_hover_cursor(egui::CursorIcon::Grab);
        let ga = ui::motion::hover(ctx, Id::new("minimap-grip-show"), over || g.dragged() || g.has_focus());
        if ga > 0.01 {
            let h = ui::motion::hover(ctx, g.id, g.hovered() || g.dragged());
            ui.painter().circle(grip.center(), 10.0, ui::mix(t.hover, h, t.bg).gamma_multiply(ga), Stroke::new(1.0, t.border.gamma_multiply(ga)));
            icon(ui, "grip", grip.center(), 12.0, t.icon.gamma_multiply(ga));
        }
        if g.drag_started()
            && let Some(p) = g.interact_pointer_pos()
        {
            m.moving = Some(p - card.min);
        }
        if g.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if g.drag_stopped() {
            m.moving = None;
            // Within a short reach of an edge it sticks to it, as windows do.
            let reach = 28.0;
            let frac = |v: f32, lo: f32, span: f32| -> f32 {
                if span <= 0.0 {
                    return 0.0;
                }
                let k = (v - lo) / span;
                if v - lo < reach {
                    0.0
                } else if lo + span - v < reach {
                    1.0
                } else {
                    k.clamp(0.0, 1.0)
                }
            };
            ed.prefs.minimap_at = Some([frac(card.min.x, room.min.x, range.x), frac(card.min.y, room.min.y, range.y)]);
        }
        ui::tip(g, "Trascina per spostare la minimappa", None);
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("minimap-rect"), resp.response.rect));
}
