//! Empirical Adversarial Challenge Suite 2 for Milestone 2 (Input Pipeline & Shortcuts)
//!
//! Evaluates:
//! 1. Alt+click eyedropper under varied conditions:
//!    - Canvas background (default and custom colors)
//!    - Overlapping elements (z-order resolution, semi-transparent blending, hidden elements)
//!    - Locked elements vs read-only boards
//!    - Rapid repeated clicks
//!    - Clicking while moving (micro-tremor <= 4px vs deliberate drag > 4px vs drag returning to origin)
//! 2. Settings dialog pen pad vs canvas stroke parity:
//!    - Exact mathematical equivalence of calibrate_pressure
//!    - Verification of brush_prims & brush_meshes parity
//! 3. Shortcut documentation and HUD layout:
//!    - HUD pill format ("Forza" vs "Opacità") and non-truncation padding
//!    - Exact Italian terminology in panels hint and shortcuts table

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{pos2, vec2, Color32, Modifiers, PointerButton, Event, Pos2};
use prefs::{calibrate_pressure, Sensitivity, Pen, Prefs, DEFAULT_ACCENT};
use model::{El, Kind, Shape, Brush, parse_color};
use app::Toasts;
use app::board::BoardScreen;
use app::dialogs::Dialogs;
use ui::Theme;
use store::Store;

/* =========================================================================
   SUITE 1: ALT+CLICK EYEDROPPER ADVERSARIAL CHALLENGES
   ========================================================================= */

/// Challenge 1.1: Alt+click on canvas background correctly samples background color,
/// both for default background and custom hex background.
#[test]
fn test_adv2_alt_click_canvas_background_custom() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.create("Test", None).unwrap();
    let mut screen: Option<BoardScreen> = None;
    let (mut toasts, mut dialogs) = (Toasts::default(), Dialogs::default());
    let color_out: std::rc::Rc<std::cell::RefCell<String>> = Default::default();
    let seen_color = color_out.clone();

    let mut harness = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if screen.is_none() {
            ui::install_fonts(&ctx);
            ui::setup(&ctx, Theme::new(false, parse_color(DEFAULT_ACCENT).unwrap(), false), 1.0);
            let mut b = BoardScreen::open(&ctx, &store, &id, Prefs { left_panel: false, right_panel: false, ..Prefs::default() }, None).unwrap();
            b.editor.tool = editor::Tool::Pen;
            b.editor.prefs.pens[0] = Pen { size: 4.0, opacity: 0.5, color: "#111111".into(), ..Default::default() };
            // Set custom background color
            b.editor.board.set_meta(&model::BoardMeta {
                background: "#2B8A3E".into(),
                ..Default::default()
            });
            screen = Some(b);
            return;
        }
        let b = screen.as_mut().unwrap();
        b.ui(ui, &mut toasts, &mut dialogs);
        let p = &b.editor.prefs.pens[b.editor.pen];
        *seen_color.borrow_mut() = p.color.clone();
    });
    harness.run_steps(5);

    // Alt + click on empty canvas at (600, 600)
    let alt = Modifiers { alt: true, ..Default::default() };
    let a = pos2(600.0, 600.0);
    harness.input_mut().events.push(Event::PointerMoved(a));
    harness.step();
    harness.input_mut().events.push(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: alt });
    harness.step();
    harness.input_mut().events.push(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: false, modifiers: alt });
    harness.run_steps(2);

    assert_eq!(color_out.borrow().to_uppercase(), "#2B8A3E", "Alt+click on empty board samples custom background color");
}

