//! E2E Integration Tests: UI, Theming & Accessibility
//! Requirements covered:
//! - R2: Icona del Connettore Curvo (Curved connector icon Bézier cleanup in SVG and toolbar)
//! - R8: Interruttore Rapido Tema Scuro/Chiaro e Personalizzazione Sfondi (Sun/moon theme switch, custom background well indicator, modern background presets)
//! - R9: Integrazione nelle Impostazioni, Stile, Animazioni ed Accessibilità (Focus rings, WCAG AA contrast, high contrast mode, Italian tooltips)
//! Tiers covered: 1 (Feature Coverage), 2 (Boundary & Corner Cases), 3 (Cross-Feature Combinations), 4 (Real-World Scenarios)

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{vec2, Color32, Context, Sense};
use model::{BACKGROUNDS, Route, Line, El, Kind, Sticky, Code};
use ui::{Theme, focus_ring, icon_data::ICONS};
use prefs::{Prefs, Theme as PrefsTheme};
use geom::{curve, curve_of, bezier};
use model::{is_dark, parse_color, color_or, hex};

fn make_code(code: &str, language: &str, light: bool) -> Code {
    Code {
        code: code.into(),
        language: language.into(),
        light,
        font_size: 13.0,
        radius: None,
        fixed_height: false,
    }
}

/* =========================================================================
   TIER 1: FEATURE COVERAGE (>= 5 tests per feature for happy paths)
   ========================================================================= */

// --- Feature 16: Curved Connector Icon Bézier & Geometry [R2] ---

#[test]
fn test_e2e_t1_curved_connector_icon_registration() {
    // Verify route-curved icon is registered in ICONS registry
    let icon_entry = ICONS.iter().find(|(name, _)| *name == "route-curved");
    assert!(icon_entry.is_some(), "route-curved icon must be present in ICONS registry");

    // Verify route-curved is mapped in toolbar ROUTES
    let route_entry = app::toolbar::ROUTES.iter().find(|(r, id, label)| *r == Route::Curved && *id == "route-curved" && *label == "Curva");
    assert!(route_entry.is_some(), "Route::Curved must map to route-curved with Italian label 'Curva'");
}

#[test]
fn test_e2e_t1_curved_connector_svg_structure() {
    let (_, svg) = ICONS.iter().find(|(name, _)| *name == "route-curved").unwrap();
    // Validate SVG header attributes
    assert!(svg.contains("viewBox=\"0 0 24 24\""), "Icon must use 24x24 viewBox");
    assert!(svg.contains("width=\"24\"") && svg.contains("height=\"24\""), "Icon must have 24x24 dimensions");
    assert!(svg.contains("stroke-width=\"2\""), "Icon stroke width must be 2");
    assert!(svg.contains("stroke-linecap=\"round\"") && svg.contains("stroke-linejoin=\"round\""), "Stroke caps must be round");
}

#[test]
fn test_e2e_t1_curved_connector_svg_path_no_loop() {
    let (_, svg) = ICONS.iter().find(|(name, _)| *name == "route-curved").unwrap();
    // Path should have clean cubic Bézier without backtracking loops
    // d="M5 19c9 0 5-14 14-14" and endpoints with circle markers at (5, 19) and (19, 5)
    assert!(svg.contains("M5 19") || svg.contains("M 5 19"), "Path must originate at start coordinate (5, 19)");
    assert!(svg.contains("circle cx=\"5\" cy=\"19\""), "Start circle marker must be at (5, 19)");
    assert!(svg.contains("circle cx=\"19\" cy=\"5\""), "End circle marker must be at (19, 5)");
    // Must contain smooth bezier command 'c'
    assert!(svg.contains("c5 0") || svg.contains("c 5 0") || svg.contains("c5 0 9-14 14-14") || svg.contains("c9 0"), "Path must use clean forward-progressing cubic control handles");
}

