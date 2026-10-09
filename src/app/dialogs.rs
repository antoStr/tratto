//! Dialogs: settings, keyboard shortcuts, export and sharing.

use egui::{Color32, Id, RichText, Sense, Stroke, vec2};

use super::Toasts;
use super::board::BoardScreen;
use crate::prefs::{Prefs, Smoothing, Theme, ToolbarPos, Wheel};
use crate::ui::{self, Kind as Btn};

#[derive(Default)]
pub struct Dialogs {
    pub settings: bool,
}

/// A centred dialog over a dimmed window, white, 13 px corners; returns false when closed.
pub fn dialog(ctx: &egui::Context, id: &str, title: &str, width: f32, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let t = ui::theme(ctx);
    let frame = egui::Frame::new().fill(t.bg).corner_radius(ui::RADIUS_LG).inner_margin(egui::Margin::same(0)).shadow(egui::Shadow { offset: [0, 18], blur: 48, spread: 0, color: Color32::from_black_alpha(46) });
    let resp = egui::Modal::new(Id::new(id)).frame(frame).backdrop_color(Color32::from_black_alpha(90)).show(ctx, |ui| {
        ui.set_width(width);
        let mut open = true;
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 8, top: 8, bottom: 8 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(ui::medium(13.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::small_icon_button(ui, "x", "Chiudi", false).clicked() {
                        open = false;
                    }
                });
            });
        });
        let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
        egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            body(ui);
        });
        open
    });
    resp.inner && !resp.should_close()
}

fn row(ui: &mut egui::Ui, label: &str, hint: Option<&str>, control: impl FnOnce(&mut egui::Ui)) {
    let t = ui::theme(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(200.0);
            ui.label(RichText::new(label).font(ui::medium(11.0)).color(t.text));
            if let Some(h) = hint {
                ui.label(RichText::new(h).size(10.0).color(t.text2));
            }
        });
        control(ui);
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    Access,
    Look,
    Pen,
    Updates,
}

pub const ACCENTS: [(&str, &str); 7] = [("#0D99FF", "Blu"), ("#7B61FF", "Viola"), ("#E84393", "Rosa"), ("#E03131", "Rosso"), ("#F76707", "Arancione"), ("#14AE5C", "Verde"), ("#0C8599", "Petrolio")];