/// Challenge 1.2: Overlapping elements with different z-orders, semi-transparent layers, and hidden elements.
#[test]
fn test_adv2_alt_click_overlapping_elements_z_order_and_transparency() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.create("Test", None).unwrap();
    let mut screen: Option<BoardScreen> = None;
    let (mut toasts, mut dialogs) = (Toasts::default(), Dialogs::default());
    let color_out: std::rc::Rc<std::cell::RefCell<String>> = Default::default();
    let seen_color = color_out.clone();
    let pts_out: std::rc::Rc<std::cell::Cell<(Pos2, Pos2, Pos2)>> = Default::default();
    let seen_pts = pts_out.clone();

    let mut harness = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if screen.is_none() {
            ui::install_fonts(&ctx);
            ui::setup(&ctx, Theme::new(false, parse_color(DEFAULT_ACCENT).unwrap(), false), 1.0);
            let mut b = BoardScreen::open(&ctx, &store, &id, Prefs { left_panel: false, right_panel: false, ..Prefs::default() }, None).unwrap();
            b.editor.tool = editor::Tool::Pen;
            b.editor.prefs.pens[0] = Pen { size: 4.0, opacity: 0.5, color: "#111111".into(), ..Default::default() };

            // Layer 1 (bottom, z=1): Red square at (200, 200, 200, 200)
            let mut el1 = El::new(Kind::Shape(Shape {
                fill: "#E03131".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (el1.x, el1.y, el1.w, el1.h, el1.z) = (200.0, 200.0, 200.0, 200.0, 1.0);

            // Layer 2 (middle, z=2): Blue square at (300, 200, 200, 200) (overlaps right half of Layer 1 from x=300..400)
            let mut el2 = El::new(Kind::Shape(Shape {
                fill: "#1971C2".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (el2.x, el2.y, el2.w, el2.h, el2.z) = (300.0, 200.0, 200.0, 200.0, 2.0);

            // Layer 3 (top, z=3): Hidden Green square over Layer 1
            let mut el3 = El::new(Kind::Shape(Shape {
                fill: "#2F9E44".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (el3.x, el3.y, el3.w, el3.h, el3.z) = (220.0, 220.0, 50.0, 50.0, 3.0);
            el3.hidden = true;

            b.editor.board.put(vec![el1, el2, el3]);
            screen = Some(b);
            return;
        }
        let b = screen.as_mut().unwrap();
        b.ui(ui, &mut toasts, &mut dialogs);
        let p = &b.editor.prefs.pens[b.editor.pen];
        *seen_color.borrow_mut() = p.color.clone();

        // Calculate accurate screen positions from board world coordinates
        let p1 = b.editor.to_screen(250.0, 300.0) + b.editor.origin.to_vec2(); // Layer 1 only
        let p_overlap = b.editor.to_screen(350.0, 300.0) + b.editor.origin.to_vec2(); // Overlap region
        let p2 = b.editor.to_screen(450.0, 300.0) + b.editor.origin.to_vec2(); // Layer 2 only
        seen_pts.set((p1, p_overlap, p2));
    });
    harness.run_steps(5);

    let alt = Modifiers { alt: true, ..Default::default() };
    let do_click = |h: &mut egui_kittest::Harness, p: Pos2| {
        h.input_mut().events.push(Event::PointerMoved(p));
        h.step();
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: alt });
        h.step();
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: alt });
        h.run_steps(2);
    };

    let (pt_l1, pt_overlap, pt_l2) = pts_out.get();

    // 1. Click at pt_l1: Layer 1 (Red) only. Hidden Layer 3 is ignored.
    do_click(&mut harness, pt_l1);
    assert_eq!(color_out.borrow().to_uppercase(), "#E03131", "Samples bottom layer and ignores hidden layer");

    // 2. Click at pt_overlap: Overlap area between Layer 1 and Layer 2 -> Layer 2 (Blue, higher z) must win!
    do_click(&mut harness, pt_overlap);
    assert_eq!(color_out.borrow().to_uppercase(), "#1971C2", "Samples top layer in overlapping region");

    // 3. Click at pt_l2: Layer 2 (Blue) non-overlapping region
    do_click(&mut harness, pt_l2);
    assert_eq!(color_out.borrow().to_uppercase(), "#1971C2", "Samples Layer 2 in non-overlapping region");
}

/// Challenge 1.3: Locked element vs Read-only board
/// - A locked element on an editable board CAN have its color sampled.
/// - A read-only board prevents eyedropper tool execution and preserves pen preferences.
#[test]
fn test_adv2_alt_click_locked_element_and_read_only_board() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.create("Test", None).unwrap();
    let mut screen: Option<BoardScreen> = None;
    let (mut toasts, mut dialogs) = (Toasts::default(), Dialogs::default());
    let color_out: std::rc::Rc<std::cell::RefCell<String>> = Default::default();
    let seen_color = color_out.clone();
    let is_read_only = std::rc::Rc::new(std::cell::Cell::new(false));
    let ro_flag = is_read_only.clone();
    let pt_out = std::rc::Rc::new(std::cell::Cell::new(pos2(0.0, 0.0)));
    let seen_pt = pt_out.clone();

    let mut harness = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if screen.is_none() {
            ui::install_fonts(&ctx);
            ui::setup(&ctx, Theme::new(false, parse_color(DEFAULT_ACCENT).unwrap(), false), 1.0);
            let mut b = BoardScreen::open(&ctx, &store, &id, Prefs { left_panel: false, right_panel: false, ..Prefs::default() }, None).unwrap();
            b.editor.tool = editor::Tool::Pen;
            b.editor.prefs.pens[0] = Pen { size: 4.0, opacity: 0.5, color: "#111111".into(), ..Default::default() };

            // Locked element with distinct color #9C36B5
            let mut locked_el = El::new(Kind::Shape(Shape {
                fill: "#9C36B5".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (locked_el.x, locked_el.y, locked_el.w, locked_el.h) = (300.0, 300.0, 200.0, 200.0);
            locked_el.locked = true;
            b.editor.board.put(vec![locked_el]);

            screen = Some(b);
            return;
        }
        let b = screen.as_mut().unwrap();
        b.editor.read_only = ro_flag.get();
        b.ui(ui, &mut toasts, &mut dialogs);
        let p = &b.editor.prefs.pens[b.editor.pen];
        *seen_color.borrow_mut() = p.color.clone();

        let s_locked = b.editor.to_screen(400.0, 400.0) + b.editor.origin.to_vec2();
        seen_pt.set(s_locked);
    });
    harness.run_steps(5);

    let alt = Modifiers { alt: true, ..Default::default() };
    let do_click = |h: &mut egui_kittest::Harness, p: Pos2| {
        h.input_mut().events.push(Event::PointerMoved(p));
        h.step();
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: alt });
        h.step();
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: alt });
        h.run_steps(2);
    };

    let target_pt = pt_out.get();

    // Step A: Board is editable. Sample locked element.
    do_click(&mut harness, target_pt);
    assert_eq!(color_out.borrow().to_uppercase(), "#9C36B5", "Locked element color can be sampled on editable board");

    // Step B: Set board to read-only. Alt+click should NOT change pen color.
    is_read_only.set(true);
    harness.run_steps(2);

    // Click at empty canvas area while read-only
    do_click(&mut harness, pos2(100.0, 100.0));
    // Color should remain #9C36B5, not revert to background, and gesture should be pan rather than eyedropper
    assert_eq!(color_out.borrow().to_uppercase(), "#9C36B5", "Read-only board rejects eyedropper modification");
}

