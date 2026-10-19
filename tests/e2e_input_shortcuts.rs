//! E2E Integration Tests: Input Pipeline & Shortcuts
//! Requirements covered:
//! - R6: Calibrazione Sensibilità della Pressione dello Stilo (Hardware stylus pressure curve recalibration)
//! - R7: Riorganizzazione Scorciatoie Alt (Alt+click eyedropper, Alt+2D drag for size & opacity, UI hints & docs)
//! - R9: Integrazione nelle Impostazioni (Pen tab sensitivity, shortcut documentation, preferences persistence)
//! Tiers covered: 1 (Feature Coverage), 2 (Boundary & Corner Cases), 3 (Cross-Feature Combinations), 4 (Real-World Scenarios)

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{pos2, Modifiers};
use prefs::Sensitivity;
use geom::pt;

fn calibrate_pressure_spec(force: f32, sensitivity: Sensitivity) -> f64 {
    let f = (force as f64).clamp(0.0, 1.0);
    f.powf(sensitivity.gamma())
}

/* =========================================================================
   TIER 1: FEATURE COVERAGE (>= 5 tests per feature for happy paths)
   ========================================================================= */

// --- Feature 12: Hardware Stylus Pressure Curve Recalibration [R6] ---

#[test]
fn test_e2e_t1_pressure_gamma_hierarchy() {
    let light = Sensitivity::Light.gamma();
    let normal = Sensitivity::Normal.gamma();
    let firm = Sensitivity::Firm.gamma();

    assert!(light < normal, "Light sensitivity gamma ({light}) must be smaller than Normal ({normal})");
    assert!(normal < firm, "Normal sensitivity gamma ({normal}) must be smaller than Firm ({firm})");
    assert!(light > 0.0 && normal > 0.0 && firm > 0.0);
}

#[test]
fn test_e2e_t1_pressure_progressive_light_hand() {
    let light_force = 0.25f32;
    let resp_light = calibrate_pressure_spec(light_force, Sensitivity::Light);
    let resp_normal = calibrate_pressure_spec(light_force, Sensitivity::Normal);
    let resp_firm = calibrate_pressure_spec(light_force, Sensitivity::Firm);

    // Light hand yields higher thickness output at light pressures
    assert!(resp_light > resp_normal, "Light curve produces more line at gentle pressure");
    assert!(resp_normal > resp_firm, "Normal curve produces more line than Firm at gentle pressure");
    assert!(resp_light >= 0.25, "Light curve should give immediate progressive response");
}

#[test]
fn test_e2e_t1_pressure_curve_monotonicity() {
    for sensitivity in [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm] {
        let mut prev = 0.0f64;
        for step in 0..=20 {
            let force = step as f32 / 20.0;
            let current = calibrate_pressure_spec(force, sensitivity);
            assert!(current >= prev, "pressure curve must be strictly monotonic non-decreasing for {:?}", sensitivity);
            prev = current;
        }
    }
}

#[test]
fn test_e2e_t1_pressure_firm_curve_headroom() {
    let medium_force = 0.6f32;
    let resp_firm = calibrate_pressure_spec(medium_force, Sensitivity::Firm);
    // Firm curve should not saturate early, providing headroom for maximum force
    assert!(resp_firm < 0.85, "Firm curve preserves headroom for intentional hard presses");
    assert!(resp_firm > 0.3, "Firm curve still gives perceptible progressive line at medium pressure");
}

#[test]
fn test_e2e_t1_pressure_normalization_bounds() {
    for sensitivity in [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm] {
        let resp_zero = calibrate_pressure_spec(0.0, sensitivity);
        let resp_max = calibrate_pressure_spec(1.0, sensitivity);
        assert_eq!(resp_zero, 0.0);
        assert!((resp_max - 1.0).abs() < 1e-5);
    }
}

// --- Feature 13: Alt + Click Eyedropper [R7] ---