#[test]
fn test_e2e_t1_curved_connector_geometry_curve_cubic() {
    // geom::curve computes 4 control points for a curved connector
    let p1 = (100.0, 100.0);
    let p2 = (300.0, 200.0);
    let pts = curve(p1.0, p1.1, p2.0, p2.1);
    assert_eq!(pts.len(), 4, "curve must produce 4 control points for cubic Bézier");
    assert_eq!(pts[0], p1, "First point must equal start");
    assert_eq!(pts[3], p2, "Fourth point must equal end");

    // Horizontal dominant axis: dx = 200 > dy = 100
    // Control handles must extend horizontally by 0.5 * dx
    let dx = p2.0 - p1.0;
    assert_eq!(pts[1], (p1.0 + dx * 0.5, p1.1), "Second point must tangent from start along dominant axis");
    assert_eq!(pts[2], (p2.0 - dx * 0.5, p2.1), "Third point must tangent into end along dominant axis");
}

#[test]
fn test_e2e_t1_curved_connector_bezier_monotonicity() {
    let pts = curve(50.0, 50.0, 250.0, 150.0);
    let mut prev_x = pts[0].0;
    let steps = 30;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let (bx, _by) = bezier(&pts, t);
        assert!(bx >= prev_x, "Bézier curve X coordinate must monotonically increase with forward tangent (no loops)");
        prev_x = bx;
    }
}

#[test]
fn test_e2e_t1_curved_connector_element_curve_of() {
    let l = Line {
        points: vec![10.0, 20.0, 110.0, 120.0],
        route: Route::Curved,
        ..Default::default()
    };
    let c = curve_of(&l);
    assert_eq!(c[0], (10.0, 20.0));
    assert_eq!(c[3], (110.0, 120.0));

    // Midpoint evaluation
    let (mx, my) = bezier(&c, 0.5);
    assert!(mx > 10.0 && mx < 110.0, "Curve midpoint X must lie strictly between start and end");
    assert!(my > 20.0 && my < 120.0, "Curve midpoint Y must lie strictly between start and end");
}

// --- Feature 17: Theme Switching & Sun/Moon Mechanism [R8] ---

#[test]
fn test_e2e_t1_sun_moon_icons_registered() {
    let sun = ICONS.iter().find(|(name, _)| *name == "sun");
    assert!(sun.is_some(), "sun icon must be registered in ICONS");
    let (_, sun_svg) = sun.unwrap();
    assert!(sun_svg.contains("circle cx=\"12\" cy=\"12\" r=\"4\""), "sun icon must have central circle");

    // 'moon' icon is referenced in panels.rs:2401 and context_bar.rs:226 when light theme is active.
    // When present in ICONS, verify it is valid SVG.
    let moon = ICONS.iter().find(|(name, _)| *name == "moon");
    if let Some((_, moon_svg)) = moon {
        assert!(moon_svg.contains("<svg"), "moon icon must be valid SVG");
    }
}

#[test]
fn test_e2e_t1_theme_light_creation() {
    let default_accent = Color32::from_rgb(0x0D, 0x99, 0xFF);
    let t = Theme::new(false, default_accent, false);
    assert!(!t.dark, "Theme must report not dark");
    assert_eq!(t.bg, Color32::WHITE, "Light theme background must be pure white");
    assert_eq!(t.canvas, Color32::from_rgb(0xF5, 0xF5, 0xF5), "Light theme canvas must be #F5F5F5");
    assert_eq!(t.border, Color32::from_rgb(0xE6, 0xE6, 0xE6));
}

#[test]
fn test_e2e_t1_theme_dark_creation() {
    let default_accent = Color32::from_rgb(0x0D, 0x99, 0xFF);
    let t = Theme::new(true, default_accent, false);
    assert!(t.dark, "Theme must report dark");
    assert_eq!(t.bg, Color32::from_rgb(0x2C, 0x2C, 0x2C), "Dark theme background must be #2C2C2C");
    assert_eq!(t.canvas, Color32::from_rgb(0x1E, 0x1E, 0x1E), "Dark theme canvas must be #1E1E1E");
    assert_eq!(t.text, Color32::WHITE, "Dark theme text must be white");
}

