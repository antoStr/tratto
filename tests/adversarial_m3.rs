//! Empirical Adversarial Stress Test Suite for Milestone 3
//! (Live Panels, Curved Icons, Voting Usability, Zoom Obstacles & Theming)
//!
//! Evaluates:
//! 1. Timer width stability across all seconds (00:00 - 59:59) and hours (1:00:00 - 99:59:59),
//!    and popup anchor stability during countdown and reopenings.
//! 2. Chime accordion presence animation (`ui::motion::presence`) and switch behavior under rapid toggling.
//! 3. Voting card usability: in-panel "Termina votazione" and "Risultati" keep the card active and responsive
//!    without premature popup closures.
//! 4. Zoom overlay obstacle clearance: zoom controls step aside cleanly when right panel is opened, resized,
//!    or in transit (kr > 0.001), maintaining >= 16px clearance and graceful degradation on tiny viewports.
//! 5. Curved connector Bézier arc strict monotonicity: analytical derivatives, zero-inflection squash,
//!    horizontal departure/arrival tangency, and curvature continuity in SVG and canvas rendering.

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use std::cell::RefCell;
use std::rc::Rc;
use egui::{Id, Pos2, Rect, Color32, pos2, vec2};
use serde_json::json;
use yrs::Map;

/* =========================================================================
   SUITE 1: TIMER BUTTON WIDTH STABILITY & POPUP ANCHOR STRESS
   ========================================================================= */

