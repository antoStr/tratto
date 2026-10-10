//! FigJam's contextual toolbar: the main settings of what is selected, in a dark bar floating
//! just above it, so most changes never need the side panel. It rises in on a spring each time
//! the selection changes or a drag ends.

use std::sync::Arc;

use egui::{Color32, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::editor::{AlignKind, Editor, Request, Tool};
use crate::model::{Align, El, INK_COLORS, Kind, SECTION_COLORS, STICKY_COLORS};
use crate::ui::{self, Theme, icon};

const B: f32 = 28.0;

fn same<T: PartialEq>(els: &[Arc<El>], get: impl Fn(&El) -> Option<T>) -> Option<T> {
    let mut it = els.iter().map(|e| get(e));
    let first = it.next()??;
    it.all(|v| v.as_ref() == Some(&first)).then_some(first)
}

pub fn context_bar(ctx: &egui::Context, free: Rect, ed: &mut Editor) {
    let els = ed.selected();
    let shown = !ed.read_only && ed.editing.is_none() && ed.gesture.is_none() && ed.tool == Tool::Select && !els.is_empty() && !els.iter().all(|e| e.is_comment());
    // A new selection, or the same one after a drag, rises in again.
    let key = Id::new(("ctx-bar", &ed.selection));
    let was = ctx.data(|d| d.get_temp::<Id>(Id::new("ctx-bar-key")));
    if !shown {
        ctx.data_mut(|d| d.remove::<Id>(Id::new("ctx-bar-key")));
        return;
    }
    if was != Some(key) {
        ctx.data_mut(|d| d.insert_temp(Id::new("ctx-bar-key"), key));
        ui::motion::reset(ctx, Id::new("ctx-bar"));
    }
    let Some((b, rot)) = ed.selection_frame() else { return };
    // Screen box of the (possibly turned) selection frame.
    let (c, s) = (rot.cos(), rot.sin());
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let corners = [(b.x, b.y), (b.right(), b.y), (b.right(), b.bottom()), (b.x, b.bottom())].map(|(x, y)| {
        let (dx, dy) = (x - cx, y - cy);
        ed.to_screen(cx + dx * c - dy * s, cy + dx * s + dy * c) + ed.origin.to_vec2()
    });
    let sel = Rect::from_points(&corners);
    let k = ui::motion::appear(ctx, Id::new("ctx-bar"), true);
    // Clear of the "+" handles FigJam puts around a selection.
    let gap = 36.0;
    let h = ctx.data(|d| d.get_temp::<f32>(Id::new("ctx-bar-h"))).unwrap_or(40.0);
    let above = sel.min.y - gap - h >= free.min.y + 8.0;
    let rise = (1.0 - k.min(1.0)) * 6.0;
    let (pivot, at) = if above {
        (egui::Align2::CENTER_BOTTOM, pos2(sel.center().x, sel.min.y - gap + rise))
    } else {
        (egui::Align2::CENTER_TOP, pos2(sel.center().x, (sel.max.y + gap).min(free.max.y - 120.0) - rise))
    };
    let light = ui::theme(ctx);
    // The bar is dark in both themes, like Figma's menus: its widgets draw with the dark theme.
    let high_contrast = light.border == Theme::new(light.dark, light.brand, true).border;
    let dark = Theme::new(true, light.brand, high_contrast);
    let resp = egui::Area::new(Id::new("ctx-bar")).pivot(pivot).fixed_pos(at).constrain_to(free.shrink(8.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_opacity(k.clamp(0.0, 1.0));
        ctx.data_mut(|d| d.insert_temp(Id::NULL, dark));
        egui::Frame::new().fill(dark.menu).corner_radius(12).inner_margin(egui::Margin::same(4)).shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(60) }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                controls(ui, ed, &els, &dark);
            });
        });
        ctx.data_mut(|d| d.insert_temp(Id::NULL, light));
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("ctx-bar-h"), resp.response.rect.height()));
}

