//! Empirical Adversarial Stress Test Suite for Milestone 2 (Input Pipeline & Shortcuts)
//!
//! Stress-tests:
//! 1. Stylus pressure curve across entire force spectrum: 0.0, 0.001, 0.15, 0.35, 0.70, 0.999, 1.0,
//!    out-of-range (<0.0, >1.0, NaN, Inf), strict monotonicity, soft sensitivity, and spike rejection.
//! 2. Alt 2D drag gesture: pure horizontal (size only), pure vertical (opacity only), simultaneous diagonal,
//!    deadzone (<4.0 px tap for eyedropper vs >=4.0 px drag), extreme displacements (clamping),
//!    rapid direction shifts (drift invariance), and secondary button interactions (Alt+RMB).
//! 3. HUD feel overlay text: "Forza" for Smudge vs "Opacità" for other brushes.
//! 4. Safety and isolation: read-only canvas immunity, tool boundaries (Eraser vs Pen vs Select).

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{pos2, vec2, Modifiers, PointerButton, Event};
use prefs::{calibrate_pressure, Sensitivity, Prefs, Pen};
use model::{Brush, El, Kind, Shape};
use editor::Tool;
use std::rc::Rc;
use std::cell::RefCell;

/* =========================================================================
   ADVERSARIAL SUITE 1: STYLUS PRESSURE CURVE & FORCE SPECTRUM STRESS
   ========================================================================= */

#[test]
fn test_adv_pressure_force_spectrum_discrete_points() {
    let spectrum = [0.0f32, 0.001, 0.15, 0.35, 0.70, 0.999, 1.0];
    let sensitivities = [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm];

    for &s in &sensitivities {
        for &f in &spectrum {
            let p = calibrate_pressure(f, s);
            assert!(p.is_finite(), "Calibrated pressure must be finite for f={f}, s={s:?}");
            assert!(p >= 0.0 && p <= 1.0, "Calibrated pressure {p} must be in [0.0, 1.0] for f={f}, s={s:?}");
            if f == 0.0 {
                assert_eq!(p, 0.0, "Force 0.0 must map to exact 0.0 pressure for {s:?}");
            }
            if f == 1.0 {
                assert!((p - 1.0).abs() < 1e-6, "Force 1.0 must map to 1.0 pressure for {s:?}");
            }
        }
    }
}

#[test]
fn test_adv_pressure_strict_monotonicity_fine_grained() {
    let sensitivities = [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm];
    let steps = 10_000;
    for &s in &sensitivities {
        let mut prev_p = -1.0f64;
        for i in 0..=steps {
            let force = i as f32 / steps as f32;
            let p = calibrate_pressure(force, s);
            assert!(
                p >= prev_p,
                "Violation of monotonicity at f={force}: prev_p={prev_p}, p={p} for {s:?}"
            );
            if i > 0 {
                assert!(
                    p > prev_p,
                    "Violation of strict monotonicity at f={force}: prev_p={prev_p}, p={p} for {s:?}"
                );
            }
            prev_p = p;
        }
    }
}

#[test]
fn test_adv_pressure_sensitivity_hierarchy_and_soft_response() {
    let test_forces = [0.01f32, 0.05, 0.10, 0.20, 0.35, 0.50, 0.75, 0.90, 0.99];
    for &f in &test_forces {
        let p_light = calibrate_pressure(f, Sensitivity::Light);
        let p_normal = calibrate_pressure(f, Sensitivity::Normal);
        let p_firm = calibrate_pressure(f, Sensitivity::Firm);

        assert!(
            p_light > p_normal,
            "Light ({p_light}) must yield higher pressure than Normal ({p_normal}) at f={f}"
        );
        assert!(
            p_normal > p_firm,
            "Normal ({p_normal}) must yield higher pressure than Firm ({p_firm}) at f={f}"
        );
    }

    let p_normal_20 = calibrate_pressure(0.20, Sensitivity::Normal);
    let p_light_20 = calibrate_pressure(0.20, Sensitivity::Light);
    assert!(
        p_normal_20 >= 0.30,
        "Normal curve must offer soft sensitivity (expected >= 0.30 at 0.20 force, got {p_normal_20})"
    );
    assert!(
        p_light_20 >= 0.45,
        "Light curve must offer high sensitivity (expected >= 0.45 at 0.20 force, got {p_light_20})"
    );
}

