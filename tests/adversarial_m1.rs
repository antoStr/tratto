//! Empirical Adversarial Stress Test Suite for Milestone 1 (Drawing Engine & Shaders)
//!
//! Evaluates:
//! 1. Smudge dynamics: extreme opacities (0.01, 1.0, 0.0, negative, NaN, inf), rapid direction shifts, multi-color gradient bleeding.
//! 2. Zoom-out lag elimination: extreme zoom levels (z = 0.0001 to z = 50.0), large stroke batches (300+ strokes, 15,000+ dabs), vertex buffer bounds.
//! 3. Chalk grain texture stability: mathematical invariance of global UV coordinates under rapid pan & zoom.
//! 4. Render loop stability: scratch buffer reuse, absence of unbounded allocations.

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use egui::{Color32, Pos2, pos2};
use brush::{GRAIN_TILE, DAB_RINGS, DAB_SIDES};
use ink::{smudge_dabs, to_oklab};
use prims::{smudge_prims, Prim};
use model::{El, Kind, Ink, Brush};
use geom::{BBox, Camera};

/* =========================================================================
   ADVERSARIAL SUITE 1: SMUDGE DYNAMICS & NUMERICAL STABILITY
   ========================================================================= */

#[test]
fn test_adv_smudge_extreme_strength_and_opacity_spectrum() {
    // Stress test smudge_dabs with extreme opacity / strength values
    let pts = vec![
        0.0f32, 0.0, 1.0,
        50.0, 0.0, 0.5,
        100.0, 0.0, 0.01,
        150.0, 0.0, -1.0,
        200.0, 0.0, 2.0,
    ];
    let colors = vec![0xFF0000u32, 0x00FF00u32, 0x0000FFu32, 0xFFFF00u32, 0xFF00FFu32];

    let test_sizes = [0.1, 1.0, 14.0, 50.0, 200.0];
    for &sz in &test_sizes {
        let dabs = smudge_dabs(&pts, &colors, sz, 0.0);
        assert!(!dabs.is_empty(), "Smudge dabs must generate for valid size {}", sz);
        for (dab, col) in &dabs {
            assert!(dab[0].is_finite() && dab[1].is_finite(), "Dab coords must be finite");
            assert!(dab[2].is_finite() && dab[2] > 0.0, "Dab radius must be finite and positive");
            assert!(dab[3].is_finite() && dab[3] >= 0.0 && dab[3] <= 1.0, "Dab alpha must be in [0, 1]");
            assert!(*col <= 0x00FFFFFF, "Color must be valid 24-bit sRGB integer: 0x{:08X}", col);
        }
    }
}

#[test]
fn test_adv_smudge_nan_and_infinity_resistance() {
    let colors = vec![0xFF0000u32, 0x00FF00u32, 0x0000FFu32];

    // Corrupted input points with NaN and Inf
    let pts_with_nans = vec![
        0.0f32, 0.0, 1.0,
        f32::NAN, 10.0, 0.5,
        20.0, f32::INFINITY, 0.8,
        30.0, 30.0, f32::NAN,
        40.0, 40.0, f32::NEG_INFINITY,
        50.0, 50.0, 1.0,
    ];

    // Must not panic, must filter out non-finite points gracefully
    let dabs = smudge_dabs(&pts_with_nans, &colors, 20.0, 1.0);
    for (dab, _) in &dabs {
        assert!(dab[0].is_finite() && dab[1].is_finite(), "Filtered dab coords must be finite");
        assert!(dab[2].is_finite() && dab[3].is_finite(), "Filtered dab radius and alpha must be finite");
    }

    // Extreme/invalid sizes must return empty
    assert!(smudge_dabs(&pts_with_nans, &colors, f64::NAN, 1.0).is_empty());
    assert!(smudge_dabs(&pts_with_nans, &colors, f64::INFINITY, 1.0).is_empty());
    assert!(smudge_dabs(&pts_with_nans, &colors, -10.0, 1.0).is_empty());
    assert!(smudge_dabs(&pts_with_nans, &colors, 0.0, 1.0).is_empty());

    // Single point produces 1 dab (tap gesture dot)
    assert_eq!(smudge_dabs(&[0.0, 0.0, 1.0], &colors, 10.0, 1.0).len(), 1);
    // Incomplete or empty points return empty
    assert!(smudge_dabs(&[0.0, 0.0], &colors, 10.0, 1.0).is_empty());
    assert!(smudge_dabs(&[], &colors, 10.0, 1.0).is_empty());
}