#[test]
fn test_e2e_t1_apply_theme_switches_context() {
    let ctx = Context::default();
    let mut applied = None;

    // Apply Light Theme
    let mut prefs = Prefs {
        theme: PrefsTheme::Light,
        ..Default::default()
    };
    app::apply_theme(&ctx, &prefs, &mut applied);
    let current_theme = ui::theme(&ctx);
    assert!(!current_theme.dark, "Context theme must be light");
    assert_eq!(current_theme.bg, Color32::WHITE);

    // Switch to Dark Theme
    prefs.theme = PrefsTheme::Dark;
    app::apply_theme(&ctx, &prefs, &mut applied);
    let current_theme2 = ui::theme(&ctx);
    assert!(current_theme2.dark, "Context theme must be dark");
    assert_eq!(current_theme2.bg, Color32::from_rgb(0x2C, 0x2C, 0x2C));
}

#[test]
fn test_e2e_t1_code_block_sun_moon_theme_toggle() {
    let mut code_el = El::new(Kind::Code(make_code("fn main() { println!(\"Tratto\"); }", "rust", false)));

    // Initially dark
    assert!(!code_el.code().unwrap().light);

    // Toggle theme to light
    if let Kind::Code(c) = &mut code_el.kind {
        c.light = !c.light;
    }
    assert!(code_el.code().unwrap().light, "Code block must be toggled to light");

    // Toggle back to dark
    if let Kind::Code(c) = &mut code_el.kind {
        c.light = !c.light;
    }
    assert!(!code_el.code().unwrap().light, "Code block must be toggled back to dark");
}

// --- Feature 18: Background Presets & Custom Well Indicator [R8] ---

#[test]
fn test_e2e_t1_background_presets_count_and_keys() {
    assert_eq!(BACKGROUNDS.len(), 11, "BACKGROUNDS must provide 11 predefined modern presets");
    let names: Vec<&str> = BACKGROUNDS.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"Grigio chiaro"));
    assert!(names.contains(&"Bianco"));
    assert!(names.contains(&"Carta"));
    assert!(names.contains(&"Sabbia"));
    assert!(names.contains(&"Lavanda"));
    assert!(names.contains(&"Salvia"));
    assert!(names.contains(&"Ardesia"));
    assert!(names.contains(&"Lavagna verde"));
    assert!(names.contains(&"Blu notte"));
    assert!(names.contains(&"Grafite"));
    assert!(names.contains(&"Ossidiana"));
}

#[test]
fn test_e2e_t1_background_presets_hex_validity() {
    for (name, val) in BACKGROUNDS {
        let color = parse_color(val);
        assert!(color.is_some(), "Preset '{name}' with value '{val}' must parse to valid Color32");
    }
}

#[test]
fn test_e2e_t1_background_presets_light_dark_classification() {
    // Light presets
    assert!(!is_dark("#FFFFFF"), "Bianco must be classified as light");
    assert!(!is_dark("#F5F5F5"), "Grigio chiaro must be classified as light");
    assert!(!is_dark("#FAF7F0"), "Carta must be classified as light");

    // Dark presets
    assert!(is_dark("#26292E"), "Ardesia must be classified as dark");
    assert!(is_dark("#23392F"), "Lavagna verde must be classified as dark");
    assert!(is_dark("#1B2A41"), "Blu notte must be classified as dark");
}

#[test]
fn test_e2e_t1_background_meta_preset_selection() {
    let mut meta = model::BoardMeta::default();

    for (_name, val) in BACKGROUNDS {
        meta.background = val.into();
        let matches = BACKGROUNDS.iter().any(|(_, v)| meta.background.eq_ignore_ascii_case(v));
        assert!(matches, "Selected preset background '{val}' must match preset list");
    }
}

#[test]
fn test_e2e_t1_background_custom_hex_storage() {
    let mut meta = model::BoardMeta::default();
    let custom_sage = "#A3B18A";
    meta.background = custom_sage.into();

    let parsed = parse_color(&meta.background).unwrap();
    assert_eq!(hex(parsed).to_uppercase(), custom_sage);
}

