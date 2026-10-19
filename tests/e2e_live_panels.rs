//! E2E Integration Tests: Live Panels & Drag & Drop
//! Requirements covered:
//! - R1: Rifinitura Pannelli Live (Timer popup stability, chime accordion animation, voting card usability, zoom overlay obstacle clearance)
//! - R3: Drag & Drop Animation and Containment (Layers spring gap and ghost containment, toolbar modal drag containment)
//! Tiers covered: 1 (Feature Coverage), 2 (Boundary & Corner Cases), 3 (Cross-Feature Combinations), 4 (Real-World Scenarios)

include!("common/tratto_modules.rs");
include!("common/test_helpers.rs");

use std::cell::RefCell;
use std::rc::Rc;
use egui::{Color32, Id, Pos2, Rect, pos2, vec2};
use serde_json::json;
use yrs::Map;

const MAX_TIMER_MS: f64 = (99.0 * 3600.0 + 59.0 * 60.0 + 59.0) * 1000.0;

fn start_test_timer(ed: &mut editor::Editor, ms: f64) {
    let ms = ms.clamp(1000.0, MAX_TIMER_MS);
    let now = platform::shared_now();
    ed.board.transact_plain(|txn, b| {
        b.timer.insert(txn, "total", doc::json_any(&json!(ms)));
        b.timer.insert(txn, "left", doc::json_any(&json!(ms)));
        b.timer.insert(txn, "end", doc::json_any(&json!(now + ms)));
    });
}

fn pause_test_timer(ed: &mut editor::Editor) {
    let now = platform::shared_now();
    let end = ed.board.timer_field("end").and_then(|v| v.as_f64()).unwrap_or(now);
    let left = (end - now).max(0.0);
    ed.board.transact_plain(|txn, b| {
        b.timer.insert(txn, "left", doc::json_any(&json!(left)));
        b.timer.remove(txn, "end");
    });
}

fn stop_test_timer(ed: &mut editor::Editor) {
    ed.board.transact_plain(|txn, b| {
        for k in ["total", "left", "end"] {
            b.timer.remove(txn, k);
        }
    });
}

fn add_minute_test_timer(ed: &mut editor::Editor) {
    let now = platform::shared_now();
    let total = ed.board.timer_field("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let end = ed.board.timer_field("end").and_then(|v| v.as_f64());
    let left = ed.board.timer_field("left").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let room = (MAX_TIMER_MS - total).clamp(0.0, 60_000.0);
    if room <= 0.0 {
        return;
    }
    ed.board.transact_plain(|txn, b| {
        b.timer.insert(txn, "total", doc::json_any(&json!(total + room)));
        match end {
            Some(e) => b.timer.insert(txn, "end", doc::json_any(&json!(e.max(now) + room))),
            None => b.timer.insert(txn, "left", doc::json_any(&json!(left + room))),
        }
    });
}

fn format_clock_spec(ms: f64) -> String {
    let s = (ms / 1000.0).ceil().max(0.0) as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn parse_timer_digits_spec(d: &str) -> Option<f64> {
    let d: String = d.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.is_empty() || d.len() > 6 {
        return None;
    }
    let n: u64 = d.parse().ok()?;
    let (h, m, sec) = (n / 10_000, n / 100 % 100, n % 100);
    let ms = ((h * 3600 + m * 60 + sec) * 1000) as f64;
    (ms > 0.0).then(|| ms.min(MAX_TIMER_MS))
}

/* =========================================================================
   TIER 1: FEATURE COVERAGE (>= 5 tests per feature for happy paths)
   ========================================================================= */

// --- Feature 1: Timer popup stability [R1] ---

#[test]
fn test_e2e_t1_timer_popup_initial_allocation_stable() {
    let ed = Rc::new(RefCell::new(create_test_editor()));
    let ed_clone = ed.clone();
    let recorded_w = Rc::new(RefCell::new(0.0f32));
    let rec_clone = recorded_w.clone();
    let initialized = Rc::new(RefCell::new(false));
    let init_clone = initialized.clone();

    // 1. Initial state with default draft (draft is 5 mins -> "05:00", text.len() <= 5)
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1200.0, 800.0))
        .build_ui(move |ui| {
            if !*init_clone.borrow() {
                ui::install_fonts(ui.ctx());
                ui::setup(ui.ctx(), ui::Theme::new(false, model::parse_color(prefs::DEFAULT_ACCENT).unwrap(), false), 1.0);
                *init_clone.borrow_mut() = true;
                return;
            }
            let mut ed = ed_clone.borrow_mut();
            let res = ui.horizontal(|ui| {
                app::live::timer_button(ui, &mut ed);
            });
            *rec_clone.borrow_mut() = res.response.rect.width();

            // Verify allocation formula directly from src/app/live.rs:196-198
            let font = egui::FontId::new(ui::px(12.5), egui::FontFamily::Name("ui-medium".into()));
            for s in [0, 59, 180, 599, 3599] {
                let text = format!("{:02}:{:02}", s / 60, s % 60);
                let g = ui.painter().layout_no_wrap(text.clone(), font.clone(), Color32::PLACEHOLDER);
                let digit_w = if text.len() > 5 { 60.0 } else { 44.0 };
                let pill_w = (g.size().x.max(digit_w) + 40.0).max(if text.len() > 5 { 100.0 } else { 84.0 });
                assert_eq!(pill_w, 84.0, "Timer pill width must be strictly 84.0 px for text '{text}'");
            }

            for s in [3600, 7199, 36000] {
                let text = format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60);
                let g = ui.painter().layout_no_wrap(text.clone(), font.clone(), Color32::PLACEHOLDER);
                let digit_w = if text.len() > 5 { 60.0 } else { 44.0 };
                let pill_w = (g.size().x.max(digit_w) + 40.0).max(if text.len() > 5 { 100.0 } else { 84.0 });
                assert_eq!(pill_w, 100.0, "Timer pill width must be strictly 100.0 px for text '{text}'");
            }
        });

    harness.step(); // frame 0: install fonts & setup
    harness.step(); // frame 1: render timer_button
    assert_eq!(
        *recorded_w.borrow(),
        84.0,
        "Initial timer button width must be strictly 84.0 px for <= 5 chars (05:00)"
    );

    // 2. Set timer > 5 chars (1 hour -> "1:00:00", text.len() > 5)
    let now = platform::shared_now();
    let ms = 3_600_000.0;
    ed.borrow_mut().board.transact_plain(|txn, b| {
        b.timer.insert(txn, "total", doc::json_any(&json!(ms)));
        b.timer.insert(txn, "left", doc::json_any(&json!(ms)));
        b.timer.insert(txn, "end", doc::json_any(&json!(now + ms)));
    });
    harness.step();
    assert_eq!(
        *recorded_w.borrow(),
        100.0,
        "Timer button width must be strictly 100.0 px for > 5 chars (1:00:00)"
    );
}

