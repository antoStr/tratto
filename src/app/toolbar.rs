//! The floating toolbar with its contextual tray, the zoom controls and the minimap.

use egui::{Color32, Id, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::board::BoardUi;
use crate::editor::{Editor, Tool};
use crate::geom::{BBox, union};
use crate::model::{FONTS, FontKind, HIGHLIGHT_COLORS, INK_COLORS, STICKY_COLORS, ShapeKind, TAPE_COLORS};
use crate::prefs::{EraserMode, Pen, ShapeTool, ToolbarPos};
use crate::ui::{self, Theme, icon, icon_button};

const ICON: f32 = 22.0;
const BTN: f32 = 40.0;

pub const SHAPES: [(ShapeTool, &str, Option<&str>); 21] = [
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
];

/// The paths a connector can take, as FigJam offers them.
pub const ROUTES: [(crate::model::Route, &str, &str); 3] = [
    (crate::model::Route::Straight, "route-straight", "Dritta"),
    (crate::model::Route::Elbow, "route-elbow", "A gomito"),
    (crate::model::Route::Curved, "route-curved", "Curva"),
];

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
        ShapeTool::Kind(k @ (ShapeKind::Pill | ShapeKind::Process | ShapeKind::Document | ShapeKind::Cylinder | ShapeKind::Speech | ShapeKind::Chevron | ShapeKind::Trapezoid)) => {
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
    sep_h(ui, t, BTN);
}

/// Height of the tray's row for a tool: everything in it sits on one centre line.
const TRAY_H: f32 = 32.0;

fn tool_button(ui: &mut Ui, ed: &mut Editor, tool: Tool, name: &str, label: &str, kbd: Option<&str>) -> egui::Response {
    let r = icon_button(ui, name, label, kbd, vec2(BTN, BTN), ICON, false, ed.tool == tool);
    if r.clicked() {
        ed.set_tool(tool);
    }
    r
}

/// A bar of the colour the tool will use under its icon, like Office's font-colour button.
/// FigJam's "+": code, widgets, pictures and templates, in a light card of big tiles.
fn more_tools(ui: &mut Ui, ed: &mut Editor, t: &Theme, top: bool) {
    let r = icon_button(ui, "plus", "Altri strumenti", None, vec2(BTN, BTN), ICON, false, false);
    egui::Popup::menu(&r).frame(ui::float_frame(t).inner_margin(egui::Margin::same(8))).align(if top { egui::RectAlign::BOTTOM } else { egui::RectAlign::TOP }).gap(10.0).show(|ui| {
        use crate::model::{CheckItem, PollOption, Widget};
        ui.label(egui::RichText::new("Aggiungi").font(ui::medium(11.0)).color(t.text2));
        ui.add_space(4.0);
        let items: [(&str, &str, &str); 6] = [
            ("code", "Blocco di codice", "Codice con i colori della sintassi"),
            ("chart-column", "Sondaggio", "Ognuno vota un'opzione"),
            ("list-checks", "Lista di cose da fare", "Caselle da spuntare insieme"),
            ("hash", "Contatore", "Un numero con + e −"),
            ("image-plus", "Immagine", "Dal computer"),
            ("layout-template", "Modelli", "Brainstorming, Kanban e altri"),
        ];
        egui::Grid::new("more-tools").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
            for (i, (icon_name, label, hint)) in items.into_iter().enumerate() {
                if tile(ui, icon_name, label, hint, t).clicked() {
                    match i {
                        0 => ed.insert_code(),
                        1 => ed.insert_widget(Widget::Poll { question: String::new(), options: (1..=3).map(|n| PollOption { text: format!("Opzione {n}"), votes: Vec::new() }).collect() }),
                        2 => ed.insert_widget(Widget::Checklist { title: String::new(), items: (0..3).map(|_| CheckItem { text: String::new(), done: false }).collect() }),
                        3 => ed.insert_widget(Widget::Counter { label: String::new(), value: 0 }),
                        4 => ed.requests.push(crate::editor::Request::InsertImage),
                        _ => {
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
fn tile(ui: &mut Ui, name: &str, label: &str, hint: &str, t: &Theme) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(216.0, 48.0), Sense::click());
    let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    let s = ui::motion::press(ui.ctx(), resp.id, resp.is_pointer_button_down_on());
    let r2 = Rect::from_center_size(r.center(), r.size() * s);
    ui.painter().rect_filled(r2, 10.0, t.hover.gamma_multiply(k));
    let sq = Rect::from_min_size(r2.min + vec2(6.0, 6.0), vec2(36.0, 36.0));
    ui.painter().rect_filled(sq, 9.0, t.selected);
    icon(ui, name, sq.center(), 18.0, if t.dark { Color32::WHITE } else { ui::mix(t.brand, 0.75, Color32::BLACK) });
    ui.painter().text(pos2(sq.max.x + 10.0, r2.center().y - 7.0), egui::Align2::LEFT_CENTER, label, ui::medium(12.0), t.text);
    ui.painter().text(pos2(sq.max.x + 10.0, r2.center().y + 8.0), egui::Align2::LEFT_CENTER, hint, egui::FontId::proportional(11.0), t.text2);
    ui::focus_ring(ui, &resp, r);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp
}

/// FigJam's marker drawer: pen, highlighter and washi tape drawn as the real things, in the colour
/// they will draw with. They peek out of the toolbar, rise a little under the pointer and stand
/// up when chosen, on springs.
fn drawer(ui: &mut Ui, ed: &mut Editor, t: &Theme) {
    const SLOT: f32 = 28.0;
    let (rect, _) = ui.allocate_exact_size(vec2(SLOT * 3.0 + 6.0, BTN), Sense::hover());
    // The drawer runs into the toolbar's bottom edge: the tools vanish into it.
    let clip = Rect::from_min_max(pos2(rect.min.x, rect.min.y - 6.0), pos2(rect.max.x, rect.max.y + 5.0));
    let pen = ed.prefs.pens.get(ed.pen).cloned().unwrap_or_default();
    let items = [
        (Tool::Pen, "Penna", "P", pen.color.clone()),
        (Tool::Highlighter, "Evidenziatore", "M", ed.prefs.highlighter.color.clone()),
        (Tool::Tape, "Nastro adesivo", "W", ed.prefs.tape.color.clone()),
    ];
    for (i, (tool, label, kbd, color)) in items.into_iter().enumerate() {
        let slot = Rect::from_min_size(pos2(rect.min.x + 3.0 + i as f32 * SLOT, rect.min.y - 6.0), vec2(SLOT, BTN + 11.0));
        let resp = ui.interact(slot, ui.id().with(("drawer", i)), Sense::click());
        let on = ed.tool == tool;
        let lift = ui::motion::spring(ui.ctx(), resp.id.with("lift"), if on { 1.0 } else if resp.hovered() { 0.45 } else { 0.0 }, 0.3, 0.7);
        let top = slot.min.y + 18.0 - 14.0 * lift;
        let c = crate::model::parse_color(&color).unwrap_or(crate::model::DARK);
        let p = ui.painter().with_clip_rect(clip);
        match tool {
            Tool::Pen => marker(&p, slot.center().x, top, c, t, 11.0, false),
            Tool::Highlighter => marker(&p, slot.center().x, top, c, t, 15.0, true),
            _ => tape_roll(&p, slot.center().x, top, c, t),
        }
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
        if ui::tip(resp, label, Some(kbd)).clicked() {
            ed.set_tool(tool);
        }
    }
}

/// A marker standing in the drawer: coloured tip, collar, light body.
fn marker(p: &egui::Painter, x: f32, top: f32, c: Color32, t: &Theme, w: f32, chisel: bool) {
    let body = if t.dark { Color32::from_gray(0x5A) } else { Color32::from_gray(0xF2) };
    let edge = if t.dark { Color32::from_gray(0x6E) } else { Color32::from_gray(0xD4) };
    let shoulder = top + 9.0;
    let h = w / 2.0;
    p.rect(Rect::from_min_max(pos2(x - h, shoulder), pos2(x + h, top + 60.0)), egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, body, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_max(pos2(x - h, shoulder + 3.0), pos2(x + h, shoulder + 7.0)), 0.0, c);
    let tip = if chisel {
        vec![pos2(x - h + 1.5, shoulder + 0.5), pos2(x + h - 1.5, shoulder + 0.5), pos2(x + h - 1.5, top + 3.0), pos2(x - h + 4.0, top)]
    } else {
        vec![pos2(x - h + 1.5, shoulder + 0.5), pos2(x + h - 1.5, shoulder + 0.5), pos2(x + 2.0, top + 1.0), pos2(x - 2.0, top + 1.0)]
    };
    p.add(egui::Shape::convex_polygon(tip, c, Stroke::new(1.0, ui::mix(c, 0.75, Color32::BLACK))));
}

/// A roll of washi tape with its loose end standing up.
fn tape_roll(p: &egui::Painter, x: f32, top: f32, c: Color32, t: &Theme) {
    let edge = ui::mix(c, 0.7, Color32::BLACK);
    let strip = Rect::from_min_max(pos2(x - 6.0, top), pos2(x + 6.0, top + 16.0));
    p.add(egui::Shape::convex_polygon(vec![strip.left_bottom(), strip.left_top() + vec2(0.0, 2.0), strip.center_top() + vec2(-2.0, 0.0), strip.center_top() + vec2(2.0, 2.5), strip.right_top() + vec2(0.0, 0.5), strip.right_bottom()], c, Stroke::new(1.0, edge)));
    let centre = pos2(x, top + 25.0);
    p.circle(centre, 11.0, c, Stroke::new(1.0, edge));
    p.circle(centre, 4.5, if t.dark { Color32::from_gray(0x3A) } else { Color32::WHITE }, Stroke::new(1.0, edge));
}

/// A pad of sticky notes: the top one straight, the one under it a little askew.
fn sticky_pad(ui: &Ui, c: Pos2, color: Color32, t: &Theme, lift: f32) {
    let p = ui.painter();
    let under = ui::mix(color, 0.82, Color32::BLACK);
    let a = -0.16f32;
    let (s, k) = (a.sin(), a.cos());
    let rot = |x: f32, y: f32| c + vec2(1.5 + x * k - y * s, 1.0 + x * s + y * k);
    p.add(egui::Shape::convex_polygon(vec![rot(-8.0, -8.0), rot(8.0, -8.0), rot(8.0, 8.0), rot(-8.0, 8.0)], under, Stroke::NONE));
    let top = Rect::from_center_size(c + vec2(-1.0, -1.0 - 2.0 * lift), vec2(16.0, 16.0));
    p.rect(top, 2.0, color, Stroke::new(1.0, ui::mix(color, 0.7, if t.dark { Color32::WHITE } else { Color32::BLACK })), egui::StrokeKind::Inside);
    // The fold at the corner.
    p.add(egui::Shape::convex_polygon(vec![top.right_bottom() - vec2(5.0, 0.0), top.right_bottom() - vec2(0.0, 5.0), top.right_bottom() - vec2(5.0, 5.0)], under, Stroke::NONE));
}


pub fn toolbar(ctx: &egui::Context, stage: Rect, ed: &mut Editor, st: &mut BoardUi) {
    let t = ui::theme(ctx);
    let top = ed.prefs.toolbar_pos == ToolbarPos::Top;
    let anchor = if top { egui::Align2::CENTER_TOP } else { egui::Align2::CENTER_BOTTOM };
    let dy = if top { 16.0 } else { -16.0 };
    let resp = egui::Area::new(Id::new("toolbar")).anchor(anchor, [stage.center().x - ctx.content_rect().center().x, dy + if top { stage.min.y } else { stage.max.y - ctx.content_rect().max.y }]).order(egui::Order::Foreground).show(ctx, |ui| {
        // FigJam's toolbar: a big friendly card with the markers waiting in a drawer.
        let [contact, soft] = ui::card_shadows(&t);
        let under = ui.painter().add(egui::Shape::Noop);
        let card = egui::Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.border)).corner_radius(16).shadow(soft).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui::row(ui, BTN, |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                tool_button(ui, ed, Tool::Select, "mouse-pointer-2", "Seleziona", Some("V"));
                tool_button(ui, ed, Tool::Hand, "hand", "Mano", Some("H"));
                sep(ui, &t);
                if ed.read_only {
                    laser_button(ui, ed);
                    return;
                }
                drawer(ui, ed, &t);
                tool_button(ui, ed, Tool::Eraser, "eraser", "Gomma", Some("E"));
                tool_button(ui, ed, Tool::Lasso, "lasso", "Lazo", Some("Q"));
                let on = ed.ruler.visible;
                if icon_button(ui, "ruler", if on { "Nascondi righello" } else { "Mostra righello" }, Some("U"), vec2(BTN, BTN), ICON, on, false).clicked() {
                    ed.toggle_ruler();
                }
                sep(ui, &t);
                // The sticky tool drawn as a little pad in the colour the next note will have.
                let r = tool_button(ui, ed, Tool::Sticky, "", "Nota adesiva", Some("S"));
                sticky_pad(ui, r.rect.center(), crate::model::color_or(&ed.prefs.sticky_color, Color32::YELLOW), &t, ui::motion::hover(ctx, r.id.with("lift"), r.hovered()));
                // Shape tool with its menu of shapes.
                let shape = ed.prefs.last_shape;
                let (label, kbd) = SHAPES.iter().find(|s| s.0 == shape).map_or(("Forma", None), |s| (s.1, s.2));
                let r = tool_button(ui, ed, Tool::Shape, "", label, kbd);
                shape_icon(ui, shape, r.rect.center(), ICON, if ed.tool == Tool::Shape { Color32::WHITE } else { t.icon });
                let (cr, chevron) = ui.allocate_exact_size(vec2(14.0, BTN), Sense::click());
                ui::hover_fill(ui, chevron.id, cr, chevron.hovered(), 4.0, t.hover);
                icon(ui, "chevron-down", cr.center(), 12.0, t.icon2);
                egui::Popup::menu(&chevron).frame(ui::menu_frame(&t)).align(if top { egui::RectAlign::BOTTOM } else { egui::RectAlign::TOP }).show(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (k, label, kbd) in SHAPES {
                        let resp = ui::menu_item(ui, None, label, kbd, false);
                        shape_icon(ui, k, pos2(resp.rect.min.x + 12.0, resp.rect.center().y), 14.0, t.menu_text);
                        if resp.clicked() {
                            ed.prefs.last_shape = k;
                            ed.set_tool(Tool::Shape);
                        }
                    }
                });
                tool_button(ui, ed, Tool::Arrow, "arrow-up-right", "Connettore e freccia: parti da una forma per collegarla", Some("X"));
                tool_button(ui, ed, Tool::Line, "minus", "Linea", Some("L"));
                tool_button(ui, ed, Tool::Text, "type", "Testo", Some("T"));
                tool_button(ui, ed, Tool::Section, "square-dashed-top-solid", "Sezione", Some("Maiusc+S"));
                tool_button(ui, ed, Tool::Table, "table", "Tabella", None);
                sep(ui, &t);
                tool_button(ui, ed, Tool::Stamp, "smile", "Reazioni", None);
                tool_button(ui, ed, Tool::Comment, "message-circle", "Commento", Some("C"));
                more_tools(ui, ed, &t, top);
                sep(ui, &t);
                laser_button(ui, ed);
            });
        });
        ui.painter().set(under, contact.as_shape(card.response.rect, egui::CornerRadius::same(16)));
    });
    let bar = resp.response.rect;
    ctx.data_mut(|d| d.insert_temp(Id::new("toolbar-rect"), bar));
    if !ed.read_only {
        tray(ctx, bar, top, ed, &t);
    }
    if ed.lost {
        back_to_content(ctx, bar, top, ed, &t);
    }
    let _ = st;
}