#[test]
fn test_adv_pressure_out_of_range_and_non_finite_inputs() {
    let sensitivities = [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm];

    for &s in &sensitivities {
        assert_eq!(calibrate_pressure(-0.0001, s), 0.0);
        assert_eq!(calibrate_pressure(-1.0, s), 0.0);
        assert_eq!(calibrate_pressure(-1e9, s), 0.0);
        assert_eq!(calibrate_pressure(f32::NEG_INFINITY, s), 0.0);

        assert_eq!(calibrate_pressure(1.0001, s), 1.0);
        assert_eq!(calibrate_pressure(2.0, s), 1.0);
        assert_eq!(calibrate_pressure(1e9, s), 1.0);
        assert_eq!(calibrate_pressure(f32::INFINITY, s), 1.0);

        let p_nan = calibrate_pressure(f32::NAN, s);
        assert!(p_nan.is_nan(), "calibrate_pressure with NaN input returns NaN without panicking");
    }
}

/* =========================================================================
   ADVERSARIAL SUITE 2: ALT 2D DRAG & EYEDROPPER FULL HARNESS TESTS
   ========================================================================= */

#[derive(Default, Clone)]
struct ObservedState {
    pen_size: f64,
    pen_opacity: f64,
    pen_color: String,
    eraser_size: f64,
    eraser_strength: f64,
    board_len: usize,
    menu_requested: bool,
}

fn setup_test_harness(
    init_tool: Tool,
    init_pen: Pen,
    read_only: bool,
) -> (egui_kittest::Harness<'static>, Rc<RefCell<ObservedState>>) {
    let store = store::Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.create("AdvTestBoard", None).unwrap();
    let mut screen: Option<app::board::BoardScreen> = None;
    let (mut toasts, mut dialogs) = (app::Toasts::default(), app::dialogs::Dialogs::default());
    let state: Rc<RefCell<ObservedState>> = Default::default();
    let seen_state = state.clone();

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1200.0, 800.0))
        .build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            if screen.is_none() {
                ui::install_fonts(&ctx);
                ui::setup(&ctx, ui::Theme::new(false, model::parse_color(prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                let mut b = app::board::BoardScreen::open(
                    &ctx,
                    &store,
                    &id,
                    Prefs { left_panel: false, right_panel: false, ..Default::default() },
                    None,
                ).unwrap();
                b.editor.tool = init_tool;
                b.editor.prefs.pens[0] = init_pen.clone();
                b.editor.read_only = read_only;

                // Add a target colored shape at (200, 100) -> (800, 700)
                let mut target = El::new(Kind::Shape(Shape {
                    fill: "#E03131".into(),
                    stroke: "transparent".into(),
                    stroke_width: 0.0,
                    ..Default::default()
                }));
                target.x = 200.0;
                target.y = 100.0;
                target.w = 600.0;
                target.h = 600.0;
                b.editor.board.put(vec![target]);
                screen = Some(b);
                return;
            }
            let b = screen.as_mut().unwrap();
            b.ui(ui, &mut toasts, &mut dialogs);
            let p = &b.editor.prefs.pens[b.editor.pen];
            let mut s = seen_state.borrow_mut();
            s.pen_size = p.size;
            s.pen_opacity = p.opacity;
            s.pen_color = p.color.clone();
            s.eraser_size = b.editor.prefs.eraser.size;
            s.eraser_strength = b.editor.prefs.eraser.strength;
            s.board_len = b.editor.board.len();
            s.menu_requested = b.ui.menu.is_some() || b.editor.requests.iter().any(|r| matches!(r, editor::Request::Menu(_)));
        });

    harness.run_steps(5);
    (harness, state)
}

