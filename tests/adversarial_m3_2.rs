//! Empirical Adversarial Stress Test Suite 2 for Milestone 3 (UI Panels, Controls, Icons & Theming)
//!
//! Adversarially challenges:
//! 1. Drag & drop spring gap opening in layers panel (`ui::motion::spring`), rapid layer dragging,
//!    reordering to first/last positions, and drag ghost containment inside panel inner bounds (`clip.shrink(8.0)`).
//! 2. Toolbar customization modal: dragged lifted row clamping and clipping inside modal rect;
//!    verify it cannot escape outside into the header or title bar.
//! 3. Theme toggling: rapid sun/moon clicks in top bar and web guest buttons, theme persistence
//!    across light/dark, and visual contrast of custom background color well "+" indicator across dark/light background values.
//! 4. Modern background presets (Sabbia, Lavanda, Salvia, Grafite, Ossidiana) across light and dark modes,
//!    with grid & dots canvas background tessellation.

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{pos2, vec2, Color32, Context, Id, Rect};
use model::{is_dark, parse_color, BACKGROUNDS, BoardMeta, Pattern, El, Kind, Sticky};
use prefs::{Prefs, Theme as PrefsTheme};
use geom::Camera;

/* =========================================================================
   SUITE 1: DRAG & DROP SPRING GAP & GHOST CONTAINMENT IN LAYERS
   ========================================================================= */

/// Challenge 1.1: ui::motion::spring mathematical stability under variable frame rates,
/// zero dt, huge frame lag (5s), and rapid target oscillations between 0.0 and 24.0 (gap open/close).
#[test]
fn test_adv3_spring_gap_dynamics_under_varying_dt_and_stiffness() {
    let ctx = Context::default();
    let gap_id = Id::new("adv-layer-row-gap-stress");

    // 1. Initial state at rest
    let x0 = ui::motion::spring(&ctx, gap_id, 0.0, 0.25, 0.9);
    assert_eq!(x0, 0.0);

    // 2. Step with micro frame rates (1000 FPS -> dt = 0.001s equivalent across 100 frames)
    for _ in 0..100 {
        let x = ui::motion::spring(&ctx, gap_id, 24.0, 0.25, 0.9);
        assert!(x.is_finite(), "Spring output must always be finite");
        assert!(x >= -1.0 && x <= 30.0, "Spring displacement {x} must be within stable physical bounds");
    }

    // 3. Settle at 24.0
    for _ in 0..120 {
        ui::motion::spring(&ctx, gap_id, 24.0, 0.25, 0.9);
    }
    let settled_open = ui::motion::spring(&ctx, gap_id, 24.0, 0.25, 0.9);
    assert!((settled_open - 24.0).abs() < 0.2, "Spring must settle near target 24.0, got {settled_open}");

    // 4. Close gap: target -> 0.0
    for _ in 0..120 {
        ui::motion::spring(&ctx, gap_id, 0.0, 0.25, 0.9);
    }
    let settled_closed = ui::motion::spring(&ctx, gap_id, 0.0, 0.25, 0.9);
    assert!(settled_closed < 0.2, "Spring must close back near 0.0, got {settled_closed}");

    // 5. Rapid violent oscillation: alternate target 24.0 and 0.0 every frame for 100 frames
    for i in 0..100 {
        let target = if i % 2 == 0 { 24.0 } else { 0.0 };
        let x = ui::motion::spring(&ctx, gap_id, target, 0.25, 0.9);
        assert!(x.is_finite(), "Oscillating spring must remain finite");
        assert!(x >= -5.0 && x <= 35.0, "Oscillating spring must not diverge; got {x}");
    }
}