fn laser_button(ui: &mut Ui, ed: &mut Editor) {
    let r = tool_button(ui, ed, Tool::Laser, "", "Puntatore laser", Some("K"));
    let c = r.rect.center();
    let red = Color32::from_rgb(0xFF, 0x3B, 0x30);
    ui.painter().circle_filled(c, 7.5, Color32::from_rgba_unmultiplied(0xFF, 0x3B, 0x30, 46));
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
fn tray(ctx: &egui::Context, bar: Rect, top: bool, ed: &mut Editor, t: &Theme) {
    let tool = ed.tool;
    if !matches!(tool, Tool::Pen | Tool::Highlighter | Tool::Tape | Tool::Eraser | Tool::Shape | Tool::Line | Tool::Arrow | Tool::Text | Tool::Sticky | Tool::Stamp | Tool::Comment | Tool::Section | Tool::Table) {
        return;
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
    // The tallest thing in the tray sets its height; everything else is centred on it.
    let h = match tool {
        Tool::Shape => 88.0,
        Tool::Stamp => 62.0,
        _ => TRAY_H,
    };
    egui::Area::new(Id::new(("tray", format!("{tool:?}")))).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ui::float_frame(t).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui::row(ui, h, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
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
                        ui::slider_inline(ui, "Spessore", &mut pen.size, 1.0, 64.0, 0.5, true, PX, 196.0);
                        ui.add_space(8.0);
                        ui::slider_inline(ui, "Opacità", &mut pen.opacity, 0.1, 1.0, 0.05, false, PCT, 172.0);
                        sep_h(ui, t, h);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icon_button(ui, "shapes", "Da tratto a forma: trasforma linee, cerchi e rettangoli disegnati a mano", None, vec2(32.0, 32.0), 18.0, p.ink_to_shape, false).clicked() {
                            p.ink_to_shape = !p.ink_to_shape;
                        }
                        let label = if p.finger_draw { "Le dita disegnano (tocca per usarle per spostarti)" } else { "Le dita spostano la lavagna (tocca per disegnare con le dita)" };
                        if icon_button(ui, "hand", label, None, vec2(32.0, 32.0), 18.0, p.finger_draw, false).clicked() {
                            p.finger_draw = !p.finger_draw;
                        }
                    }
                    Tool::Highlighter => {
                        if let Some(c) = ui::swatches(ui, &HIGHLIGHT_COLORS, &p.highlighter.color, false) {
                            p.highlighter.color = c;
                        }
                        sep_h(ui, t, h);
                        ui::slider_inline(ui, "Spessore", &mut p.highlighter.size, 4.0, 96.0, 0.0, true, PX, 196.0);
                    }
                    Tool::Tape => {
                        if let Some(c) = ui::swatches(ui, &TAPE_COLORS, &p.tape.color, false) {
                            p.tape.color = c;
                        }
                        sep_h(ui, t, h);
                        ui::slider_inline(ui, "Larghezza", &mut p.tape.size, 8.0, 120.0, 0.0, true, PX, 204.0);
                    }
                    Tool::Eraser => {
                        ui::segmented(ui, &mut p.eraser.mode, &[(EraserMode::Pixel, "Pixel"), (EraserMode::Stroke, "Tratto intero")], 196.0);
                        sep_h(ui, t, h);
                        let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                        let d = (p.eraser.size as f32).clamp(4.0, 26.0);
                        ui.painter().circle(r.center(), d / 2.0, t.bg2, Stroke::new(1.0, t.border_strong));
                        ui::slider_inline(ui, "Dimensione", &mut p.eraser.size, 2.0, 240.0, 0.0, true, PX, 212.0);
                        if p.eraser.mode == EraserMode::Pixel {
                            ui::slider_inline(ui, "Forza", &mut p.eraser.strength, 0.1, 1.0, 0.05, false, PCT, 164.0);
                        }
                    }
                    Tool::Shape | Tool::Line | Tool::Arrow => {
                        if tool == Tool::Shape {
                            egui::Grid::new("shape-grid").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
                                for (i, (k, label, kbd)) in SHAPES.iter().enumerate() {
                                    let on = p.last_shape == *k;
                                    let r = icon_button(ui, "", label, *kbd, vec2(28.0, 28.0), 16.0, on, false);
                                    shape_icon(ui, *k, r.rect.center(), 16.0, t.icon);
                                    if r.clicked() {
                                        p.last_shape = *k;
                                    }
                                    if i % 7 == 6 {
                                        ui.end_row();
                                    }
                                }
                            });
                            sep_h(ui, t, h);
                            color_pop(ui, "Riempimento", &mut p.shape_style.fill, true, false, top, t);
                        }
                        if tool != Tool::Shape {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (r, name, label) in ROUTES {
                                if icon_button(ui, name, label, None, vec2(28.0, 28.0), 16.0, p.route == r, false).clicked() {
                                    p.route = r;
                                }
                            }
                            ui.spacing_mut().item_spacing.x = 6.0;
                            sep_h(ui, t, h);
                        }
                        color_pop(ui, "Contorno", &mut p.shape_style.stroke, false, true, top, t);
                        sep_h(ui, t, h);
                        ui::slider_inline(ui, "Spessore", &mut p.shape_style.stroke_width, 0.5, 32.0, 0.5, true, PX, 196.0);
                    }
                    Tool::Text => {
                        font_picker(ui, &mut p.text.font, 168.0, t);
                        sep_h(ui, t, h);
                        color_pop(ui, "Colore testo", &mut p.text.color, false, false, top, t);
                        let mut size = p.text.font_size.round() as i64;
                        if ui::segmented(ui, &mut size, &[(16, "S"), (24, "M"), (36, "L"), (56, "XL")], 150.0) {
                            p.text.font_size = size as f64;
                        }
                        if let Some(v) = ui::Num::new(Id::new("tray-font-size"), "icon:text-size", "Dimensione del testo in pixel: trascina per cambiarla", Some(p.text.font_size)).width(76.0).range(4.0, 400.0).show(ui) {
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
                    Tool::Table => {
                        let [rows, cols] = p.table_size;
                        if let Some(v) = ui::Num::new(Id::new("tray-table-rows"), "Righe", "Righe della nuova tabella: trascina o scrivi", Some(rows as f64)).width(92.0).range(1.0, 20.0).show(ui) {
                            p.table_size[0] = v as u8;
                        }
                        if let Some(v) = ui::Num::new(Id::new("tray-table-cols"), "Colonne", "Colonne della nuova tabella: trascina o scrivi", Some(cols as f64)).width(108.0).range(1.0, 12.0).show(ui) {
                            p.table_size[1] = v as u8;
                        }
                        sep_h(ui, t, h);
                        ui::hint(ui, "Clicca sulla lavagna per metterla. Poi un clic su una cella per scriverci.");
                    }
                    Tool::Comment => ui::hint(ui, "Clicca dove vuoi lasciare un commento. Gli altri lo vedono e possono rispondere."),
                    Tool::Section => ui::hint(ui, "Trascina per disegnare una sezione: quello che ci metti dentro si sposta insieme a lei."),
                    _ => {}
                }
            });
        });
    });
}

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
    let outline: Vec<crate::geom::Pt> = crate::ink::stroke_outline(&pts, (pen.size * 1.2).min(12.0), false).into_iter().map(|q| crate::geom::pt(q.x + rect.min.x as f64, q.y + rect.min.y as f64)).collect();
    let color = crate::model::color_or(&pen.color, crate::model::DARK);
    let color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (pen.opacity.clamp(0.1, 1.0) * 255.0) as u8);
    ui.painter().add(crate::paint::fill_mesh(&crate::prims::Path::smooth_outline(&outline), color));
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
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
    ui.painter().rect_filled(rect, ui::RADIUS, t.bg2);
    let k = ui::motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
    if k > 0.0 {
        ui.painter().rect_stroke(rect, ui::RADIUS, Stroke::new(1.0, t.border_strong.gamma_multiply(k)), egui::StrokeKind::Inside);
    }
    let family = egui::FontFamily::Name(crate::text::prefix(*value).into());
    ui.painter().text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::new(12.0, family), t.text);
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
                ui.label(RichText::new(g).size(10.0).color(t.menu_text2));
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