fn controls(ui: &mut Ui, ed: &mut Editor, els: &[Arc<El>], t: &Theme) {
    let ids: Vec<String> = els.iter().filter(|e| !e.locked).map(|e| e.id.clone()).collect();
    let types: std::collections::HashSet<&str> = els.iter().map(|e| e.type_name()).collect();
    let only = |k: &str| types.len() == 1 && types.contains(k);
    let locked = els.iter().all(|e| e.locked);
    let mut sep = false;
    let gap = |ui: &mut Ui, sep: &mut bool| {
        if *sep {
            let (r, _) = ui.allocate_exact_size(vec2(9.0, B), Sense::hover());
            ui.painter().vline(r.center().x, (r.center().y - 8.0)..=(r.center().y + 8.0), Stroke::new(1.0, t.menu_sep));
        }
        *sep = true;
    };
    if !locked {
        if only("sticky") {
            gap(ui, &mut sep);
            let cur = same(els, |e| e.sticky().map(|s| s.color.clone()));
            for c in STICKY_COLORS {
                if dot(ui, c, cur.as_deref() == Some(c), t).clicked() {
                    ed.board.update(&ids, |el| {
                        if let Kind::Sticky(s) = &mut el.kind {
                            s.color = c.into();
                        }
                    });
                }
            }
        }
        if only("shape") {
            gap(ui, &mut sep);
            let cur = same(els, |e| e.shape().map(|s| s.shape));
            let r = ui::icon_button(ui, "", "Forma", None, vec2(B + 12.0, B), 16.0, false, false);
            if let Some(k) = cur {
                super::toolbar::shape_icon(ui, crate::prefs::ShapeTool::Kind(k), r.rect.center() - vec2(5.0, 0.0), 16.0, t.icon);
            }
            icon(ui, "chevron-down", r.rect.right_center() - vec2(8.0, 0.0), 10.0, t.icon2);
            egui::Popup::menu(&r).frame(ui::float_frame(t).inner_margin(egui::Margin::same(6))).show(|ui| {
                egui::Grid::new("ctx-shapes").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
                    let mut i = 0;
                    for (tool, label, _) in super::toolbar::SHAPES.iter() {
                        let crate::prefs::ShapeTool::Kind(kind) = tool else { continue };
                        let b = ui::icon_button(ui, "", label, None, vec2(B, B), 16.0, cur == Some(*kind), false);
                        super::toolbar::shape_icon(ui, *tool, b.rect.center(), 16.0, t.icon);
                        if b.clicked() {
                            let kind = *kind;
                            ed.board.update(&ids, |el| {
                                if let Kind::Shape(s) = &mut el.kind {
                                    s.shape = kind;
                                }
                            });
                        }
                        i += 1;
                        if i % 5 == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
            let fill = same(els, |e| e.shape().map(|s| s.fill.clone()));
            if let Some(c) = color_button(ui, "Riempimento", fill.as_deref(), &INK_COLORS, true, false, t) {
                ed.board.update(&ids, |el| {
                    if let Kind::Shape(s) = &mut el.kind {
                        s.fill = c.clone();
                    }
                });
            }
        }
        if only("table") {
            gap(ui, &mut sep);
            let one = els.len() == 1;
            let mut change = |ui: &mut Ui, name: &str, label: &str, f: fn(&mut crate::model::Table)| {
                if ui::icon_button(ui, name, label, None, vec2(B, B), 16.0, false, false).clicked() {
                    ed.board.update(&ids, |el| {
                        if let Kind::Table(t) = &mut el.kind {
                            f(t);
                        }
                        crate::text::fit_text(el);
                    });
                }
            };
            if one {
                ui.label(egui::RichText::new("Righe").color(t.text2));
                change(ui, "minus", "Togli l'ultima riga", |t| t.remove_row(t.rows.len() - 1));
                change(ui, "plus", "Aggiungi una riga", |t| t.insert_row(t.rows.len()));
                gap(ui, &mut sep);
                ui.label(egui::RichText::new("Colonne").color(t.text2));
                change(ui, "minus", "Togli l'ultima colonna", |t| t.remove_col(t.cols.len() - 1));
                change(ui, "plus", "Aggiungi una colonna", |t| t.insert_col(t.cols.len()));
                gap(ui, &mut sep);
            }
            let header = els.iter().all(|e| e.table().is_some_and(|t| t.header));
            if ui::icon_button(ui, "panels-top-left", if header { "Riga normale in cima" } else { "Prima riga come intestazione" }, None, vec2(B, B), 16.0, header, false).clicked() {
                ed.board.update(&ids, |el| {
                    if let Kind::Table(t) = &mut el.kind {
                        t.header = !header;
                    }
                    crate::text::fit_text(el);
                });
            }
        }
        if only("code") {
            gap(ui, &mut sep);
            let lang = same(els, |e| e.code().map(|c| c.language.clone())).unwrap_or_default();
            if let Some(l) = super::panels::language_picker(ui, &lang, t) {
                ed.prefs.code_language = l.clone();
                ed.board.update(&ids, |el| {
                    if let Kind::Code(c) = &mut el.kind {
                        c.language = l.clone();
                    }
                });
            }
            let light = els.iter().all(|e| e.code().is_some_and(|c| c.light));
            if ui::icon_button(ui, if light { "moon" } else { "sun" }, if light { "Tema scuro" } else { "Tema chiaro" }, None, vec2(B, B), 16.0, false, false).clicked() {
                ed.board.update(&ids, |el| {
                    if let Kind::Code(c) = &mut el.kind {
                        c.light = !light;
                    }
                });
            }
            if els.len() == 1 && ui::icon_button(ui, "copy", "Copia il codice", None, vec2(B, B), 16.0, false, false).clicked() {
                ui.ctx().copy_text(els[0].code().map(|c| c.code.clone()).unwrap_or_default());
            }
        }
        if only("widget") && els.len() == 1 {
            gap(ui, &mut sep);
            if ui::button(ui, "Modifica", ui::Kind::Ghost, Some("pencil"), false, true).clicked() {
                ed.prefs.right_panel = true;
                ed.prefs.focus = false;
            }
            let reset: Option<(&str, fn(&mut crate::model::Widget))> = match els[0].widget() {
                Some(crate::model::Widget::Poll { .. }) => Some(("Azzera i voti", |w| {
                    if let crate::model::Widget::Poll { options, .. } = w {
                        options.iter_mut().for_each(|o| o.votes.clear());
                    }
                })),
                Some(crate::model::Widget::Checklist { .. }) => Some(("Togli le spunte", |w| {
                    if let crate::model::Widget::Checklist { items, .. } = w {
                        items.iter_mut().for_each(|i| i.done = false);
                    }
                })),
                Some(crate::model::Widget::Counter { .. }) => Some(("Azzera", |w| {
                    if let crate::model::Widget::Counter { value, .. } = w {
                        *value = 0;
                    }
                })),
                None => None,
            };
            if let Some((label, f)) = reset
                && ui::icon_button(ui, "refresh-cw", label, None, vec2(B, B), 16.0, false, false).clicked()
            {
                ed.board.update(&ids, |el| {
                    if let Kind::Widget(w) = &mut el.kind {
                        f(w);
                    }
                    crate::text::fit_text(el);
                });
            }
        }
        if only("section") {
            gap(ui, &mut sep);
            let cur = same(els, |e| if let Kind::Section { fill } = &e.kind { Some(fill.clone()) } else { None });
            if let Some(c) = color_button(ui, "Colore della sezione", cur.as_deref(), &SECTION_COLORS, false, false, t) {
                ed.board.update(&ids, |el| {
                    if let Kind::Section { fill } = &mut el.kind {
                        *fill = c.clone();
                    }
                });
            }
        }
        // Outline or ink colour, and line ends.
        let colorable = !types.is_empty() && types.iter().all(|k| matches!(*k, "ink" | "highlighter" | "text" | "shape" | "line"));
        if colorable {
            if !only("shape") {
                gap(ui, &mut sep);
            }
            let main = same(els, |e| match &e.kind {
                Kind::Ink(i) | Kind::Highlighter(i) => Some(i.color.clone()),
                Kind::Text(x) => Some(x.color.clone()),
                Kind::Shape(s) => Some(s.stroke.clone()),
                Kind::Line(l) => Some(l.stroke.clone()),
                _ => None,
            });
            let label = if only("shape") || only("line") { "Contorno" } else { "Colore" };
            if let Some(c) = color_button(ui, label, main.as_deref(), &INK_COLORS, false, only("shape"), t) {
                ed.board.update(&ids, |el| match &mut el.kind {
                    Kind::Ink(i) | Kind::Highlighter(i) => i.color = c.clone(),
                    Kind::Text(x) => x.color = c.clone(),
                    Kind::Shape(s) => s.stroke = c.clone(),
                    Kind::Line(l) => l.stroke = c.clone(),
                    _ => {}
                });
            }
        }
        if only("line") && !els.iter().any(|e| e.line().is_some_and(|l| l.tape)) {
            let route = same(els, |e| e.line().map(|l| l.route));
            for (r, name, label) in super::toolbar::ROUTES {
                if ui::icon_button(ui, name, label, None, vec2(B, B), 16.0, route == Some(r), false).clicked() {
                    ed.prefs.route = r;
                    ed.board.update(&ids, |el| {
                        if let Kind::Line(l) = &mut el.kind {
                            l.route = r;
                        }
                    });
                }
            }
        }
        if only("line") {
            let all = |f: fn(&crate::model::Line) -> bool| els.iter().all(|e| e.line().is_some_and(f));
            let (start, end, dash) = (all(|l| l.arrow_start), all(|l| l.arrow_end), all(|l| l.dash));
            if ui::icon_button(ui, "arrow-left", "Freccia all'inizio", None, vec2(B, B), 16.0, start, false).clicked() {
                ed.board.update(&ids, |el| {
                    if let Kind::Line(l) = &mut el.kind {
                        l.arrow_start = !start;
                    }
                });
            }
            if ui::icon_button(ui, "move-right", "Freccia alla fine", None, vec2(B, B), 16.0, end, false).clicked() {
                ed.board.update(&ids, |el| {
                    if let Kind::Line(l) = &mut el.kind {
                        l.arrow_end = !end;
                    }
                });
            }
            if ui::icon_button(ui, "more-horizontal", if dash { "Linea continua" } else { "Linea tratteggiata" }, None, vec2(B, B), 16.0, dash, false).clicked() {
                ed.board.update(&ids, |el| {
                    if let Kind::Line(l) = &mut el.kind {
                        l.dash = !dash;
                    }
                });
            }
        }
        // Text: font, size, weight, alignment.
        let texty = els.iter().all(|e| e.text().is_some() || e.sticky().is_some() || e.shape().is_some_and(|s| s.text.as_ref().is_some_and(|t| !t.is_empty())));
        if texty {
            gap(ui, &mut sep);
            let mut f = same(els, |e| e.text().map(|x| x.font).or_else(|| e.sticky().map(|s| s.font)).or_else(|| e.shape().map(|s| s.font.unwrap_or_default()))).unwrap_or_default();
            if super::toolbar::font_picker(ui, &mut f, 112.0, t) {
                ed.board.update(&ids, |el| {
                    match &mut el.kind {
                        Kind::Text(x) => x.font = f,
                        Kind::Sticky(s) => s.font = f,
                        Kind::Shape(s) => s.font = Some(f),
                        _ => {}
                    }
                    crate::text::fit_text(el);
                });
            }
            if only("text") {
                let size = same(els, |e| e.text().map(|x| x.font_size.round() as i64));
                let mut step = |ui: &mut Ui, name: &str, label: &str, k: f64| {
                    if ui::icon_button(ui, name, label, None, vec2(22.0, B), 12.0, false, false).clicked() {
                        ed.board.update(&ids, |el| {
                            if let Kind::Text(x) = &mut el.kind {
                                x.font_size = (x.font_size * k).round().clamp(4.0, 2000.0);
                            }
                            crate::text::fit_text(el);
                        });
                    }
                };
                step(ui, "minus", "Testo più piccolo", 1.0 / 1.2);
                let txt = size.map_or("–".to_string(), |v| v.to_string());
                ui.add_sized(vec2(26.0, B), egui::Label::new(egui::RichText::new(txt).color(t.text).monospace()));
                step(ui, "plus", "Testo più grande", 1.2);
                let bold = els.iter().all(|e| e.text().is_some_and(|x| x.bold));
                let italic = els.iter().all(|e| e.text().is_some_and(|x| x.italic));
                if ui::icon_button(ui, "bold", "Grassetto", None, vec2(B, B), 16.0, bold, false).clicked() {
                    ed.board.update(&ids, |el| {
                        if let Kind::Text(x) = &mut el.kind {
                            x.bold = !bold;
                        }
                        crate::text::fit_text(el);
                    });
                }
                if ui::icon_button(ui, "italic", "Corsivo", None, vec2(B, B), 16.0, italic, false).clicked() {
                    ed.board.update(&ids, |el| {
                        if let Kind::Text(x) = &mut el.kind {
                            x.italic = !italic;
                        }
                        crate::text::fit_text(el);
                    });
                }
            }
            if types.iter().all(|k| *k == "text" || *k == "sticky") {
                let align = same(els, |e| e.text().map(|x| x.align).or_else(|| e.sticky().map(|s| s.align))).unwrap_or_default();
                let (next, name) = match align {
                    Align::Left => (Align::Center, "text-align-start"),
                    Align::Center => (Align::Right, "text-align-center"),
                    Align::Right => (Align::Left, "text-align-end"),
                };
                if ui::icon_button(ui, name, "Allineamento del testo", None, vec2(B, B), 16.0, false, false).clicked() {
                    ed.board.update(&ids, |el| match &mut el.kind {
                        Kind::Text(x) => x.align = next,
                        Kind::Sticky(s) => s.align = next,
                        _ => {}
                    });
                }
            }
        }
        if els.len() > 1 {
            gap(ui, &mut sep);
            let r = ui::icon_button(ui, "align-start-vertical", "Allinea e distribuisci", None, vec2(B, B), 16.0, false, false);
            egui::Popup::menu(&r).frame(ui::float_frame(t).inner_margin(egui::Margin::same(6))).show(|ui| {
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
                        if ui::icon_button(ui, name, label, None, vec2(B, B), 16.0, false, false).clicked() {
                            ed.align(k);
                        }
                    }
                    if els.len() > 2 {
                        if ui::icon_button(ui, "align-horizontal-distribute-center", "Distribuisci in orizzontale", None, vec2(B, B), 16.0, false, false).clicked() {
                            ed.distribute(false);
                        }
                        if ui::icon_button(ui, "align-vertical-distribute-center", "Distribuisci in verticale", None, vec2(B, B), 16.0, false, false).clicked() {
                            ed.distribute(true);
                        }
                    }
                });
            });
        }
    }
    gap(ui, &mut sep);
    if ui::icon_button(ui, if locked { "lock" } else { "unlock" }, if locked { "Sblocca" } else { "Blocca" }, None, vec2(B, B), 16.0, locked, false).clicked() {
        ed.toggle_lock();
    }
    let more = ui::icon_button(ui, "more-horizontal", "Altro", None, vec2(B, B), 16.0, false, false);
    if more.clicked() {
        ed.requests.push(Request::Menu(more.rect.left_bottom() + vec2(0.0, 6.0)));
    }
}