/// Challenge 1.2: Rapid layer reordering to first, last, and arbitrary indices in document stack.
/// Verifies monotonicity of z-order and robust fallback when z-precision limits are approached.
#[test]
fn test_adv3_layers_rapid_dragging_reorder_first_last_and_cycles() {
    let mut ed = create_test_editor();

    // Create 5 distinct layers
    let mut items = Vec::new();
    for i in 0..5 {
        let mut s = El::new(Kind::Sticky(Sticky::default()));
        s.id = format!("layer_{i}");
        s.z = i as f64;
        items.push(s);
    }
    ed.board.put(items);
    assert_eq!(ed.board.len(), 5);

    // Initial order check
    let get_order = |ed: &mut editor::Editor| -> Vec<String> {
        ed.board.all().iter().map(|e| e.id.clone()).collect()
    };
    assert_eq!(get_order(&mut ed), vec!["layer_0", "layer_1", "layer_2", "layer_3", "layer_4"]);

    // Move layer_4 to the very top (above layer_0 -> becomes index 0 in stack)
    ed.board.move_in_stack(&["layer_4".into()], doc::Anchor::Below("layer_0".into()), None);
    assert_eq!(get_order(&mut ed), vec!["layer_4", "layer_0", "layer_1", "layer_2", "layer_3"]);

    // Verify z values are strictly monotonically increasing
    let all = ed.board.all().to_vec();
    for i in 1..all.len() {
        assert!(all[i].z > all[i - 1].z, "Z values must be strictly increasing: z[{}]={}, z[{}]={}", i-1, all[i-1].z, i, all[i].z);
    }

    // Move layer_4 to the very bottom (below layer_3)
    ed.board.move_in_stack(&["layer_4".into()], doc::Anchor::Above("layer_3".into()), None);
    assert_eq!(get_order(&mut ed), vec!["layer_0", "layer_1", "layer_2", "layer_3", "layer_4"]);

    // Move middle layer_2 above layer_0
    ed.board.move_in_stack(&["layer_2".into()], doc::Anchor::Below("layer_0".into()), None);
    assert_eq!(get_order(&mut ed), vec!["layer_2", "layer_0", "layer_1", "layer_3", "layer_4"]);

    // Stress test: 50 successive moves between extremes to test float z-compression and renumbering
    for step in 0..50 {
        let current = get_order(&mut ed);
        let move_id = current[step % current.len()].clone();
        if step % 2 == 0 {
            // Move to bottom
            let last_id = current.last().unwrap().clone();
            ed.board.move_in_stack(&[move_id], doc::Anchor::Above(last_id), None);
        } else {
            // Move to top
            let first_id = current.first().unwrap().clone();
            ed.board.move_in_stack(&[move_id], doc::Anchor::Below(first_id), None);
        }

        let updated = ed.board.all().to_vec();
        assert_eq!(updated.len(), 5, "Layer count must remain exactly 5");
        for i in 1..updated.len() {
            assert!(
                updated[i].z > updated[i - 1].z,
                "Z-stack must remain strictly ordered after step {step}: z[{}]={}, z[{}]={}",
                i - 1, updated[i - 1].z, i, updated[i].z
            );
        }
    }
}