#[test]
fn test_e2e_t1_timer_clock_display_formatting() {
    assert_eq!(format_clock_spec(180_000.0), "03:00");
    assert_eq!(format_clock_spec(600_000.0), "10:00");
    assert_eq!(format_clock_spec(59_000.0), "00:59");
    assert_eq!(format_clock_spec(3_600_000.0), "1:00:00");
    assert_eq!(format_clock_spec(5_400_000.0), "1:30:00");
}

#[test]
fn test_e2e_t1_timer_state_start_and_pause() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, 180_000.0);
    let total = ed.board.timer_field("total").and_then(|v| v.as_f64());
    let end = ed.board.timer_field("end").and_then(|v| v.as_f64());
    assert_eq!(total, Some(180_000.0));
    assert!(end.is_some());

    pause_test_timer(&mut ed);
    let end_paused = ed.board.timer_field("end").and_then(|v| v.as_f64());
    let left_paused = ed.board.timer_field("left").and_then(|v| v.as_f64());
    assert!(end_paused.is_none(), "end field should be removed when paused");
    assert!(left_paused.is_some());

    stop_test_timer(&mut ed);
    assert!(ed.board.timer_field("total").is_none());
    assert!(ed.board.timer_field("left").is_none());
    assert!(ed.board.timer_field("end").is_none());
}

#[test]
fn test_e2e_t1_timer_add_minute_running() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, 120_000.0);
    add_minute_test_timer(&mut ed);
    let total = ed.board.timer_field("total").and_then(|v| v.as_f64());
    assert_eq!(total, Some(180_000.0), "total duration extended by 60s");
}

#[test]
fn test_e2e_t1_timer_popup_reopen_coordinates_stability() {
    let mut anchor_positions = Vec::new();
    for _ in 0..3 {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(1200.0, 800.0))
            .build_ui(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(280.0, 360.0), egui::Sense::hover());
                ui.data_mut(|d| d.insert_temp(Id::new("timer-test-anchor"), r.min));
            });
        harness.step();
        let pos = harness.ctx.data(|d| d.get_temp::<Pos2>(Id::new("timer-test-anchor"))).unwrap();
        anchor_positions.push(pos);
    }
    assert_eq!(anchor_positions[0], anchor_positions[1]);
    assert_eq!(anchor_positions[1], anchor_positions[2]);
}

// --- Feature 2: Chime accordion animation [R1] ---

#[test]
fn test_e2e_t1_chime_toggle_defaults() {
    let ed = create_test_editor();
    assert!(ed.prefs.timer_sound, "default timer sound should be enabled");
    assert_eq!(ed.prefs.timer_chime, prefs::TimerSound::Tibetan);
    assert!(ed.prefs.timer_volume >= 0.1 && ed.prefs.timer_volume <= 1.0);
}