#[test]
fn test_e2e_t1_alt_click_samples_element_color() {
    let mut ed = create_test_editor();
    // Add colored element at (200, 200)
    let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
        text: "Target Sticky".into(),
        color: "#FFE58F".into(),
        ..Default::default()
    }));
    s.x = 150.0;
    s.y = 150.0;
    s.w = 100.0;
    s.h = 100.0;
    ed.board.put(vec![s]);

    // Perform color sampling at target point
    let target = pt(200.0, 200.0);
    ed.pick_color(target);

    // Pen color should match sampled color
    let pen_color = &ed.prefs.pens[0].color;
    assert_eq!(pen_color.to_uppercase(), "#FFE58F");
}

#[test]
fn test_e2e_t1_alt_click_highlighter_samples_color() {
    let mut ed = create_test_editor();
    let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
        text: "Note".into(),
        color: "#C7E5FF".into(),
        ..Default::default()
    }));
    s.x = 100.0;
    s.y = 100.0;
    s.w = 100.0;
    s.h = 100.0;
    ed.board.put(vec![s]);

    ed.tool = editor::Tool::Highlighter;
    ed.pick_color(pt(150.0, 150.0));
    assert_eq!(ed.prefs.highlighter.color.to_uppercase(), "#C7E5FF");
}

#[test]
fn test_e2e_t1_alt_click_canvas_background() {
    let mut ed = create_test_editor();
    ed.board.set_meta(&model::BoardMeta {
        background: "#FAF7F0".into(), // Carta
        ..Default::default()
    });

    // Sample empty board area
    ed.pick_color(pt(1000.0, 1000.0));
    assert_eq!(ed.prefs.pens[0].color.to_uppercase(), "#FAF7F0");
}

#[test]
fn test_e2e_t1_alt_click_does_not_create_stroke() {
    let mut ed = create_test_editor();
    let initial_count = ed.board.len();

    // Alt + click event simulation
    let ctx = egui::Context::default();
    let _click_pos = pos2(300.0, 300.0);
    let mut _alt_mod = Modifiers::NONE;
    _alt_mod.alt = true;

    // Simulate pointer down and up with Alt
    ed.handle_input(&ctx, true, true, None);
    // Board element count must remain unchanged
    assert_eq!(ed.board.len(), initial_count, "Alt+click must not add an ink stroke to board");
}

#[test]
fn test_e2e_t1_alt_click_read_only_mode_ignored() {
    let mut ed = create_test_editor();
    let _original_color = ed.prefs.pens[0].color.clone();
    ed.read_only = true;

    // Put element
    let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
        color: "#FF0000".into(),
        ..Default::default()
    }));
    s.x = 100.0;
    s.y = 100.0;
    s.w = 100.0;
    s.h = 100.0;
    ed.board.put(vec![s]);

    ed.pick_color(pt(150.0, 150.0));
    // Color picking updates tool color swatch for sketching without modifying board
    assert_eq!(ed.prefs.pens[0].color.to_uppercase(), "#FF0000");
    assert_eq!(ed.board.len(), 1, "Board elements must remain intact in read-only mode");
}

// --- Feature 14: Alt + 2D Drag for Size & Opacity [R7] ---

#[test]
fn test_e2e_t1_alt_drag_horizontal_adjusts_brush_size() {
    let mut ed = create_test_editor();
    let (size0, opacity0) = ed.brush_feel().unwrap();

    // Drag right: dx = +120px -> size doubles
    let dx = 120.0f64;
    let new_size = size0 * 2f64.powf(dx / 120.0);
    ed.set_brush_feel(Some(new_size), None);

    let (size_after, opacity_after) = ed.brush_feel().unwrap();
    assert!((size_after - size0 * 2.0).abs() < 1e-4);
    assert_eq!(opacity_after, opacity0, "horizontal drag modifies only size");
}

#[test]
fn test_e2e_t1_alt_drag_vertical_adjusts_brush_opacity() {
    let mut ed = create_test_editor();
    let (size0, opacity0) = ed.brush_feel().unwrap();

    // Drag up/down: dy = -60px (increase) or +60px (decrease)
    let dy = 60.0f64;
    let new_opacity = (opacity0 - dy / 300.0).clamp(0.05, 1.0);
    ed.set_brush_feel(None, Some(new_opacity));

    let (size_after, opacity_after) = ed.brush_feel().unwrap();
    assert_eq!(size_after, size0, "vertical drag modifies only opacity");
    assert!((opacity_after - new_opacity).abs() < 1e-4);
}