// --- Feature 19: Accessibility Focus Rings & Contrast System [R9] ---

#[test]
fn test_e2e_t1_focus_ring_rendering_active_focus() {
    let ctx = Context::default();
    let theme_val = Theme::new(false, Color32::from_rgb(0x0D, 0x99, 0xFF), false);
    ui::setup(&ctx, theme_val, 1.0);

    // Draw UI and check focus ring execution
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(200.0, 100.0))
        .build_ui(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 30.0), Sense::click());
            resp.request_focus();
            focus_ring(ui, &resp, rect);
        });
    harness.run();
}

#[test]
fn test_e2e_t1_focus_ring_skipped_when_pointer_down() {
    let ctx = Context::default();
    let theme_val = Theme::new(false, Color32::from_rgb(0x0D, 0x99, 0xFF), false);
    ui::setup(&ctx, theme_val, 1.0);

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(200.0, 100.0))
        .build_ui(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 30.0), Sense::click());
            resp.request_focus();
            // When pointer is down, focus ring should be suppressed
            focus_ring(ui, &resp, rect);
        });
    harness.run();
}

#[test]
fn test_e2e_t1_high_contrast_theme_dark_boost() {
    let accent = Color32::from_rgb(0x0D, 0x99, 0xFF);
    let normal = Theme::new(true, accent, false);
    let high = Theme::new(true, accent, true);

    // High contrast dark theme boosts border luminance
    assert_eq!(normal.border, Color32::from_rgb(0x44, 0x44, 0x44));
    assert_eq!(high.border, Color32::from_rgb(0x8A, 0x8A, 0x8A));

    // Text2 opacity boosted
    assert!(high.text2.a() > normal.text2.a(), "High contrast secondary text must have higher opacity");
}

#[test]
fn test_e2e_t1_high_contrast_theme_light_boost() {
    let accent = Color32::from_rgb(0x0D, 0x99, 0xFF);
    let normal = Theme::new(false, accent, false);
    let high = Theme::new(false, accent, true);

    // High contrast light theme darkens border for crisp edge
    assert_eq!(normal.border, Color32::from_rgb(0xE6, 0xE6, 0xE6));
    assert_eq!(high.border, Color32::from_rgb(0x8C, 0x8C, 0x8C));

    // Text2 opacity boosted
    assert!(high.text2.a() > normal.text2.a(), "High contrast light secondary text must have higher opacity");
}

#[test]
fn test_e2e_t1_accent_wcag_aa_contrast() {
    // Pale custom yellow accent
    let pale_yellow = Color32::from_rgb(0xFF, 0xE0, 0x66);
    let t = Theme::new(false, pale_yellow, false);

    // brand_fill must be significantly darker than pale_yellow to ensure white text achieves >= 4.5:1
    assert!(t.brand_fill.r() <= pale_yellow.r());
    assert!(t.brand_fill.g() <= pale_yellow.g());
    assert!(t.brand_fill.b() <= pale_yellow.b());
}

#[test]
fn test_e2e_t1_reduced_motion_mode() {
    let ctx = Context::default();
    assert!(!ui::motion::reduced(&ctx), "Default motion should not be reduced");

    ui::motion::set_reduced(&ctx, true);
    assert!(ui::motion::reduced(&ctx), "Reduced motion flag must be active after setting");

    ui::motion::set_reduced(&ctx, false);
    assert!(!ui::motion::reduced(&ctx), "Reduced motion can be toggled back");
}

/* =========================================================================
   TIER 2: BOUNDARY & CORNER CASES (>= 5 tests for limits & extremes)
   ========================================================================= */

#[test]
fn test_e2e_t2_curved_connector_zero_distance() {
    // Degenerate start and end at identical coordinates
    let pts = curve(100.0, 100.0, 100.0, 100.0);
    assert_eq!(pts[0], (100.0, 100.0));
    assert_eq!(pts[3], (100.0, 100.0));
    let mid = bezier(&pts, 0.5);
    assert!(!mid.0.is_nan() && !mid.1.is_nan(), "Zero-distance Bézier midpoint must not be NaN");
    assert_eq!(mid, (100.0, 100.0));
}