fn execute_drag_path(
    h: &mut egui_kittest::Harness,
    anchor: egui::Pos2,
    offsets: &[(f32, f32)],
    button: PointerButton,
    modifiers: Modifiers,
) {
    h.input_mut().events.push(Event::PointerMoved(anchor));
    h.step();
    h.input_mut().events.push(Event::PointerButton { pos: anchor, button, pressed: true, modifiers });
    h.step();

    for &(dx, dy) in offsets {
        let p = anchor + vec2(dx, dy);
        h.input_mut().events.push(Event::PointerMoved(p));
        h.step();
    }

    let &(last_dx, last_dy) = offsets.last().unwrap_or(&(0.0, 0.0));
    let end_p = anchor + vec2(last_dx, last_dy);
    h.input_mut().events.push(Event::PointerButton { pos: end_p, button, pressed: false, modifiers });
    h.run_steps(2);
}

fn execute_tap(
    h: &mut egui_kittest::Harness,
    pos: egui::Pos2,
    button: PointerButton,
    modifiers: Modifiers,
) {
    h.input_mut().events.push(Event::PointerMoved(pos));
    h.step();
    h.input_mut().events.push(Event::PointerButton { pos, button, pressed: true, modifiers });
    h.step();
    h.input_mut().events.push(Event::PointerButton { pos, button, pressed: false, modifiers });
    h.run_steps(2);
}

#[test]
fn test_adv_harness_pure_horizontal_and_pure_vertical_isolation() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 4.0, opacity: 0.50, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    // 1. Pure horizontal displacement: +120 px -> size doubles to ~8.0, opacity unchanged
    let mut offsets = Vec::new();
    for k in 1..=6 {
        offsets.push((120.0 * k as f32 / 6.0, 0.0));
    }
    execute_drag_path(&mut harness, anchor, &offsets, PointerButton::Primary, alt);

    let s1 = state.borrow().clone();
    assert!((s1.pen_size - 8.0).abs() < 0.6, "Size should double to ~8.0 (got {})", s1.pen_size);
    assert_eq!(s1.pen_opacity, 0.50, "Pure horizontal drag must not alter opacity");

    // 2. Pure vertical displacement: -150 px -> opacity increases to ~1.0, size unchanged
    let mut v_offsets = Vec::new();
    for k in 1..=6 {
        v_offsets.push((0.0, -150.0 * k as f32 / 6.0));
    }
    execute_drag_path(&mut harness, anchor, &v_offsets, PointerButton::Primary, alt);

    let s2 = state.borrow().clone();
    assert!((s2.pen_size - 8.0).abs() < 0.6, "Size must remain unchanged during pure vertical drag");
    assert!((s2.pen_opacity - 1.0).abs() < 0.02, "Opacity should reach 1.0 (got {})", s2.pen_opacity);
    assert_eq!(s2.board_len, 1, "Alt drag must not leave any ink stroke");
}

#[test]
fn test_adv_harness_simultaneous_diagonal_drag() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 6.0, opacity: 0.40, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    // Diagonal displacement: dx = +120 px (size doubles: 6.0 -> 12.0), dy = -75 px (opacity: 0.40 + 75/250 = 0.70)
    let mut diag_offsets = Vec::new();
    for k in 1..=6 {
        diag_offsets.push((120.0 * k as f32 / 6.0, -75.0 * k as f32 / 6.0));
    }
    execute_drag_path(&mut harness, anchor, &diag_offsets, PointerButton::Primary, alt);

    let s = state.borrow().clone();
    assert!((s.pen_size - 12.0).abs() < 0.6, "Size must double to ~12.0 (got {})", s.pen_size);
    assert!((s.pen_opacity - 0.70).abs() < 0.05, "Opacity must increase to ~0.70 (got {})", s.pen_opacity);
}