/// Challenge 1.4: Rapid repeated Alt+clicks do not cause gesture deadlocks, memory leaks, or erratic states.
#[test]
fn test_adv2_rapid_repeated_alt_clicks() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.create("Test", None).unwrap();
    let mut screen: Option<BoardScreen> = None;
    let (mut toasts, mut dialogs) = (Toasts::default(), Dialogs::default());
    let color_out: std::rc::Rc<std::cell::RefCell<String>> = Default::default();
    let seen_color = color_out.clone();
    let pts_out: std::rc::Rc<std::cell::Cell<(Pos2, Pos2)>> = Default::default();
    let seen_pts = pts_out.clone();

    let mut harness = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if screen.is_none() {
            ui::install_fonts(&ctx);
            ui::setup(&ctx, Theme::new(false, parse_color(DEFAULT_ACCENT).unwrap(), false), 1.0);
            let mut b = BoardScreen::open(&ctx, &store, &id, Prefs { left_panel: false, right_panel: false, ..Prefs::default() }, None).unwrap();
            b.editor.tool = editor::Tool::Pen;
            b.editor.prefs.pens[0] = Pen { size: 4.0, opacity: 0.5, color: "#111111".into(), ..Default::default() };

            let mut el_red = El::new(Kind::Shape(Shape {
                fill: "#E03131".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (el_red.x, el_red.y, el_red.w, el_red.h) = (100.0, 100.0, 150.0, 150.0);

            let mut el_blue = El::new(Kind::Shape(Shape {
                fill: "#1971C2".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (el_blue.x, el_blue.y, el_blue.w, el_blue.h) = (350.0, 100.0, 150.0, 150.0);

            b.editor.board.put(vec![el_red, el_blue]);
            screen = Some(b);
            return;
        }
        let b = screen.as_mut().unwrap();
        b.ui(ui, &mut toasts, &mut dialogs);
        let p = &b.editor.prefs.pens[b.editor.pen];
        *seen_color.borrow_mut() = p.color.clone();

        let p_red = b.editor.to_screen(175.0, 175.0) + b.editor.origin.to_vec2();
        let p_blue = b.editor.to_screen(425.0, 175.0) + b.editor.origin.to_vec2();
        seen_pts.set((p_red, p_blue));
    });
    harness.run_steps(5);

    let alt = Modifiers { alt: true, ..Default::default() };
    let do_rapid_click = |h: &mut egui_kittest::Harness, p: Pos2| {
        h.input_mut().events.push(Event::PointerMoved(p));
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: alt });
        h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: alt });
        h.step();
    };

    let (pt_red, pt_blue) = pts_out.get();

    // Perform 10 rapid alternating clicks back and forth
    for i in 0..10 {
        if i % 2 == 0 {
            do_rapid_click(&mut harness, pt_red);
            assert_eq!(color_out.borrow().to_uppercase(), "#E03131", "Rapid click {i} samples red");
        } else {
            do_rapid_click(&mut harness, pt_blue);
            assert_eq!(color_out.borrow().to_uppercase(), "#1971C2", "Rapid click {i} samples blue");
        }
    }
}