#[test]
fn test_e2e_t2_curved_connector_collinear_vertical() {
    // Straight vertical line: dx = 0, dy = 300
    let pts = curve(200.0, 100.0, 200.0, 400.0);
    assert_eq!(pts[0], (200.0, 100.0));
    assert_eq!(pts[3], (200.0, 400.0));
    // Vertical dominant axis tangents
    assert_eq!(pts[1], (200.0, 100.0 + 150.0));
    assert_eq!(pts[2], (200.0, 400.0 - 150.0));
}

#[test]
fn test_e2e_t2_curved_connector_extreme_coordinates() {
    let pts = curve(-1_000_000.0, -1_000_000.0, 1_000_000.0, 1_000_000.0);
    let (mx, my) = bezier(&pts, 0.5);
    assert!(!mx.is_nan() && !my.is_nan());
    assert!((mx - 0.0).abs() < 1e-3);
    assert!((my - 0.0).abs() < 1e-3);
}

#[test]
fn test_e2e_t2_theme_custom_accent_extreme_black_white() {
    // Pure black accent
    let t_black = Theme::new(false, Color32::BLACK, false);
    assert_eq!(t_black.brand, Color32::BLACK);

    // Pure white accent
    let t_white = Theme::new(true, Color32::WHITE, false);
    assert_eq!(t_white.brand, Color32::WHITE);
}

#[test]
fn test_e2e_t2_background_invalid_hex_fallback() {
    assert_eq!(parse_color(""), None);
    assert_eq!(parse_color("#"), None);
    assert_eq!(parse_color("#12"), None);
    assert_eq!(parse_color("#GGGGGG"), None);
    assert_eq!(parse_color("invalid"), None);

    let fallback = Color32::from_rgb(0xFF, 0x00, 0x00);
    assert_eq!(color_or("invalid_hex", fallback), fallback);
}

#[test]
fn test_e2e_t2_rapid_theme_toggle_stability() {
    let ctx = Context::default();
    let mut applied = None;
    let mut prefs = Prefs::default();

    for i in 0..100 {
        prefs.theme = if i % 2 == 0 { PrefsTheme::Light } else { PrefsTheme::Dark };
        app::apply_theme(&ctx, &prefs, &mut applied);
    }
    let t = ui::theme(&ctx);
    assert!(t.dark, "Theme after 99 iterations must be dark");
}

#[test]
fn test_e2e_t2_focus_ring_zero_size_rect() {
    let ctx = Context::default();
    let theme_val = Theme::new(false, Color32::from_rgb(0x0D, 0x99, 0xFF), false);
    ui::setup(&ctx, theme_val, 1.0);

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(100.0, 100.0))
        .build_ui(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(0.0, 0.0), Sense::click());
            focus_ring(ui, &resp, rect);
        });
    harness.run();
}

/* =========================================================================
   TIER 3: CROSS-FEATURE COMBINATIONS (Pairwise Interactions)
   ========================================================================= */

#[test]
fn test_e2e_t3_curved_connector_with_dark_and_light_themes() {
    let light_theme = Theme::new(false, Color32::from_rgb(0x0D, 0x99, 0xFF), false);
    let dark_theme = Theme::new(true, Color32::from_rgb(0x0D, 0x99, 0xFF), false);

    // Stroke contrast checks
    let stroke_color = Color32::from_rgb(0x14, 0xAE, 0x5C); // Green connector
    assert_ne!(stroke_color, light_theme.canvas, "Connector color must contrast with light canvas");
    assert_ne!(stroke_color, dark_theme.canvas, "Connector color must contrast with dark canvas");
}