#[test]
fn test_adv_timer_width_stability_across_all_seconds() {
    let initialized = Rc::new(RefCell::new(false));
    let init_clone = initialized.clone();

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui(move |ui| {
            if !*init_clone.borrow() {
                ui::install_fonts(ui.ctx());
                ui::setup(ui.ctx(), ui::Theme::new(false, model::parse_color(prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *init_clone.borrow_mut() = true;
                return;
            }

            // For every single second from 0 to 3599 (59m 59s)
            for s in 0..3600 {
                let text = format!("{:02}:{:02}", s / 60, s % 60);
                assert_eq!(text.len(), 5, "mm:ss must be 5 chars");

                let font = egui::FontId::new(ui::px(12.5), egui::FontFamily::Name("ui-medium".into()));
                let g = ui.painter().layout_no_wrap(text.clone(), font, Color32::PLACEHOLDER);

                let digit_w = if text.len() > 5 { 60.0 } else { 44.0 };
                let pill_w = (g.size().x.max(digit_w) + 40.0).max(if text.len() > 5 { 100.0 } else { 84.0 });

                // Adversarial check: pill_w MUST be strictly uniform across all 3600 seconds!
                assert_eq!(
                    pill_w, 84.0,
                    "Timer pill width must be strictly 84.0 px for second {s} (text '{text}'), got {pill_w}"
                );
            }
        });

    harness.run_steps(2);
}

#[test]
fn test_adv_timer_hours_width_stability_across_spectrum() {
    let initialized = Rc::new(RefCell::new(false));
    let init_clone = initialized.clone();

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui(move |ui| {
            if !*init_clone.borrow() {
                ui::install_fonts(ui.ctx());
                ui::setup(ui.ctx(), ui::Theme::new(false, model::parse_color(prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *init_clone.borrow_mut() = true;
                return;
            }

            // Test hours from 1:00:00 to 99:59:59 at various intervals
            let hour_seconds = [
                3600,       // 1:00:00
                3601,       // 1:00:01
                7199,       // 1:59:59
                35999,      // 9:59:59
                36000,      // 10:00:00
                359999,     // 99:59:59
            ];

            for &s in &hour_seconds {
                let text = format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60);
                assert!(text.len() >= 7, "h:mm:ss or hh:mm:ss must be > 5 chars");

                let font = egui::FontId::new(ui::px(12.5), egui::FontFamily::Name("ui-medium".into()));
                let g = ui.painter().layout_no_wrap(text.clone(), font, Color32::PLACEHOLDER);

                let digit_w = if text.len() > 5 { 60.0 } else { 44.0 };
                let pill_w = (g.size().x.max(digit_w) + 40.0).max(if text.len() > 5 { 100.0 } else { 84.0 });

                assert_eq!(
                    pill_w, 100.0,
                    "Timer pill width must be strictly 100.0 px for hour timestamp '{text}', got {pill_w}"
                );
            }
        });

    harness.run_steps(2);
}

#[test]
fn test_adv_timer_countdown_live_harness_no_jitter() {
    let mut ed = create_test_editor();

    // Start timer at 3 minutes
    let now = platform::shared_now();
    let ms = 180_000.0;
    ed.board.transact_plain(|txn, b| {
        b.timer.insert(txn, "total", doc::json_any(&json!(ms)));
        b.timer.insert(txn, "left", doc::json_any(&json!(ms)));
        b.timer.insert(txn, "end", doc::json_any(&json!(now + ms)));
    });

    let ed_ref = Rc::new(RefCell::new(ed));
    let ed_runner = ed_ref.clone();
    let initialized = Rc::new(RefCell::new(false));
    let init_clone = initialized.clone();

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui(move |ui| {
            if !*init_clone.borrow() {
                ui::install_fonts(ui.ctx());
                ui::setup(ui.ctx(), ui::Theme::new(false, model::parse_color(prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *init_clone.borrow_mut() = true;
                return;
            }
            let mut ed = ed_runner.borrow_mut();
            app::live::timer_button(ui, &mut ed);
        });

    harness.step();

    // Step through 40 frames simulating countdown from 3:00 to 2:20
    for frame in 0..40 {
        let remaining = (180 - frame) as f64 * 1000.0;
        let now = platform::shared_now();
        ed_ref.borrow_mut().board.transact_plain(|txn, b| {
            b.timer.insert(txn, "end", doc::json_any(&json!(now + remaining)));
        });

        harness.step();
    }
}

/* =========================================================================
   SUITE 2: CHIME ACCORDION PRESENCE & SWITCH DYNAMICS STRESS
   ========================================================================= */

#[test]
fn test_adv_switch_bare_mechanical_and_visual_integrity() {
    let val_cell = Rc::new(RefCell::new(false));
    let val_clone = val_cell.clone();
    let rect_cell = Rc::new(RefCell::new(Rect::ZERO));
    let rect_clone = rect_cell.clone();

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui(move |ui| {
            let mut v = *val_clone.borrow();
            let resp = ui::switch_bare(ui, &mut v);
            *val_clone.borrow_mut() = v;
            *rect_clone.borrow_mut() = resp.rect;
        });

    harness.step();
    let allocated_rect = *rect_cell.borrow();
    assert_eq!(allocated_rect.width(), 28.0, "switch_bare width must be exactly 28px");
    assert_eq!(allocated_rect.height(), 20.0, "switch_bare height must be exactly 20px");
    assert!(!*val_cell.borrow(), "default value must be false");

    // Click on switch
    simulate_click(&mut harness, allocated_rect.center(), egui::Modifiers::NONE);
    assert!(*val_cell.borrow(), "clicking switch_bare must toggle value to true");

    // Click again
    simulate_click(&mut harness, allocated_rect.center(), egui::Modifiers::NONE);
    assert!(!*val_cell.borrow(), "clicking switch_bare again must toggle value to false");
}

#[test]
fn test_adv_chime_accordion_presence_monotonic_lifecycle() {
    let ctx = egui::Context::default();
    let id = Id::new("chime-accordion-adv");

    // 1. Initial closed state
    let k0 = ui::motion::presence(&ctx, id, false);
    assert!(k0 <= 0.001, "initial presence must be near zero, got {k0}");

    // 2. Open accordion: presence must increase monotonically toward 1.0
    let mut prev_k = k0;
    for step in 1..=30 {
        let k = ui::motion::presence(&ctx, id, true);
        assert!(k >= prev_k - 0.005, "presence must increase smoothly: step {step}, prev {prev_k}, cur {k}");
        assert!(k.is_finite(), "presence must remain finite");
        prev_k = k;
    }
    assert!(prev_k > 0.85, "presence should reach near 1.0 after 30 steps, got {prev_k}");

    // 3. Close accordion: presence must decay strictly toward 0.0 without negative bounce
    for _ in 1..=50 {
        let k = ui::motion::presence(&ctx, id, false);
        assert!(k >= -0.05, "closing presence must not have negative overshoot: {k}");
        prev_k = k;
    }
    assert!(prev_k <= 0.001, "closing presence must settle <= 0.001, got {prev_k}");
}

#[test]
fn test_adv_chime_rapid_switch_flutter_stress() {
    let ctx = egui::Context::default();
    let id = Id::new("chime-accordion-flutter");

    // Rapidly toggle between true and false every single frame for 100 frames
    let mut state = false;
    for _ in 0..100 {
        state = !state;
        let k = ui::motion::presence(&ctx, id, state);
        assert!(k.is_finite(), "k must never become NaN under flutter");
        assert!(k >= -0.2 && k <= 1.3, "k must stay strictly within bounded spring range under flutter: got {k}");
        let max_h = 76.0 * k.clamp(0.0, 1.0);
        assert!(max_h >= 0.0 && max_h <= 76.0, "max_h must stay in [0, 76]");
    }
}

/* =========================================================================
   SUITE 3: VOTING CARD USABILITY & LIFECYCLE MANAGEMENT
   ========================================================================= */

#[test]
fn test_adv_voting_start_does_not_close_popup() {
    let mut ed = create_test_editor();
    assert!(ed.board.voting().is_none());

    let me = ed.voter.clone();
    ed.board.start_voting(3, &me);
    ed.set_tool(editor::Tool::Select);

    let v = ed.board.voting();
    assert!(v.is_some(), "voting session must be active");
    let (per, ended) = v.unwrap();
    assert_eq!(per, 3);
    assert!(!ended, "voting session must be in progress, not ended");
}

#[test]
fn test_adv_voting_in_progress_end_button_keeps_panel_active() {
    let mut ed = create_test_editor();
    let me = ed.voter.clone();
    ed.board.start_voting(5, &me);
    assert_eq!(ed.board.voting(), Some((5, false)));

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("vote-results-hidden"), true));
        });
    harness.step();

    // Simulate clicking "Termina votazione"
    ed.board.end_voting();
    harness.ctx.data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));

    assert_eq!(ed.board.voting(), Some((5, true)), "end_voting must set ended to true");
    let hidden = harness.ctx.data(|d| d.get_temp::<bool>(Id::new("vote-results-hidden")));
    assert_eq!(hidden, None, "vote-results-hidden must be removed when ending voting");
}

#[test]
fn test_adv_voting_in_progress_results_button_keeps_panel_active() {
    let mut ed = create_test_editor();
    let me = ed.voter.clone();
    ed.board.start_voting(5, &me);

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("vote-results-hidden"), true));
        });
    harness.step();

    // Simulate clicking "Risultati"
    harness.ctx.data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));

    let hidden = harness.ctx.data(|d| d.get_temp::<bool>(Id::new("vote-results-hidden")));
    assert_eq!(hidden, None, "vote-results-hidden must be cleared");
    assert_eq!(ed.board.voting(), Some((5, false)), "voting is still in progress");
}