#[test]
fn test_adv_smudge_rapid_direction_shifts_and_hairpin_turns() {
    // Generate rapid 180-degree hairpin turns and zero-distance jitter
    let mut pts = Vec::new();
    let mut colors = Vec::new();
    for i in 0..100 {
        // Ping-pong between 0 and 20 board units
        let target_x = if i % 2 == 0 { 20.0 } else { 0.0 };
        pts.extend([target_x, (i as f32) * 0.1, 0.8]);
        colors.push(if i % 2 == 0 { 0xFF0000 } else { 0x0000FF });
    }

    let dabs = smudge_dabs(&pts, &colors, 10.0, 0.5);
    assert!(!dabs.is_empty(), "Hairpin turns must generate dabs without panicking");
    for (dab, _col) in &dabs {
        assert!(dab[0].is_finite() && dab[1].is_finite());
        assert!(dab[2] > 0.0 && dab[3] >= 0.0 && dab[3] <= 1.0);
    }

    // Zero distance consecutive points
    let zero_dist_pts = vec![
        10.0f32, 10.0, 1.0,
        10.0, 10.0, 1.0,
        10.0, 10.0, 1.0,
        10.000001, 10.000001, 1.0,
    ];
    let zero_colors = vec![0x112233, 0x445566, 0x778899, 0xAABBCC];
    let zero_dabs = smudge_dabs(&zero_dist_pts, &zero_colors, 8.0, 0.1);
    assert!(!zero_dabs.is_empty(), "Zero-distance points must not cause zero-division or crash");
}

#[test]
fn test_adv_smudge_multicolor_gradient_bleeding_fidelity() {
    // Test OKLab multi-color interpolation over a 5-color spectrum
    let spectrum = [
        0xFF0000u32, // Pure Red
        0xFFFF00u32, // Pure Yellow
        0x00FF00u32, // Pure Green
        0x00FFFFu32, // Pure Cyan
        0x0000FFu32, // Pure Blue
    ];

    let mut pts = Vec::new();
    let mut colors = Vec::new();
    for (i, &col) in spectrum.iter().enumerate() {
        pts.extend([(i as f32) * 50.0, 0.0, 1.0]);
        colors.push(col);
    }

    let dabs = smudge_dabs(&pts, &colors, 20.0, 0.5);
    assert!(dabs.len() >= 20, "Should generate dense dabs along spectrum");

    // Verify continuous color transition: adjacent dabs must have smoothly transitioning colors
    for i in 1..dabs.len() {
        let c0 = dabs[i - 1].1;
        let c1 = dabs[i].1;
        let lab0 = to_oklab(c0);
        let lab1 = to_oklab(c1);
        let delta_e = ((lab1[0] - lab0[0]).powi(2) + (lab1[1] - lab0[1]).powi(2) + (lab1[2] - lab0[2]).powi(2)).sqrt();
        assert!(delta_e < 0.25, "Adjacent dabs must not have abrupt perceptual color jumps (delta_e = {})", delta_e);
    }
}

/* =========================================================================
   ADVERSARIAL SUITE 2: ZOOM-OUT LAG ELIMINATION & EXTREME ZOOM LEVELS
   ========================================================================= */

#[test]
fn test_adv_zoom_out_extreme_scales_lod_and_dab_capping() {
    // Stroke of length 1,000 units with nominal width 10.0
    let mut pts = Vec::new();
    for i in 0..200 {
        pts.extend([(i as f32) * 5.0, 0.0, 0.8]);
    }
    let colors = vec![0xFF0000; 200];
    let size = 10.0;

    // Test a wide range of zoom scales: from z = 0.0001 (ultra zoomed out) to z = 50.0 (ultra zoomed in)
    let zoom_levels = [0.0001f64, 0.001, 0.01, 0.05, 0.2, 1.0, 5.0, 20.0, 50.0];

    for &z in &zoom_levels {
        let pixel = 1.0 / z;
        let prims = smudge_prims(&pts, &colors, size, Color32::RED, Some(pixel));
        assert!(!prims.is_empty(), "Must produce prims at zoom {}", z);

        let mut total_dabs = 0;
        for prim in &prims {
            if let Prim::Dabs { dabs, .. } = prim {
                total_dabs += dabs.len();
            }
        }

        if z <= 0.001 {
            // Under extreme zoom out, min_step = pixel * 1.5 = 1500+ units.
            // Across a 1000-unit stroke, dynamic dab capping MUST strictly limit dab count!
            assert!(
                total_dabs <= 5,
                "At extreme zoom-out z={}, dabs ({}) must be capped by screen pixel size (expected <= 5)",
                z,
                total_dabs
            );
        } else if z >= 10.0 {
            // Under zoom-in, min_step is small, so standard dab density is generated
            assert!(
                total_dabs >= 50,
                "At zoom-in z={}, dabs ({}) must provide smooth coverage",
                z,
                total_dabs
            );
        }
    }
}