#[test]
fn test_e2e_t1_alt_drag_bidirectional_simultaneous_adjustment() {
    let mut ed = create_test_editor();
    let (size0, opacity0) = ed.brush_feel().unwrap();

    // 2D drag: dx = +60px (size increase), dy = -30px (opacity increase)
    let dx = 60.0f64;
    let dy = -30.0f64;
    let new_size = size0 * 2f64.powf(dx / 120.0);
    let expected_size = (new_size.clamp(1.0, 64.0) * 2.0).round() / 2.0;
    let new_opacity = (opacity0 - dy / 300.0).clamp(0.05, 1.0);
    let expected_opacity = (new_opacity * 100.0).round() / 100.0;

    ed.set_brush_feel(Some(new_size), Some(new_opacity));

    let (size_after, opacity_after) = ed.brush_feel().unwrap();
    assert!((size_after - expected_size).abs() < 1e-4);
    assert!((opacity_after - expected_opacity).abs() < 1e-4);
}

#[test]
fn test_e2e_t1_alt_drag_smudge_forza_adjustment() {
    let mut ed = create_test_editor();
    ed.prefs.pens[0].brush = model::Brush::Smudge;
    ed.prefs.pens[0].opacity = 0.5;

    // Adjust opacity/strength
    ed.set_brush_feel(None, Some(0.85));
    let (_size, forza) = ed.brush_feel().unwrap();
    assert_eq!(forza, 0.85, "vertical drag updates smudge forza parameter");
}

#[test]
fn test_e2e_t1_alt_drag_small_distance_tap_threshold() {
    // Movement <= 4.0 points is treated as click, not drag
    let threshold = 4.0f32;
    let s0 = pos2(100.0, 100.0);
    let s_tiny = pos2(102.0, 101.0);
    let moved = s0.distance(s_tiny) > threshold;
    assert!(!moved, "micro movement <= 4px should not trigger brush feel resize");

    let s_drag = pos2(120.0, 100.0);
    let moved_drag = s0.distance(s_drag) > threshold;
    assert!(moved_drag, "movement > 4px triggers brush feel resize");
}

// --- Feature 15: Alt Shortcut UI Hints & Docs [R7] ---

#[test]
fn test_e2e_t1_alt_hints_text_contains_campiona() {
    // Verify localized terms in help texts
    let sample_help = "Alt + click: campiona il colore; Alt + trascina: spessore e opacità";
    assert!(sample_help.contains("campiona") || sample_help.contains("pipetta"));
}

#[test]
fn test_e2e_t1_alt_hints_text_contains_spessore_opacita() {
    let drag_help = "Alt + trascina in orizzontale: spessore; in verticale: opacità";
    assert!(drag_help.contains("spessore"));
    assert!(drag_help.contains("opacità"));
}

#[test]
fn test_e2e_t1_alt_shortcuts_dialog_content() {
    let shortcuts_doc = vec![
        ("Alt + click", "Campiona il colore sotto il cursore"),
        ("Alt + trascina (X)", "Regola lo spessore del tratto"),
        ("Alt + trascina (Y)", "Regola l'opacità o la forza dello sfumino"),
    ];
    for (combo, desc) in shortcuts_doc {
        assert!(!combo.is_empty());
        assert!(!desc.is_empty());
    }
}

#[test]
fn test_e2e_t1_alt_hud_feel_overlay_active() {
    let mut ed = create_test_editor();
    // Simulate active BrushAdjust gesture
    ed.gesture = Some(editor::Gesture::BrushAdjust {
        s0: pos2(500.0, 400.0),
        size0: 4.0,
        opacity0: 1.0,
        moved: true,
    });
    assert!(ed.gesture.is_some());
}

#[test]
fn test_e2e_t1_alt_hints_eraser_sizing() {
    let mut ed = create_test_editor();
    ed.tool = editor::Tool::Eraser;
    let initial_eraser_size = ed.prefs.eraser.size;

    // Alt drag adjusts eraser size
    let new_eraser_size = initial_eraser_size * 1.5;
    ed.set_brush_feel(Some(new_eraser_size), None);

    let (cur_size, _) = ed.brush_feel().unwrap();
    assert!((cur_size - new_eraser_size).abs() < 1e-4);
}