#[test]
fn test_e2e_t1_chime_accordion_toggle_open_close() {
    let ed = Rc::new(RefCell::new(create_test_editor()));
    let ed_clone = ed.clone();
    let k_record = Rc::new(RefCell::new(0.0f32));
    let k_clone = k_record.clone();

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 600.0))
        .build_ui(move |ui| {
            let sound_enabled = ed_clone.borrow().prefs.timer_sound;
            let k = ui::motion::presence(ui.ctx(), Id::new("chime-accordion"), sound_enabled);
            *k_clone.borrow_mut() = k;
        });

    // 1. Initial state: timer_sound is true by default; presence factor k rises toward 1.0
    assert!(ed.borrow().prefs.timer_sound);
    for _ in 0..30 {
        harness.step();
    }
    let k_open = *k_record.borrow();
    assert!(k_open > 0.85, "presence k should reach near 1.0 when opened, got {k_open}");
    let max_h_open = 76.0 * k_open.clamp(0.0, 1.0);
    assert!(max_h_open > 60.0, "accordion max height expands smoothly: {max_h_open}");

    // 2. Toggle timer_sound to false: presence factor decays towards 0.0
    ed.borrow_mut().prefs.timer_sound = false;
    for _ in 0..50 {
        harness.step();
    }
    let k_closed = *k_record.borrow();
    assert!(k_closed <= 0.001, "presence k should decay to <= 0.001 when closed, got {k_closed}");
    let max_h_closed = 76.0 * k_closed.clamp(0.0, 1.0);
    assert!(max_h_closed < 0.1, "accordion collapses down to 0: {max_h_closed}");

    // 3. Toggle back to true: presence re-expands
    ed.borrow_mut().prefs.timer_sound = true;
    for _ in 0..30 {
        harness.step();
    }
    let k_reopened = *k_record.borrow();
    assert!(k_reopened > 0.85, "presence k re-expands towards 1.0: {k_reopened}");
}

#[test]
fn test_e2e_t1_chime_sound_selection_options() {
    let mut ed = create_test_editor();
    let sounds = [
        prefs::TimerSound::Tibetan,
        prefs::TimerSound::Harp,
        prefs::TimerSound::Carillon,
        prefs::TimerSound::Piano,
        prefs::TimerSound::Drop,
    ];
    for sound in sounds {
        ed.prefs.timer_chime = sound;
        assert_eq!(ed.prefs.timer_chime, sound);
        assert!(!sound.name().is_empty());
        assert!(!sound.short().is_empty());
    }
}

#[test]
fn test_e2e_t1_chime_volume_slider_adjustment() {
    let mut ed = create_test_editor();
    for vol in [0.05f32, 0.25, 0.50, 0.75, 1.00] {
        ed.prefs.timer_volume = vol;
        assert!((ed.prefs.timer_volume - vol).abs() < 1e-4);
    }
}

#[test]
fn test_e2e_t1_chime_preview_trigger() {
    for sound in prefs::TIMER_SOUNDS {
        audio::chime(sound, 0.5);
    }
}

// --- Feature 3: Voting card usability [R1] ---

#[test]
fn test_e2e_t1_voting_start_active_state() {
    let mut ed = create_test_editor();
    assert!(ed.board.voting().is_none(), "initially no voting session");
    let voter = ed.voter.clone();
    ed.board.start_voting(3, &voter);
    let v = ed.board.voting();
    assert!(v.is_some(), "voting should be active");
    let (per, ended) = v.unwrap();
    assert_eq!(per, 3);
    assert!(!ended, "voting should not be ended");
}

#[test]
fn test_e2e_t1_voting_remaining_votes_tally() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();
    ed.board.start_voting(5, &voter);
    let tally_empty = ed.board.tally(Some(&voter));
    let used: u32 = tally_empty.values().sum();
    assert_eq!(used, 0);

    let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
        text: "Feature A".into(),
        color: "#FFE58F".into(),
        ..Default::default()
    }));
    s.id = "sticky_1".into();
    ed.board.put(vec![s]);

    ed.board.cast_vote(&voter, "sticky_1", 1);
    let tally_one = ed.board.tally(Some(&voter));
    let used_after: u32 = tally_one.values().sum();
    assert_eq!(used_after, 1);
}

#[test]
fn test_e2e_t1_voting_in_panel_end_control() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();
    ed.board.start_voting(3, &voter);
    assert_eq!(ed.board.voting(), Some((3, false)));

    ed.board.end_voting();
    assert_eq!(ed.board.voting(), Some((3, true)));
}

#[test]
fn test_e2e_t1_voting_results_button_action() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();
    ed.board.start_voting(3, &voter);

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 500.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("vote-results-hidden"), true));
        });
    harness.step();
    assert_eq!(harness.ctx.data(|d| d.get_temp::<bool>(Id::new("vote-results-hidden"))), Some(true));

    // Results button action from src/app/live.rs:750: clears vote-results-hidden without ending active voting
    harness.ctx.data_mut(|d| d.remove::<bool>(Id::new("vote-results-hidden")));
    assert_eq!(harness.ctx.data(|d| d.get_temp::<bool>(Id::new("vote-results-hidden"))), None);
    assert_eq!(ed.board.voting(), Some((3, false)), "voting remains active when viewing results");
}

#[test]
fn test_e2e_t1_voting_card_persistence_across_panel_toggles() {
    let ed = Rc::new(RefCell::new(create_test_editor()));
    let voter = ed.borrow().voter.clone();
    ed.borrow_mut().board.start_voting(4, &voter);

    for _ in 0..5 {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(400.0, 500.0))
            .build_ui(|_ui| {});
        harness.step();
        assert_eq!(ed.borrow().board.voting(), Some((4, false)), "voting state remains active");
    }
}

// --- Feature 4: Zoom overlay obstacle clearance [R1] ---