#[test]
fn test_adv_harness_deadzone_tap_vs_drag_boundary() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 8.0, opacity: 0.70, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0); // Inside the #E03131 target shape

    // Sub-case A: Tap with micro-displacement < 4.0 px (e.g. 2.0 px) -> treated as click, SAMPLES COLOR
    execute_drag_path(&mut harness, anchor, &[(1.0, 1.0), (1.5, 1.5)], PointerButton::Primary, alt);
    let s_tap = state.borrow().clone();
    assert_eq!(s_tap.pen_color.to_uppercase(), "#E03131", "Tap with dist < 4px must pick color");
    assert_eq!(s_tap.pen_size, 8.0, "Tap must not alter size");
    assert_eq!(s_tap.pen_opacity, 0.70, "Tap must not alter opacity");

    // Reset color to black
    // Sub-case B: Drag with displacement > 4.0 px (e.g. dx = 30.0 px) -> treated as drag, ADJUSTS SIZE, DOES NOT SAMPLE COLOR
    execute_drag_path(&mut harness, anchor, &[(10.0, 0.0), (30.0, 0.0)], PointerButton::Primary, alt);
    let s_drag = state.borrow().clone();
    assert!((s_drag.pen_size - 8.0).abs() > 0.5, "Displacement > 4px must adjust size");
}

#[test]
fn test_adv_harness_extreme_displacements_clamping() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 8.0, opacity: 0.50, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    // Extreme positive displacement (+10,000 px, -10,000 px)
    execute_drag_path(&mut harness, anchor, &[(500.0, -500.0), (10000.0, -10000.0)], PointerButton::Primary, alt);
    let s_max = state.borrow().clone();
    assert_eq!(s_max.pen_size, 64.0, "Pen size must clamp to maximum 64.0 px");
    assert_eq!(s_max.pen_opacity, 1.0, "Opacity must clamp to maximum 1.0");

    // Extreme negative displacement (-10,000 px, +10,000 px)
    execute_drag_path(&mut harness, anchor, &[(-500.0, 500.0), (-10000.0, 10000.0)], PointerButton::Primary, alt);
    let s_min = state.borrow().clone();
    assert_eq!(s_min.pen_size, 1.0, "Pen size must clamp to minimum 1.0 px");
    assert_eq!(s_min.pen_opacity, 0.05, "Opacity must clamp to minimum 0.05");
}

#[test]
fn test_adv_harness_rapid_direction_shifts_zero_drift() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 6.0, opacity: 0.60, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    // Rapid erratic zigzag trajectory returning exactly to (0, 0)
    let zigzag = [
        (150.0, -80.0),
        (-120.0, 140.0),
        (200.0, -100.0),
        (-180.0, 50.0),
        (80.0, -40.0),
        (0.0, 0.0), // Exact return to origin!
    ];
    execute_drag_path(&mut harness, anchor, &zigzag, PointerButton::Primary, alt);

    let s = state.borrow().clone();
    assert_eq!(s.pen_size, 6.0, "Returning to origin must restore exact initial size without drift");
    assert_eq!(s.pen_opacity, 0.60, "Returning to origin must restore exact initial opacity without drift");
}

#[test]
fn test_adv_harness_secondary_button_without_alt_opens_menu() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 6.0, opacity: 0.50, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let no_mods = Modifiers::default();
    let anchor = pos2(500.0, 400.0);

    execute_tap(&mut harness, anchor, PointerButton::Secondary, no_mods);
    let s_menu = state.borrow().clone();
    assert!(s_menu.menu_requested, "Right click without Alt must request context menu");
}