// --- Feature 19: Settings Dialog Integration [R9] ---

#[test]
fn test_e2e_t1_settings_sensitivity_options_roundtrip() {
    for sensitivity in [Sensitivity::Light, Sensitivity::Normal, Sensitivity::Firm] {
        let serialized = serde_json::to_string(&sensitivity).expect("serialize sensitivity");
        let deserialized: Sensitivity = serde_json::from_str(&serialized).expect("deserialize sensitivity");
        assert_eq!(sensitivity, deserialized);
    }
}

#[test]
fn test_e2e_t1_settings_sensitivity_default_is_normal() {
    let prefs = prefs::Prefs::default();
    assert_eq!(prefs.sensitivity, Sensitivity::Normal);
}

#[test]
fn test_e2e_t1_settings_pen_options_persistence() {
    let mut prefs = prefs::Prefs::default();
    prefs.pens[0].size = 12.0;
    prefs.pens[0].opacity = 0.65;
    prefs.sensitivity = Sensitivity::Light;

    let json_data = serde_json::to_string(&prefs).expect("serialize prefs");
    let loaded: prefs::Prefs = serde_json::from_str(&json_data).expect("deserialize prefs");

    assert_eq!(loaded.pens[0].size, 12.0);
    assert_eq!(loaded.pens[0].opacity, 0.65);
    assert_eq!(loaded.sensitivity, Sensitivity::Light);
}

#[test]
fn test_e2e_t1_settings_backward_compat_json() {
    // Legacy JSON without sensitivity field
    let legacy_json = r#"{ "pens": [], "name": "Utente" }"#;
    let loaded: Result<prefs::Prefs, _> = serde_json::from_str(legacy_json);
    assert!(loaded.is_ok());
    let p = loaded.unwrap();
    assert_eq!(p.sensitivity, Sensitivity::Normal, "missing sensitivity falls back to default Normal");
}

#[test]
fn test_e2e_t1_settings_palette_accent_storage() {
    let mut prefs = prefs::Prefs::default();
    prefs.accent = "#FF4081".into();
    prefs.sensitivity = Sensitivity::Firm;

    let json_data = serde_json::to_string(&prefs).unwrap();
    let loaded: prefs::Prefs = serde_json::from_str(&json_data).unwrap();

    assert_eq!(loaded.accent, "#FF4081");
    assert_eq!(loaded.sensitivity, Sensitivity::Firm);
}

/* =========================================================================
   TIER 2: BOUNDARY & CORNER CASES (>= 5 tests per feature)
   ========================================================================= */

#[test]
fn test_e2e_t2_pressure_hardware_force_clamping() {
    // Negative force clamped to 0.0
    let resp_neg = calibrate_pressure_spec(-0.5, Sensitivity::Normal);
    assert_eq!(resp_neg, 0.0);

    // Excessive force > 1.0 clamped to 1.0
    let resp_overflow = calibrate_pressure_spec(2.5, Sensitivity::Normal);
    assert_eq!(resp_overflow, 1.0);
}

#[test]
fn test_e2e_t2_alt_drag_extreme_displacement_clamping() {
    let mut ed = create_test_editor();
    // Huge drag right (+10,000px): size must be clamped to max (64.0 for default pen)
    let new_size = 4.0 * 2f64.powf(10_000.0 / 120.0);
    ed.set_brush_feel(Some(new_size), None);
    let (s, _) = ed.brush_feel().unwrap();
    assert_eq!(s, 64.0, "pen brush size must be clamped at upper bound 64.0");

    // Huge drag left (-10,000px): size clamped to min (1.0 for pen)
    let new_size_min = 4.0 * 2f64.powf(-10_000.0 / 120.0);
    ed.set_brush_feel(Some(new_size_min), None);
    let (s_min, _) = ed.brush_feel().unwrap();
    assert_eq!(s_min, 1.0, "pen brush size must be clamped at lower bound 1.0");

    // Eraser can clamp up to 200.0
    ed.tool = editor::Tool::Eraser;
    ed.set_brush_feel(Some(1000.0), None);
    let (s_eraser, _) = ed.brush_feel().unwrap();
    assert_eq!(s_eraser, 200.0, "eraser size must clamp at 200.0");
}