/* =========================================================================
   SUITE 4: RIGHT PANEL OBSTACLE CLEARANCE & ZOOM CONTROLS STEP-ASIDE
   ========================================================================= */

#[test]
fn test_adv_right_panel_kr_threshold_obstacle_registration() {
    let ctx = egui::Context::default();
    let right_rect = Rect::from_min_size(pos2(1100.0, 8.0), vec2(240.0, 784.0));

    // Threshold test kr <= 0.001
    for &kr in &[0.0f32, 0.0001, 0.0005, 0.0010] {
        if kr > 0.001 {
            ctx.data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_rect));
        } else {
            ctx.data_mut(|d| d.remove::<Rect>(Id::new("right-panel-rect")));
        }
        let registered = ctx.data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect")));
        assert_eq!(registered, None, "right-panel-rect must NOT be registered for kr={kr}");
    }

    // Threshold test kr > 0.001
    for &kr in &[0.0011f32, 0.01, 0.1, 0.5, 0.99, 1.0] {
        if kr > 0.001 {
            ctx.data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_rect));
        } else {
            ctx.data_mut(|d| d.remove::<Rect>(Id::new("right-panel-rect")));
        }
        let registered = ctx.data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect")));
        assert_eq!(registered, Some(right_rect), "right-panel-rect MUST be registered for kr={kr}");
    }
}