/// The reactions, drawn with the same pictures as on the board.
pub fn stamp_picker(ui: &mut Ui, value: Option<&str>) -> Option<String> {
    let t = ui::theme(ui.ctx());
    let mut picked = None;
    egui::Grid::new("stamps").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
        for (i, s) in crate::assets::STAMPS.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
            let on = value == Some(s.emoji);
            if on {
                ui.painter().rect_filled(r, ui::RADIUS, t.selected);
            }
            ui::hover_fill(ui, resp.id, r, resp.hovered() && !on, ui::RADIUS as f32, t.hover);
            ui::icons::picture(ui, &format!("stamp:{}", s.emoji), s.svg, Rect::from_center_size(r.center(), vec2(22.0, 22.0)));
            if ui::tip(resp, s.name, None).clicked() {
                picked = Some(s.emoji.to_string());
            }
            if i % 8 == 7 {
                ui.end_row();
            }
        }
    });
    picked
}

/* ---------------- zoom controls ---------------- */

/// Bottom right, Whiteboard style: minimap on/off, zoom in, zoom level (menu), zoom out.
pub fn view_controls(ctx: &egui::Context, stage: Rect, ed: &mut Editor) {
    let t = ui::theme(ctx);
    // In the corner, unless the toolbar reaches it: then just above the toolbar, gliding there.
    let bar = ctx.data(|d| d.get_temp::<Rect>(Id::new("toolbar-rect")));
    let size = ctx.data(|d| d.get_temp::<egui::Vec2>(Id::new("view-controls-size"))).unwrap_or(vec2(40.0, 130.0));
    let corner = stage.max - vec2(16.0, 16.0);
    let clash = bar.is_some_and(|b| b.intersects(Rect::from_min_max(corner - size - vec2(8.0, 8.0), corner)));
    let lift = ui::motion::spring(ctx, Id::new("view-controls-lift"), if clash { bar.map_or(0.0, |b| (corner.y - b.min.y + 12.0).max(0.0)) } else { 0.0 }, 0.3, 0.9);
    // The minimap put down in this corner: the controls step to its left.
    let mini = if ed.prefs.minimap { ctx.data(|d| d.get_temp::<Rect>(Id::new("minimap-rect"))) } else { None };
    let spot = Rect::from_min_max(corner - size - vec2(0.0, lift), corner - vec2(0.0, lift));
    let aside = ui::motion::spring(ctx, Id::new("view-controls-aside"), mini.filter(|m| m.expand(6.0).intersects(spot)).map_or(0.0, |m| (corner.x - m.min.x + 10.0).max(0.0)), 0.3, 0.9);
    let resp = egui::Area::new(Id::new("view-controls")).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(corner - vec2(aside, lift)).order(egui::Order::Foreground).show(ctx, |ui| {
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
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, &label, egui::FontId::proportional(10.0), t.text);
                egui::Popup::menu(&resp).frame(ui::menu_frame(&t)).align(egui::RectAlign::LEFT_END).show(|ui| zoom_items(ui, ed));
                if icon_button(ui, "minus", "Riduci", Some("Ctrl+−"), vec2(32.0, 28.0), 16.0, false, false).clicked() {
                    ed.zoom_by(0.8);
                }
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("view-controls-size"), resp.response.rect.size()));
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
    /// Pointer offset from the card's corner while the card itself is being moved.
    moving: Option<egui::Vec2>,
}