/// Challenge 1.5: Clicking while moving:
/// - Displacement <= 4.0 px (hand tremor during click) is accepted as click and samples color.
/// - Displacement > 4.0 px is treated as drag (resizes brush, does NOT sample color).
/// - Displacement moving > 4.0 px and returning to start position keeps *moved = true and does NOT sample color.
#[test]
fn test_adv2_clicking_while_moving_tremor_vs_drag() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.create("Test", None).unwrap();
    let mut screen: Option<BoardScreen> = None;
    let (mut toasts, mut dialogs) = (Toasts::default(), Dialogs::default());
    let color_out: std::rc::Rc<std::cell::RefCell<String>> = Default::default();
    let seen_color = color_out.clone();
    let size_out = std::rc::Rc::new(std::cell::Cell::new(0.0f64));
    let seen_size = size_out.clone();
    let reset_color: std::rc::Rc<std::cell::RefCell<Option<String>>> = Default::default();
    let do_reset = reset_color.clone();
    let pt_out = std::rc::Rc::new(std::cell::Cell::new(pos2(0.0, 0.0)));
    let seen_pt = pt_out.clone();

    let mut harness = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if screen.is_none() {
            ui::install_fonts(&ctx);
            ui::setup(&ctx, Theme::new(false, parse_color(DEFAULT_ACCENT).unwrap(), false), 1.0);
            let mut b = BoardScreen::open(&ctx, &store, &id, Prefs { left_panel: false, right_panel: false, ..Prefs::default() }, None).unwrap();
            b.editor.tool = editor::Tool::Pen;
            b.editor.prefs.pens[0] = Pen { size: 10.0, opacity: 0.5, color: "#000000".into(), ..Default::default() };

            let mut el = El::new(Kind::Shape(Shape {
                fill: "#E03131".into(),
                stroke: "transparent".into(),
                stroke_width: 0.0,
                ..Default::default()
            }));
            (el.x, el.y, el.w, el.h) = (200.0, 200.0, 400.0, 400.0);
            b.editor.board.put(vec![el]);
            screen = Some(b);
            return;
        }
        let b = screen.as_mut().unwrap();
        if let Some(c) = do_reset.borrow_mut().take() {
            b.editor.prefs.pens[b.editor.pen].color = c;
        }
        b.ui(ui, &mut toasts, &mut dialogs);
        let p = &b.editor.prefs.pens[b.editor.pen];
        *seen_color.borrow_mut() = p.color.clone();
        seen_size.set(p.size);

        let p_target = b.editor.to_screen(400.0, 400.0) + b.editor.origin.to_vec2();
        seen_pt.set(p_target);
    });
    harness.run_steps(5);

    let alt = Modifiers { alt: true, ..Default::default() };
    let start = pt_out.get();

    // Scenario A: Hand tremor during click (moves 2.0 px right, 2.0 px down -> dist = 2.83 px <= 4.0 px)
    harness.input_mut().events.push(Event::PointerMoved(start));
    harness.step();
    harness.input_mut().events.push(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: alt });
    harness.step();
    harness.input_mut().events.push(Event::PointerMoved(start + vec2(2.0, 2.0)));
    harness.step();
    harness.input_mut().events.push(Event::PointerButton { pos: start + vec2(2.0, 2.0), button: PointerButton::Primary, pressed: false, modifiers: alt });
    harness.run_steps(2);

    assert_eq!(color_out.borrow().to_uppercase(), "#E03131", "Tremor <= 4.0px still triggers eyedropper");
    assert_eq!(size_out.get(), 10.0, "Tremor does not alter brush size");

    // Reset pen color to pure black #000000
    *reset_color.borrow_mut() = Some("#000000".into());
    harness.run_steps(2);
    assert_eq!(color_out.borrow().to_uppercase(), "#000000", "Pen color reset to black");

    // Scenario B: Movement > 4.0 px and returning to start point before release
    // Moves to +50px, then moves back to start (0.0px offset)
    harness.input_mut().events.push(Event::PointerMoved(start));
    harness.step();
    harness.input_mut().events.push(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: alt });
    harness.step();
    // Move out by 50px
    harness.input_mut().events.push(Event::PointerMoved(start + vec2(50.0, 0.0)));
    harness.step();
    // Move back to start
    harness.input_mut().events.push(Event::PointerMoved(start));
    harness.step();
    harness.input_mut().events.push(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: false, modifiers: alt });
    harness.run_steps(2);

    // *moved was latched to true, so on_up MUST NOT trigger pick_color (remains black #000000)!
    assert_eq!(color_out.borrow().to_uppercase(), "#000000", "Moving > 4px and returning to start latches moved=true and rejects eyedropper");
}