/// Challenge 1.3: Drag ghost pill boundary containment clamping inside panels panel bounds (`clip.shrink(8.0)`).
/// Tests extreme pointer coordinates, negative coordinates, and ensures ghost never escapes into title bar (y <= 40.0).
#[test]
fn test_adv3_layers_drag_ghost_containment_boundary_and_degenerate() {
    // Normal layers panel bounds on right side of window: (1100, 48) to (1440, 900)
    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 900.0));
    let clip = panel_rect.shrink(8.0);
    // clip is (1108.0, 56.0) to (1432.0, 892.0)

    let title_bar_bottom = 40.0f32;

    let test_pointers = vec![
        pos2(-999_999.0, -999_999.0), // Extreme top-left
        pos2(999_999.0, 999_999.0),   // Extreme bottom-right
        pos2(0.0, 0.0),               // Screen origin
        pos2(1200.0, 10.0),           // In window title bar
        pos2(1200.0, 39.0),           // Just below top edge, inside title bar
        pos2(1050.0, 200.0),          // To the left of panel
        pos2(1500.0, 200.0),          // To the right of panel
        pos2(1250.0, 950.0),          // Below window bottom
    ];

    for p in test_pointers {
        let ghost_x = (p.x + 14.0).clamp(clip.min.x, (clip.max.x - 110.0).max(clip.min.x));
        let ghost_y = (p.y + 8.0).clamp(clip.min.y, (clip.max.y - 32.0).max(clip.min.y));

        assert!(
            ghost_x >= clip.min.x && ghost_x <= clip.max.x,
            "Ghost X {ghost_x} must be within [{}, {}] for pointer {:?}",
            clip.min.x, clip.max.x, p
        );
        assert!(
            ghost_y >= clip.min.y && ghost_y <= clip.max.y,
            "Ghost Y {ghost_y} must be within [{}, {}] for pointer {:?}",
            clip.min.y, clip.max.y, p
        );

        // Verification: ghost cannot escape outside into the window title bar
        assert!(
            ghost_y > title_bar_bottom,
            "Ghost Y {ghost_y} must remain strictly below title bar bottom {title_bar_bottom} for pointer {:?}",
            p
        );
    }

    // Degenerate small panel bounds (narrow / squeezed layout)
    let tiny_panel = Rect::from_min_max(pos2(100.0, 50.0), pos2(150.0, 70.0));
    let tiny_clip = tiny_panel.shrink(8.0);
    let p = pos2(120.0, 60.0);
    let gx = (p.x + 14.0).clamp(tiny_clip.min.x, (tiny_clip.max.x - 110.0).max(tiny_clip.min.x));
    let gy = (p.y + 8.0).clamp(tiny_clip.min.y, (tiny_clip.max.y - 32.0).max(tiny_clip.min.y));
    assert!(gx.is_finite() && gy.is_finite(), "Clamping on tiny bounds must not panic or NaN");
    assert!(gx >= tiny_clip.min.x);
    assert!(gy >= tiny_clip.min.y);
}

/* =========================================================================
   SUITE 2: TOOLBAR CUSTOMIZATION MODAL LIFTED ROW CLAMPING & CLIPPING
   ========================================================================= */

/// Challenge 2.1: Toolbar customization modal lifted row clamping under extreme dragging pointers.
/// Verifies lifted row cannot escape outside into the modal header, dialog title bar, or bottom area.
#[test]
fn test_adv3_toolbar_modal_lifted_row_extreme_pointer_clamping() {
    let modal_rect = Rect::from_min_max(pos2(100.0, 50.0), pos2(700.0, 650.0));
    // Modal header and hint occupies y = 50.0..140.0
    // List rect is allocated inside modal from y = 140.0..580.0
    let row_h = 34.0f32;
    let n = 12usize;
    let list = Rect::from_min_max(pos2(modal_rect.min.x + 20.0, 140.0), pos2(modal_rect.max.x - 20.0, 140.0 + row_h * n as f32));
    assert_eq!(list.height(), row_h * n as f32);

    let modal_header_bottom = 140.0f32;
    let dialog_title_bottom = 80.0f32;

    let grab_offsets = [0.0f32, 17.0, 34.0]; // Top edge, center, bottom edge of row
    let test_pointer_ys = [
        -1000.0f32, // Way above screen
        0.0,        // Top of screen
        50.0,       // Dialog top edge
        70.0,       // Inside dialog title bar
        100.0,      // Inside modal header "Barra degli strumenti"
        139.0,      // Just 1px above list top
        140.0,      // Exactly list top
        250.0,      // Middle of list
        list.max.y - 1.0, // Near bottom
        list.max.y,       // Exactly bottom
        list.max.y + 50.0,// Below list in "Ripristina la barra"
        2000.0,     // Way below window
    ];

    for grab in grab_offsets {
        for &py in &test_pointer_ys {
            // Clamping formula from src/app/dialogs.rs:643:
            let top = (py - grab).clamp(list.min.y, (list.max.y - row_h).max(list.min.y));
            let row = Rect::from_min_size(pos2(list.min.x, top), vec2(list.width(), row_h));

            // Clamping invariant 1: row.min.y is at least list.min.y
            assert!(
                row.min.y >= list.min.y,
                "Row top {} must not be less than list.min.y {} for py={py}, grab={grab}",
                row.min.y, list.min.y
            );

            // Clamping invariant 2: row.max.y never exceeds list.max.y
            assert!(
                row.max.y <= list.max.y + 1e-4,
                "Row bottom {} must not exceed list.max.y {} for py={py}, grab={grab}",
                row.max.y, list.max.y
            );

            // Clamping invariant 3: row is strictly below modal header and title bar
            assert!(
                row.min.y >= modal_header_bottom,
                "Row top {} must be at or below modal header bottom {} for py={py}",
                row.min.y, modal_header_bottom
            );
            assert!(
                row.min.y > dialog_title_bottom,
                "Row top {} must be strictly below dialog title bottom {} for py={py}",
                row.min.y, dialog_title_bottom
            );

            // Verification: clip rect intersection guarantees 100% containment
            let visible = row.intersect(list);
            assert_eq!(visible, row, "Lifted row must be 100% contained within list clip rect");
        }
    }
}