/// What the minimap frames: the content, but never much less than a screenful around it, so a
/// single dot doesn't fill the map and where you are stays readable.
fn map_area(content: Option<BBox>, view: BBox) -> BBox {
    let Some(c) = content else { return view };
    let (w, h) = (c.w.max(view.w * 0.8), c.h.max(view.h * 0.8));
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
    if m.tex.is_none() || (m.drawn != version && m.grab.is_none() && quiet > 300.0) {
        m.drawn = version;
        let content = union(ed.board.all().iter().filter(|e| !e.hidden).map(|e| crate::geom::aabb(e)));
        let fit = fit_for(map_area(content, ed.view()), mw, mh);
        m.shown = Some(fit);
        let ppp = ctx.pixels_per_point() as f64;
        let (w, h) = ((mw * ppp) as u32, (mh * ppp) as u32);
        let b = BBox { x: -fit.x / fit.k, y: -fit.y / fit.k, w: mw / fit.k, h: mh / fit.k };
        let els = ed.board.paint_order().to_vec();
        let env = crate::prims::Env { zoom: fit.k, pixel: Some(1.0 / (fit.k * ppp)), hairline: true, editing: None, comments: false, editing_cell: None };
        let bg = crate::model::color_or(&ed.board.meta().background, Color32::from_gray(0xF5));
        let images = &ed.painter.images;
        if let Some(pm) = crate::raster::render(&els, Some(bg), w, h, b, &env, &|k| images.pixmap(k)) {
            let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
            match &mut m.tex {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => m.tex = Some(ctx.load_texture("minimap", img, egui::TextureOptions::LINEAR)),
            }
        }
    } else if m.drawn != version {
        ctx.request_repaint_after(std::time::Duration::from_millis((310.0 - quiet).max(10.0) as u64));
    }
    let Some(f) = m.shown else { return };
    let view = ed.view();
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
    let pos = pos2(px, py - ui::motion::travel(ctx, k) * 8.0);
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
        let fill = egui::epaint::RectShape::filled(map, egui::CornerRadius::same(8), if m.tex.is_some() { Color32::WHITE } else { bg });
        ui.painter().add(match &m.tex {
            Some(tex) => fill.with_texture(tex.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
            None => fill,
        });
        // Where you are: kept inside the map on each axis, so it shows the way back when you
        // are far off and never spills over the card.
        let to = |x: f64, y: f64| map.min + vec2((x * f.k + f.x) as f32, (y * f.k + f.y) as f32);
        let vr = Rect::from_min_max(to(view.x, view.y), to(view.right(), view.bottom()));
        let inner = map.shrink(1.5);
        let (x0, x1) = clamp_span(vr.min.x, vr.max.x, inner.min.x, inner.max.x, 6.0);
        let (y0, y1) = clamp_span(vr.min.y, vr.max.y, inner.min.y, inner.max.y, 6.0);
        let v = Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
        // Everything in view: no rectangle, the map alone says it.
        if !vr.contains_rect(inner) {
            ui.painter().with_clip_rect(map).rect(v, egui::CornerRadius::same(3), t.brand.gamma_multiply(0.1), Stroke::new(1.5, t.brand), egui::StrokeKind::Inside);
        }
        let point = |p: Pos2| ((p.x.clamp(map.min.x, map.max.x) - map.min.x) as f64 - f.x) / f.k;
        let point_y = |p: Pos2| ((p.y.clamp(map.min.y, map.max.y) - map.min.y) as f64 - f.y) / f.k;
        if resp.drag_started() || resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (px, py) = (point(p), point_y(p));
                let inside = view.contains(crate::geom::pt(px, py));
                // Grabbing the rectangle drags it from where it was taken; elsewhere the view centres on the click.
                m.grab = Some(if inside { (px - view.center().x, py - view.center().y) } else { (0.0, 0.0) });
            }
        }
        if let (Some(g), Some(p)) = (m.grab, resp.interact_pointer_pos()) {
            ed.center_on(crate::geom::pt(point(p) - g.0, point_y(p) - g.1));
        }
        if resp.drag_stopped() || resp.clicked() {
            m.grab = None;
        }
        let over = ui.rect_contains_pointer(card);
        ui::tip(resp, "Minimappa: clicca o trascina per spostarti sulla lavagna", None);
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
