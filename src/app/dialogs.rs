//! Dialogs: settings, keyboard shortcuts, export and sharing.

use egui::{Color32, Id, Rect, RichText, Sense, Stroke, pos2, vec2};

use super::Toasts;
use super::board::BoardScreen;
use crate::prefs::{Prefs, Sensitivity, Smoothing, Theme, ToolbarPos, Wheel};
use crate::ui::{self, Kind as Btn};

#[derive(Default)]
pub struct Dialogs {
    pub settings: bool,
}

/// A centred dialog over a dimmed window, 13 px corners. It grows in from a little smaller and
/// fades away when closed (as long as it is still called while `open` is false); `open` turns
/// false when it is closed with its X, a click outside or Esc. Returns whether it is on screen.
pub fn dialog(ctx: &egui::Context, id: &str, title: &str, width: f32, open: &mut bool, body: impl FnOnce(&mut egui::Ui)) -> bool {
    dialog_opt(ctx, id, title, width, open, true, body)
}

pub fn dialog_opt(ctx: &egui::Context, id: &str, title: &str, width: f32, open: &mut bool, scroll: bool, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let t = ui::theme(ctx);
    let shown = Id::new(id).with("shown");
    let k = ui::motion::appear(ctx, shown, *open);
    if !*open && k <= 0.02 {
        ui::motion::reset(ctx, shown);
        return false;
    }
    let fade = k.clamp(0.0, 1.0);
    let area = egui::Modal::default_area(Id::new(id)).interactable(*open);
    let resp = egui::Modal::new(Id::new(id)).area(area).frame(egui::Frame::NONE).backdrop_color(Color32::from_black_alpha((80.0 * fade) as u8)).show(ctx, |ui| {
        // From 96% and a few points lower, on the spring of `appear`.
        let c = ui.ctx().content_rect().center();
        let still = ui::motion::reduced(ui.ctx());
        let s = if still { 1.0 } else { 0.96 + 0.04 * k.min(1.02) };
        let lift = ui::motion::travel(ui.ctx(), k) * 10.0;
        let transform = egui::emath::TSTransform::new(vec2(c.x * (1.0 - s), c.y * (1.0 - s) + lift), s);
        ui.multiply_opacity(fade);
        // Room for what the dialog may become (an area lays out in last frame's size): a share
        // dialog that grows from its form to the link must not stay cut at its first height.
        ui.set_max_size(ui.ctx().content_rect().size());
        let max_w = (ui.ctx().content_rect().width() - 24.0).max(260.0);
        let dialog_w = width.min(max_w);
        let max_h = (ui.ctx().content_rect().height() - 80.0).max(200.0);
        ui.with_visual_transform(transform, |ui| {
            let frame = egui::Frame::new()
                .fill(t.bg)
                .corner_radius(ui::RADIUS_LG)
                .stroke(if t.dark { Stroke::new(1.0, t.border) } else { Stroke::NONE })
                .shadow(egui::Shadow { offset: [0, 16], blur: 44, spread: 0, color: Color32::from_black_alpha(if t.dark { 110 } else { 46 }) });
            frame.show(ui, |ui| {
                ui.set_width(dialog_w);
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 10, top: 10, bottom: 10 }).show(ui, |ui| {
                    ui::row(ui, 24.0, |ui| {
                        ui.label(RichText::new(title).font(ui::medium(ui::px(13.0))).color(t.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                                *open = false;
                            }
                        });
                    });
                });
                let (r, _) = ui.allocate_exact_size(vec2(dialog_w, 1.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
                if scroll {
                    egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 10.0;
                            body(ui);
                        });
                    });
                } else {
                    egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        body(ui);
                    });
                }
            });
        });
    });
    if *open && resp.should_close() {
        *open = false;
    }
    true
}

/// A pad to try the pen on: the pressure Tratto reads from it, live, and the line it gives;
/// and how the pen reaches Tratto (or what to do when it brings no pressure).
fn pressure_pad(ui: &mut egui::Ui, p: &Prefs) {
    let t = ui::theme(ui.ctx());
    let id = Id::new("pressure-pad");
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::drag());
    ui.painter().rect(rect, ui::RADIUS, t.bg2, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    // The pressure now: a pen's touch (Windows Ink, a tablet, an iPad), or the pen behind the
    // mouse pointer (Wintab, a browser).
    let force = ui.input(|i| i.events.iter().rev().find_map(|e| if let egui::Event::Touch { force: Some(f), .. } = e { Some(*f) } else { None })).or_else(|| crate::pen::mouse_pen().map(|m| m.pressure));
    let (mut pts, mut last): (Vec<f32>, Option<f32>) = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if resp.drag_started() {
        pts.clear();
        last = None;
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let pressure = force.map_or(-1.0, |f| f.clamp(0.0, 1.0).powf(p.sensitivity.gamma() as f32));
        if pts.len() < 3 * 4000 {
            pts.extend([pos.x - rect.min.x, pos.y - rect.min.y, pressure]);
        }
        last = Some(pressure);
    }
    if pts.len() >= 3 {
        let pen = p.pens.first().cloned().unwrap_or_default();
        let color = crate::model::color_or(&pen.color, crate::model::DARK);
        let color = if t.dark && color == crate::model::DARK { crate::prims::LIGHT_INK } else { color };
        let size = if pen.brush == crate::model::Brush::Airbrush { 24.0 } else { pen.size.clamp(3.0, 14.0) };
        let prims = crate::prims::brush_prims(&pts, size, Some(pen.brush), false, color, (rect.min.x as f64, rect.min.y as f64), None);
        let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
        for m in crate::paint::brush_meshes(ui.ctx(), prims, 1.0, |x, y| rect.min + vec2(x, y)) {
            painter.add(m);
        }
    }
    let font = egui::FontId::proportional(ui::px(10.0));
    let text = match last {
        Some(v) if v >= 0.0 => format!("Pressione: {:.0}%", v * 100.0),
        Some(_) => "Nessuna pressione: mouse, o penna senza pressione".into(),
        None => "Prova qui la penna".into(),
    };
    ui.painter().with_clip_rect(rect).text(rect.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, text, font.clone(), t.text2);
    ui.data_mut(|d| d.insert_temp(id, (pts, last)));
    let how = match crate::pen::source() {
        Some(s) => format!("Penna collegata tramite {s}."),
        None => "Nessuna penna ancora usata. Con una tavoletta grafica, la pressione arriva da Windows Ink o, se è spento nel suo pannello di controllo, da Wintab.".into(),
    };
    ui.add(egui::Label::new(RichText::new(how).font(font).color(t.text2)).wrap());
}

