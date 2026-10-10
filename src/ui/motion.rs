//! Motion in the manner of Figma and Apple: springs that carry on from wherever they are when
//! the target changes (so nothing jumps when you change your mind half-way), quick quiet exits,
//! small rises for things that appear and a slight give when a button is pressed.

use egui::{Context, Id};

#[derive(Clone, Copy)]
struct Spring {
    x: f32,
    v: f32,
}

/// A value following `target` like a SwiftUI spring: `response` is roughly how long it takes
/// (seconds); `damping` 1 settles without overshoot, lower bounces a little.
pub fn spring(ctx: &Context, id: Id, target: f32, response: f32, damping: f32) -> f32 {
    let Some(mut s) = ctx.data(|d| d.get_temp::<Spring>(id)) else {
        ctx.data_mut(|d| d.insert_temp(id, Spring { x: target, v: 0.0 }));
        return target;
    };
    if s.x != target || s.v != 0.0 {
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 20.0);
        let w = std::f32::consts::TAU / response.max(0.01);
        let (k, c) = (w * w, 2.0 * damping * w);
        // Small steps keep a stiff spring stable whatever the frame rate.
        let n = (dt * 480.0).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            s.v += (-k * (s.x - target) - c * s.v) * h;
            s.x += s.v * h;
        }
        let scale = target.abs().max(1.0);
        if (s.x - target).abs() < 0.002 * scale && s.v.abs() < 0.02 * scale {
            s = Spring { x: target, v: 0.0 };
        } else {
            ctx.request_repaint();
        }
        ctx.data_mut(|d| d.insert_temp(id, s));
    }
    s.x
}

/// How far something is shown, 0 to 1 (a touch above 1 while it settles): appears with a soft
/// spring, leaves quicker and without bounce.
pub fn presence(ctx: &Context, id: Id, shown: bool) -> f32 {
    if shown { spring(ctx, id, 1.0, 0.36, 0.82) } else { spring(ctx, id, 0.0, 0.2, 1.0) }
}

/// Like `presence`, for something that is drawn only while shown: starts from 0 the first time.
pub fn appear(ctx: &Context, id: Id, shown: bool) -> f32 {
    if shown && ctx.data(|d| d.get_temp::<Spring>(id)).is_none() {
        ctx.data_mut(|d| d.insert_temp(id, Spring { x: 0.0, v: 0.0 }));
    }
    presence(ctx, id, shown)
}

/// Appears afresh whenever `key` changes (a new selection, a new element under the pointer).
pub fn entering(ctx: &Context, slot: Id, key: impl std::hash::Hash) -> f32 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    key.hash(&mut h);
    let key = slot.with(h.finish());
    if ctx.data(|d| d.get_temp::<Id>(slot.with("key"))) != Some(key) {
        ctx.data_mut(|d| d.insert_temp(slot.with("key"), key));
        reset(ctx, slot);
    }
    appear(ctx, slot, true)
}

/// Forgets a spring, so the next `appear` starts from nothing again.
pub fn reset(ctx: &Context, id: Id) {
    ctx.data_mut(|d| d.remove::<Spring>(id));
}

/// 0 to 1 over a short ease, for hover and selection colours.
pub fn hover(ctx: &Context, id: Id, on: bool) -> f32 {
    ctx.animate_bool_with_time_and_easing(id, on, 0.12, egui::emath::easing::cubic_out)
}

/// Scale of a pressed control: a slight give, like a physical button.
pub fn press(ctx: &Context, id: Id, down: bool) -> f32 {
    1.0 - 0.06 * ctx.animate_bool_with_time_and_easing(id.with("press"), down, 0.1, egui::emath::easing::cubic_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn springs_settle_on_the_target() {
        let ctx = Context::default();
        let id = Id::new("s");
        let mut x = spring(&ctx, id, 0.0, 0.3, 0.8);
        assert_eq!(x, 0.0, "starts where it is asked to be");
        let mut peak = 0.0f32;
        for _ in 0..240 {
            ctx.begin_pass(egui::RawInput { predicted_dt: 1.0 / 60.0, ..Default::default() });
            x = spring(&ctx, id, 100.0, 0.3, 0.8);
            peak = peak.max(x);
            ctx.end_pass().textures_delta.clear();
        }
        assert_eq!(x, 100.0, "settled exactly");
        assert!(peak > 100.0 && peak < 110.0, "a small overshoot, not a wobble: {peak}");
    }
}