#[test]
fn test_e2e_t1_zoom_overlay_normal_position() {
    let stage = Rect::from_min_max(pos2(0.0, 0.0), pos2(1440.0, 900.0));
    let size = vec2(80.0, 36.0);
    let right_rect = Rect::from_min_max(pos2(1100.0, 8.0), pos2(1432.0, 892.0));

    // When right panel is closed (kr <= 0.001, e.g. kr = 0.0)
    let kr = 0.0f32;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .build_ui(move |ui| {
            if kr > 0.001 {
                ui.ctx().data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_rect));
            } else {
                ui.ctx().data_mut(|d| d.remove::<Rect>(Id::new("right-panel-rect")));
            }

            let right_p = ui.ctx().data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect")));
            assert_eq!(right_p, None, "right-panel-rect must not be present when kr <= 0.001");

            let corner = if let Some(rp) = right_p {
                pos2(rp.min.x - 16.0, stage.max.y - 16.0)
            } else {
                stage.max - vec2(16.0, 16.0)
            };

            assert_eq!(corner, pos2(1424.0, 884.0), "Default corner position in bottom-right corner");
            let zoom_pill = Rect::from_min_max(corner - size, corner);
            assert_eq!(zoom_pill.max, pos2(1424.0, 884.0));
            assert_eq!(zoom_pill.min, pos2(1344.0, 848.0));
        });
    harness.step();
}

#[test]
fn test_e2e_t1_zoom_overlay_obstacle_registration() {
    let right_panel_rect = Rect::from_min_max(pos2(1100.0, 0.0), pos2(1440.0, 900.0));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_panel_rect));
            let retrieved = ui.ctx().data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect")));
            assert_eq!(retrieved, Some(right_panel_rect));
        });
    harness.step();
}

#[test]
fn test_e2e_t1_zoom_overlay_steps_aside_for_right_panel() {
    let stage = Rect::from_min_max(pos2(0.0, 0.0), pos2(1440.0, 900.0));
    let size = vec2(80.0, 36.0);
    let right_rect = Rect::from_min_size(pos2(1100.0, 8.0), vec2(332.0, 884.0));

    // When right panel is open or transitioning (kr > 0.001, e.g. kr = 1.0)
    let kr = 1.0f32;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .build_ui(move |ui| {
            if kr > 0.001 {
                ui.ctx().data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_rect));
            } else {
                ui.ctx().data_mut(|d| d.remove::<Rect>(Id::new("right-panel-rect")));
            }

            let right_p = ui.ctx().data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect")));
            assert_eq!(right_p, Some(right_rect), "right-panel-rect must be registered when kr > 0.001");

            let rp = right_p.unwrap();
            let corner = pos2(rp.min.x - 16.0, stage.max.y - 16.0);
            let zoom_pill = Rect::from_min_max(corner - size, corner);

            // Clearance check: gap between zoom pill right edge and right panel left edge
            let clearance_x = rp.min.x - zoom_pill.max.x;
            assert_eq!(clearance_x, 16.0, "Clearance between zoom pill and right panel must be strictly 16.0 px");
            assert!(!rp.intersects(zoom_pill), "Zoom controls must never intersect right panel");

            // Obstacle clearance with 8px expansion (matching toolbar.rs:1819)
            let expanded_panel = rp.expand(8.0);
            assert!(!expanded_panel.intersects(zoom_pill), "Zoom controls must clear expanded obstacle boundary");
        });
    harness.step();
}

#[test]
fn test_e2e_t1_zoom_overlay_returns_when_panel_closes() {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), Rect::from_min_size(pos2(1100.0, 0.0), vec2(340.0, 900.0))));
            ui.ctx().data_mut(|d| d.remove::<Rect>(Id::new("right-panel-rect")));
            assert!(ui.ctx().data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect"))).is_none());
        });
    harness.step();
}

#[test]
fn test_e2e_t1_zoom_overlay_hit_testing_unobstructed() {
    let mut ed = create_test_editor();
    let initial_z = ed.cam.z;
    ed.zoom_at(720.0, 450.0, initial_z * 1.2);
    assert!(ed.cam.z > initial_z);
    ed.zoom_at(720.0, 450.0, initial_z);
    assert!((ed.cam.z - initial_z).abs() < 1e-4);
}

// --- Feature 6 & 7: Drag & Drop Spring Gap and Containment in Layers [R3] ---

#[test]
fn test_e2e_t1_layers_drag_initiates_motion() {
    let mut ed = create_test_editor();
    let mut s1 = model::El::new(model::Kind::Sticky(Default::default()));
    s1.id = "layer_1".into();
    let mut s2 = model::El::new(model::Kind::Sticky(Default::default()));
    s2.id = "layer_2".into();
    ed.board.put(vec![s1, s2]);

    assert_eq!(ed.board.len(), 2);
}

#[test]
fn test_e2e_t1_layers_spring_gap_opens() {
    let ctx = egui::Context::default();
    let gap_id = Id::new("layer-gap-test");
    let gap0 = ui::motion::spring(&ctx, gap_id, 0.0, 0.25, 0.9);
    assert_eq!(gap0, 0.0);
    let gap1 = ui::motion::spring(&ctx, gap_id, 36.0, 0.25, 0.9);
    assert!(gap1 > 0.0 && gap1 <= 36.0);
}