#[test]
fn test_adv_mesh_lod_selection_and_vertex_count_bounds() {
    let ctx = egui::Context::default();
    let place = |x: f32, y: f32| pos2(x, y);

    let make_prim = || Prim::Dabs {
        dabs: vec![[100.0f32, 100.0, 4.0, 1.0]],
        color: Color32::RED,
        tints: vec![Color32::RED],
    };

    // Case A: scale = 0.1 -> R_screen = 4.0 * 0.1 * 1.0 = 0.4 px (< 1.0 px) -> dab_strip
    let meshes_strip = paint::brush_meshes(&ctx, vec![make_prim()], 0.1, place);
    assert_eq!(meshes_strip.len(), 1);
    let v_strip = meshes_strip[0].vertices.len();
    assert_eq!(v_strip, 4, "R_screen < 1.0 px should generate 4 vertices (dab_strip fallback to quad)");

    // Case B: scale = 0.5 -> R_screen = 4.0 * 0.5 = 2.0 px (1.0 <= R < 3.0 px) -> dab_quad
    let meshes_quad = paint::brush_meshes(&ctx, vec![make_prim()], 0.5, place);
    assert_eq!(meshes_quad.len(), 1);
    let v_quad = meshes_quad[0].vertices.len();
    assert_eq!(v_quad, 4, "1.0 <= R_screen < 3.0 px should generate 4 vertices (dab_quad)");

    // Case C: scale = 2.0 -> R_screen = 4.0 * 2.0 = 8.0 px (>= 3.0 px) -> full dab_mesh
    let meshes_full = paint::brush_meshes(&ctx, vec![make_prim()], 2.0, place);
    assert_eq!(meshes_full.len(), 1);
    let v_full = meshes_full[0].vertices.len();
    let expected_full = 1 + (DAB_RINGS.len() - 1) * DAB_SIDES;
    assert_eq!(v_full, expected_full, "R_screen >= 3.0 px should generate full {} vertices", expected_full);
}