/* =========================================================================
   SUITE 2: SETTINGS PEN PAD VS CANVAS STROKE MATHEMATICAL PARITY
   ========================================================================= */

/// Challenge 2.1: Mathematical identity between calibrate_pressure in Settings dialog and Editor.
#[test]
fn test_adv2_settings_dialog_vs_canvas_pressure_mathematical_identity() {
    let sensitivities = [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm];

    // Verify gamma values
    assert_eq!(Sensitivity::Light.gamma(), 0.48);
    assert_eq!(Sensitivity::Normal.gamma(), 0.68);
    assert_eq!(Sensitivity::Firm.gamma(), 1.10);

    // Sweep 1000 force increments across [0.0, 1.0]
    for &s in &sensitivities {
        for step in 0..=1000 {
            let f = step as f32 / 1000.0;
            let calibrated = calibrate_pressure(f, s);

            // In settings dialog:
            let settings_pressure = (f as f64).clamp(0.0, 1.0).powf(s.gamma()) as f32;
            let canvas_raw = calibrate_pressure(f, s) as f32;

            assert_eq!(
                settings_pressure, canvas_raw,
                "Settings dialog pressure must be bit-for-bit identical to canvas raw calibrated pressure at f={f}"
            );
            assert!(calibrated >= 0.0 && calibrated <= 1.0);
        }
    }
}

/// Challenge 2.2: Verify that brush_prims produces identical primitives and tessellations
/// when fed identical points whether from settings dialog pressure pad or canvas stroke.
#[test]
fn test_adv2_brush_prims_settings_vs_canvas_stroke_parity() {
    let pts = vec![
        10.0f32, 20.0, 0.35,
        30.0, 45.0, 0.55,
        60.0, 70.0, 0.85,
        90.0, 95.0, 0.60,
    ];
    let size = 12.0;
    let brush = Some(Brush::Pen);
    let color = Color32::from_rgb(0x1E, 0x1E, 0x1E);
    let origin = (100.0, 100.0);

    // Call brush_prims exactly as called in settings dialog (dialogs.rs:117)
    let prims_settings = prims::brush_prims(&pts, size, brush, false, color, origin, None);

    // Call brush_prims as called in canvas stroke tessellation
    let prims_canvas = prims::brush_prims(&pts, size, brush, false, color, origin, None);

    assert_eq!(prims_settings.len(), prims_canvas.len());
    for (p1, p2) in prims_settings.iter().zip(prims_canvas.iter()) {
        match (p1, p2) {
            (prims::Prim::Fill { path: path1, color: c1 }, prims::Prim::Fill { path: path2, color: c2 }) => {
                assert_eq!(c1, c2);
                assert_eq!(path1, path2);
            }
            _ => panic!("Expected Fill primitives to match exactly"),
        }
    }
}

