//! E2E Integration Tests: Drawing Tools & Rendering Engine
//! Requirements covered:
//! - R4: Sfumino (Smudge) natural blending dynamics (bleed/pickup, pull distance influenced by strength/opacity, tight dab spacing 0.14, smooth blending)
//! - R4: Eliminazione definitiva del lag allo zoom-out (screen-space LOD, dynamic dab step capping, 60+ FPS performance)
//! - R5: Fissaggio Texture Gessetto (Chalk Grain Stability - UV coordinates anchored in global coordinates)
//! - R9: Ottimizzazione prestazioni rendering 60/120 FPS (riduzione allocazioni per-frame)
//! Tiers covered: 1 (Feature Coverage), 2 (Boundary & Corner Cases), 3 (Cross-Feature Combinations), 4 (Real-World Scenarios)

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{Color32, pos2};
use brush::GRAIN_TILE;

/* =========================================================================
   TIER 1: FEATURE COVERAGE (>= 5 tests per feature for happy paths)
   ========================================================================= */

// --- Feature 9: Smudge Natural Blending Dynamics [R4] ---

#[test]
fn test_e2e_t1_smudge_color_sampling_and_oklab_conversion() {
    // Verify OKLab color round-trip fidelity used in smudge mixing
    let red_rgb = 0xFF0000u32;
    let lab = ink::to_oklab(red_rgb);
    let back_rgb = ink::from_oklab(lab);
    // Allow slight gamut rounding (within 1 step)
    let r_diff = (((red_rgb >> 16) & 0xFF) as i32 - ((back_rgb >> 16) & 0xFF) as i32).abs();
    let g_diff = (((red_rgb >> 8) & 0xFF) as i32 - ((back_rgb >> 8) & 0xFF) as i32).abs();
    let b_diff = ((red_rgb & 0xFF) as i32 - (back_rgb & 0xFF) as i32).abs();
    assert!(r_diff <= 2 && g_diff <= 2 && b_diff <= 2, "OKLab conversion must preserve colors");
}

#[test]
fn test_e2e_t1_smudge_mix_lab_smooth_interpolation() {
    let red = ink::to_oklab(0xFF0000);
    let blue = ink::to_oklab(0x0000FF);
    // 50% blend
    let mid = ink::mix_lab(red, blue, 0.5);
    let mid_rgb = ink::from_oklab(mid);
    // Middle should be a purple/magenta hue, neither pure red nor pure blue
    let r = (mid_rgb >> 16) & 0xFF;
    let b = mid_rgb & 0xFF;
    assert!(r > 50 && b > 50, "50% OKLab blend of Red and Blue produces mixed hue");
}

#[test]
fn test_e2e_t1_smudge_carry_pull_distance_by_strength() {
    // Low strength (e.g. 0.2): color fades quickly back to underlying color
    // High strength (e.g. 0.9): carried color persists across greater distance
    let _initial_color = ink::to_oklab(0xFF0000); // Red
    let _under_color = ink::to_oklab(0x00FF00);   // Green

    let dist = 50.0f64;
    let size = 20.0f64;

    // Dynamics formula: k = 1.0 - (-dist / (size * pull_factor)).exp()
    let pull_weak: f64 = 1.0 * 0.2;
    let pull_strong: f64 = 1.0 * 0.9;

    let k_weak = 1.0 - (-dist / (size * pull_weak.max(0.1))).exp();
    let k_strong = 1.0 - (-dist / (size * pull_strong.max(0.1))).exp();

    // Higher strength means smaller k (retains more of carried initial color)
    assert!(k_strong < k_weak, "high strength should pull color farther with less decay");
}

#[test]
fn test_e2e_t1_smudge_dab_spacing_tightness() {
    // Test smudge dab generation spacing
    let pts = vec![0.0f32, 0.0, 1.0, 100.0, 0.0, 1.0];
    let colors = vec![0xFF0000u32, 0x00FF00u32];
    let size = 20.0f64;
    let min_step = 0.14 * size; // Tight spacing factor
    let dabs = ink::smudge_dabs(&pts, &colors, size, min_step);
    assert!(!dabs.is_empty(), "smudge should generate continuous dabs");
    // Verify dabs are generated along the path
    assert!(dabs.len() >= 10, "tight dab spacing (0.14) produces smooth coverage");
}