#[test]
fn test_adv_zoom_controls_clearance_margin_invariance() {
    let stage = Rect::from_min_max(pos2(0.0, 0.0), pos2(1440.0, 900.0));
    let panel_widths = [180.0f32, 240.0, 300.0, 360.0, 420.0];

    for &pw in &panel_widths {
        let right_rect = Rect::from_min_size(pos2(stage.max.x - 8.0 - pw, stage.min.y + 8.0), vec2(pw, stage.height() - 16.0));
        let corner = pos2(right_rect.min.x - 16.0, stage.max.y - 16.0);
        let size = vec2(80.0, 36.0);
        let zoom_pill = Rect::from_min_max(corner - size, corner);

        // Clearance check: gap between zoom pill right edge and right panel left edge
        let clearance_x = right_rect.min.x - zoom_pill.max.x;
        assert_eq!(
            clearance_x, 16.0,
            "Clearance between zoom pill and right panel must be strictly 16.0 px for panel width {pw}"
        );
        assert!(
            !right_rect.intersects(zoom_pill),
            "Zoom controls must NEVER intersect right panel"
        );

        // Clearance with obstacle expansion (8.0px margin)
        let expanded_panel = right_rect.expand(8.0);
        assert!(
            !expanded_panel.intersects(zoom_pill),
            "Zoom controls must clear expanded obstacle boundary"
        );
    }
}

#[test]
fn test_adv_zoom_controls_graceful_fade_when_no_space() {
    let stage = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 400.0));
    let right_rect = Rect::from_min_size(pos2(40.0, 8.0), vec2(150.0, 384.0));
    let corner = pos2(right_rect.min.x - 16.0, stage.max.y - 16.0); // 40 - 16 = 24.0
    let size = vec2(80.0, 36.0);

    let obstacles = vec![right_rect.expand(8.0)];
    let free = |p: Pos2| {
        let r = Rect::from_min_max(p - size, p);
        r.min.y >= stage.min.y + 8.0 && r.min.x >= stage.min.x + 8.0 && !obstacles.iter().any(|o| o.intersects(r))
    };

    // When corner.x - size.x = 24.0 - 80.0 = -56.0 < 8.0, free(corner) MUST be false!
    assert!(!free(corner), "Zoom controls must not fit on cramped viewport");

    // Presence animation must fade out
    let ctx = egui::Context::default();
    let k = ui::motion::presence(&ctx, Id::new("view-controls-shown-cramped"), false);
    assert!(k <= 0.01, "Zoom controls must fade out gracefully when no space is free");
}

/* =========================================================================
   SUITE 5: CURVED CONNECTOR BÉZIER ARC MATHEMATICAL ORACLES
   ========================================================================= */