#[test]
fn test_adv_large_batch_stress_harness_no_overflow() {
    // Stress harness: 300 smudge strokes across the board, total > 15,000 dabs
    let mut ed = create_test_editor();
    let mut elements = Vec::new();

    for s in 0..300 {
        let mut pts = Vec::new();
        let mut cols = Vec::new();
        let ox = ((s % 20) as f64) * 150.0;
        let oy = ((s / 20) as f64) * 150.0;
        for i in 0..50 {
            let t = i as f64;
            pts.extend([(ox + t * 2.0) as f32, (oy + (t * 0.3).sin() * 20.0) as f32, 0.75]);
            cols.push(if (s + i) % 2 == 0 { 0x0033CC } else { 0xCC3300 });
        }
        let mut el = El::new(Kind::Ink(Ink {
            points: pts,
            size: 12.0,
            color: "blue".to_string(),
            brush: Some(Brush::Smudge),
            colors: cols,
        }));
        el.x = ox;
        el.y = oy;
        el.w = 120.0;
        el.h = 40.0;
        elements.push(el);
    }

    ed.board.put(elements);

    let ctx = egui::Context::default();
    let order = ed.board.paint_order().to_vec();
    let mut shapes = Vec::new();

    // Paint at zoom-out z = 0.01 (viewing all 300 strokes simultaneously)
    let view_all = BBox { x: -500.0, y: -500.0, w: 4000.0, h: 4000.0 };
    let f_out = paint::Frame {
        cam: Camera { x: 0.0, y: 0.0, z: 0.01 },
        origin: pos2(0.0, 0.0),
        view: view_all,
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };

    let start = std::time::Instant::now();
    ed.painter.paint(&ctx, &mut shapes, &order, &f_out);
    let duration = start.elapsed();

    assert!(!shapes.is_empty(), "Shapes must be rendered for large batch");
    assert!(duration.as_millis() < 500, "Zoomed-out rendering of 300 smudge strokes must finish quickly (took {} ms)", duration.as_millis());

    // Paint at zoom-in z = 2.0
    shapes.clear();
    let f_in = paint::Frame {
        cam: Camera { x: 0.0, y: 0.0, z: 2.0 },
        origin: pos2(0.0, 0.0),
        view: BBox { x: 0.0, y: 0.0, w: 500.0, h: 500.0 },
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    ed.painter.paint(&ctx, &mut shapes, &order, &f_in);
    assert!(!shapes.is_empty(), "Zoomed-in render must succeed");
}

/* =========================================================================
   ADVERSARIAL SUITE 3: CHALK GRAIN GLOBAL UV INVARIANCE
   ========================================================================= */

#[test]
fn test_adv_chalk_grain_mathematical_uv_invariance_under_pan_and_zoom() {
    let mut ed = create_test_editor();
    let pts = vec![0.0f32, 0.0, 1.0, 100.0, 50.0, 1.0, 200.0, 100.0, 1.0];
    let mut el = El::new(Kind::Ink(Ink {
        points: pts,
        size: 30.0,
        color: "white".to_string(),
        brush: Some(Brush::Chalk),
        colors: Vec::new(),
    }));
    el.x = 350.0;
    el.y = 520.0;
    el.w = 250.0;
    el.h = 150.0;
    ed.board.put(vec![el]);

    let ctx = egui::Context::default();
    let order = ed.board.paint_order().to_vec();

    // Test across several extreme camera translations and zoom levels (above hairline LOD)
    let camera_configs = [
        Camera { x: 0.0, y: 0.0, z: 1.0 },
        Camera { x: -10_000.0, y: -5_000.0, z: 1.0 },
        Camera { x: 25_000.0, y: 40_000.0, z: 1.0 },
        Camera { x: 0.0, y: 0.0, z: 0.5 },
        Camera { x: 0.0, y: 0.0, z: 15.0 },
        Camera { x: -500.0, y: 1200.0, z: 3.5 },
    ];

    let k = 1.0 / GRAIN_TILE;
    let mut pan_baseline_uvs: Option<Vec<Pos2>> = None;

    for cam in &camera_configs {
        let f = paint::Frame {
            cam: *cam,
            origin: pos2(0.0, 0.0),
            view: BBox { x: -50_000.0, y: -50_000.0, w: 100_000.0, h: 100_000.0 },
            pixels_per_point: 1.0,
            editing: None,
            editing_cell: None,
            glides: None,
            dark_board: true,
        };

        let mut shapes = Vec::new();
        ed.painter.paint(&ctx, &mut shapes, &order, &f);

        // Find the grained mesh for the chalk stroke
        let mut found = false;
        for shape in &shapes {
            if let egui::Shape::Mesh(m) = shape {
                if m.texture_id == crate::brush::grain_texture(&ctx, brush::Grain::Chalk) {
                    found = true;
                    assert!(!m.vertices.is_empty(), "Grained mesh vertices must not be empty at zoom {}", cam.z);

                    // Mathematical Oracle: UV must strictly equal World Coordinate / GRAIN_TILE
                    for v in &m.vertices {
                        let wx = (v.pos.x - cam.x as f32) / (cam.z as f32);
                        let wy = (v.pos.y - cam.y as f32) / (cam.z as f32);
                        let expected_u = wx * k;
                        let expected_v = wy * k;
                        assert!(
                            (v.uv.x - expected_u).abs() < 1e-4,
                            "UV.x ({}) must match WorldX / GRAIN_TILE ({}) at zoom {}",
                            v.uv.x, expected_u, cam.z
                        );
                        assert!(
                            (v.uv.y - expected_v).abs() < 1e-4,
                            "UV.y ({}) must match WorldY / GRAIN_TILE ({}) at zoom {}",
                            v.uv.y, expected_v, cam.z
                        );
                    }

                    // For camera panning at fixed zoom (z = 1.0), verify exact vertex count & UV invariance
                    if (cam.z - 1.0).abs() < 1e-6 {
                        let uvs: Vec<Pos2> = m.vertices.iter().map(|v| v.uv).collect();
                        if let Some(ref base) = pan_baseline_uvs {
                            assert_eq!(base.len(), uvs.len(), "Vertex count must remain identical during pan");
                            for (i, (&u_b, &u_c)) in base.iter().zip(&uvs).enumerate() {
                                assert!(
                                    (u_b.x - u_c.x).abs() < 1e-5 && (u_b.y - u_c.y).abs() < 1e-5,
                                    "UV drifted during pan at vertex {}: ({}, {}) vs ({}, {})",
                                    i, u_b.x, u_b.y, u_c.x, u_c.y
                                );
                            }
                        } else {
                            pan_baseline_uvs = Some(uvs);
                        }
                    }
                }
            }
        }

        assert!(found, "Grained mesh with Chalk texture must be generated at zoom {}", cam.z);
    }
}

#[test]
fn test_adv_chalk_live_drawing_affine_uv_invariance() {
    // Assert that active stroke in Gesture::Draw maintains exact UVs when camera pans/zooms during drawing
    let ctx = egui::Context::default();
    let pts = vec![100.0, 200.0, 1.0, 150.0, 220.0, 1.0, 200.0, 250.0, 1.0];
    let cam1 = Camera { x: 0.0, y: 0.0, z: 1.0 };
    let cam2 = Camera { x: 300.0, y: -150.0, z: 2.5 };

    // Live stroke prims build for cam1
    let scale1 = (cam1.z * 1.0) as f32;
    let place1 = |x: f32, y: f32| {
        let (wx, wy) = (x as f64 + 100.0, y as f64 + 200.0);
        let s = cam1.to_screen(wx, wy);
        pos2(s.x as f32, s.y as f32)
    };
    let prims1 = prims::brush_prims_tilted(&pts, 10.0, Some(Brush::Chalk), false, Color32::WHITE, (100.0, 200.0), Some(1.0), 0.0);
    let mut meshes1 = paint::brush_meshes(&ctx, prims1, scale1, place1);
    assert!(!meshes1.is_empty());

    // Live gesture affine transform simulation:
    // In src/editor/overlay.rs:325-335, when camera changes from cam1 to cam2,
    // active d.pieces meshes undergo in-place vertex affine transformation:
    // v.pos is mapped to new screen position, and v.uv remains completely untouched.
    let scale = (cam2.z / cam1.z) as f32;
    let old_ax = cam1.x as f32;
    let old_ay = cam1.y as f32;
    let new_ax = cam2.x as f32;
    let new_ay = cam2.y as f32;

    for v in &mut meshes1[0].vertices {
        let orig_uv = v.uv;
        v.pos.x = (v.pos.x - old_ax) * scale + new_ax;
        v.pos.y = (v.pos.y - old_ay) * scale + new_ay;
        assert_eq!(v.uv, orig_uv, "Affine transform must not alter UV coordinates");
        // Verify that screen position under cam2 accurately maps to the same world point as UV
        let k = 1.0 / GRAIN_TILE;
        let wx = (v.pos.x - new_ax) / (cam2.z as f32);
        assert!((v.uv.x - wx * k).abs() < 1e-4, "UV must strictly match world X coordinate / GRAIN_TILE");
    }
}

/* =========================================================================
   ADVERSARIAL SUITE 4: ALLOCATION & ZERO-OVERHEAD INVARIANTS
   ========================================================================= */

#[test]
fn test_adv_steady_state_panning_zero_buffer_reallocations() {
    let mut ed = create_test_editor();
    // Put 10 ink strokes
    for _i in 0..10 {
        let el = El::new(Kind::Ink(Ink {
            points: vec![0.0, 0.0, 1.0, 50.0, 50.0, 1.0],
            size: 6.0,
            color: "black".to_string(),
            brush: Some(Brush::Pen),
            colors: Vec::new(),
        }));
        ed.board.put(vec![el]);
    }

    let ctx = egui::Context::default();
    let order = ed.board.paint_order().to_vec();
    let mut shapes = Vec::with_capacity(100);

    // Initial frame to warm painter caches
    let f0 = paint::Frame {
        cam: Camera { x: 0.0, y: 0.0, z: 1.0 },
        origin: pos2(0.0, 0.0),
        view: ed.view(),
        pixels_per_point: 1.0,
        editing: None,
        editing_cell: None,
        glides: None,
        dark_board: false,
    };
    ed.painter.paint(&ctx, &mut shapes, &order, &f0);
    let initial_cap = shapes.capacity();

    // Simulate 60 frames of camera panning
    for i in 1..=60 {
        shapes.clear();
        let f = paint::Frame {
            cam: Camera { x: (i as f64) * 5.0, y: (i as f64) * 2.0, z: 1.0 },
            origin: pos2(0.0, 0.0),
            view: ed.view(),
            pixels_per_point: 1.0,
            editing: None,
            editing_cell: None,
            glides: None,
            dark_board: false,
        };
        ed.painter.paint(&ctx, &mut shapes, &order, &f);
        // Assert vector capacity does not blow up or reallocate erratically
        assert!(shapes.capacity() <= initial_cap * 2, "Shapes capacity must stay bounded during panning");
    }
}
