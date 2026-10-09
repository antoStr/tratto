//! The floating toolbar with its contextual tray, the zoom controls and the minimap.

use egui::{Color32, Id, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::board::BoardUi;
use crate::editor::{Editor, Tool};
use crate::geom::{BBox, union};
use crate::model::{FONTS, FontKind, HIGHLIGHT_COLORS, INK_COLORS, STICKY_COLORS, ShapeKind, TAPE_COLORS};
use crate::prefs::{EraserMode, Pen, ShapeTool, ToolbarPos};
use crate::ui::{self, Theme, icon, icon_button};

const ICON: f32 = 20.0;
const BTN: f32 = 36.0;

pub const SHAPES: [(ShapeTool, &str, Option<&str>); 14] = [
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

fn sep(ui: &mut Ui, t: &Theme) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, BTN), Sense::hover());
    ui.painter().vline(r.center().x, (r.center().y - 10.0)..=(r.center().y + 10.0), Stroke::new(1.0, t.border));
}

fn tool_button(ui: &mut Ui, ed: &mut Editor, tool: Tool, name: &str, label: &str, kbd: Option<&str>) -> egui::Response {
    let r = icon_button(ui, name, label, kbd, vec2(BTN, BTN), ICON, false, ed.tool == tool);
    if r.clicked() {
        ed.set_tool(tool);
    }
    r
}

/// A bar of the colour the tool will use under its icon, like Office's font-colour button.
fn ink_bar(ui: &Ui, r: &egui::Response, color: &str) {
    let c = crate::model::parse_color(color).unwrap_or(crate::model::DARK);
    let bar = Rect::from_center_size(r.rect.center_bottom() - vec2(0.0, 5.0), vec2(14.0, 3.0));
    ui.painter().rect_filled(bar, 1.5, c);
    if c == Color32::WHITE {
        ui.painter().rect_stroke(bar, 1.5, Stroke::new(0.5, Color32::from_black_alpha(90)), egui::StrokeKind::Outside);
    }
}