/// A setting: its name (and a line about it) on the left, the control on the right.
fn row(ui: &mut egui::Ui, label: &str, hint: Option<&str>, control: impl FnOnce(&mut egui::Ui)) {
    let t = ui::theme(ui.ctx());
    // On a phone the name goes above its control, which then has the whole width.
    if ui::narrow(ui.ctx()) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(RichText::new(label).font(ui::medium(ui::px(11.0))).color(t.text));
            if let Some(h) = hint {
                ui.label(RichText::new(h).size(ui::px(10.0)).color(t.text2));
            }
            ui::wrap_row(ui, 32.0, control);
        });
        return;
    }
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(176.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(label).font(ui::medium(ui::px(11.0))).color(t.text));
            if let Some(h) = hint {
                ui.label(RichText::new(h).size(ui::px(10.0)).color(t.text2));
            }
        });
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui::row(ui, 28.0, control);
        });
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Hash, Debug)]
enum SettingsTab {
    #[default]
    Access,
    Profile,
    Look,
    Toolbar,
    Pen,
    Keys,
    Updates,
}

pub const ACCENTS: [(&str, &str); 7] = [("#9747FF", "Viola"), ("#0D99FF", "Blu"), ("#E84393", "Rosa"), ("#E03131", "Rosso"), ("#F76707", "Arancione"), ("#14AE5C", "Verde"), ("#0C8599", "Petrolio")];

/// Opens the settings on a section (0 Accessibilità … 3 Aggiornamenti), for pictures in tests.
#[cfg(test)]
pub fn open_settings_tab(ctx: &egui::Context, n: usize) {
    let tab = [SettingsTab::Access, SettingsTab::Profile, SettingsTab::Look, SettingsTab::Toolbar, SettingsTab::Pen, SettingsTab::Keys, SettingsTab::Updates][n.min(6)];
    ctx.data_mut(|d| d.insert_temp(Id::new("settings-tab"), tab));
}

/// Size of the settings: the same for every section, so nothing jumps when you switch.
const SETTINGS_W: f32 = 700.0;
const SETTINGS_H: f32 = 440.0;
const SIDEBAR_W: f32 = 168.0;