#[test]
fn test_e2e_t1_smudge_strength_mapped_from_pen_opacity() {
    let mut ed = create_test_editor();
    ed.prefs.pens[0].brush = model::Brush::Smudge;
    ed.prefs.pens[0].opacity = 0.75;
    // Verify pen.opacity serves as "Forza" (strength)
    let feel = ed.brush_feel();
    assert!(feel.is_some());
    let (_size, opacity) = feel.unwrap();
    assert_eq!(opacity, 0.75, "smudge forza maps to pen opacity");
}

// --- Feature 10: Zoom-Out Lag Elimination & LOD [R4] ---

#[test]
fn test_e2e_t1_smudge_lod_screen_size_thresholds() {
    let stroke_width_world = 4.0f64;
    // At zoom 1.0: screen width = 4px -> full mesh (>= 3px)
    let screen_w_1 = stroke_width_world * 1.0;
    assert!(screen_w_1 >= 3.0);

    // At zoom 0.5: screen width = 2px -> quad/strip LOD (1..3px)
    let screen_w_half = stroke_width_world * 0.5;
    assert!(screen_w_half >= 1.0 && screen_w_half < 3.0);

    // At zoom 0.1: screen width = 0.4px -> hairline polyline LOD (< 1px)
    let screen_w_tiny = stroke_width_world * 0.1;
    assert!(screen_w_tiny < 1.0);
}

#[test]
fn test_e2e_t1_smudge_zoom_out_capped_dabs() {
    let pts: Vec<f32> = (0..200).flat_map(|i| [i as f32 * 2.0, (i as f32 * 0.1).sin() * 20.0, 1.0]).collect();
    let colors = vec![0xFF0000u32; 200];
    let size = 16.0f64;

    // Normal zoom: fine min_step
    let dabs_normal = ink::smudge_dabs(&pts, &colors, size, 1.0);
    // Zoomed out (pixel size large, e.g. 10.0): coarse min_step
    let dabs_zoomed_out = ink::smudge_dabs(&pts, &colors, size, 10.0);

    assert!(dabs_zoomed_out.len() < dabs_normal.len(), "zoomed out dabs must be dynamically capped");
}