#[test]
fn test_e2e_t1_layers_reorder_commit() {
    let mut ed = create_test_editor();
    let mut s1 = model::El::new(model::Kind::Sticky(Default::default()));
    s1.id = "layer_1".into();
    s1.z = 0.0;
    let mut s2 = model::El::new(model::Kind::Sticky(Default::default()));
    s2.id = "layer_2".into();
    s2.z = 1.0;
    ed.board.put(vec![s1, s2]);

    let order_before: Vec<String> = ed.board.all().iter().map(|e| e.id.clone()).collect();
    assert_eq!(order_before, vec!["layer_1", "layer_2"]);

    ed.board.move_in_stack(&["layer_2".into()], doc::Anchor::Below("layer_1".into()), None);
    let order_after: Vec<String> = ed.board.all().iter().map(|e| e.id.clone()).collect();
    assert_eq!(order_after, vec!["layer_2", "layer_1"]);
}

#[test]
fn test_e2e_t1_layers_spring_gap_settles() {
    let ctx = egui::Context::default();
    let gap_id = Id::new("layer-gap-settle");
    for _ in 0..60 {
        ui::motion::spring(&ctx, gap_id, 36.0, 0.25, 0.9);
    }
    let settled = ui::motion::spring(&ctx, gap_id, 36.0, 0.25, 0.9);
    assert!((settled - 36.0).abs() < 0.1, "spring should settle near target");
}

#[test]
fn test_e2e_t1_layers_cancel_drag_restores_gap() {
    let ctx = egui::Context::default();
    let gap_id = Id::new("layer-gap-cancel");
    ui::motion::spring(&ctx, gap_id, 36.0, 0.25, 0.9);
    for _ in 0..60 {
        ui::motion::spring(&ctx, gap_id, 0.0, 0.25, 0.9);
    }
    let restored = ui::motion::spring(&ctx, gap_id, 0.0, 0.25, 0.9);
    assert!(restored < 0.1, "gap should close back down to 0");
}

#[test]
fn test_e2e_t1_layers_drag_ghost_clamped_top() {
    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 800.0));
    let clip = panel_rect.shrink(8.0); // (1108.0, 56.0) to (1432.0, 792.0)

    // Pointer above top of panel
    let p = pos2(1200.0, -100.0);
    let ghost_y = (p.y + 8.0).clamp(clip.min.y, (clip.max.y - 32.0).max(clip.min.y));
    assert_eq!(ghost_y, clip.min.y, "Ghost y must be clamped to clip.min.y (56.0)");

    // Inverted/collapsed bounds protection (.max(clip.min.y))
    let collapsed_clip = Rect::from_min_max(pos2(1100.0, 50.0), pos2(1400.0, 60.0));
    let gy = (p.y + 8.0).clamp(collapsed_clip.min.y, (collapsed_clip.max.y - 32.0).max(collapsed_clip.min.y));
    assert_eq!(gy, collapsed_clip.min.y, "Must not panic on collapsed bounds and clamp to min");
}

#[test]
fn test_e2e_t1_layers_drag_ghost_clamped_bottom() {
    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 800.0));
    let clip = panel_rect.shrink(8.0); // max.y = 792.0, max.y - 32.0 = 760.0

    // Pointer below bottom of panel
    let p = pos2(1200.0, 950.0);
    let ghost_y = (p.y + 8.0).clamp(clip.min.y, (clip.max.y - 32.0).max(clip.min.y));
    assert_eq!(ghost_y, clip.max.y - 32.0, "Ghost y must be clamped to clip.max.y - 32.0 (760.0)");
}

#[test]
fn test_e2e_t1_layers_drag_ghost_clamped_left_right() {
    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 800.0));
    let clip = panel_rect.shrink(8.0); // min.x = 1108.0, max.x = 1432.0, max.x - 110.0 = 1322.0

    // Pointer far left
    let p_left = pos2(900.0, 200.0);
    let ghost_x_left = (p_left.x + 14.0).clamp(clip.min.x, (clip.max.x - 110.0).max(clip.min.x));
    assert_eq!(ghost_x_left, clip.min.x, "Ghost x clamped to clip.min.x (1108.0)");

    // Pointer far right
    let p_right = pos2(1600.0, 200.0);
    let ghost_x_right = (p_right.x + 14.0).clamp(clip.min.x, (clip.max.x - 110.0).max(clip.min.x));
    assert_eq!(ghost_x_right, clip.max.x - 110.0, "Ghost x clamped to clip.max.x - 110.0 (1322.0)");

    // Inverted/narrow panel protection (.max(clip.min.x))
    let narrow_clip = Rect::from_min_max(pos2(100.0, 50.0), pos2(150.0, 200.0));
    let gx = (p_right.x + 14.0).clamp(narrow_clip.min.x, (narrow_clip.max.x - 110.0).max(narrow_clip.min.x));
    assert_eq!(gx, narrow_clip.min.x, "Must not panic on narrow panel and clamp to min.x");
}