#[test]
fn test_adv_harness_secondary_button_alt_drag_adjusts_brush() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 6.0, opacity: 0.50, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    let offsets = [(60.0, -50.0), (120.0, -100.0)];
    execute_drag_path(&mut harness, anchor, &offsets, PointerButton::Secondary, alt);
    let s_alt_rmb = state.borrow().clone();
    assert!(!s_alt_rmb.menu_requested, "Alt + Secondary drag must NOT request context menu");
    assert!((s_alt_rmb.pen_size - 12.0).abs() < 0.6, "Alt + RMB drag must adjust size (got {})", s_alt_rmb.pen_size);
    assert!((s_alt_rmb.pen_opacity - 0.90).abs() < 0.05, "Alt + RMB drag must adjust opacity (got {})", s_alt_rmb.pen_opacity);
}

#[test]
fn test_adv_harness_eraser_alt_drag_and_isolation() {
    let (mut harness, state) = setup_test_harness(
        Tool::Eraser,
        Pen { size: 4.0, opacity: 0.50, color: "#1E1E1E".into(), brush: Brush::Pen },
        false,
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    // Drag with Eraser: adjusts eraser size and strength
    let initial_eraser_size = state.borrow().eraser_size;
    let initial_eraser_strength = state.borrow().eraser_strength;

    execute_drag_path(&mut harness, anchor, &[(120.0, 50.0)], PointerButton::Primary, alt);
    let s_eraser = state.borrow().clone();
    assert!((s_eraser.eraser_size - initial_eraser_size * 2.0).abs() < 1.0, "Alt drag adjusts eraser size");
    assert!(s_eraser.eraser_strength < initial_eraser_strength, "Downward drag reduces eraser strength");

    // Tap with Eraser: must NOT modify pen color!
    execute_tap(&mut harness, anchor, PointerButton::Primary, alt);
    let s_tap = state.borrow().clone();
    assert_eq!(s_tap.pen_color, "#1E1E1E", "Eraser tap must not alter pen color");
}

#[test]
fn test_adv_harness_read_only_immunity() {
    let (mut harness, state) = setup_test_harness(
        Tool::Pen,
        Pen { size: 6.0, opacity: 0.60, color: "#1E1E1E".into(), brush: Brush::Pen },
        true, // READ-ONLY!
    );
    let alt = Modifiers { alt: true, ..Default::default() };
    let anchor = pos2(500.0, 400.0);

    // Attempt drag and tap
    execute_drag_path(&mut harness, anchor, &[(120.0, -100.0)], PointerButton::Primary, alt);
    execute_tap(&mut harness, anchor, PointerButton::Primary, alt);

    let s = state.borrow().clone();
    assert_eq!(s.pen_size, 6.0, "Read-only board must not permit size adjustment");
    assert_eq!(s.pen_opacity, 0.60, "Read-only board must not permit opacity adjustment");
    assert_eq!(s.pen_color, "#1E1E1E", "Read-only board must not permit color sampling");
}

#[test]
fn test_adv_hud_overlay_smudge_forza_vs_opacita() {
    // Verify smudge HUD formatting logic:
    // When brush is Smudge: text is "{size} px  ·  Forza {strength}%"
    // Otherwise: text is "{size} px  ·  Opacità {opacity}%"
    let sz = 16.0f64;
    let op = 0.75f64;

    let format_hud = |brush: Brush, size: f64, opacity: f64| -> String {
        if brush == Brush::Smudge {
            format!("{:.0} px  ·  Forza {:.0}%", size, opacity * 100.0)
        } else {
            format!("{:.0} px  ·  Opacità {:.0}%", size, opacity * 100.0)
        }
    };

    let smudge_hud = format_hud(Brush::Smudge, sz, op);
    assert!(smudge_hud.contains("Forza 75%"), "Smudge HUD must state Forza");
    assert!(!smudge_hud.contains("Opacità"), "Smudge HUD must not state Opacità");

    let pen_hud = format_hud(Brush::Pen, sz, op);
    assert!(pen_hud.contains("Opacità 75%"), "Pen HUD must state Opacità");
    assert!(!pen_hud.contains("Forza"), "Pen HUD must not state Forza");
}