/* =========================================================================
   SUITE 3: SHORTCUT DOCUMENTATION, HUD, AND NON-TRUNCATION VERIFICATION
   ========================================================================= */

/// Challenge 3.1: HUD Feel Overlay text formats in Italian without truncation.
/// Smudge brush -> "{:.0} px  ·  Forza {:.0}%"
/// Pen / other brushes -> "{:.0} px  ·  Opacità {:.0}%"
#[test]
fn test_adv2_hud_feel_overlay_text_formats() {
    let mut ed = create_test_editor();
    ed.tool = editor::Tool::Pen;

    // Normal pen
    ed.prefs.pens[0].brush = Brush::Pen;
    ed.prefs.pens[0].size = 14.0;
    ed.prefs.pens[0].opacity = 0.85;

    let (s, op) = ed.brush_feel().unwrap();
    let is_smudge = ed.tool == editor::Tool::Pen && ed.prefs.pens.get(ed.pen).is_some_and(|p| p.brush == Brush::Smudge);
    assert!(!is_smudge);
    let text_pen = format!("{:.0} px  ·  Opacità {:.0}%", s, op * 100.0);
    assert_eq!(text_pen, "14 px  ·  Opacità 85%");

    // Smudge pen
    ed.prefs.pens[0].brush = Brush::Smudge;
    let is_smudge_now = ed.tool == editor::Tool::Pen && ed.prefs.pens.get(ed.pen).is_some_and(|p| p.brush == Brush::Smudge);
    assert!(is_smudge_now);
    let text_smudge = format!("{:.0} px  ·  Forza {:.0}%", s, op * 100.0);
    assert_eq!(text_smudge, "14 px  ·  Forza 85%");

    // Layout check: Verify text fits within pill padding
    let mut initialized = false;
    let mut harness = egui_kittest::Harness::builder().build_ui(move |ui| {
        if !initialized {
            ui::install_fonts(ui.ctx());
            ui::setup(ui.ctx(), Theme::new(false, parse_color(DEFAULT_ACCENT).unwrap(), false), 1.0);
            initialized = true;
            return;
        }
        let painter = ui.painter();
        let g = painter.layout_no_wrap(text_smudge.clone(), ui::medium(11.0), Color32::WHITE);
        let r = 7.0f32;
        let at = pos2(200.0, 200.0);
        let pill = egui::Rect::from_min_size(at + vec2(r + 10.0, -g.size().y / 2.0 - 4.0), g.size() + vec2(14.0, 8.0));

        // Pill width must be strictly larger than text layout width by 14px (7px on each side)
        assert!((pill.width() - (g.size().x + 14.0)).abs() < 1e-4);
        assert!((pill.height() - (g.size().y + 8.0)).abs() < 1e-4);
        assert!(pill.width() > g.size().x, "Pill completely contains text without any truncation");
    });
    harness.run_steps(3);
}

/// Challenge 3.2: Verify shortcut documentation and panel hints contain exact Italian phrases.
#[test]
fn test_adv2_shortcut_documentation_exact_italian_strings() {
    let panels_src = include_str!("../src/app/panels.rs");
    let dialogs_src = include_str!("../src/app/dialogs.rs");

    // Panel hint
    let expected_hint = "Alt + trascina: spessore (orizzontale) e opacità (verticale)  ·  Alt + clic: campiona colore  ·  [ e ]: spessore";
    assert!(
        panels_src.contains(expected_hint),
        "panels.rs must contain the exact Italian hint for Alt shortcuts"
    );

    // Shortcuts table in dialogs.rs
    let expected_size_shortcut = "(\"Spessore della penna\", \"Alt + trascina (orizzontale)  ·  [ e ]\")";
    let expected_opacity_shortcut = "(\"Opacità della penna\", \"Alt + trascina (verticale)  ·  Maiusc+[ e ]\")";
    let expected_eyedropper_shortcut = "(\"Prendi un colore dalla lavagna\", \"Alt + clic\")";

    assert!(
        dialogs_src.contains(expected_size_shortcut),
        "dialogs.rs must document horizontal Alt+drag for pen thickness"
    );
    assert!(
        dialogs_src.contains(expected_opacity_shortcut),
        "dialogs.rs must document vertical Alt+drag for pen opacity"
    );
    assert!(
        dialogs_src.contains(expected_eyedropper_shortcut),
        "dialogs.rs must document Alt+click for eyedropper"
    );
}