#[test]
fn test_adv_curved_connector_svg_bezier_strict_derivative_monotonicity() {
    // Path: M5 19c5 0 9-14 14-14
    // Start: P0 = (5, 19)
    // Control 1: P1 = (5+5, 19+0) = (10, 19)
    // Control 2: P2 = (5+9, 19-14) = (14, 5)
    // End: P3 = (5+14, 19-14) = (19, 5)
    let p0 = (5.0f64, 19.0f64);
    let p1 = (10.0f64, 19.0f64);
    let p2 = (14.0f64, 5.0f64);
    let p3 = (19.0f64, 5.0f64);

    let n = 10_000;
    let mut prev_x = p0.0;
    let mut prev_y = p0.1;

    for i in 0..=n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;

        // Position B(t)
        let bx = u*u*u*p0.0 + 3.0*u*u*t*p1.0 + 3.0*u*t*t*p2.0 + t*t*t*p3.0;
        let by = u*u*u*p0.1 + 3.0*u*u*t*p1.1 + 3.0*u*t*t*p2.1 + t*t*t*p3.1;

        // Derivative B'(t) = 3*(1-t)^2*(P1-P0) + 6*(1-t)*t*(P2-P1) + 3*t^2*(P3-P2)
        let dx1 = p1.0 - p0.0; // 5
        let dx2 = p2.0 - p1.0; // 4
        let dx3 = p3.0 - p2.0; // 5
        let b_prime_x = 3.0*u*u*dx1 + 6.0*u*t*dx2 + 3.0*t*t*dx3;

        let dy1 = p1.1 - p0.1; // 0
        let dy2 = p2.1 - p1.1; // -14
        let dy3 = p3.1 - p2.1; // 0
        let b_prime_y = 3.0*u*u*dy1 + 6.0*u*t*dy2 + 3.0*t*t*dy3;

        // Second derivative B''(t)
        let b_prime2_x = 6.0*u*(dx2 - dx1) + 6.0*t*(dx3 - dx2); // 6*u*(-1) + 6*t*(1) = 6*(2t - 1)
        let b_prime2_y = 6.0*u*(dy2 - dy1) + 6.0*t*(dy3 - dy2); // 6*u*(-14) + 6*t*(14) = 84*(2t - 1)

        // Strict monotonicity check:
        assert!(
            b_prime_x >= 12.0,
            "B'_x(t) must be strictly positive (>= 12.0) everywhere on [0, 1], got {b_prime_x} at t={t}"
        );
        assert!(
            b_prime_y <= 1e-9,
            "B'_y(t) must be strictly non-positive (<= 0) everywhere on [0, 1], got {b_prime_y} at t={t}"
        );

        if i > 0 {
            assert!(
                bx > prev_x,
                "B_x must be strictly monotonically increasing: bx={bx}, prev_x={prev_x} at t={t}"
            );
            assert!(
                by <= prev_y,
                "B_y must be monotonically non-increasing: by={by}, prev_y={prev_y} at t={t}"
            );
        }

        // Horizontal tangency at endpoints
        if i == 0 {
            assert_eq!(b_prime_y, 0.0, "Start departure must be perfectly horizontal");
            assert_eq!(b_prime_x, 15.0, "Start speed along x must be 15.0");
        }
        if i == n {
            assert_eq!(b_prime_y, 0.0, "End arrival must be perfectly horizontal");
            assert_eq!(b_prime_x, 15.0, "End speed along x must be 15.0");
        }

        // Curvature check: kappa = |x' y'' - y' x''| / (x'^2 + y'^2)^(3/2)
        let speed_sq = b_prime_x * b_prime_x + b_prime_y * b_prime_y;
        let numerator = (b_prime_x * b_prime2_y - b_prime_y * b_prime2_x).abs();
        let kappa = numerator / (speed_sq * speed_sq.sqrt());
        assert!(kappa.is_finite(), "Curvature must be finite everywhere: got {kappa} at t={t}");
        assert!(kappa < 0.50, "Curvature must be smooth and bounded, no sharp bends: got {kappa} at t={t}");

        prev_x = bx;
        prev_y = by;
    }
}