/// Settings (Ctrl+,), also from the board list. Sections on the left (the highlight glides to
/// the chosen one), the chosen section on the right, fading in.
pub fn settings(ctx: &egui::Context, open: &mut bool, p: &mut Prefs, host: bool) {
    let tab_id = Id::new("settings-tab");
    let mut tab: SettingsTab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    let _ = dialog_opt(ctx, "settings", "Impostazioni", SETTINGS_W, open, false, |ui| {
        let t = ui::theme(ui.ctx());
        let all: &[(SettingsTab, &str, &str)] = &[
            (SettingsTab::Access, "person", "Accessibilità"),
            (SettingsTab::Profile, "user-round", "Profilo"),
            (SettingsTab::Look, "palette", "Personalizzazione"),
            (SettingsTab::Toolbar, "sliders-horizontal", "Barra degli strumenti"),
            (SettingsTab::Pen, "pen", "Penna"),
            (SettingsTab::Keys, "keyboard", "Scorciatoie"),
            (SettingsTab::Updates, "refresh-cw", "Aggiornamenti"),
        ];
        let tabs: &[(SettingsTab, &str, &str)] = if host { all } else { &all[..all.len() - 1] };
        let h = SETTINGS_H.min((ui.ctx().content_rect().height() - 160.0).max(280.0));
        // A phone: the sections as chips above, the chosen one below with the whole width.
        if ui::narrow(ui.ctx()) {
            let chips = ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for (v, icon, label) in tabs {
                    let on = *v == tab;
                    let g = ui.painter().layout_no_wrap(label.to_string(), if on { ui::medium(ui::px(11.0)) } else { egui::FontId::proportional(ui::px(11.0)) }, t.text);
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 40.0, 36.0), Sense::click());
                    ui.painter().rect_filled(r, 18.0, if on { t.selected } else { t.bg2 });
                    ui::icon(ui, icon, pos2(r.min.x + 16.0, r.center().y), 14.0, if on { t.text } else { t.icon2 });
                    ui.painter().galley(pos2(r.min.x + 28.0, r.center().y - g.size().y / 2.0), g, t.text);
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *label));
                    ui::focus_ring(ui, &resp, r);
                    if resp.clicked() {
                        tab = *v;
                    }
                }
            });
            ui.add_space(4.0);
            // The section scrolls in what the window has left under the title and the chips
            // (with bigger text the chips take more rows, and the dialog went off the bottom).
            let h = (ui.ctx().content_rect().height() - 40.0 - 44.0 - 36.0 - chips.response.rect.height()).max(80.0);
            egui::ScrollArea::vertical().id_salt(("settings-scroll", tab)).auto_shrink([false, true]).max_height(h).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                settings_section(ui, tab, p, host);
            });
            return;
        }
        // As wide as its longest name needs (bigger text makes them longer), within reason.
        let side_w = tabs.iter().map(|(_, _, l)| ui.painter().layout_no_wrap(l.to_string(), ui::medium(ui::px(11.0)), t.text).size().x).fold(0.0, f32::max) + 34.0 + 12.0 + 14.0;
        let side_w = side_w.clamp(SIDEBAR_W, SETTINGS_W * 0.4);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // Sections.
            ui.vertical(|ui| {
                ui.set_width(side_w);
                ui.set_height(h);
                ui.spacing_mut().item_spacing.y = 2.0;
                let rects: Vec<egui::Rect> = tabs.iter().map(|_| ui.allocate_exact_size(vec2(side_w - 12.0, ui::ctl(32.0)), Sense::hover()).0).collect();
                let at = tabs.iter().position(|x| x.0 == tab).unwrap_or(0);
                let y = ui::motion::spring(ui.ctx(), Id::new("settings-tab-y"), rects[at].min.y - rects[0].min.y, 0.3, 0.88);
                ui.painter().rect_filled(rects[0].translate(vec2(0.0, y)), ui::RADIUS, t.selected);
                for (i, (v, icon, label)) in tabs.iter().enumerate() {
                    let r = rects[i];
                    let resp = ui.interact(r, Id::new(("settings-tab", i)), Sense::click());
                    ui::hover_fill(ui, resp.id, r, resp.hovered() && i != at, ui::RADIUS as f32, t.hover);
                    let on = i == at;
                    ui::icon(ui, icon, pos2(r.min.x + 16.0, r.center().y), 16.0, if on { t.text } else { t.icon2 });
                    ui.painter().with_clip_rect(r).text(pos2(r.min.x + 34.0, r.center().y), egui::Align2::LEFT_CENTER, *label, if on { ui::medium(ui::px(11.0)) } else { egui::FontId::proportional(ui::px(11.0)) }, t.text);
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *label));
                    ui::focus_ring(ui, &resp, r);
                    if resp.clicked() {
                        tab = *v;
                    }
                }
            });
            let (line, _) = ui.allocate_exact_size(vec2(1.0, h), Sense::hover());
            ui.painter().vline(line.center().x, line.y_range(), Stroke::new(1.0, t.border));
            ui.add_space(20.0);
            // The chosen section slides in from the side it comes from (from below the list
            // when going down it, from above when going up), fading in.
            let content_w = (ui.available_width() - 8.0).max(200.0);
            let index = tabs.iter().position(|x| x.0 == tab).unwrap_or(0) as f32;
            let from_id = Id::new("settings-tab-from");
            let (seen, dir) = ui.data(|d| d.get_temp::<(f32, f32)>(from_id)).unwrap_or((index, 0.0));
            let dir = if seen != index { (index - seen).signum() } else { dir };
            ui.data_mut(|d| d.insert_temp(from_id, (index, dir)));
            ui.vertical(|ui| {
                ui.set_width(content_w);
                ui.set_height(h);
                let k = ui::motion::entering(ui.ctx(), Id::new("settings-content"), tab);
                ui.multiply_opacity(k.clamp(0.0, 1.0));
                let shift = ui::motion::travel(ui.ctx(), k.clamp(0.0, 1.0)) * 28.0 * dir;
                ui.with_visual_transform(egui::emath::TSTransform::from_translation(vec2(0.0, shift)), |ui| {
                    egui::ScrollArea::vertical().id_salt(("settings-scroll", tab)).auto_shrink(false).max_height(h).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 12.0;
                        ui.set_width(content_w - 8.0);
                        settings_section(ui, tab, p, host);
                    });
                });
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