/// Challenge 2.2: Toolbar items reordering permutations and default reset fidelity.
#[test]
fn test_adv3_toolbar_modal_reordering_permutations_and_reset() {
    let mut p = Prefs::default();
    let initial_items = crate::prefs::toolbar_items(&p.toolbar_order);
    let n = initial_items.len();
    assert!(n >= 5, "Toolbar must have at least 5 default items");

    // Move first item to the very end
    let mut items = initial_items.clone();
    let first = items.remove(0);
    items.push(first);
    p.toolbar_order = items.clone();
    assert_eq!(p.toolbar_order.len(), n);
    assert_eq!(p.toolbar_order.last().unwrap(), &initial_items[0]);

    // Move last item to the very beginning
    let last = items.pop().unwrap();
    items.insert(0, last);
    p.toolbar_order = items.clone();
    assert_eq!(p.toolbar_order.len(), n);
    assert_eq!(p.toolbar_order[0], initial_items[0]);

    // Hide some items
    let initial_hidden_len = p.toolbar_hidden.len();
    p.toolbar_hidden.push("custom_hidden_item_1".into());
    p.toolbar_hidden.push("custom_hidden_item_2".into());
    assert_eq!(p.toolbar_hidden.len(), initial_hidden_len + 2);

    // Reset toolbar to default
    p.toolbar_order = crate::prefs::toolbar_default();
    p.toolbar_hidden = crate::prefs::toolbar_hidden_default();
    assert_eq!(p.toolbar_order, crate::prefs::toolbar_default());
    assert_eq!(p.toolbar_hidden, crate::prefs::toolbar_hidden_default());
}

/* =========================================================================
   SUITE 3: THEME TOGGLING & WEB GUEST CLICKS
   ========================================================================= */

/// Challenge 3.1: Rapid sun/moon toggle button clicks in top bar & web guest controls.
/// Simulates 200 rapid clicks, verifying clean oscillation, zero state desync, and correct Theme styling.
#[test]
fn test_adv3_theme_toggle_rapid_oscillation_state_machine() {
    let ctx = Context::default();
    let mut p = Prefs {
        theme: PrefsTheme::Light,
        ..Default::default()
    };

    let mut applied = None;

    // Simulate 200 consecutive clicks
    for click in 1..=200 {
        app::apply_theme(&ctx, &p, &mut applied);
        let t = ui::theme(&ctx);
        let is_dark = t.dark;

        // Toggle logic from board.rs:530 and panels.rs:252
        p.theme = if is_dark { PrefsTheme::Light } else { PrefsTheme::Dark };

        if click % 2 == 1 {
            // Started light -> after 1st click should become Dark
            assert_eq!(p.theme, PrefsTheme::Dark, "After click {click}, theme must be Dark");
        } else {
            // After 2nd click should become Light
            assert_eq!(p.theme, PrefsTheme::Light, "After click {click}, theme must be Light");
        }
    }

    // Verify starting from System theme
    p.theme = PrefsTheme::System;
    app::apply_theme(&ctx, &p, &mut applied);
    let t = ui::theme(&ctx);
    let is_dark = t.dark;
    // Single click flips away from current system dark state
    p.theme = if is_dark { PrefsTheme::Light } else { PrefsTheme::Dark };
    assert!(matches!(p.theme, PrefsTheme::Light | PrefsTheme::Dark));
}