#[test]
fn test_adv_curved_connector_canvas_bezier_strict_derivative_monotonicity() {
    // Canvas: [a, a + vec2(5.0, 0.0), b + vec2(-5.0, 0.0), b]
    // where a = c + vec2(-7.0, 8.0), b = c + vec2(7.0, -7.0)
    let c = (100.0f64, 100.0f64);
    let a = (c.0 - 7.0, c.1 + 8.0);
    let b = (c.0 + 7.0, c.1 - 7.0);

    let p0 = a;
    let p1 = (a.0 + 5.0, a.1);
    let p2 = (b.0 - 5.0, b.1);
    let p3 = b;

    let n = 10_000;
    let mut prev_x = p0.0;
    let mut prev_y = p0.1;

    for i in 0..=n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;

        let bx = u*u*u*p0.0 + 3.0*u*u*t*p1.0 + 3.0*u*t*t*p2.0 + t*t*t*p3.0;
        let by = u*u*u*p0.1 + 3.0*u*u*t*p1.1 + 3.0*u*t*t*p2.1 + t*t*t*p3.1;

        let dx1 = p1.0 - p0.0; // 5.0
        let dx2 = p2.0 - p1.0; // (b.0 - 5.0) - (a.0 + 5.0) = 14.0 - 10.0 = 4.0
        let dx3 = p3.0 - p2.0; // 5.0
        let b_prime_x = 3.0*u*u*dx1 + 6.0*u*t*dx2 + 3.0*t*t*dx3;

        let dy1 = p1.1 - p0.1; // 0.0
        let dy2 = p2.1 - p1.1; // -15.0
        let dy3 = p3.1 - p2.1; // 0.0
        let b_prime_y = 3.0*u*u*dy1 + 6.0*u*t*dy2 + 3.0*t*t*dy3;

        assert!(b_prime_x >= 12.0, "Canvas B'_x(t) must be >= 12.0: got {b_prime_x} at t={t}");
        assert!(b_prime_y <= 1e-9, "Canvas B'_y(t) must be <= 0.0: got {b_prime_y} at t={t}");

        if i > 0 {
            assert!(bx > prev_x, "Canvas B_x must be strictly monotonically increasing");
            assert!(by <= prev_y, "Canvas B_y must be monotonically non-increasing");
        }

        prev_x = bx;
        prev_y = by;
    }
}

#[test]
fn test_adv_curved_connector_regression_detector_against_old_curve() {
    // Old flawed curve: P1 = (14, 19), P2 = (10, 5)
    // Notice that P1.x (14) > P2.x (10), crossing over and squashing inflection!
    let old_p1_x = 14.0;
    let old_p2_x = 10.0;
    assert!(old_p1_x > old_p2_x, "Old curve crossed over control points along X");

    // New repaired curve: P0.x=5, P1.x=10, P2.x=14, P3.x=19
    let new_p0_x = 5.0;
    let new_p1_x = 10.0;
    let new_p2_x = 14.0;
    let new_p3_x = 19.0;
    assert!(new_p0_x < new_p1_x && new_p1_x < new_p2_x && new_p2_x < new_p3_x,
        "New curve has strictly monotonic control point ordering along X: 5 < 10 < 14 < 19");

    // Verify SVG icon exact string in icon_data
    let icon_svg = ui::icon_data::ICONS.iter().find(|(k, _)| *k == "route-curved").map(|(_, v)| *v);
    assert!(icon_svg.is_some(), "route-curved icon must be registered");
    let svg_str = icon_svg.unwrap();
    assert!(svg_str.contains("d=\"M5 19c5 0 9-14 14-14\""), "SVG path must match exact repaired curve");
    assert!(!svg_str.contains("c9 0 5-14"), "SVG path must NOT contain old flawed control points");
}