fn settings_section(ui: &mut egui::Ui, tab: SettingsTab, p: &mut Prefs, host: bool) {
    match tab {
        SettingsTab::Access => {
            ui::heading(ui, "Accessibilità");
            row(ui, "Tema", None, |ui| {
                ui::segmented(ui, &mut p.theme, &[(Theme::System, "Sistema"), (Theme::Light, "Chiaro"), (Theme::Dark, "Scuro")], 270.0);
            });
            row(ui, "Dimensione dell'interfaccia", None, |ui| {
                let mut v = (p.ui_scale * 100.0).round() as i32;
                if ui::segmented(ui, &mut v, &[(90, "90%"), (100, "100%"), (115, "115%"), (130, "130%"), (150, "150%")], 270.0) {
                    p.ui_scale = v as f32 / 100.0;
                }
            });
            row(ui, "Dimensione del testo", Some("Solo l'interfaccia."), |ui| {
                let mut v = (p.text_scale * 100.0).round() as i32;
                if ui::segmented(ui, &mut v, &[(100, "Normale"), (118, "Grande"), (136, "Molto grande")], 270.0) {
                    p.text_scale = v as f32 / 100.0;
                }
            });
            ui.add_space(2.0);
            ui::switch(ui, &mut p.high_contrast, "Contrasto elevato");
            ui::switch(ui, &mut p.big_handles, "Maniglie di selezione grandi");
            ui::switch(ui, &mut p.reduce_motion, "Riduci le animazioni");
        }
        SettingsTab::Profile => {
            ui::heading(ui, "Profilo");
            ui::hint(ui, "Nome e colore che vedono gli altri.");
            profile_preview(ui, p);
            ui::field_label(ui, "Nome");
            ui::text_field(ui, &mut p.name, "Come ti vedranno gli altri", 40);
            ui::field_label(ui, "Il tuo colore");
            let cur = p.own_color().unwrap_or("").to_string();
            if let Some(c) = ui::color_dots(ui, &super::panels::PERSON_COLORS, &cur, "Il tuo colore") {
                p.color = c;
            }
            ui::field_label(ui, "Colore del puntatore laser");
            if let Some(c) = ui::color_dots(ui, &super::toolbar::LASER_COLORS, &p.laser_color, "Colore del puntatore laser") {
                p.laser_color = c;
            }
        }
        SettingsTab::Toolbar => toolbar_settings(ui, p),
        SettingsTab::Keys => {
            ui::heading(ui, "Scorciatoie da tastiera");
            shortcut_list(ui);
        }
        SettingsTab::Look => {
            ui::heading(ui, "Personalizzazione");
            row(ui, "Colore principale", None, |ui| {
                let t = ui::theme(ui.ctx());
                ui.spacing_mut().item_spacing.x = 2.0;
                for (value, name) in ACCENTS {
                    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                    let h = ui::motion::hover(ui.ctx(), resp.id.with("h"), resp.hovered());
                    ui.painter().circle_filled(r.center(), 9.0 + h * 0.6, crate::model::parse_color(value).unwrap());
                    if p.accent.eq_ignore_ascii_case(value) {
                        ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(2.0, t.text));
                    }
                    if ui::tip(resp, name, None).clicked() {
                        p.accent = value.into();
                    }
                }
                ui.add_space(4.0);
                if let Some(c) = ui::color_well(ui, "Colore personalizzato", p.accent()) {
                    p.accent = crate::model::hex(c);
                }
            });
            row(ui, "Barra degli strumenti", None, |ui| {
                ui::segmented(ui, &mut p.toolbar_pos, &[(ToolbarPos::Bottom, "In basso"), (ToolbarPos::Top, "In alto")], 200.0);
            });
            row(ui, "Controlli dello zoom", None, |ui| {
                ui::segmented(ui, &mut p.zoom_full, &[(false, "− +"), (true, "Con percentuale")], 270.0);
            });
            row(ui, "Dimensione della barra", None, |ui| {
                let mut v = (p.toolbar_size * 100.0).round() as i32;
                if ui::segmented(ui, &mut v, &[(85, "Compatta"), (100, "Normale"), (120, "Grande")], 270.0) {
                    p.toolbar_size = v as f32 / 100.0;
                }
            });
            ui::switch(ui, &mut p.timer_banner, "Timer grande in alto");
            ui::switch(ui, &mut p.timer_sound, "Suono a fine timer");
            ui::switch(ui, &mut p.sticky_arrows, "Freccia tra le note aggiunte con +");
            ui::switch(ui, &mut p.peer_rulers, "Righelli degli altri");
            ui.add_space(4.0);
            ui::heading(ui, "Ombre");
            ui::switch(ui, &mut p.shadows, "Ombra sotto note, codice e widget nuovi");
            if p.shadows {
                row(ui, "Densità", None, |ui| {
                    ui::slider_inline(ui, "", &mut p.shadow_opacity, 0.02, 0.5, 0.01, false, |v| format!("{}%", (v * 100.0).round()), 200.0);
                });
                row(ui, "Sfumatura", None, |ui| {
                    ui::slider_inline(ui, "", &mut p.shadow_blur, 0.0, 32.0, 1.0, false, |v| format!("{}", v.round()), 200.0);
                });
            }
            ui.add_space(4.0);
            ui::heading(ui, "Pannelli");
            ui::switch(ui, &mut p.left_panel, "Livelli e modelli, a sinistra");
            ui::switch(ui, &mut p.right_panel, "Design e condivisione, a destra");
            ui::switch(ui, &mut p.minimap, "Minimappa");
            if host {
                ui.add_space(4.0);
                ui::heading(ui, "Sfondo delle nuove lavagne");
                super::panels::background_fields(ui, &mut p.new_board);
            }
        }
        SettingsTab::Pen => {
            ui::heading(ui, "Penna");
            row(ui, "Levigatura del tratto", None, |ui| {
                ui::segmented(ui, &mut p.ink_smoothing, &[(Smoothing::Low, "Bassa"), (Smoothing::Medium, "Media"), (Smoothing::High, "Alta")], 240.0);
            });
            ui::switch(ui, &mut p.pressure, "Spessore secondo la pressione");
            if p.pressure {
                row(ui, "Sensibilità", Some("Quanto premere per il tratto pieno"), |ui| {
                    ui::segmented(ui, &mut p.sensitivity, &[(Sensitivity::Light, "Leggera"), (Sensitivity::Normal, "Normale"), (Sensitivity::Firm, "Decisa")], 240.0);
                });
                pressure_pad(ui, p);
            }
            ui::switch(ui, &mut p.ink_to_shape, "Riconosci le forme disegnate");
            ui::switch(ui, &mut p.finger_draw, "Disegna con le dita");
            let mut zoom = p.wheel == Wheel::Zoom;
            if ui::switch(ui, &mut zoom, "La rotellina del mouse fa zoom") {
                p.wheel = if zoom { Wheel::Zoom } else { Wheel::Pan };
            }
            ui::hint(ui, "Tasto laterale della penna: lazo · Tieni premuto: menu");
        }
        SettingsTab::Updates => {
            ui::heading(ui, "Aggiornamenti");
            ui::hint(ui, &format!("Versione installata: {}", env!("CARGO_PKG_VERSION")));
            #[cfg(not(target_arch = "wasm32"))]
            crate::updates::settings_ui(ui);
            #[cfg(target_os = "linux")]
            linux_install(ui);
            let _ = host;
        }
    }
}

/// Linux: Tratto in the applications menu, from its AppImage.
#[cfg(target_os = "linux")]
fn linux_install(ui: &mut egui::Ui) {
    if crate::install::appimage().is_none() {
        return;
    }
    ui.add_space(6.0);
    ui::heading(ui, "Installazione");
    let installed = crate::install::in_menu();
    ui::hint(ui, if installed { "Tratto è nel menu delle applicazioni." } else { "Aggiungi Tratto al menu delle applicazioni." });
    let result = if installed {
        ui::button(ui, "Togli dal menu delle applicazioni", Btn::Secondary, Some("trash-2"), false, true).clicked().then(crate::install::uninstall)
    } else {
        ui::button(ui, "Aggiungi al menu delle applicazioni", Btn::Primary, Some("download"), false, true).clicked().then(crate::install::install)
    };
    if let Some(Err(e)) = result {
        ui.label(RichText::new(e).color(ui::theme(ui.ctx()).danger_text));
    }
}