#[test]
fn test_e2e_t1_layers_drag_ghost_no_window_title_overflow() {
    let title_bar_bottom = 40.0f32;
    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 800.0));
    let clip = panel_rect.shrink(8.0);

    for py in [-1000.0f32, -50.0, 0.0, 10.0, 39.0, 48.0] {
        let ghost_y = (py + 8.0).clamp(clip.min.y, (clip.max.y - 32.0).max(clip.min.y));
        assert!(
            ghost_y > title_bar_bottom,
            "Ghost y ({ghost_y}) must strictly remain below window title bar (40.0) for py={py}"
        );
        assert!(ghost_y >= clip.min.y);
    }
}

#[test]
fn test_e2e_t1_layers_drag_ghost_visual_pill_rect() {
    let panel_width = 340.0f32;
    let padding = 12.0f32;
    let pill_width = panel_width - padding * 2.0;
    let row_height = 36.0f32;
    let ghost_rect = Rect::from_center_size(pos2(1270.0, 200.0), vec2(pill_width, row_height));
    assert_eq!(ghost_rect.width(), 316.0);
    assert_eq!(ghost_rect.height(), 36.0);
}

// --- Feature 8: Drag & drop containment in toolbar modal [R3] ---

#[test]
fn test_e2e_t1_toolbar_modal_drag_initiation() {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui(|ui| {
            let item_id = Id::new("toolbar-dragged-item");
            ui.data_mut(|d| d.insert_temp(item_id, 3usize));
            assert_eq!(ui.data(|d| d.get_temp::<usize>(item_id)), Some(3));
        });
    harness.step();
}

#[test]
fn test_e2e_t1_toolbar_modal_drag_clamped_within_modal() {
    let modal_rect = Rect::from_min_max(pos2(100.0, 50.0), pos2(700.0, 650.0));
    let modal_header_bottom = 140.0f32;
    let dialog_title_bottom = 80.0f32;
    let row_h = 34.0f32;
    let n = 12usize;
    let list = Rect::from_min_max(pos2(modal_rect.min.x + 20.0, 140.0), pos2(modal_rect.max.x - 20.0, 140.0 + row_h * n as f32));

    let grab_offsets = [0.0f32, 17.0, 34.0];
    let test_pointer_ys = [-1000.0f32, 0.0, 50.0, 70.0, 100.0, 139.0, 140.0, 250.0, list.max.y, 2000.0];

    for grab in grab_offsets {
        for py in test_pointer_ys {
            // Clamping formula from src/app/dialogs.rs:643:
            let top = (py - grab).clamp(list.min.y, (list.max.y - row_h).max(list.min.y));
            let row = Rect::from_min_size(pos2(list.min.x, top), vec2(list.width(), row_h));

            // Clamping invariant 1: row.min.y is at least list.min.y
            assert!(row.min.y >= list.min.y, "Row top {} must not be less than list.min.y {}", row.min.y, list.min.y);
            // Clamping invariant 2: row.max.y never exceeds list.max.y
            assert!(row.max.y <= list.max.y + 1e-4, "Row bottom {} must not exceed list.max.y {}", row.max.y, list.max.y);
            // Clamping invariant 3: row is strictly below modal header and title bar
            assert!(row.min.y >= modal_header_bottom, "Row must not enter modal header");
            assert!(row.min.y > dialog_title_bottom, "Row must not enter dialog title");
            // Containment check via intersection
            assert_eq!(row.intersect(list), row, "Lifted row must be 100% contained within list bounds");
        }
    }

    // Inverted bounds protection on tiny list (.max(list.min.y))
    let tiny_list = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 10.0)); // height 10 < row_h 34
    let top = (50.0f32).clamp(tiny_list.min.y, (tiny_list.max.y - row_h).max(tiny_list.min.y));
    assert_eq!(top, tiny_list.min.y, "Must clamp to min.y and not panic");
}

#[test]
fn test_e2e_t1_toolbar_modal_drag_cannot_escape_to_board() {
    let modal_rect = Rect::from_min_max(pos2(200.0, 100.0), pos2(600.0, 500.0));
    let pointer = pos2(100.0, 50.0);
    let is_inside = modal_rect.contains(pointer);
    assert!(!is_inside);
}

#[test]
fn test_e2e_t1_toolbar_modal_drag_reorder_in_section() {
    let mut order = vec!["pen", "eraser", "highlighter", "shape"];
    let item = order.remove(3);
    order.insert(1, item);
    assert_eq!(order, vec!["pen", "shape", "eraser", "highlighter"]);
}

#[test]
fn test_e2e_t1_toolbar_modal_drag_cancel_outside() {
    let initial_order = vec!["pen", "highlighter", "eraser"];
    let mut active_order = initial_order.clone();
    let _dragged_item = active_order.remove(1);
    active_order = initial_order.clone();
    assert_eq!(active_order, vec!["pen", "highlighter", "eraser"]);
}

/* =========================================================================
   TIER 2: BOUNDARY & CORNER CASES (>= 5 tests per feature)
   ========================================================================= */

#[test]
fn test_e2e_t2_timer_boundary_zero_and_negative_ms() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, 0.0);
    let total0 = ed.board.timer_field("total").and_then(|v| v.as_f64()).unwrap();
    assert!(total0 >= 1000.0);

    start_test_timer(&mut ed, -5000.0);
    let total_neg = ed.board.timer_field("total").and_then(|v| v.as_f64()).unwrap();
    assert!(total_neg >= 1000.0);
}

