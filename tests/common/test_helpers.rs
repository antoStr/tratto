// Shared test helper utilities for E2E integration test suites.

#[allow(dead_code)]
pub fn setup_test_theme(ctx: &egui::Context, dark: bool) {
    ui::install_fonts(ctx);
    ui::setup(ctx, ui::Theme::new(dark, model::parse_color(prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
}

#[allow(dead_code)]
pub fn create_test_editor() -> editor::Editor {
    let ctx = egui::Context::default();
    let store = store::Store::open(std::path::Path::new(":memory:")).expect("memory store");
    let id = store.create("E2E Test Board", None).expect("create board");
    let b = app::board::BoardScreen::open(&ctx, &store, &id, prefs::Prefs::default(), None).expect("open board");
    b.editor
}

#[allow(dead_code)]
pub fn simulate_click(harness: &mut egui_kittest::Harness<'_>, pos: egui::Pos2, modifiers: egui::Modifiers) {
    harness.input_mut().events.push(egui::Event::PointerMoved(pos));
    harness.step();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.step();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.step();
}

#[allow(dead_code)]
pub fn simulate_drag(
    harness: &mut egui_kittest::Harness<'_>,
    start: egui::Pos2,
    end: egui::Pos2,
    steps: usize,
    button: egui::PointerButton,
    modifiers: egui::Modifiers,
) {
    harness.input_mut().events.push(egui::Event::PointerMoved(start));
    harness.step();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: start,
        button,
        pressed: true,
        modifiers,
    });
    harness.step();
    let n = steps.max(1);
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let p = egui::pos2(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t);
        harness.input_mut().events.push(egui::Event::PointerMoved(p));
        harness.step();
    }
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: end,
        button,
        pressed: false,
        modifiers,
    });
    harness.step();
}