const SHORTCUTS: [(&str, &[(&str, &str)]); 4] = [
    (
        "Strumenti",
        &[
            ("Seleziona", "V"),
            ("Mano", "H  ·  Spazio"),
            ("Penna", "P"),
            ("Penne 1–4", "1 2 3 4"),
            ("Evidenziatore", "M"),
            ("Gomma", "E"),
            ("Emote", "tieni premuto E"),
            ("Lazo", "Q"),
            ("Righello", "U"),
            ("Rettangolo / Ellisse", "R / O"),
            ("Connettore / Freccia", "X  ·  Maiusc+L"),
            ("Linea", "L"),
            ("Testo", "T"),
            ("Nota adesiva", "S"),
            ("Sezione", "Maiusc+S"),
            ("Nastro adesivo", "W"),
            ("Commento", "C"),
            ("Chat vicino al cursore", "/"),
            ("Immagine", "I"),
            ("Puntatore laser", "K"),
        ],
    ),
    (
        "Modifica",
        &[
            ("Annulla", "Ctrl+Z"),
            ("Ripeti", "Ctrl+Maiusc+Z"),
            ("Copia / Incolla", "Ctrl+C / Ctrl+V"),
            ("Duplica", "Ctrl+D"),
            ("Elimina", "Canc"),
            ("Scrivi nella forma o nota", "Invio  ·  doppio clic"),
            ("Seleziona tutto", "Ctrl+A"),
            ("Metti in una cartella", "Ctrl+G"),
            ("Togli dalla cartella", "Ctrl+Maiusc+G"),
            ("Sposta di 1 / 10", "Frecce / Maiusc+Frecce"),
            ("Porta avanti / indietro", "Ctrl+] / Ctrl+["),
            ("Blocca", "Ctrl+Maiusc+L"),
            ("Nascondi", "Ctrl+Maiusc+H"),
            ("Duplica trascinando", "Alt + trascina"),
            ("Apri un link", "Ctrl + clic"),
            ("Senza aggancio", "Ctrl + trascina"),
        ],
    ),
    (
        "Diagrammi e mappe",
        &[
            ("Nodo collegato dopo questo", "Tab"),
            ("Nodo accanto (stesso ramo)", "Maiusc+Tab"),
            ("Nodo collegato in una direzione", "Ctrl+frecce"),
            ("Nodo collegato col mouse", "+ attorno a forme e note"),
            ("Scrivi nella cella successiva", "Tab nella tabella"),
        ],
    ),
    (
        "Vista",
        &[
            ("Zoom avanti / indietro", "Ctrl++ / Ctrl+−"),
            ("Zoom con rotellina", "Ctrl + rotellina"),
            ("Adatta alla lavagna", "Maiusc+1"),
            ("Adatta alla selezione", "Maiusc+2"),
            ("Zoom 100%", "Maiusc+0"),
            ("Nascondi o mostra i pannelli", "Ctrl+\\"),
            ("Modalità zen", "Alt+Z  ·  Ctrl+Maiusc+\\"),
            ("Esporta", "Ctrl+Maiusc+E"),
            ("Impostazioni", "Ctrl+,"),
        ],
    ),
];

/// How the others see you: your cursor with your name, in your colour, and your laser.
fn profile_preview(ui: &mut egui::Ui, p: &Prefs) {
    let t = ui::theme(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::hover());
    ui.painter().rect_filled(r, 10.0, t.bg2);
    let color = p.own_color().and_then(crate::model::parse_color).unwrap_or(Color32::from_rgb(0x0D, 0x99, 0xFF));
    let name = if p.name.trim().is_empty() { "Il tuo nome".to_string() } else { p.name.trim().to_string() };
    let tip = r.min + vec2(64.0, 22.0);
    let arrow = [(2.0, 1.5), (15.5, 7.6), (9.0, 9.4), (6.9, 16.0)].map(|(a, b)| tip + vec2(a, b) * 1.4);
    ui.painter().add(egui::Shape::convex_polygon(vec![arrow[0], arrow[1], arrow[2]], color, Stroke::NONE));
    ui.painter().add(egui::Shape::convex_polygon(vec![arrow[0], arrow[2], arrow[3]], color, Stroke::NONE));
    ui.painter().add(egui::Shape::closed_line(arrow.to_vec(), Stroke::new(1.4, Color32::WHITE)));
    let on = if crate::model::is_dark(&crate::model::hex(color)) { Color32::WHITE } else { crate::model::DARK };
    let g = ui.painter().layout_no_wrap(name, ui::medium(ui::px(11.0)), on);
    let label = egui::Rect::from_min_size(tip + vec2(20.0, 22.0), g.size() + vec2(12.0, 4.0));
    ui.painter().rect_filled(label, egui::CornerRadius { nw: 4, ne: 12, sw: 12, se: 12 }, color);
    ui.painter().galley(label.min + vec2(6.0, 2.0), g, on);
    // A short laser trail on the right.
    let laser = crate::model::parse_color(&p.laser_color).unwrap_or(Color32::RED);
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            let s = i as f32 / 16.0;
            pos2(r.max.x - 190.0 + s * 140.0, r.center().y + (s * std::f32::consts::TAU).sin() * 12.0)
        })
        .collect();
    for (i, w) in pts.windows(2).enumerate() {
        let k = (i + 1) as f32 / pts.len() as f32;
        ui.painter().line_segment([w[0], w[1]], Stroke::new(2.0 + 3.0 * k, laser.gamma_multiply(0.2 + 0.8 * k)));
    }
    ui.painter().circle_filled(*pts.last().unwrap(), 3.0, Color32::WHITE);
}