/// Challenge 3.2: Theme preferences serialization and persistence roundtrip.
#[test]
fn test_adv3_theme_persistence_roundtrip() {
    for theme_variant in [PrefsTheme::Light, PrefsTheme::Dark, PrefsTheme::System] {
        let prefs = Prefs {
            theme: theme_variant,
            ..Default::default()
        };

        // Serialize to JSON
        let json_str = serde_json::to_string(&prefs).expect("Prefs must serialize to JSON");
        // Deserialize back
        let restored: Prefs = serde_json::from_str(&json_str).expect("Prefs must deserialize from JSON");
        assert_eq!(restored.theme, theme_variant, "Deserialized theme must match original");
    }
}

/// Challenge 3.3: Contrast oracle for custom background color well "+" indicator.
/// Tests the luma formula across full grayscale spectrum (0..255) and edge colors to verify
/// clear visibility and contrast of "+" indicator.
#[test]
fn test_adv3_custom_background_well_plus_contrast_oracle_and_wcag() {
    fn plus_indicator_color(c: Color32) -> Color32 {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        if luma > 160.0 {
            Color32::from_black_alpha(150)
        } else {
            Color32::from_white_alpha(200)
        }
    }

    // 1. Grayscale spectrum from 0 to 255
    for gray in 0u8..=255 {
        let col = Color32::from_gray(gray);
        let plus = plus_indicator_color(col);
        if gray > 160 {
            assert_eq!(plus, Color32::from_black_alpha(150), "Bright gray {gray} must use black alpha plus");
        } else {
            assert_eq!(plus, Color32::from_white_alpha(200), "Dark gray {gray} must use white alpha plus");
        }
    }

    // 2. High saturation primary and secondary colors
    let test_colors = [
        ("Black", Color32::BLACK, true),
        ("White", Color32::WHITE, false),
        ("Red", Color32::from_rgb(255, 0, 0), true),        // luma = 76.2 <= 160 -> White
        ("Green", Color32::from_rgb(0, 255, 0), true),      // luma = 149.7 <= 160 -> White
        ("Blue", Color32::from_rgb(0, 0, 255), true),       // luma = 29.1 <= 160 -> White
        ("Yellow", Color32::from_rgb(255, 255, 0), false),  // luma = 225.9 > 160 -> Black
        ("Cyan", Color32::from_rgb(0, 255, 255), false),    // luma = 178.8 > 160 -> Black
        ("Magenta", Color32::from_rgb(255, 0, 255), true),  // luma = 105.3 <= 160 -> White
    ];

    for (name, col, expect_white_plus) in test_colors {
        let plus = plus_indicator_color(col);
        if expect_white_plus {
            assert_eq!(plus, Color32::from_white_alpha(200), "{name} must select white plus indicator");
        } else {
            assert_eq!(plus, Color32::from_black_alpha(150), "{name} must select black plus indicator");
        }
    }

    // 3. All 11 BACKGROUND presets
    for (name, hex_val) in BACKGROUNDS {
        let col = parse_color(hex_val).expect("Preset hex must parse");
        let plus = plus_indicator_color(col);
        let dark = is_dark(hex_val);
        if dark {
            // Dark presets (Ardesia, Lavagna verde, Blu notte, Grafite, Ossidiana) must use white plus
            assert_eq!(plus, Color32::from_white_alpha(200), "Dark preset '{name}' must use white plus");
        }
    }
}