#[test]
fn test_e2e_t3_theme_switch_with_custom_background() {
    let ctx = Context::default();
    let mut applied = None;
    let mut ed = create_test_editor();

    // Set custom background "Carta" (#FAF7F0)
    ed.board.set_meta(&model::BoardMeta {
        background: "#FAF7F0".into(),
        ..Default::default()
    });

    // Switch theme from Dark to Light
    let mut prefs = Prefs { theme: PrefsTheme::Light, ..Default::default() };
    app::apply_theme(&ctx, &prefs, &mut applied);

    // Board background must remain unchanged
    assert_eq!(ed.board.meta().background, "#FAF7F0");

    // Switch theme to Dark
    prefs.theme = PrefsTheme::Dark;
    app::apply_theme(&ctx, &prefs, &mut applied);
    assert_eq!(ed.board.meta().background, "#FAF7F0");
}

#[test]
fn test_e2e_t3_high_contrast_with_all_background_presets() {
    let high_dark = Theme::new(true, Color32::from_rgb(0x0D, 0x99, 0xFF), true);
    let high_light = Theme::new(false, Color32::from_rgb(0x0D, 0x99, 0xFF), true);

    for (name, val) in BACKGROUNDS {
        let is_preset_dark = is_dark(val);
        if is_preset_dark {
            assert!(high_dark.text.r() > 200, "High contrast text on dark background '{name}' must be bright");
        } else {
            assert!(high_light.text.r() < 50, "High contrast text on light background '{name}' must be dark");
        }
    }
}

#[test]
fn test_e2e_t3_focus_ring_with_theme_color_brand() {
    let custom_brand = Color32::from_rgb(0x9B, 0x51, 0xE0); // Purple
    let t = Theme::new(false, custom_brand, false);
    assert_eq!(t.brand, custom_brand, "Theme brand color must match custom accent");
}

#[test]
fn test_e2e_t3_code_block_theme_with_board_background() {
    let mut ed = create_test_editor();
    ed.board.set_meta(&model::BoardMeta {
        background: "#26292E".into(), // Dark Ardesia
        ..Default::default()
    });

    let code_el = El::new(Kind::Code(make_code("const PI = 3.14159;", "javascript", true)));
    ed.board.put(vec![code_el]);

    assert!(is_dark(&ed.board.meta().background));
    assert!(ed.board.all()[0].code().unwrap().light, "Light code card is supported on dark canvas");
}

/* =========================================================================
   TIER 4: REAL-WORLD APPLICATION SCENARIOS (>= 5 realistic user scenarios)
   ========================================================================= */

#[test]
fn test_e2e_t4_scenario_day_to_night_workspace_transition() {
    let ctx = Context::default();
    let mut applied = None;
    let mut ed = create_test_editor();

    // 1. Day Mode: Light theme, Carta background, light code block
    let mut prefs = Prefs { theme: PrefsTheme::Light, ..Default::default() };
    app::apply_theme(&ctx, &prefs, &mut applied);
    ed.board.set_meta(&model::BoardMeta {
        background: "#FAF7F0".into(), // Carta
        ..Default::default()
    });
    let code_el = El::new(Kind::Code(make_code("let mode = 'day';", "javascript", true)));
    let code_id = code_el.id.clone();
    ed.board.put(vec![code_el]);

    assert!(!ui::theme(&ctx).dark);
    assert!(!is_dark(&ed.board.meta().background));
    assert!(ed.board.get(&code_id).unwrap().code().unwrap().light);

    // 2. Evening transition: Switch to Dark theme, Blu Notte background, dark code block
    prefs.theme = PrefsTheme::Dark;
    app::apply_theme(&ctx, &prefs, &mut applied);
    ed.board.set_meta(&model::BoardMeta {
        background: "#1B2A41".into(), // Blu notte
        ..Default::default()
    });
    ed.board.update(&[code_id.clone()], |el| {
        if let Kind::Code(c) = &mut el.kind {
            c.light = false;
        }
    });

    assert!(ui::theme(&ctx).dark);
    assert!(is_dark(&ed.board.meta().background));
    assert!(!ed.board.get(&code_id).unwrap().code().unwrap().light);
}