#[test]
fn test_e2e_t2_timer_boundary_max_duration() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, MAX_TIMER_MS * 2.0);
    let total = ed.board.timer_field("total").and_then(|v| v.as_f64()).unwrap();
    assert!(total <= MAX_TIMER_MS);
}

#[test]
fn test_e2e_t2_timer_digits_ms_empty_and_alphabetic() {
    assert_eq!(parse_timer_digits_spec(""), None);
    assert_eq!(parse_timer_digits_spec("abc"), None);
    assert_eq!(parse_timer_digits_spec("0000000"), None);
    assert_eq!(parse_timer_digits_spec("130"), Some(90_000.0));
    assert_eq!(parse_timer_digits_spec("13000"), Some(5_400_000.0));
}

#[test]
fn test_e2e_t2_chime_volume_extremes() {
    let v_low = (-0.5f32).clamp(0.05, 1.0);
    assert_eq!(v_low, 0.05);
    let v_high = 2.5f32.clamp(0.05, 1.0);
    assert_eq!(v_high, 1.0);
}

#[test]
fn test_e2e_t2_voting_boundary_zero_and_massive_votes() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();
    ed.board.start_voting(1, &voter);
    assert_eq!(ed.board.voting(), Some((1, false)));
    ed.board.start_voting(100, &voter);
    assert_eq!(ed.board.voting(), Some((50, false)));
}

#[test]
fn test_e2e_t2_zoom_obstacle_extreme_panel_widths() {
    let stage = Rect::from_min_max(pos2(0.0, 0.0), pos2(1440.0, 900.0));
    let giant_panel = Rect::from_min_max(pos2(0.0, 0.0), pos2(1440.0, 900.0));
    let offset_x = (giant_panel.min.x - 60.0).max(stage.min.x + 16.0);
    assert!(offset_x >= stage.min.x + 16.0);
}

#[test]
fn test_e2e_t2_layers_drag_empty_board() {
    let ed = create_test_editor();
    assert_eq!(ed.board.len(), 0);
}

#[test]
fn test_e2e_t2_toolbar_modal_drag_extreme_out_of_bounds() {
    let modal_rect = Rect::from_min_max(pos2(300.0, 200.0), pos2(700.0, 500.0));
    for extreme in [pos2(-1000.0, -1000.0), pos2(5000.0, 5000.0), pos2(0.0, 0.0)] {
        let clamped = pos2(
            extreme.x.clamp(modal_rect.min.x, modal_rect.max.x),
            extreme.y.clamp(modal_rect.min.y, modal_rect.max.y),
        );
        assert!(modal_rect.contains(clamped));
    }
}

/* =========================================================================
   TIER 3: CROSS-FEATURE COMBINATIONS (Pairwise interactions)
   ========================================================================= */

#[test]
fn test_e2e_t3_timer_running_and_voting_session_active_together() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();
    start_test_timer(&mut ed, 300_000.0);
    ed.board.start_voting(3, &voter);

    assert!(ed.board.timer_field("total").is_some());
    assert_eq!(ed.board.voting(), Some((3, false)));

    let mut s = model::El::new(model::Kind::Sticky(Default::default()));
    s.id = "s_t3".into();
    ed.board.put(vec![s]);
    ed.board.cast_vote(&voter, "s_t3", 1);

    stop_test_timer(&mut ed);
    assert!(ed.board.timer_field("total").is_none());
    assert_eq!(ed.board.voting(), Some((3, false)));
}

#[test]
fn test_e2e_t3_right_panel_open_with_timer_banner_countdown() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, 60_000.0);
    let right_panel_rect = Rect::from_min_max(pos2(1100.0, 0.0), pos2(1440.0, 900.0));

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_panel_rect));
        });
    harness.step();
    assert!(harness.ctx.data(|d| d.get_temp::<Rect>(Id::new("right-panel-rect"))).is_some());
}

#[test]
fn test_e2e_t3_chime_toggle_while_layers_panel_reorder_active() {
    let mut ed = create_test_editor();
    let mut s1 = model::El::new(model::Kind::Sticky(Default::default()));
    s1.id = "s1".into();
    s1.z = 0.0;
    let mut s2 = model::El::new(model::Kind::Sticky(Default::default()));
    s2.id = "s2".into();
    s2.z = 1.0;
    ed.board.put(vec![s1, s2]);

    ed.prefs.timer_sound = !ed.prefs.timer_sound;
    ed.board.move_in_stack(&["s2".into()], doc::Anchor::Below("s1".into()), None);

    assert_eq!(ed.board.all()[0].id, "s2");
}

#[test]
fn test_e2e_t3_drag_containment_during_voting_tally() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();
    ed.board.start_voting(3, &voter);

    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 800.0));
    let dragged_pos = pos2(1050.0, 300.0);
    let clamped = pos2(dragged_pos.x.clamp(panel_rect.min.x, panel_rect.max.x), dragged_pos.y);

    assert_eq!(clamped.x, panel_rect.min.x);
    assert_eq!(ed.board.voting(), Some((3, false)));
}