/// Every shortcut, grouped, with what it does.
fn shortcut_list(ui: &mut egui::Ui) {
    let t = ui::theme(ui.ctx());
    for (group, list) in SHORTCUTS {
        ui::heading(ui, group);
        egui::Grid::new(group).num_columns(2).spacing(vec2(24.0, 6.0)).show(ui, |ui| {
            for (label, keys) in list.iter() {
                ui.label(RichText::new(*label).color(t.text));
                ui.label(RichText::new(*keys).color(t.text2));
                ui.end_row();
            }
        });
        ui.add_space(8.0);
    }
    ui::hint(ui, "Tasto laterale: lazo. Retro della penna: gomma. Due dita: sposta e zoom.");
}

pub fn shortcuts(ctx: &egui::Context, open: &mut bool) {
    let _ = dialog_opt(ctx, "shortcuts", "Scorciatoie da tastiera", 640.0, open, false, |ui| {
        egui::ScrollArea::vertical().max_height(480.0).auto_shrink([false, true]).show(ui, shortcut_list);
    });
}

/// The toolbar as you want it: drag the buttons into order, show or hide each one, and keep
/// toolbars under a name to switch between them.
/// One row of the toolbar list: grip, icon and name on `fill`.
fn draw_toolbar_row(ui: &mut egui::Ui, row: Rect, id: &str, name: &str, hidden: bool, fill: Color32, t: &ui::Theme) {
    let r = row.shrink2(vec2(0.0, 2.0));
    ui.painter().rect_filled(r, ui::RADIUS, fill);
    ui::icon(ui, "grip-vertical", pos2(r.min.x + 12.0, r.center().y), 14.0, t.icon2);
    ui::icon(ui, super::toolbar::toolbar_icon(id), pos2(r.min.x + 36.0, r.center().y), 16.0, if hidden { t.text3 } else { t.icon });
    ui.painter().text(pos2(r.min.x + 56.0, r.center().y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(ui::px(11.0)), if hidden { t.text3 } else { t.text });
}

fn toolbar_settings(ui: &mut egui::Ui, p: &mut Prefs) {
    let t = ui::theme(ui.ctx());
    ui::heading(ui, "Barra degli strumenti");
    ui::hint(ui, "Trascina per riordinare.");
    let mut items = crate::prefs::toolbar_items(&p.toolbar_order);
    let mut moved: Option<(usize, usize)> = None;
    // Dragged like a list on a phone: the row taken lifts and follows the pointer, the others
    // slide out of its way on a spring.
    let drag_id = ui.id().with("toolbar-drag");
    let mut drag: Option<(usize, f32)> = ui.data(|d| d.get_temp(drag_id));
    let dragging = drag.is_some();
    ui.spacing_mut().item_spacing.y = 0.0;
    let n = items.len();
    let row_h = ui::ctl(34.0);
    let (list, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h * n as f32), Sense::hover());
    let pointer_y = ui.ctx().input(|i| i.pointer.interact_pos()).map(|q| q.y);
    let target = match (drag, pointer_y) {
        (Some((_, grab)), Some(y)) => Some((((y - grab - list.min.y) / row_h).round().clamp(0.0, n as f32 - 1.0)) as usize),
        _ => None,
    };
    let mut lifted: Option<(usize, Rect)> = None;
    for (i, id) in items.clone().iter().enumerate() {
        let name = crate::prefs::TOOLBAR_ITEMS.iter().find(|x| x.0 == id).map_or("", |x| x.1);
        // Where this row goes while another is dragged over the list.
        let slot = match (drag, target) {
            (Some((from, _)), Some(to)) if i != from => {
                if from < i && i <= to {
                    i - 1
                } else if to <= i && i < from {
                    i + 1
                } else {
                    i
                }
            }
            _ => i,
        };
        let dy = ui::motion::spring(ui.ctx(), drag_id.with(("y", id.as_str())), slot as f32 * row_h, 0.28, 0.92);
        let y = list.min.y + dy;
        let held = drag.is_some_and(|(from, _)| from == i);
        let row = if held {
            let top = pointer_y.map_or(y, |py| (py - drag.unwrap().1).clamp(list.min.y - row_h * 0.5, list.max.y - row_h * 0.5));
            ui::motion::hold(ui.ctx(), drag_id.with(("y", id.as_str())), top - list.min.y);
            Rect::from_min_size(pos2(list.min.x, top), vec2(list.width(), row_h))
        } else {
            Rect::from_min_size(pos2(list.min.x, y), vec2(list.width(), row_h))
        };
        let drag_zone = if ui::narrow(ui.ctx()) { Rect::from_min_size(row.min, vec2(56.0, row_h)) } else { row };
        let resp = ui.interact(drag_zone, drag_id.with(("row", id.as_str())), Sense::click_and_drag());
        if resp.drag_started()
            && let Some(q) = resp.interact_pointer_pos()
        {
            drag = Some((i, q.y - row.min.y));
        }
        if held {
            lifted = Some((i, row));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            continue;
        }
        if resp.hovered() && !dragging {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        let over = ui.rect_contains_pointer(row);
        let h = ui::motion::hover(ui.ctx(), resp.id.with("h"), (resp.hovered() || over) && !dragging);
        draw_toolbar_row(ui, row, id, name, p.toolbar_hidden.contains(id), t.hover.gamma_multiply(h), &t);
        if dragging {
            continue;
        }
        // Up, down, and shown or not.
        let r = row.shrink2(vec2(0.0, 2.0));
        let hidden = p.toolbar_hidden.contains(id);
        let mut x = r.max.x - 4.0;
        let mut row_ui = ui.new_child(egui::UiBuilder::new().max_rect(r).id_salt(("row-btn", id.as_str())));
        let mut button = |name: &str, label: &str, on: bool| {
            let b = Rect::from_min_size(pos2(x - 24.0, r.center().y - 12.0), vec2(24.0, 24.0));
            x -= 26.0;
            let resp = row_ui.put(b, |ui: &mut egui::Ui| ui::small_icon_button(ui, name, label, on));
            resp.clicked()
        };
        if button(if hidden { "eye-off" } else { "eye" }, if hidden { "Mostra nella barra" } else { "Nascondi dalla barra" }, false) {
            if hidden {
                p.toolbar_hidden.retain(|x| x != id);
            } else {
                p.toolbar_hidden.push(id.clone());
            }
        }
        // The arrows show under the pointer (or on narrow screens) so the list stays calm.
        if (over || ui::narrow(ui.ctx())) && i + 1 < n && button("arrow-down", "Sposta più a destra", false) {
            moved = Some((i, i + 2));
        }
        if (over || ui::narrow(ui.ctx())) && i > 0 && button("arrow-up", "Sposta più a sinistra", false) {
            moved = Some((i, i - 1));
        }
    }
    // The row being moved, over the others: lifted on a soft shadow.
    if let Some((i, row)) = lifted {
        let id = &items[i];
        let name = crate::prefs::TOOLBAR_ITEMS.iter().find(|x| x.0 == id).map_or("", |x| x.1);
        let layer = egui::LayerId::new(egui::Order::Tooltip, drag_id.with("lifted"));
        let painter = ui.ctx().layer_painter(layer);
        let r = row.shrink2(vec2(0.0, 2.0));
        painter.add(egui::epaint::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(40) }.as_shape(r, ui::RADIUS));
        painter.rect_filled(r, ui::RADIUS, t.bg);
        let mut child = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(row));
        draw_toolbar_row(&mut child, row, id, name, p.toolbar_hidden.contains(id), t.selected, &t);
    }
    // Let go: it stays where it was shown.
    if let Some((from, _)) = drag
        && !ui.ctx().input(|i| i.pointer.any_down())
    {
        if let Some(to) = target
            && to != from
        {
            moved = Some((from, if to > from { to + 1 } else { to }));
        }
        drag = None;
    }
    ui.data_mut(|d| match drag {
        Some(v) => {
            d.insert_temp(drag_id, v);
        }
        None => d.remove::<(usize, f32)>(drag_id),
    });
    if let Some((from, to)) = moved
        && from < items.len()
    {
        let it = items.remove(from);
        let to = if to > from { to - 1 } else { to };
        items.insert(to.min(items.len()), it);
        p.toolbar_order = items.clone();
    }
    ui.spacing_mut().item_spacing.y = 12.0;
    ui.add_space(8.0);
    ui::row(ui, 28.0, |ui| {
        if ui::button(ui, "Ripristina la barra predefinita", Btn::Secondary, Some("rotate-ccw"), false, true).clicked() {
            p.toolbar_order = crate::prefs::toolbar_default();
            p.toolbar_hidden = crate::prefs::toolbar_hidden_default();
        }
    });
    ui.add_space(4.0);
    ui::heading(ui, "Profili della barra");
    ui::hint(ui, "Salva questa barra con un nome.");
    let mut remove = None;
    if p.toolbar_profiles.is_empty() {
        ui::hint(ui, "Nessun profilo salvato.");
    }
    for (i, prof) in p.toolbar_profiles.clone().iter().enumerate() {
        ui::row(ui, 30.0, |ui| {
            let current = crate::prefs::toolbar_items(&prof.order) == crate::prefs::toolbar_items(&p.toolbar_order) && prof.hidden == p.toolbar_hidden;
            ui::icon(ui, if current { "check" } else { "sliders-horizontal" }, ui.cursor().min + vec2(8.0, 15.0), 14.0, if current { t.brand } else { t.icon2 });
            ui.add_space(20.0);
            ui.label(RichText::new(&prof.name).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::small_icon_button(ui, "trash-2", "Elimina il profilo", false).clicked() {
                    remove = Some(i);
                }
                if !current && ui::button(ui, "Usa", Btn::Secondary, None, false, true).clicked() {
                    p.toolbar_order = crate::prefs::toolbar_items(&prof.order);
                    p.toolbar_hidden = prof.hidden.clone();
                }
            });
        });
    }
    if let Some(i) = remove {
        p.toolbar_profiles.remove(i);
    }
    let nid = Id::new("toolbar-profile-name");
    let mut name: String = ui.data(|d| d.get_temp(nid)).unwrap_or_default();
    ui::row(ui, ui::ctl(34.0), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let ok = !name.trim().is_empty();
            let save_clicked = ui::button(ui, "Salva profilo", Btn::Primary, Some("save"), true, ok).clicked();
            ui.add_space(6.0);
            let field = ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| ui::text_field(ui, &mut name, "Nome del profilo", 30)).inner;
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (save_clicked || enter) && !name.trim().is_empty() {
                let prof = crate::prefs::ToolbarProfile { name: name.trim().to_string(), order: crate::prefs::toolbar_items(&p.toolbar_order), hidden: p.toolbar_hidden.clone() };
                match p.toolbar_profiles.iter_mut().find(|x| x.name == prof.name) {
                    Some(x) => *x = prof,
                    None => p.toolbar_profiles.push(prof),
                }
                name.clear();
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(nid, name));
}