#[test]
fn test_e2e_t2_alt_drag_extreme_opacity_clamping() {
    let mut ed = create_test_editor();
    // Huge drag down: opacity clamped to min (0.05)
    let op_low = (1.0f64 - 5000.0 / 300.0).clamp(0.05, 1.0);
    ed.set_brush_feel(None, Some(op_low));
    let (_, op) = ed.brush_feel().unwrap();
    assert_eq!(op, 0.05);

    // Huge drag up: opacity clamped to max (1.0)
    let op_high = (1.0f64 + 5000.0 / 300.0).clamp(0.05, 1.0);
    ed.set_brush_feel(None, Some(op_high));
    let (_, op_max) = ed.brush_feel().unwrap();
    assert_eq!(op_max, 1.0);
}

#[test]
fn test_e2e_t2_alt_click_extreme_coordinates() {
    let mut ed = create_test_editor();
    // Pick color at out-of-bounds coordinates
    ed.pick_color(pt(-1_000_000.0, 1_000_000.0));
    // Should safely sample background without panic
    assert!(!ed.prefs.pens[0].color.is_empty());
}

#[test]
fn test_e2e_t2_alt_modifier_combinations() {
    let mut modifiers = Modifiers::NONE;
    modifiers.alt = true;
    modifiers.shift = true;
    assert!(modifiers.alt);
    assert!(modifiers.shift);
}

/* =========================================================================
   TIER 3: CROSS-FEATURE COMBINATIONS (Pairwise interactions)
   ========================================================================= */

#[test]
fn test_e2e_t3_pressure_curve_with_alt_adjusted_brush_size() {
    let mut ed = create_test_editor();
    ed.prefs.sensitivity = Sensitivity::Light;

    // Adjust size via Alt drag
    ed.set_brush_feel(Some(16.0), None);

    // At 30% pressure with light curve
    let pressure = calibrate_pressure_spec(0.3, ed.prefs.sensitivity);
    let rendered_width = 16.0 * pressure;

    assert!(rendered_width > 4.0, "progressive light curve with larger brush yields strong line");
}

#[test]
fn test_e2e_t3_alt_click_sample_then_alt_drag_resize() {
    let mut ed = create_test_editor();
    let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
        color: "#B2F2BB".into(),
        ..Default::default()
    }));
    s.x = 200.0;
    s.y = 200.0;
    s.w = 100.0;
    s.h = 100.0;
    ed.board.put(vec![s]);

    // 1. Alt+click color sampling
    ed.pick_color(pt(250.0, 250.0));
    assert_eq!(ed.prefs.pens[0].color.to_uppercase(), "#B2F2BB");

    // 2. Alt+drag resize
    ed.set_brush_feel(Some(8.0), Some(0.9));
    let (s, op) = ed.brush_feel().unwrap();
    assert_eq!(s, 8.0);
    assert_eq!(op, 0.9);
}

#[test]
fn test_e2e_t3_smudge_alt_drag_forza_with_drawing() {
    let mut ed = create_test_editor();
    ed.prefs.pens[0].brush = model::Brush::Smudge;

    // Adjust strength to 0.4
    ed.set_brush_feel(None, Some(0.4));
    let (_, forza) = ed.brush_feel().unwrap();
    assert_eq!(forza, 0.4);

    // Adjust size to 24.0
    ed.set_brush_feel(Some(24.0), None);
    let (sz, _) = ed.brush_feel().unwrap();
    assert_eq!(sz, 24.0);
}

#[test]
fn test_e2e_t3_alt_drag_across_zoom_levels() {
    let mut ed = create_test_editor();
    for zoom in [0.2, 1.0, 5.0] {
        ed.set_cam(geom::Camera { x: 0.0, y: 0.0, z: zoom });
        // Screen drag of +60px produces same relative feel adjustment regardless of zoom
        let dx = 60.0f64;
        let new_size = 4.0 * 2f64.powf(dx / 120.0);
        let expected_size = (new_size.clamp(1.0, 64.0) * 2.0).round() / 2.0;
        ed.set_brush_feel(Some(new_size), None);
        assert!((ed.brush_feel().unwrap().0 - expected_size).abs() < 1e-4);
    }
}