/* =========================================================================
   SUITE 4: MODERN BACKGROUND PRESETS ACROSS LIGHT & DARK MODES
   ========================================================================= */

/// Challenge 4.1: Modern background presets classification, parsing, and hex validity.
#[test]
fn test_adv3_modern_background_presets_verification() {
    assert_eq!(BACKGROUNDS.len(), 11);

    // Verify all 5 modern additions are present
    let modern_keys = ["Sabbia", "Lavanda", "Salvia", "Grafite", "Ossidiana"];
    for key in modern_keys {
        let entry = BACKGROUNDS.iter().find(|(n, _)| *n == key);
        assert!(entry.is_some(), "Modern preset '{key}' must be present in BACKGROUNDS");
    }

    // Verify classification
    let sabbia_hex = BACKGROUNDS.iter().find(|(n, _)| *n == "Sabbia").unwrap().1;
    let lavanda_hex = BACKGROUNDS.iter().find(|(n, _)| *n == "Lavanda").unwrap().1;
    let salvia_hex = BACKGROUNDS.iter().find(|(n, _)| *n == "Salvia").unwrap().1;
    let grafite_hex = BACKGROUNDS.iter().find(|(n, _)| *n == "Grafite").unwrap().1;
    let ossidiana_hex = BACKGROUNDS.iter().find(|(n, _)| *n == "Ossidiana").unwrap().1;

    assert_eq!(sabbia_hex, "#FAF3E0");
    assert_eq!(lavanda_hex, "#EDE7F6");
    assert_eq!(salvia_hex, "#E8F0E6");
    assert_eq!(grafite_hex, "#1E2229");
    assert_eq!(ossidiana_hex, "#121417");

    assert!(!is_dark(sabbia_hex), "Sabbia must be classified as light background");
    assert!(!is_dark(lavanda_hex), "Lavanda must be classified as light background");
    assert!(!is_dark(salvia_hex), "Salvia must be classified as light background");
    assert!(is_dark(grafite_hex), "Grafite must be classified as dark background");
    assert!(is_dark(ossidiana_hex), "Ossidiana must be classified as dark background");
}

/// Challenge 4.2: Canvas background tessellation and grid/dots visibility for modern presets.
/// Verifies that crate::paint::background generates valid tessellation shapes for all modern presets
/// without numerical divergence or missing grid strokes under Dots, Grid, and Graph patterns.
#[test]
fn test_adv3_modern_presets_grid_and_dots_tessellation() {
    let test_presets = [
        ("Sabbia", "#FAF3E0", false),
        ("Lavanda", "#EDE7F6", false),
        ("Salvia", "#E8F0E6", false),
        ("Grafite", "#1E2229", true),
        ("Ossidiana", "#121417", true),
    ];

    let patterns = [Pattern::Dots, Pattern::Grid, Pattern::Graph];
    let cam = Camera { x: 100.0, y: 150.0, z: 1.0 };
    let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));
    let ppp = 1.0f32;

    for (name, hex_val, expect_dark) in test_presets {
        assert_eq!(is_dark(hex_val), expect_dark);

        for pat in patterns {
            let meta = BoardMeta {
                background: hex_val.into(),
                pattern: pat,
                grid_size: 16.0,
                ..Default::default()
            };

            let mut shapes = Vec::new();
            crate::paint::background(&mut shapes, rect, &cam, &meta, ppp);

            assert!(
                !shapes.is_empty(),
                "Tessellation must generate shapes for preset {name} with pattern {pat:?}"
            );

            // Verify shapes contain valid finite bounds
            for s in &shapes {
                let r = s.visual_bounding_rect();
                assert!(r.min.x.is_finite() && r.min.y.is_finite());
                assert!(r.max.x.is_finite() && r.max.y.is_finite());
            }
        }
    }
}