/// A sticky colour as a dot, ringed when it is the current one.
fn dot(ui: &mut Ui, color: &str, on: bool, t: &Theme) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(22.0, B), Sense::click());
    let c = crate::model::parse_color(color).unwrap_or(Color32::YELLOW);
    let k = ui::motion::hover(ui.ctx(), resp.id, resp.hovered());
    let rad = 7.0 + k;
    ui.painter().circle_filled(r.center(), rad, c);
    if on {
        ui.painter().circle_stroke(r.center(), rad + 2.5, Stroke::new(1.5, t.menu_text));
    }
    ui::focus_ring(ui, &resp, r);
    ui::tip(resp, "Colore della nota", None)
}

/// A colour well that opens a palette; `ring` draws it as an outline colour.
fn color_button(ui: &mut Ui, label: &str, current: Option<&str>, palette: &[&str], transparent: bool, ring: bool, t: &Theme) -> Option<String> {
    let r = ui::icon_button(ui, "", label, None, vec2(B, B), 16.0, false, false);
    let c = current.and_then(crate::model::parse_color);
    let p = r.rect.center();
    match (c, ring) {
        (Some(c), true) => {
            ui.painter().circle_stroke(p, 7.0, Stroke::new(3.0, c));
        }
        (Some(c), false) => {
            ui.painter().circle(p, 8.0, c, Stroke::new(1.0, Color32::from_white_alpha(40)));
        }
        (None, _) => {
            // Mixed or transparent: an empty well with a slash.
            ui.painter().circle(p, 8.0, Color32::WHITE, Stroke::NONE);
            ui.painter().line_segment([p + vec2(-5.5, 5.5), p + vec2(5.5, -5.5)], Stroke::new(1.5, Color32::from_rgb(0xF2, 0x48, 0x22)));
        }
    }
    let mut picked = None;
    egui::Popup::menu(&r).frame(ui::float_frame(t).inner_margin(egui::Margin::same(8))).show(|ui| {
        ui.set_max_width(7.0 * 26.0);
        picked = ui::swatches(ui, palette, current.unwrap_or(""), transparent);
    });
    picked
}