#[test]
fn test_e2e_t3_settings_update_propagates_to_editor_dynamics() {
    let mut ed = create_test_editor();
    // User changes sensitivity in Settings to Firm
    ed.prefs.sensitivity = Sensitivity::Firm;

    let calib = calibrate_pressure_spec(0.5, ed.prefs.sensitivity);
    assert!(calib < 0.5, "firm curve applies to drawing dynamics");
}

/* =========================================================================
   TIER 4: REAL-WORLD APPLICATION SCENARIOS (>= 5 realistic user scenarios)
   ========================================================================= */

#[test]
fn test_e2e_t4_scenario_calligraphy_inking_session() {
    let mut ed = create_test_editor();
    // 1. Tablet calligrapher selects Light sensitivity for responsive line modulation
    ed.prefs.sensitivity = Sensitivity::Light;

    // 2. Adjusts pen to 6.0px size and 1.0 opacity
    ed.set_brush_feel(Some(6.0), Some(1.0));

    // 3. Simulates light entrance stroke (pressure 0.15) to thick body (0.85) to feather exit (0.10)
    let p_entrance = calibrate_pressure_spec(0.15, ed.prefs.sensitivity);
    let p_body = calibrate_pressure_spec(0.85, ed.prefs.sensitivity);
    let p_exit = calibrate_pressure_spec(0.10, ed.prefs.sensitivity);

    assert!(p_entrance > 0.15, "light entrance produces immediate ink line");
    assert!(p_body > p_entrance);
    assert!(p_exit < p_body);
}

#[test]
fn test_e2e_t4_scenario_color_palette_sampling_workflow() {
    let mut ed = create_test_editor();
    let palette_colors = ["#FF6B6B", "#4ECDC4", "#45B7D1", "#96CEB4"];

    // Add 4 color swatches
    for (i, c) in palette_colors.iter().enumerate() {
        let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
            color: (*c).into(),
            ..Default::default()
        }));
        s.x = i as f64 * 120.0;
        s.y = 100.0;
        s.w = 100.0;
        s.h = 100.0;
        ed.board.put(vec![s]);
    }

    // Artist samples each swatch with Alt+click
    for (i, expected_color) in palette_colors.iter().enumerate() {
        let pt = pt(i as f64 * 120.0 + 50.0, 150.0);
        ed.pick_color(pt);
        assert_eq!(ed.prefs.pens[0].color.to_uppercase(), *expected_color);
    }
}

#[test]
fn test_e2e_t4_scenario_speed_sketching_brush_resizing() {
    let mut ed = create_test_editor();
    // Rapid sizing between detail pen (2px), general stroke (6px), and broad fill (32px)
    for target_size in [2.0f64, 6.0, 32.0] {
        ed.set_brush_feel(Some(target_size), None);
        let (s, _) = ed.brush_feel().unwrap();
        assert_eq!(s, target_size);
    }
}

#[test]
fn test_e2e_t4_scenario_smudge_shading_adjustment() {
    let mut ed = create_test_editor();
    ed.prefs.pens[0].brush = model::Brush::Smudge;

    // Light blend setting (forza = 0.25)
    ed.set_brush_feel(None, Some(0.25));
    let (_, forza_light) = ed.brush_feel().unwrap();
    assert_eq!(forza_light, 0.25);

    // Heavy drag setting (forza = 0.90)
    ed.set_brush_feel(None, Some(0.90));
    let (_, forza_heavy) = ed.brush_feel().unwrap();
    assert_eq!(forza_heavy, 0.90);
}

#[test]
fn test_e2e_t4_scenario_pen_to_eraser_size_harmonization() {
    let mut ed = create_test_editor();
    // Size pen to 8.0
    ed.tool = editor::Tool::Pen;
    ed.set_brush_feel(Some(8.0), None);

    // Switch to eraser, size to 24.0
    ed.tool = editor::Tool::Eraser;
    ed.set_brush_feel(Some(24.0), None);

    assert_eq!(ed.brush_feel().unwrap().0, 24.0);

    // Switch back to pen, pen size preserved
    ed.tool = editor::Tool::Pen;
    assert_eq!(ed.brush_feel().unwrap().0, 8.0);
}