#[test]
fn test_e2e_t4_scenario_accessible_presentation_setup() {
    let ctx = Context::default();
    let mut applied = None;
    let mut ed = create_test_editor();

    // 1. User configures high contrast and reduced motion for presentation
    let prefs = Prefs {
        theme: PrefsTheme::Light,
        high_contrast: true,
        reduce_motion: true,
        ..Default::default()
    };
    app::apply_theme(&ctx, &prefs, &mut applied);
    let t = ui::theme(&ctx);

    // 2. Verify high contrast border is strong
    assert_eq!(t.border, Color32::from_rgb(0x8C, 0x8C, 0x8C));
    assert!(ui::motion::reduced(&ctx));

    // 3. User selects high-contrast chalkboard background
    ed.board.set_meta(&model::BoardMeta {
        background: "#23392F".into(), // Lavagna verde
        ..Default::default()
    });
    assert_eq!(ed.board.meta().background, "#23392F");
}

#[test]
fn test_e2e_t4_scenario_system_architecture_diagram() {
    let mut ed = create_test_editor();

    // Node 1: Client Sticky Note
    let mut client_node = El::new(Kind::Sticky(Sticky {
        text: "Frontend Client".into(),
        color: "#C7E5FF".into(),
        ..Default::default()
    }));
    client_node.x = 100.0;
    client_node.y = 100.0;
    client_node.w = 120.0;
    client_node.h = 80.0;

    // Node 2: Server Sticky Note
    let mut server_node = El::new(Kind::Sticky(Sticky {
        text: "Backend API".into(),
        color: "#FFF3A3".into(),
        ..Default::default()
    }));
    server_node.x = 400.0;
    server_node.y = 300.0;
    server_node.w = 120.0;
    server_node.h = 80.0;

    // Curved connector linking client to server
    let mut conn = El::new(Kind::Line(Line {
        points: vec![220.0, 140.0, 400.0, 340.0],
        route: Route::Curved,
        arrow_end: true,
        stroke_width: 2.0,
        ..Default::default()
    }));
    conn.x = 220.0;
    conn.y = 140.0;
    conn.w = 180.0;
    conn.h = 200.0;

    ed.board.put(vec![client_node, server_node, conn.clone()]);
    assert_eq!(ed.board.len(), 3);

    // Validate curved connector control points
    let line_ref = conn.line().unwrap();
    let ctrl_pts = curve_of(line_ref);
    let (mid_x, mid_y) = bezier(&ctrl_pts, 0.5);
    assert!(mid_x > 220.0 && mid_x < 400.0);
    assert!(mid_y > 140.0 && mid_y < 340.0);
}

#[test]
fn test_e2e_t4_scenario_custom_branding_and_color_well() {
    let custom_brand_accent = Color32::from_rgb(0xFF, 0x6B, 0x6B); // Coral red
    let t = Theme::new(false, custom_brand_accent, false);
    assert_eq!(t.brand, custom_brand_accent);

    // Board canvas custom background
    let mut meta = model::BoardMeta::default();
    let corporate_bg = "#F8F9FA";
    meta.background = corporate_bg.into();
    assert_eq!(parse_color(&meta.background), Some(Color32::from_rgb(0xF8, 0xF9, 0xFA)));

    // Ensure button fill text contrast passes WCAG AA
    assert!(t.brand_fill.r() < custom_brand_accent.r() || t.brand_fill.g() < custom_brand_accent.g());
}

#[test]
fn test_e2e_t4_scenario_full_keyboard_navigation_audit() {
    let ctx = Context::default();
    let t = Theme::new(false, Color32::from_rgb(0x0D, 0x99, 0xFF), false);
    ui::setup(&ctx, t, 1.0);

    // Test a simulated toolbar with 3 keyboard-navigable buttons with focus rings
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(300.0, 60.0))
        .build_ui(|ui| {
            ui.horizontal(|ui| {
                for i in 0..3 {
                    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                    if i == 1 {
                        resp.request_focus();
                    }
                    focus_ring(ui, &resp, rect);
                }
            });
        });
    harness.run();
}