pub fn toolbar(ctx: &egui::Context, stage: Rect, ed: &mut Editor, st: &mut BoardUi) {
    let t = ui::theme(ctx);
    let top = ed.prefs.toolbar_pos == ToolbarPos::Top;
    let anchor = if top { egui::Align2::CENTER_TOP } else { egui::Align2::CENTER_BOTTOM };
    let dy = if top { 16.0 } else { -16.0 };
    let resp = egui::Area::new(Id::new("toolbar")).anchor(anchor, [stage.center().x - ctx.content_rect().center().x, dy + if top { stage.min.y } else { stage.max.y - ctx.content_rect().max.y }]).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                tool_button(ui, ed, Tool::Select, "mouse-pointer-2", "Seleziona", Some("V"));
                tool_button(ui, ed, Tool::Hand, "hand", "Mano", Some("H"));
                sep(ui, &t);
                if ed.read_only {
                    laser_button(ui, ed);
                    return;
                }
                let pen = ed.prefs.pens.get(ed.pen).cloned().unwrap_or_default();
                let r = tool_button(ui, ed, Tool::Pen, "pen", "Penna", Some("P"));
                ink_bar(ui, &r, &pen.color);
                let hl = ed.prefs.highlighter.color.clone();
                let r = tool_button(ui, ed, Tool::Highlighter, "highlighter", "Evidenziatore", Some("M"));
                ink_bar(ui, &r, &hl);
                let tape = ed.prefs.tape.color.clone();
                let r = tool_button(ui, ed, Tool::Tape, "sticky-note", "Nastro adesivo", Some("W"));
                ink_bar(ui, &r, &tape);
                tool_button(ui, ed, Tool::Eraser, "eraser", "Gomma", Some("E"));
                tool_button(ui, ed, Tool::Lasso, "lasso", "Lazo", Some("Q"));
                let on = ed.ruler.visible;
                if icon_button(ui, "ruler", if on { "Nascondi righello" } else { "Mostra righello" }, Some("U"), vec2(BTN, BTN), ICON, on, false).clicked() {
                    ed.toggle_ruler();
                }
                sep(ui, &t);
                // The sticky tool drawn as a note in the colour the next one will have.
                let r = tool_button(ui, ed, Tool::Sticky, "", "Nota adesiva", Some("S"));
                let note = Rect::from_center_size(r.rect.center(), vec2(16.0, 16.0));
                let fg = if ed.tool == Tool::Sticky { Color32::WHITE } else { t.icon };
                ui.painter().rect_filled(note, 2.0, crate::model::color_or(&ed.prefs.sticky_color, Color32::YELLOW));
                ui.painter().rect_stroke(note, 2.0, Stroke::new(1.2, ui::mix(fg, 0.55, Color32::TRANSPARENT)), egui::StrokeKind::Inside);
                // Shape tool with its menu of shapes.
                let shape = ed.prefs.last_shape;
                let (label, kbd) = SHAPES.iter().find(|s| s.0 == shape).map_or(("Forma", None), |s| (s.1, s.2));
                let r = tool_button(ui, ed, Tool::Shape, "", label, kbd);
                shape_icon(ui, shape, r.rect.center(), ICON, if ed.tool == Tool::Shape { Color32::WHITE } else { t.icon });
                let (cr, chevron) = ui.allocate_exact_size(vec2(14.0, BTN), Sense::click());
                if chevron.hovered() {
                    ui.painter().rect_filled(cr, 4.0, t.hover);
                }
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
                sep(ui, &t);
                tool_button(ui, ed, Tool::Stamp, "smile", "Reazioni", None);
                tool_button(ui, ed, Tool::Comment, "message-circle", "Commento", Some("C"));
                if icon_button(ui, "image-plus", "Inserisci immagine", Some("I"), vec2(BTN, BTN), ICON, false, false).clicked() {
                    ed.requests.push(crate::editor::Request::InsertImage);
                }
                sep(ui, &t);
                laser_button(ui, ed);
            });
        });
    });
    let bar = resp.response.rect;
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
    if !matches!(tool, Tool::Pen | Tool::Highlighter | Tool::Tape | Tool::Eraser | Tool::Shape | Tool::Line | Tool::Arrow | Tool::Text | Tool::Sticky | Tool::Stamp | Tool::Comment | Tool::Section) {
        return;
    }
    let pivot = if top { egui::Align2::CENTER_TOP } else { egui::Align2::CENTER_BOTTOM };
    let at = if top { pos2(bar.center().x, bar.max.y + 8.0) } else { pos2(bar.center().x, bar.min.y - 8.0) };
    egui::Area::new(Id::new(("tray", format!("{tool:?}")))).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(t).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                let p = &mut ed.prefs;
                match tool {
                    Tool::Pen => {
                        let n = p.pens.len();
                        for i in 0..n {
                            pen_slot(ui, ed, i, t, top);
                        }
                        let p = &mut ed.prefs;
                        let i = ed.pen.min(p.pens.len() - 1);
                        sep(ui, t);
                        let pen = &mut p.pens[i];
                        ui::slider(ui, "Spessore", &mut pen.size, 1.0, 64.0, 0.5, true, PX, 120.0);
                        ui::slider(ui, "Opacità", &mut pen.opacity, 0.1, 1.0, 0.05, false, PCT, 100.0);
                        sep(ui, t);
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
                        sep(ui, t);
                        ui::slider(ui, "Spessore", &mut p.highlighter.size, 4.0, 96.0, 0.0, true, PX, 120.0);
                    }
                    Tool::Tape => {
                        if let Some(c) = ui::swatches(ui, &TAPE_COLORS, &p.tape.color, false) {
                            p.tape.color = c;
                        }
                        sep(ui, t);
                        ui::slider(ui, "Larghezza", &mut p.tape.size, 8.0, 120.0, 0.0, true, PX, 120.0);
                    }
                    Tool::Eraser => {
                        ui::segmented(ui, &mut p.eraser.mode, &[(EraserMode::Pixel, "Pixel"), (EraserMode::Stroke, "Tratto intero")], 196.0);
                        sep(ui, t);
                        let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                        let d = (p.eraser.size as f32).clamp(4.0, 28.0);
                        ui.painter().circle(r.center(), d / 2.0, t.bg2, Stroke::new(1.0, t.border_strong));
                        ui::slider(ui, "Dimensione", &mut p.eraser.size, 2.0, 240.0, 0.0, true, PX, 120.0);
                        if p.eraser.mode == EraserMode::Pixel {
                            ui::slider(ui, "Forza", &mut p.eraser.strength, 0.1, 1.0, 0.05, false, PCT, 100.0);
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
                            sep(ui, t);
                            color_pop(ui, "Riempimento", &mut p.shape_style.fill, true, false, top, t);
                        }
                        color_pop(ui, "Contorno", &mut p.shape_style.stroke, false, true, top, t);
                        sep(ui, t);
                        ui::slider(ui, "Spessore", &mut p.shape_style.stroke_width, 0.5, 32.0, 0.5, true, PX, 120.0);
                    }
                    Tool::Text => {
                        font_picker(ui, &mut p.text.font, 168.0, t);
                        sep(ui, t);
                        color_pop(ui, "Colore testo", &mut p.text.color, false, false, top, t);
                        let mut size = p.text.font_size.round() as i64;
                        if ui::segmented(ui, &mut size, &[(16, "S"), (24, "M"), (36, "L"), (56, "XL")], 150.0) {
                            p.text.font_size = size as f64;
                        }
                        if let Some(v) = ui::number_field(ui, Id::new("tray-font-size"), "Aa", "Dimensione del testo in pixel", Some(p.text.font_size), 0, 72.0) {
                            p.text.font_size = v.clamp(4.0, 400.0);
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
    if active {
        ui.painter().rect_filled(rect, ui::RADIUS, t.selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, ui::RADIUS, t.hover);
    }
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
    if resp.hovered() {
        ui.painter().rect_filled(rect, ui::RADIUS, t.hover);
    }
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
    if resp.hovered() {
        ui.painter().rect_stroke(rect, ui::RADIUS, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
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
            } else if resp.hovered() {
                ui.painter().rect_filled(r, ui::RADIUS, t.hover);
            }
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
    egui::Area::new(Id::new("view-controls")).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(stage.max - vec2(16.0, 16.0)).order(egui::Order::Foreground).show(ctx, |ui| {
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
                if resp.hovered() {
                    ui.painter().rect_filled(r, ui::RADIUS, t.hover);
                }
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, &label, egui::FontId::proportional(10.0), t.text);
                egui::Popup::menu(&resp).frame(ui::menu_frame(&t)).align(egui::RectAlign::LEFT_END).show(|ui| zoom_items(ui, ed));
                if icon_button(ui, "minus", "Riduci", Some("Ctrl+−"), vec2(32.0, 28.0), 16.0, false, false).clicked() {
                    ed.zoom_by(0.8);
                }
            });
        });
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

const MW: f32 = 192.0;
const MH: f32 = 128.0;

/// Map pixel = board point · k + (x, y).
#[derive(Clone, Copy, Default, PartialEq)]
struct Fit {
    k: f64,
    x: f64,
    y: f64,
}

fn fit_for(b: BBox) -> Fit {
    let pad = 0.06 * b.w.max(b.h).max(1.0);
    let k = (MW as f64 / (b.w + pad * 2.0)).min(MH as f64 / (b.h + pad * 2.0));
    Fit { k, x: (MW as f64 - b.w * k) / 2.0 - b.x * k, y: (MH as f64 - b.h * k) / 2.0 - b.y * k }
}

#[derive(Default)]
pub struct Minimap {
    tex: Option<egui::TextureHandle>,
    shown: Option<Fit>,
    drawn_at: f64,
    version: u64,
    /// Pointer offset from the view centre while dragging, in board units.
    grab: Option<(f64, f64)>,
}

/// Overview of the board and of the visible area; click or drag to move there.
pub fn minimap(ctx: &egui::Context, stage: Rect, ed: &mut Editor, m: &mut Minimap) {
    let t = ui::theme(ctx);
    let now = crate::platform::now_ms();
    let view = ed.view();
    let content = union(ed.board.all().iter().filter(|e| !e.hidden).map(|e| crate::geom::aabb(e)));
    let target = fit_for(union(content.into_iter().chain([view])).unwrap());
    let stale = m.shown.is_none_or(|f| (f.k / target.k).ln().abs() > 0.05 || (f.x - target.x).hypot(f.y - target.y) > 2.0);
    let changed = m.version != ed.board.version;
    if (m.tex.is_none() || ((stale || changed) && m.grab.is_none())) && now - m.drawn_at > 250.0 {
        m.drawn_at = now;
        m.version = ed.board.version;
        m.shown = Some(target);
        let ppp = ctx.pixels_per_point() as f64;
        let (w, h) = ((MW as f64 * ppp) as u32, (MH as f64 * ppp) as u32);
        let b = BBox { x: -target.x / target.k, y: -target.y / target.k, w: MW as f64 / target.k, h: MH as f64 / target.k };
        let els = ed.board.paint_order().to_vec();
        let env = crate::prims::Env { zoom: target.k, pixel: Some(1.0 / (target.k * ppp)), hairline: true, editing: None, comments: false };
        let bg = crate::model::color_or(&ed.board.meta().background, Color32::from_gray(0xF5));
        let images = &ed.painter.images;
        if let Some(pm) = crate::raster::render(&els, Some(bg), w, h, b, &env, &|k| images.pixmap(k)) {
            let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
            match &mut m.tex {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => m.tex = Some(ctx.load_texture("minimap", img, egui::TextureOptions::LINEAR)),
            }
        }
    } else if stale || changed {
        ctx.request_repaint_after(std::time::Duration::from_millis(260));
    }
    let Some(f) = m.shown else { return };
    egui::Area::new(Id::new("minimap")).pivot(egui::Align2::RIGHT_TOP).fixed_pos(pos2(stage.max.x - 16.0, stage.min.y + 16.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui::float_frame(&t).inner_margin(egui::Margin::same(0)).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(MW, MH), Sense::click_and_drag());
            if let Some(tex) = &m.tex {
                ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            let to = |x: f64, y: f64| rect.min + vec2((x * f.k + f.x) as f32, (y * f.k + f.y) as f32);
            let vr = Rect::from_min_max(to(view.x, view.y), to(view.right(), view.bottom()));
            // Dim what is outside the view: the bright window is where you are.
            let dim = Color32::from_black_alpha(15);
            for r in [Rect::from_min_max(rect.min, pos2(rect.max.x, vr.min.y)), Rect::from_min_max(pos2(rect.min.x, vr.max.y), rect.max), Rect::from_min_max(pos2(rect.min.x, vr.min.y), pos2(vr.min.x, vr.max.y)), Rect::from_min_max(pos2(vr.max.x, vr.min.y), pos2(rect.max.x, vr.max.y))] {
                let r = r.intersect(rect);
                if r.is_positive() {
                    ui.painter().rect_filled(r, 0.0, dim);
                }
            }
            ui.painter().with_clip_rect(rect).rect_stroke(vr.intersect(rect.shrink(0.75)), 0.0, Stroke::new(1.5, t.brand), egui::StrokeKind::Middle);
            let point = |p: Pos2| ((p.x.clamp(rect.min.x, rect.max.x) - rect.min.x) as f64 - f.x) / f.k;
            let point_y = |p: Pos2| ((p.y.clamp(rect.min.y, rect.max.y) - rect.min.y) as f64 - f.y) / f.k;
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
            let resp = ui::tip(resp, "Minimappa: clicca o trascina per spostarti sulla lavagna", None);
            let close = Rect::from_min_size(rect.right_top() + vec2(-26.0, 4.0), vec2(22.0, 22.0));
            let c = ui.interact(close, Id::new("minimap-close"), Sense::click());
            ui.painter().rect_filled(close, ui::RADIUS, if c.hovered() { t.hover } else { t.bg });
            icon(ui, "x", close.center(), 14.0, t.icon);
            if ui::tip(c, "Chiudi la minimappa", None).clicked() {
                ed.prefs.minimap = false;
            }
            let _ = resp;
        });
    });
}