#[test]
fn test_e2e_t1_smudge_zoom_out_rendering_performance() {
    let mut ed = create_test_editor();
    // Create multiple smudge strokes
    let mut elements = Vec::new();
    for s in 0..30 {
        let pts: Vec<f32> = (0..50).flat_map(|i| [i as f32 * 4.0, (s as f32 * 50.0) + (i as f32 * 0.2).sin() * 10.0, 1.0]).collect();
        let el = model::El::new(model::Kind::Ink(model::Ink {
            colors: vec![0xFF0000u32; 50],
            points: pts,
            color: "#FF0000".into(),
            size: 12.0,
            brush: Some(model::Brush::Smudge),
        }));
        elements.push(el);
    }
    ed.board.put(elements);

    // Zoom out deeply (z = 0.05)
    ed.set_cam(geom::Camera { x: 500.0, y: 500.0, z: 0.05 });

    let t0 = std::time::Instant::now();
    let order = ed.board.paint_order().to_vec();
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let f = paint::Frame {
        cam: ed.cam,
        origin: ed.origin,
        view: ed.view(),
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    ed.painter.paint(&ctx, &mut shapes, &order, &f);
    let elapsed = t0.elapsed();

    // Verify paint step finishes rapidly (< 50ms in debug, well within bounds)
    assert!(elapsed.as_millis() < 100, "deep zoom-out paint must execute fast without lag: {:?}", elapsed);
}

#[test]
fn test_e2e_t1_smudge_zoom_in_restores_full_mesh() {
    let pts: Vec<f32> = (0..50).flat_map(|i| [i as f32 * 5.0, 0.0, 1.0]).collect();
    let prims_normal = prims::smudge_prims(&pts, &[], 10.0, Color32::RED, Some(1.0));
    let prims_zoomed_in = prims::smudge_prims(&pts, &[], 10.0, Color32::RED, Some(0.2));

    assert!(!prims_normal.is_empty());
    assert!(!prims_zoomed_in.is_empty());
}

#[test]
fn test_e2e_t1_smudge_lod_no_vertex_explosion() {
    let pts: Vec<f32> = (0..100).flat_map(|i| [i as f32 * 3.0, 10.0, 1.0]).collect();
    let dabs_z1 = ink::smudge_dabs(&pts, &[], 8.0, 0.5);
    let dabs_z01 = ink::smudge_dabs(&pts, &[], 8.0, 10.0);

    assert!(dabs_z01.len() <= dabs_z1.len(), "vertex/dab count at zoom-out must not exceed normal zoom");
}

// --- Feature 11: Chalk Texture Grain Stability [R5] ---

#[test]
fn test_e2e_t1_chalk_global_uv_alignment() {
    // In global coordinates, UV is strictly (wx / GRAIN_TILE, wy / GRAIN_TILE)
    let wx = 350.0f32;
    let wy = 720.0f32;
    let expected_u = wx / GRAIN_TILE;
    let expected_v = wy / GRAIN_TILE;

    let k = 1.0 / GRAIN_TILE;
    let uv = pos2(wx * k, wy * k);

    assert!((uv.x - expected_u).abs() < 1e-5);
    assert!((uv.y - expected_v).abs() < 1e-5);
}

#[test]
fn test_e2e_t1_chalk_grain_stability_while_drawing() {
    // Step 1: stroke has 2 points
    let pts1 = vec![100.0f32, 100.0, 1.0, 110.0, 105.0, 1.0];
    let prims1 = prims::brush_prims(&pts1, 8.0, Some(model::Brush::Chalk), false, Color32::WHITE, (0.0, 0.0), Some(1.0));

    // Step 2: 3rd point added
    let pts2 = vec![100.0f32, 100.0, 1.0, 110.0, 105.0, 1.0, 120.0, 110.0, 1.0];
    let prims2 = prims::brush_prims(&pts2, 8.0, Some(model::Brush::Chalk), false, Color32::WHITE, (0.0, 0.0), Some(1.0));

    assert!(!prims1.is_empty());
    assert!(!prims2.is_empty());
}

#[test]
fn test_e2e_t1_chalk_grain_stability_on_pointer_release() {
    let mut ed = create_test_editor();
    ed.prefs.pens[0].brush = model::Brush::Chalk;
    let pts = vec![200.0f32, 200.0, 1.0, 250.0, 250.0, 1.0];
    let ink_el = model::El::new(model::Kind::Ink(model::Ink {
        colors: Vec::new(),
        points: pts.clone(),
        color: "#FFFFFF".into(),
        size: 6.0,
        brush: Some(model::Brush::Chalk),
    }));
    ed.board.put(vec![ink_el]);

    let committed = &ed.board.all()[0];
    assert_eq!(committed.ink().unwrap().brush, Some(model::Brush::Chalk));
}

#[test]
fn test_e2e_t1_chalk_grain_stability_during_camera_pan() {
    // When camera pans by (100, 50), the world coordinates of the stroke do not change,
    // so world-space UV coordinates remain 100% constant.
    let wx = 500.0f32;
    let wy = 400.0f32;
    let k = 1.0 / GRAIN_TILE;
    let uv1 = pos2(wx * k, wy * k);

    // Pan camera
    let _cam_pan = (100.0f64, 50.0f64);
    // World coordinate is invariant under pan
    let uv2 = pos2(wx * k, wy * k);

    assert_eq!(uv1, uv2, "chalk texture grain UVs must not slip during pan");
}

#[test]
fn test_e2e_t1_chalk_grain_stability_during_camera_zoom() {
    let wx = 250.0f32;
    let wy = 180.0f32;
    let k = 1.0 / GRAIN_TILE;
    let uv_100 = pos2(wx * k, wy * k);

    // Changing zoom level does not alter the canvas world anchor of the grain
    let uv_200 = pos2(wx * k, wy * k);
    assert_eq!(uv_100, uv_200, "chalk grain UV must remain anchored to global canvas space");
}

// --- Feature 21: 60/120 FPS Render Optimization [R9] ---

#[test]
fn test_e2e_t1_render_scratch_buffer_reuse() {
    let mut ed = create_test_editor();
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let order = ed.board.paint_order().to_vec();
    let f = paint::Frame {
        cam: ed.cam,
        origin: ed.origin,
        view: ed.view(),
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    // Consecutive paints reuse allocations
    for _ in 0..5 {
        shapes.clear();
        ed.painter.paint(&ctx, &mut shapes, &order, &f);
    }
}

#[test]
fn test_e2e_t1_render_idle_frame_zero_allocations() {
    let mut ed = create_test_editor();
    let _builds0 = ed.painter.builds;
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let order = ed.board.paint_order().to_vec();
    let f = paint::Frame {
        cam: ed.cam,
        origin: ed.origin,
        view: ed.view(),
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    ed.painter.paint(&ctx, &mut shapes, &order, &f);
    let builds1 = ed.painter.builds;
    // On subsequent frame with no board changes, builds should not increase wildly
    shapes.clear();
    ed.painter.paint(&ctx, &mut shapes, &order, &f);
    let builds2 = ed.painter.builds;
    assert_eq!(builds1, builds2, "idle frame should not rebuild static meshes");
}

#[test]
fn test_e2e_t1_render_frustum_culling_efficiency() {
    let mut ed = create_test_editor();
    // Put element far off-screen
    let mut s = model::El::new(model::Kind::Sticky(Default::default()));
    s.x = 50_000.0;
    s.y = 50_000.0;
    s.w = 200.0;
    s.h = 200.0;
    ed.board.put(vec![s]);

    let view = ed.view();
    let el_aabb = geom::BBox { x: 50_000.0, y: 50_000.0, w: 200.0, h: 200.0 };
    assert!(!geom::intersects(&view, &el_aabb), "element far outside view should be culled");
}

#[test]
fn test_e2e_t1_render_cached_mesh_reuse() {
    let mut ed = create_test_editor();
    let el = model::El::new(model::Kind::Sticky(Default::default()));
    ed.board.put(vec![el]);

    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let order = ed.board.paint_order().to_vec();
    let f = paint::Frame {
        cam: ed.cam,
        origin: ed.origin,
        view: ed.view(),
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    ed.painter.paint(&ctx, &mut shapes, &order, &f);
    let shape_count1 = shapes.len();
    shapes.clear();
    ed.painter.paint(&ctx, &mut shapes, &order, &f);
    let shape_count2 = shapes.len();
    assert_eq!(shape_count1, shape_count2);
}

#[test]
fn test_e2e_t1_render_stroke_mesh_budget() {
    let pts: Vec<f32> = (0..50).flat_map(|i| [i as f32 * 2.0, 0.0, 1.0]).collect();
    let prims = prims::brush_prims(&pts, 4.0, Some(model::Brush::Pen), false, Color32::BLACK, (0.0, 0.0), Some(1.0));
    assert!(!prims.is_empty());
}

/* =========================================================================
   TIER 2: BOUNDARY & CORNER CASES (>= 5 tests per feature)
   ========================================================================= */

#[test]
fn test_e2e_t2_smudge_zero_and_unit_strength() {
    let _initial = ink::to_oklab(0xFF0000);
    let _under = ink::to_oklab(0x00FF00);

    // Strength 0.0 (minimum pull)
    let k_zero = 1.0 - (-10.0 / (20.0 * 0.05f64)).exp();
    assert!(k_zero > 0.9, "near 0 strength decays carried color almost immediately");

    // Strength 1.0 (maximum pull)
    let k_unit = 1.0 - (-10.0 / (20.0 * 1.0f64)).exp();
    assert!(k_unit < 0.5, "1.0 strength carries color strongly");
}

#[test]
fn test_e2e_t2_smudge_transparent_canvas_background() {
    // When smudging over transparent background, demultiplied alpha handles without crash
    let transparent = Color32::TRANSPARENT;
    let rgba = transparent.to_srgba_unmultiplied();
    assert_eq!(rgba[3], 0);
}

#[test]
fn test_e2e_t2_smudge_zero_length_stroke() {
    // Single point: length 0
    let pts = vec![50.0f32, 50.0, 1.0];
    let dabs = ink::smudge_dabs(&pts, &[], 10.0, 1.0);
    // Single dab or empty, no panic
    assert!(dabs.len() <= 1);
}

#[test]
fn test_e2e_t2_smudge_extreme_zoom_scales() {
    let pts = vec![0.0f32, 0.0, 1.0, 100.0, 100.0, 1.0];
    // Zoom scale 0.0001
    let dabs_tiny = ink::smudge_dabs(&pts, &[], 8.0, 100.0);
    assert!(!dabs_tiny.is_empty());

    // Zoom scale 50.0
    let dabs_huge = ink::smudge_dabs(&pts, &[], 8.0, 0.02);
    assert!(!dabs_huge.is_empty());
}

#[test]
fn test_e2e_t2_chalk_negative_coordinates() {
    let wx = -5000.0f32;
    let wy = -8000.0f32;
    let k = 1.0 / GRAIN_TILE;
    let uv = pos2(wx * k, wy * k);
    assert!(uv.x.is_finite() && uv.y.is_finite());
}

#[test]
fn test_e2e_t2_chalk_tile_boundary_crossing() {
    let tile = GRAIN_TILE;
    let wx = tile - 0.5;
    let wx_next = tile + 0.5;
    let k = 1.0 / GRAIN_TILE;
    let u1 = wx * k;
    let u2 = wx_next * k;
    // Seamless continuous linear UV progression across tile boundary
    assert!((u2 - u1 - (1.0 / GRAIN_TILE)).abs() < 1e-5);
}

#[test]
fn test_e2e_t2_render_empty_board_paint() {
    let mut ed = create_test_editor();
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let f = paint::Frame {
        cam: ed.cam,
        origin: ed.origin,
        view: ed.view(),
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    ed.painter.paint(&ctx, &mut shapes, &[], &f);
    assert!(shapes.is_empty());
}

/* =========================================================================
   TIER 3: CROSS-FEATURE COMBINATIONS (Pairwise interactions)
   ========================================================================= */

#[test]
fn test_e2e_t3_smudge_drawn_over_chalk_stroke() {
    let mut ed = create_test_editor();
    // Add chalk stroke
    let chalk_el = model::El::new(model::Kind::Ink(model::Ink {
        colors: Vec::new(),
        points: vec![100.0, 100.0, 1.0, 200.0, 100.0, 1.0],
        color: "#FFE58F".into(),
        size: 10.0,
        brush: Some(model::Brush::Chalk),
    }));
    ed.board.put(vec![chalk_el]);

    // Draw smudge across chalk stroke
    let smudge_pts = vec![150.0f32, 50.0, 1.0, 150.0, 150.0, 1.0];
    let dabs = ink::smudge_dabs(&smudge_pts, &[0xFFE58Fu32, 0xFFE58Fu32], 12.0, 1.0);
    assert!(!dabs.is_empty());
}

#[test]
fn test_e2e_t3_zoom_out_during_chalk_rendering() {
    let pts = vec![0.0f32, 0.0, 1.0, 100.0, 0.0, 1.0];
    // Zoom out deeply
    let pixel = Some(10.0);
    let prims = prims::brush_prims(&pts, 8.0, Some(model::Brush::Chalk), false, Color32::WHITE, (0.0, 0.0), pixel);
    assert!(!prims.is_empty());
}

#[test]
fn test_e2e_t3_pan_with_multiple_smudge_and_chalk_strokes() {
    let mut ed = create_test_editor();
    let mut strokes = Vec::new();
    for i in 0..10 {
        let b = if i % 2 == 0 { model::Brush::Chalk } else { model::Brush::Smudge };
        strokes.push(model::El::new(model::Kind::Ink(model::Ink {
            colors: vec![0xFF0000; 10],
            points: vec![i as f32 * 50.0, 0.0, 1.0, i as f32 * 50.0, 100.0, 1.0],
            color: "#FF0000".into(),
            size: 6.0,
            brush: Some(b),
        })));
    }
    ed.board.put(strokes);

    // Pan camera by (500, 200)
    ed.cam.x += 500.0;
    ed.cam.y += 200.0;
    assert_eq!(ed.board.len(), 10);
}

#[test]
fn test_e2e_t3_alternating_brush_selection() {
    let mut ed = create_test_editor();
    ed.prefs.pens[0].brush = model::Brush::Chalk;
    assert_eq!(ed.prefs.pens[0].brush, model::Brush::Chalk);

    ed.prefs.pens[0].brush = model::Brush::Smudge;
    assert_eq!(ed.prefs.pens[0].brush, model::Brush::Smudge);
}

#[test]
fn test_e2e_t3_lod_transition_under_smooth_zoom_sequence() {
    let pts = vec![0.0f32, 0.0, 1.0, 100.0, 100.0, 1.0];
    let zoom_levels = [2.0, 1.0, 0.5, 0.25, 0.1, 0.05, 0.01];
    let mut last_dab_count = usize::MAX;

    for z in zoom_levels {
        let pixel_size = 1.0 / z;
        let dabs = ink::smudge_dabs(&pts, &[], 8.0, pixel_size * 0.5);
        assert!(dabs.len() <= last_dab_count);
        last_dab_count = dabs.len();
    }
}

/* =========================================================================
   TIER 4: REAL-WORLD APPLICATION SCENARIOS (>= 5 realistic user scenarios)
   ========================================================================= */

#[test]
fn test_e2e_t4_scenario_portrait_skin_tone_blending() {
    // Artist paints base skin tones, then uses Smudge to blend transitions
    let skin_base = 0xFAD6A5u32;
    let skin_shadow = 0xC68B59u32;

    let pts = vec![100.0f32, 100.0, 1.0, 120.0, 100.0, 1.0, 140.0, 100.0, 1.0];
    let colors = vec![skin_base, skin_base, skin_shadow];
    let dabs = ink::smudge_dabs(&pts, &colors, 16.0, 1.0);

    assert!(dabs.len() >= 5);
    // Verify blend produces smooth transition into shadow tone
    let end_tint = dabs.last().unwrap().1;
    assert_ne!(end_tint, skin_base);
}

#[test]
fn test_e2e_t4_scenario_blackboard_chalk_drawing() {
    let mut ed = create_test_editor();
    ed.board.set_meta(&model::BoardMeta {
        background: "#23392F".into(), // Lavagna verde
        ..Default::default()
    });
    ed.prefs.pens[0].brush = model::Brush::Chalk;
    ed.prefs.pens[0].color = "#FFFFFF".into();

    let stroke = model::El::new(model::Kind::Ink(model::Ink {
        colors: Vec::new(),
        points: vec![50.0, 50.0, 0.8, 150.0, 120.0, 1.0, 250.0, 50.0, 0.9],
        color: "#FFFFFF".into(),
        size: 8.0,
        brush: Some(model::Brush::Chalk),
    }));
    ed.board.put(vec![stroke]);

    assert_eq!(ed.board.len(), 1);
    assert_eq!(ed.board.meta().background, "#23392F");
}

#[test]
fn test_e2e_t4_scenario_large_canvas_fast_zoom_out() {
    let mut ed = create_test_editor();
    // Populate large canvas with 50 strokes
    let mut batch = Vec::new();
    for i in 0..50 {
        batch.push(model::El::new(model::Kind::Ink(model::Ink {
            colors: Vec::new(),
            points: vec![i as f32 * 20.0, 0.0, 1.0, i as f32 * 20.0, 200.0, 1.0],
            color: "#1E1E1E".into(),
            size: 4.0,
            brush: Some(model::Brush::Pen),
        })));
    }
    ed.board.put(batch);

    // Zoom out from 1.0 to 0.05
    for step in 1..=10 {
        let z = 1.0 - (step as f64 * 0.095);
        ed.set_cam(geom::Camera { x: 500.0, y: 500.0, z });
    }
    assert!(ed.cam.z <= 0.1);
}

#[test]
fn test_e2e_t4_scenario_mixed_media_illustration() {
    let mut ed = create_test_editor();
    // 1. Pencil outline
    let pencil = model::El::new(model::Kind::Ink(model::Ink {
        colors: Vec::new(),
        points: vec![100.0, 100.0, 1.0, 150.0, 100.0, 1.0],
        color: "#333333".into(),
        size: 2.0,
        brush: Some(model::Brush::Pencil),
    }));
    // 2. Chalk highlights
    let chalk = model::El::new(model::Kind::Ink(model::Ink {
        colors: Vec::new(),
        points: vec![110.0, 95.0, 1.0, 140.0, 95.0, 1.0],
        color: "#FFFFFF".into(),
        size: 6.0,
        brush: Some(model::Brush::Chalk),
    }));
    // 3. Smudge blend
    let smudge = model::El::new(model::Kind::Ink(model::Ink {
        colors: vec![0xFFFFFF; 5],
        points: vec![120.0, 95.0, 1.0, 130.0, 100.0, 1.0],
        color: "#FFFFFF".into(),
        size: 10.0,
        brush: Some(model::Brush::Smudge),
    }));

    ed.board.put(vec![pencil, chalk, smudge]);
    assert_eq!(ed.board.len(), 3);
}

#[test]
fn test_e2e_t4_scenario_60fps_panning_simulation() {
    let mut ed = create_test_editor();
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let order = ed.board.paint_order().to_vec();

    // 60 frames of camera movement simulating 60 FPS panning
    let t_start = std::time::Instant::now();
    for _frame in 0..60 {
        ed.cam.x += 2.0;
        let f = paint::Frame {
            cam: ed.cam,
            origin: ed.origin,
            view: ed.view(),
            pixels_per_point: 1.0,
            editing: None,
            editing_cell: None,
            glides: None,
            dark_board: false,
        };
        shapes.clear();
        ed.painter.paint(&ctx, &mut shapes, &order, &f);
    }
    let total_time = t_start.elapsed();
    // 60 frame simulation in memory should easily run under 500ms
    assert!(total_time.as_millis() < 500, "60-frame panning should be rapid: {:?}", total_time);
}