/// Settings (Ctrl+,), also from the board list.
pub fn settings(ctx: &egui::Context, open: &mut bool, p: &mut Prefs, host: bool) {
    if !*open {
        return;
    }
    let tab_id = Id::new("settings-tab");
    let mut tab: SettingsTab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    *open = dialog(ctx, "settings", "Impostazioni", 560.0, |ui| {
        let tabs: &[(SettingsTab, &str)] = if host {
            &[(SettingsTab::Access, "Accessibilità"), (SettingsTab::Look, "Personalizzazione"), (SettingsTab::Pen, "Penna"), (SettingsTab::Updates, "Aggiornamenti")]
        } else {
            &[(SettingsTab::Access, "Accessibilità"), (SettingsTab::Look, "Personalizzazione"), (SettingsTab::Pen, "Penna")]
        };
        ui::segmented(ui, &mut tab, tabs, 528.0);
        ui.add_space(4.0);
        match tab {
            SettingsTab::Access => {
                row(ui, "Tema", None, |ui| {
                    ui::segmented(ui, &mut p.theme, &[(Theme::System, "Sistema"), (Theme::Light, "Chiaro"), (Theme::Dark, "Scuro")], 220.0);
                });
                row(ui, "Dimensione dell'interfaccia", Some("Ingrandisce menu, pannelli e testi."), |ui| {
                    let mut v = (p.ui_scale * 100.0).round() as i32;
                    if ui::segmented(ui, &mut v, &[(90, "90%"), (100, "100%"), (115, "115%"), (130, "130%"), (150, "150%")], 300.0) {
                        p.ui_scale = v as f32 / 100.0;
                    }
                });
                row(ui, "Dimensione del testo", Some("Solo i testi dell'interfaccia, non quelli sulla lavagna."), |ui| {
                    let mut v = (p.text_scale * 100.0).round() as i32;
                    if ui::segmented(ui, &mut v, &[(100, "Normale"), (118, "Grande"), (136, "Molto grande")], 300.0) {
                        p.text_scale = v as f32 / 100.0;
                    }
                });
                ui::switch(ui, &mut p.high_contrast, "Contrasto elevato: testi secondari più scuri e bordi ben visibili");
                ui::switch(ui, &mut p.big_handles, "Maniglie di selezione più grandi, più facili da prendere con la penna e con le dita");
            }
            SettingsTab::Look => {
                row(ui, "Colore principale", Some("Per selezione, pulsanti ed evidenziazioni."), |ui| {
                    let t = ui::theme(ui.ctx());
                    for (value, name) in ACCENTS {
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        ui.painter().circle_filled(r.center(), 10.0, crate::model::parse_color(value).unwrap());
                        if p.accent.eq_ignore_ascii_case(value) {
                            ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(2.0, t.text));
                        }
                        if ui::tip(resp, name, None).clicked() {
                            p.accent = value.into();
                        }
                    }
                    let mut c = p.accent();
                    let before = c;
                    ui.color_edit_button_srgba(&mut c);
                    if c != before {
                        p.accent = crate::model::hex(c);
                    }
                });
                row(ui, "Barra degli strumenti", None, |ui| {
                    ui::segmented(ui, &mut p.toolbar_pos, &[(ToolbarPos::Bottom, "In basso"), (ToolbarPos::Top, "In alto")], 200.0);
                });
                ui::heading(ui, "Pannelli");
                ui::switch(ui, &mut p.left_panel, "Livelli e modelli, a sinistra");
                ui::switch(ui, &mut p.right_panel, "Design e condivisione, a destra");
                ui::switch(ui, &mut p.minimap, "Minimappa: tutta la lavagna in piccolo, clicca per spostarti");
                if host {
                    ui::heading(ui, "Sfondo delle nuove lavagne");
                    super::panels::background_fields(ui, &mut p.new_board);
                }
            }
            SettingsTab::Pen => {
                row(ui, "Levigatura del tratto", Some("Più alta toglie il tremolio, più bassa segue ogni movimento."), |ui| {
                    ui::segmented(ui, &mut p.ink_smoothing, &[(Smoothing::Low, "Bassa"), (Smoothing::Medium, "Media"), (Smoothing::High, "Alta")], 220.0);
                });
                ui::switch(ui, &mut p.pressure, "Spessore secondo la pressione: premendo di più il tratto diventa più spesso");
                ui::switch(ui, &mut p.ink_to_shape, "Da tratto a forma: cerchi, rettangoli, triangoli e linee disegnati a mano diventano forme pulite");
                ui::switch(ui, &mut p.finger_draw, "Disegna con le dita (se è spento, le dita spostano la lavagna e solo la penna disegna)");
                let mut zoom = p.wheel == Wheel::Zoom;
                if ui::switch(ui, &mut zoom, "La rotellina del mouse fa zoom") {
                    p.wheel = if zoom { Wheel::Zoom } else { Wheel::Pan };
                }
                ui::hint(ui, "Tasto laterale della penna: trascina per selezionare col lazo, tocca per aprire il menu. Tieni premuta la penna o il dito su un elemento per aprire il menu.");
            }
            SettingsTab::Updates => {
                ui::hint(ui, &format!("Versione installata: {}", env!("CARGO_PKG_VERSION")));
                #[cfg(not(target_arch = "wasm32"))]
                crate::updates::settings_ui(ui);
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}

const SHORTCUTS: [(&str, &[(&str, &str)]); 3] = [
    (
        "Strumenti",
        &[
            ("Seleziona", "V"),
            ("Mano", "H  ·  Spazio"),
            ("Penna", "P"),
            ("Penne 1–4", "1 2 3 4"),
            ("Evidenziatore", "M"),
            ("Gomma", "E"),
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
            ("Senza aggancio", "Ctrl + trascina"),
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
            ("Esporta", "Ctrl+Maiusc+E"),
            ("Impostazioni", "Ctrl+,"),
        ],
    ),
];

pub fn shortcuts(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    *open = dialog(ctx, "shortcuts", "Scorciatoie da tastiera", 640.0, |ui| {
        let t = ui::theme(ui.ctx());
        egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
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
            ui::hint(ui, "Con la penna: il tasto laterale seleziona col lazo (un tocco apre il menu), la parte superiore cancella. Tieni premuto per aprire il menu. Con due dita sposti e ingrandisci la lavagna.");
        });
    });
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
    let still = dialog(ctx, "export", "Esporta", 420.0, |ui| {
        let mut formats = vec![(Format::Png, "PNG"), (Format::Jpg, "JPG"), (Format::Svg, "SVG"), (Format::Pdf, "PDF")];
        if host {
            formats.push((Format::Tratto, "Tratto"));
        }
        ui::segmented(ui, &mut s.format, &formats, 388.0);
        ui::hint(ui, match s.format {
            Format::Png => "Immagine di alta qualità, anche con sfondo trasparente.",
            Format::Jpg => "Immagine leggera, la più facile da mandare in chat o per e-mail.",
            Format::Svg => "Vettoriale, modificabile in altri programmi.",
            Format::Pdf => "Documento da stampare o inviare.",
            Format::Tratto => "Copia completa da riaprire in Tratto, anche su un altro PC, con «Importa».",
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
        Format::Tratto => Err("Solo il proprietario può esportare la lavagna completa.".into()),
    };
    match result {
        Err(e) => toasts.error(e),
        #[cfg(not(target_arch = "wasm32"))]
        Ok((png, _)) if copy => match crate::export::copy_png(&png) {
            Ok(()) => {
                toasts.info("Immagine copiata: incollala dove vuoi con Ctrl+V.");
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
    *open = dialog(ctx, "share", &title, 480.0, |ui| crate::share::dialog(ui, b, toasts));
}