/* ---------------- export ---------------- */

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpg,
    Svg,
    Pdf,
    Tratto,
}

#[derive(Clone)]
struct ExportState {
    format: Format,
    scale: u32,
    background: bool,
    only_selection: bool,
    a4: bool,
}

pub fn export(ctx: &egui::Context, open: &mut bool, selection_first: bool, b: &mut BoardScreen, toasts: &mut Toasts) {
    let sid = Id::new("export-state");
    let has_sel = !b.editor.selection.is_empty();
    let fresh = ctx.data(|d| d.get_temp::<bool>(Id::new("export-open"))) != Some(true);
    let mut s: ExportState = ctx.data(|d| d.get_temp(sid)).unwrap_or(ExportState { format: Format::Png, scale: 2, background: true, only_selection: false, a4: false });
    if fresh {
        s.only_selection = selection_first && has_sel;
    }
    let mut run = false;
    let mut copy = false;
    let host = b.host;
    let mut still = *open;
    let _ = dialog(ctx, "export", "Esporta", 420.0, &mut still, |ui| {
        let mut formats = vec![(Format::Png, "PNG"), (Format::Jpg, "JPG"), (Format::Svg, "SVG"), (Format::Pdf, "PDF")];
        if host {
            formats.push((Format::Tratto, "Tratto"));
        }
        ui::segmented(ui, &mut s.format, &formats, 388.0);
        ui::hint(ui, match s.format {
            Format::Png => "Alta qualità, sfondo trasparente.",
            Format::Jpg => "Leggera, per chat ed e-mail.",
            Format::Svg => "Vettoriale, modificabile in altri programmi.",
            Format::Pdf => "Documento da stampare o inviare.",
            Format::Tratto => "Da riaprire in Tratto.",
        });
        if s.format != Format::Tratto {
            if s.format == Format::Pdf {
                row(ui, "Pagina", None, |ui| {
                    ui::segmented(ui, &mut s.a4, &[(false, "Come il contenuto"), (true, "A4 da stampare")], 200.0);
                });
            }
            if s.format != Format::Svg {
                row(ui, "Risoluzione", None, |ui| {
                    ui::segmented(ui, &mut s.scale, &[(1, "1×"), (2, "2×"), (3, "3×"), (4, "4×")], 200.0);
                });
            }
            if s.format != Format::Jpg {
                ui::switch(ui, &mut s.background, "Includi lo sfondo");
            }
            let label = if has_sel { format!("Solo la selezione ({})", b.editor.selection.len()) } else { "Solo la selezione".into() };
            let mut only = s.only_selection && has_sel;
            if ui::switch(ui, &mut only, &label) {
                s.only_selection = only && has_sel;
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::button(ui, "Esporta", Btn::Primary, Some("download"), false, true).clicked() {
                    run = true;
                }
                if host && s.format != Format::Tratto && ui::button(ui, "Copia", Btn::Secondary, Some("copy"), false, true).clicked() {
                    copy = true;
                }
            });
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(sid, s.clone());
        d.insert_temp(Id::new("export-open"), still);
    });
    *open = still;
    if !(run || copy) {
        return;
    }
    let opts = crate::export::Options { ids: s.only_selection.then(|| b.editor.selection.clone()), scale: s.scale as f64, background: s.background || s.format == Format::Jpg, a4: s.a4 };
    let name = crate::export::safe_filename(&b.title);
    let els = b.editor.board.paint_order().to_vec();
    let meta = b.editor.board.meta();
    #[cfg(not(target_arch = "wasm32"))]
    let images = crate::export::store_images(b.store(), &b.id);
    #[cfg(target_arch = "wasm32")]
    let images = b.images();
    let result: Result<(Vec<u8>, &str), String> = match s.format {
        _ if copy => crate::export::png(&els, &meta, &opts, &images, false).map(|p| (p, "png")),
        Format::Png => crate::export::png(&els, &meta, &opts, &images, false).map(|p| (p, "png")),
        Format::Jpg => crate::export::png(&els, &meta, &opts, &images, true).map(|p| (p, "jpg")),
        Format::Svg => crate::export::svg(&els, &meta, &opts, &images).map(|s| (s.into_bytes(), "svg")),
        Format::Pdf => crate::export::pdf(&els, &meta, &opts, &images).map(|p| (p, "pdf")),
        #[cfg(not(target_arch = "wasm32"))]
        Format::Tratto => crate::export::tratto(b.store(), &b.id, &b.title, &crate::doc::encode_state(&b.editor.board.doc)).map(|s| (s.into_bytes(), "tratto")),
        #[cfg(target_arch = "wasm32")]
        Format::Tratto => Err("Solo il proprietario può esportarla.".into()),
    };
    match result {
        Err(e) => toasts.error(e),
        #[cfg(not(target_arch = "wasm32"))]
        Ok((png, _)) if copy => match crate::export::copy_png(&png) {
            Ok(()) => {
                toasts.info("Immagine copiata");
                *open = false;
            }
            Err(e) => toasts.error(e),
        },
        #[cfg(target_arch = "wasm32")]
        Ok((bytes, ext)) => {
            crate::guest::download(&format!("{name}.{ext}"), &bytes);
            *open = false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        Ok((bytes, ext)) => {
            if let Some(path) = rfd::FileDialog::new().set_file_name(format!("{name}.{ext}")).save_file() {
                match std::fs::write(&path, bytes) {
                    Ok(()) => {
                        toasts.info("Esportazione pronta.");
                        *open = false;
                    }
                    Err(e) => toasts.error(format!("Non riesco a salvare il file: {e}")),
                }
            }
        }
    }
}

/* ---------------- sharing ---------------- */

#[cfg(not(target_arch = "wasm32"))]
pub fn share(ctx: &egui::Context, open: &mut bool, b: &mut BoardScreen, toasts: &mut Toasts) {
    let title = format!("Condividi «{}»", b.title);
    let _ = dialog(ctx, "share", &title, 480.0, open, |ui| crate::share::dialog(ui, b, toasts));
}