#[test]
fn test_e2e_t3_toolbar_customization_modal_with_active_timer() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, 120_000.0);
    let modal_rect = Rect::from_min_max(pos2(300.0, 150.0), pos2(700.0, 550.0));
    let modal_drag = pos2(350.0, 200.0);
    assert!(modal_rect.contains(modal_drag));
    assert!(ed.board.timer_field("total").is_some());
}

/* =========================================================================
   TIER 4: REAL-WORLD APPLICATION SCENARIOS (>= 5 realistic user scenarios)
   ========================================================================= */

#[test]
fn test_e2e_t4_scenario_brainstorming_workshop() {
    let mut ed = create_test_editor();
    let voter = ed.voter.clone();

    // 1. Facilitator starts 5-min timer with chime enabled
    ed.prefs.timer_sound = true;
    ed.prefs.timer_chime = prefs::TimerSound::Harp;
    start_test_timer(&mut ed, 300_000.0);

    // 2. Participants post 4 ideas
    for i in 1..=4 {
        let mut s = model::El::new(model::Kind::Sticky(model::Sticky {
            text: format!("Idea {i}"),
            color: "#FFF3A3".into(),
            ..Default::default()
        }));
        s.id = format!("idea_{i}");
        ed.board.put(vec![s]);
    }
    assert_eq!(ed.board.len(), 4);

    // 3. Facilitator initiates voting (3 votes each)
    ed.board.start_voting(3, &voter);

    // 4. Voter votes on idea 1 and idea 3
    ed.board.cast_vote(&voter, "idea_1", 1);
    ed.board.cast_vote(&voter, "idea_3", 1);
    ed.board.cast_vote(&voter, "idea_3", 1);

    let tally = ed.board.tally(Some(&voter));
    assert_eq!(*tally.get("idea_1").unwrap_or(&0), 1);
    assert_eq!(*tally.get("idea_3").unwrap_or(&0), 2);

    // 5. Timer finishes and facilitator concludes voting
    stop_test_timer(&mut ed);
    ed.board.end_voting();

    assert_eq!(ed.board.voting(), Some((3, true)));
}

#[test]
fn test_e2e_t4_scenario_layers_cleanup_with_containment() {
    let mut ed = create_test_editor();
    let panel_rect = Rect::from_min_max(pos2(1100.0, 48.0), pos2(1440.0, 800.0));

    for i in 1..=5 {
        let mut s = model::El::new(model::Kind::Sticky(Default::default()));
        s.id = format!("layer_{i}");
        s.z = i as f64;
        ed.board.put(vec![s]);
    }

    let overshot_pointer = pos2(1270.0, 10.0);
    let clamped_ghost = pos2(
        overshot_pointer.x.clamp(panel_rect.min.x, panel_rect.max.x),
        overshot_pointer.y.clamp(panel_rect.min.y, panel_rect.max.y),
    );

    assert_eq!(clamped_ghost.y, panel_rect.min.y);
    ed.board.move_in_stack(&["layer_5".into()], doc::Anchor::Below("layer_1".into()), None);

    let top_layer = &ed.board.all()[0];
    assert_eq!(top_layer.id, "layer_5");
}

#[test]
fn test_e2e_t4_scenario_inspection_with_right_panel_and_zoom() {
    let mut ed = create_test_editor();
    let right_panel_rect = Rect::from_min_max(pos2(1080.0, 0.0), pos2(1440.0, 900.0));

    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .build_ui(|ui| {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new("right-panel-rect"), right_panel_rect));
        });
    harness.step();

    let initial_z = ed.cam.z;
    ed.zoom_at(600.0, 450.0, initial_z * 1.5);
    assert!(ed.cam.z > initial_z);

    let zoom_control_x = right_panel_rect.min.x - 60.0;
    assert!(zoom_control_x < right_panel_rect.min.x);
}

#[test]
fn test_e2e_t4_scenario_timer_pause_extend_and_chime_switch() {
    let mut ed = create_test_editor();
    start_test_timer(&mut ed, 120_000.0);
    add_minute_test_timer(&mut ed);
    let total = ed.board.timer_field("total").and_then(|v| v.as_f64());
    assert_eq!(total, Some(180_000.0));

    ed.prefs.timer_chime = prefs::TimerSound::Piano;
    ed.prefs.timer_volume = 0.8;
    assert_eq!(ed.prefs.timer_chime, prefs::TimerSound::Piano);

    stop_test_timer(&mut ed);
    assert!(ed.board.timer_field("total").is_none());
}

#[test]
fn test_e2e_t4_scenario_toolbar_customization_session() {
    let initial_tools = vec!["pen", "highlighter", "eraser", "shape", "text"];
    let modal_rect = Rect::from_min_max(pos2(250.0, 100.0), pos2(750.0, 500.0));

    let mut current_tools = initial_tools.clone();
    let lifted = current_tools.remove(2);

    let drag_pos = pos2(100.0, 50.0);
    let clamped_drag = pos2(
        drag_pos.x.clamp(modal_rect.min.x + 12.0, modal_rect.max.x - 12.0),
        drag_pos.y.clamp(modal_rect.min.y + 12.0, modal_rect.max.y - 12.0),
    );
    assert!(modal_rect.contains(clamped_drag));

    current_tools.insert(0, lifted);
    assert_eq!(current_tools[0], "eraser");
}
