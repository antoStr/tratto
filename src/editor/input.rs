//! Pointer, touch, wheel, keyboard and clipboard on the board (the event handlers of the first
//! Tratto's controller). The mouse, the pen and the first finger drive one pointer; further
//! fingers only pinch.

use egui::{CursorIcon, Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase, Vec2};
use serde_json::{Value, json};

use super::*;
use crate::ink::{RULER_LENGTH, on_ruler, ruler_snapper};

#[derive(Default)]
pub struct InputState {
    kind: Option<PointerKind>,
    pointer: u64,
    force: Option<f32>,
    /// A finger or pen touched down in this frame: the press that follows is that touch.
    touch_began: Option<(u64, Option<f32>)>,
    touch_pointer: Option<u64>,
    down: bool,
    modifiers: Modifiers,
}

fn kind_of(touch: u64) -> PointerKind {
    match crate::pen::state(touch) {
        Some(_) => PointerKind::Pen,
        None => PointerKind::Touch,
    }
}

impl Editor {
    /// Handles this frame's input. `hovered`: the pointer is over the board rather than over a
    /// panel; `keys`: no text field or menu has the keyboard.
    pub fn handle_input(&mut self, ctx: &egui::Context, hovered: bool, keys: bool, double_click: Option<Pos2>) {
        let events = ctx.input(|i| i.events.clone());
        let t = now();
        self.input.touch_began = None;
        for e in events {
            match e {
                Event::Touch { id, phase, pos, force, .. } => self.on_touch(id.0, phase, pos, force, hovered, t),
                Event::PointerButton { pos, button, pressed, modifiers } => {
                    self.input.modifiers = modifiers;
                    let s = self.screen_pt(pos);
                    if pressed {
                        let (kind, pointer, force) = match self.input.touch_began.take() {
                            Some((id, f)) => (kind_of(id), id, f),
                            None => (PointerKind::Mouse, 0, None),
                        };
                        if !hovered && kind == PointerKind::Mouse {
                            continue;
                        }
                        self.input.kind = Some(kind);
                        self.input.pointer = pointer;
                        self.input.force = force;
                        if kind != PointerKind::Mouse {
                            self.input.touch_pointer = Some(pointer);
                        }
                        if button == PointerButton::Secondary && kind == PointerKind::Mouse {
                            if self.gesture.is_none() {
                                self.select_for_menu(s);
                                self.requests.push(Request::Menu(pos));
                            }
                            continue;
                        }
                        let barrel = crate::pen::state(pointer).is_some_and(|p| p.barrel);
                        let eraser = crate::pen::state(pointer).is_some_and(|p| p.eraser);
                        self.input.down = true;
                        self.on_down(s, button, kind, force, modifiers, t, barrel, eraser);
                        if kind != PointerKind::Mouse && matches!(self.gesture, Some(Gesture::Move { .. } | Gesture::Marquee { .. } | Gesture::Pan { .. } | Gesture::Lasso { .. })) {
                            self.press = Some((s, t));
                        }
                    } else if self.input.down && (button == PointerButton::Primary || button == PointerButton::Middle) {
                        self.input.down = false;
                        self.on_up(s, modifiers, false);
                        self.input.touch_pointer = None;
                    }
                }
                Event::PointerMoved(pos) => {
                    let m = ctx.input(|i| i.modifiers);
                    self.on_move(self.screen_pt(pos), m, t);
                }
                Event::PointerGone => self.on_leave(),
                Event::MouseWheel { unit, delta, modifiers, .. } => {
                    if hovered && let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
                        self.on_wheel(self.screen_pt(p), unit, delta, modifiers);
                    }
                }
                Event::Zoom(f) => {
                    if hovered && let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
                        let s = self.screen_pt(p);
                        self.stop_following();
                        self.zoom_at(s.x as f64, s.y as f64, self.cam.z * f as f64);
                    }
                }
                Event::Key { key, physical_key, pressed, modifiers, repeat } => {
                    if !pressed {
                        if key == Key::Space {
                            self.space = false;
                        }
                    } else if keys && (!repeat || matches!(key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown)) {
                        self.on_key(key, physical_key, modifiers);
                    }
                }
                Event::Copy if keys => {
                    if let Some(text) = self.copy() {
                        ctx.copy_text(text);
                    }
                }
                Event::Cut if keys => {
                    if let Some(text) = self.cut() {
                        ctx.copy_text(text);
                    }
                }
                Event::Paste(text) if keys && !self.read_only => self.requests.push(Request::Paste(text)),
                Event::WindowFocused(false) => self.space = false,
                _ => {}
            }
        }
        if let Some(p) = double_click {
            self.on_double_click(self.screen_pt(p));
        }
        self.tick();
        if hovered || self.gesture.is_some() {
            ctx.set_cursor_icon(self.cursor());
        }
    }

    fn on_touch(&mut self, id: u64, phase: TouchPhase, pos: Pos2, force: Option<f32>, hovered: bool, t: f64) {
        let s = self.screen_pt(pos);
        let pen = kind_of(id) == PointerKind::Pen;
        match phase {
            TouchPhase::Start => {
                if pen {
                    self.last_pen = t;
                } else if t - self.last_pen < 1200.0 || !hovered {
                    // Palm rejection: the hand resting on the screen while the pen is in use.
                    return;
                }
                self.input.touch_began = Some((id, force));
                if pen {
                    return;
                }
                self.touches.insert(id, s);
                if self.touches.len() == 2 {
                    // Second finger: cancel a stroke that just started and pinch instead.
                    if !matches!(self.gesture, Some(Gesture::Pinch { .. })) {
                        self.cancel_gesture();
                    }
                    self.start_pinch();
                }
            }
            TouchPhase::Move => {
                if Some(id) == self.input.touch_pointer {
                    self.input.force = force;
                }
                if pen {
                    self.last_pen = t;
                }
                if let Some(p) = self.touches.get_mut(&id) {
                    *p = s;
                    self.update_pinch();
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                self.touches.remove(&id);
                if let Some(Gesture::Pinch { ids, .. }) = &self.gesture
                    && ids.contains(&id)
                {
                    self.gesture = None;
                    self.input.down = false;
                }
            }
        }
    }

    fn start_pinch(&mut self) {
        let ids: Vec<u64> = self.touches.keys().copied().take(2).collect();
        let (a, b) = (self.touches[&ids[0]], self.touches[&ids[1]]);
        let r = self.ruler;
        let on_r = on_ruler(&r, a.x as f64, a.y as f64) && on_ruler(&r, b.x as f64, b.y as f64);
        self.gesture = Some(Gesture::Pinch {
            ids: [ids[0], ids[1]],
            d0: (a.distance(b) as f64).max(1.0),
            c0: pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
            cam: self.cam,
            ruler: on_r.then(|| [r.x, r.y, r.angle, ((b.y - a.y) as f64).atan2((b.x - a.x) as f64)]),
        });
        self.stop_following();
    }

    fn update_pinch(&mut self) {
        let Some(Gesture::Pinch { ids, d0, c0, cam, ruler }) = &self.gesture else { return };
        let (Some(&a), Some(&b)) = (self.touches.get(&ids[0]), self.touches.get(&ids[1])) else { return };
        let c = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        if let Some([x, y, angle, a0]) = *ruler {
            self.ruler.angle = angle + ((b.y - a.y) as f64).atan2((b.x - a.x) as f64) - a0;
            self.ruler.x = x + (c.x - c0.x) as f64;
            self.ruler.y = y + (c.y - c0.y) as f64;
            return;
        }
        let z = (cam.z * a.distance(b) as f64 / d0).clamp(MIN_ZOOM, MAX_ZOOM);
        let w0 = cam.to_world(c0.x as f64, c0.y as f64);
        self.set_cam(Camera { x: c.x as f64 - w0.x * z, y: c.y as f64 - w0.y * z, z });
    }

    #[allow(clippy::too_many_arguments)]
    fn on_down(&mut self, s: Pos2, button: PointerButton, kind: PointerKind, force: Option<f32>, m: Modifiers, t: f64, barrel: bool, eraser: bool) {
        if kind == PointerKind::Pen {
            self.last_pen = t;
        }
        if kind == PointerKind::Touch && (t - self.last_pen < 1200.0 || self.touches.len() > 1) {
            return;
        }
        if self.gesture.is_some() {
            return;
        }
        // Leave any text being typed before acting on the board.
        if self.editing.is_some() {
            self.stop_editing();
        }
        self.comment = None;
        let p = self.to_world(s);
        let ro = self.read_only;
        let mut tool = self.tool;
        if ro && !matches!(tool, Tool::Select | Tool::Hand | Tool::Laser) {
            tool = Tool::Hand;
        }
        let touch_pans = kind == PointerKind::Touch && !self.prefs.finger_draw;

        // The pen's eraser end, and its barrel button (lasso; a tap opens the menu).
        if !ro && eraser {
            return self.start_erase(p);
        }
        if !ro && barrel {
            self.gesture = Some(Gesture::Lasso { poly: vec![p.x, p.y], tap: Some(s) });
            return;
        }
        if button == PointerButton::Middle || self.space || tool == Tool::Hand {
            self.gesture = Some(Gesture::Pan { s0: s, cam: self.cam });
            self.stop_following();
            return;
        }

        // Ruler: draw along its edge with ink tools, otherwise drag or rotate it.
        let r = self.ruler;
        if r.visible {
            let ink_tool = matches!(tool, Tool::Pen | Tool::Highlighter) && !touch_pans;
            if !(ink_tool && ruler_snapper(&r, s.x as f64, s.y as f64, 2.0).is_some()) {
                let (sin, cos) = r.angle.sin_cos();
                let knob = (r.x + cos * (RULER_LENGTH / 2.0 - 26.0), r.y + sin * (RULER_LENGTH / 2.0 - 26.0));
                if (s.x as f64 - knob.0).hypot(s.y as f64 - knob.1) < 18.0 {
                    self.gesture = Some(Gesture::RulerRotate);
                    return;
                }
                if on_ruler(&r, s.x as f64, s.y as f64) {
                    self.gesture = Some(Gesture::Ruler { s0: s, x: r.x, y: r.y });
                    return;
                }
            }
        }

        let handle = if !ro && matches!(tool, Tool::Select | Tool::Lasso) { self.hit_handle(s, kind != PointerKind::Mouse) } else { None };
        if let Some(h) = handle {
            self.board.stop_capturing();
            let sel: Vec<El> = self.selected().iter().map(|e| (**e).clone()).collect();
            if h.is_add() {
                // Drag: a connector from this element; a click adds a connected copy on that side.
                let src = sel[0].clone();
                let el = self.connector(p, Some(src.id.clone()), true);
                self.gesture = Some(Gesture::Create { el, start: p, moved: false, spawn: Some(src) });
                self.pending_add = Some(h);
            } else if h == Handle::P0 || h == Handle::P1 {
                self.gesture = Some(Gesture::Endpoint { which: if h == Handle::P0 { 0 } else { 1 }, orig: sel[0].clone() });
            } else if h == Handle::Rot {
                let (b, _) = self.selection_frame().unwrap();
                let c = b.center();
                self.gesture = Some(Gesture::Rotate { c, a0: (p.y - c.y).atan2(p.x - c.x), orig: sel });
            } else {
                let (b, rotation) = self.selection_frame().unwrap();
                self.gesture = Some(Gesture::Resize { handle: h, start: p, orig: sel, frame: b, rotation });
            }
            return;
        }

        if self.try_vote(p, m.shift || m.alt) {
            return;
        }
        if tool == Tool::Select && self.try_widget(p) {
            return;
        }

        match tool {
            Tool::Select | Tool::Lasso => {
                if tool == Tool::Lasso && !self.inside_selection(p) {
                    self.gesture = Some(if touch_pans { Gesture::Pan { s0: s, cam: self.cam } } else { Gesture::Lasso { poly: vec![p.x, p.y], tap: None } });
                    return;
                }
                let hit = if tool == Tool::Lasso { None } else { self.hit_element(p, if kind == PointerKind::Mouse { 4.0 } else { 10.0 }) };
                if let Some(hit) = hit {
                    let was = self.selection.contains(&hit.id);
                    if m.shift {
                        if was {
                            self.selection.retain(|id| *id != hit.id);
                            return;
                        }
                        self.selection.push(hit.id.clone());
                    } else if !was {
                        self.selection = vec![hit.id.clone()];
                    }
                    if !ro {
                        self.start_move(p, s, m.alt);
                    }
                    return;
                }
                if tool == Tool::Lasso || (self.inside_selection(p) && self.selected().len() > 1) {
                    if !ro {
                        self.start_move(p, s, m.alt);
                    }
                    return;
                }
                if touch_pans {
                    self.selection.clear();
                    self.gesture = Some(Gesture::Pan { s0: s, cam: self.cam });
                    return;
                }
                let base = if m.shift { self.selection.clone() } else { Vec::new() };
                if !m.shift {
                    self.selection.clear();
                }
                self.gesture = Some(Gesture::Marquee { start: p, cur: p, base });
                return;
            }
            Tool::Pen | Tool::Highlighter if !touch_pans => {
                self.start_draw(s, tool == Tool::Highlighter, kind, self.input.pointer);
                self.add_draw_point(s, t, force);
                return;
            }
            Tool::Eraser if !touch_pans => return self.start_erase(p),
            Tool::Shape | Tool::Line | Tool::Arrow | Tool::Section | Tool::Tape if !touch_pans => return self.start_create(tool, p),
            Tool::Comment => {
                // On a pin: open its thread. Elsewhere: a new pin, its thread open to write the first message.
                if let Some(hit) = self.hit_element(p, 6.0).filter(|e| e.is_comment()) {
                    self.selection = vec![hit.id.clone()];
                    self.comment = Some(hit.id.clone());
                    return;
                }
                let mut el = self.base(El::new(Kind::Comment { thread: Vec::new() }));
                (el.x, el.y) = (p.x, p.y);
                let id = el.id.clone();
                self.board.stop_capturing();
                self.board.put([el]);
                self.set_tool_keep(Tool::Select);
                self.selection = vec![id.clone()];
                self.comment = Some(id);
                return;
            }
            Tool::Text => {
                match self.hit_element(p, 6.0).filter(|e| e.text().is_some()) {
                    Some(hit) => {
                        self.set_tool_keep(Tool::Select);
                        self.selection = vec![hit.id.clone()];
                        self.editing = Some(hit.id.clone());
                    }
                    None => self.start_text(p),
                }
                return;
            }
            Tool::Sticky => return self.start_sticky(p),
            Tool::Table => return self.insert_table(Some(p)),
            Tool::Stamp => {
                // Clicking an existing element picks it up instead of stacking another stamp on it.
                if let Some(hit) = self.hit_element(p, 6.0) {
                    self.selection = vec![hit.id.clone()];
                    if !ro {
                        self.start_move(p, s, m.alt);
                    }
                    return;
                }
                return self.place_stamp(p);
            }
            Tool::Laser => {
                self.gesture = Some(Gesture::Laser);
                self.laser.push((p.x, p.y, t));
                return;
            }
            _ => {}
        }
        self.gesture = Some(Gesture::Pan { s0: s, cam: self.cam });
    }

    fn on_move(&mut self, s: Pos2, m: Modifiers, t: f64) {
        if self.press.is_some_and(|(at, _)| at.distance(s) > 8.0) {
            self.press = None;
        }
        if self.input.kind == Some(PointerKind::Pen) {
            self.last_pen = t;
        }
        let p = self.to_world(s);
        if self.input.kind != Some(PointerKind::Touch) {
            self.presence.set("cursor", json!({ "x": r2(p.x), "y": r2(p.y) }));
        }
        if self.tool == Tool::Eraser || matches!(self.gesture, Some(Gesture::Erase { .. })) {
            self.eraser_at = Some(s);
        }
        let Some(g) = &mut self.gesture else {
            if self.input.kind != Some(PointerKind::Touch) || !self.input.down {
                let h = if matches!(self.tool, Tool::Select | Tool::Lasso) { self.hit_handle(s, self.input.kind == Some(PointerKind::Pen)) } else { None };
                let hit = if self.tool == Tool::Select && h.is_none() { self.hit_element(p, 4.0) } else { None };
                self.hot = !self.read_only
                    && hit.as_ref().is_some_and(|e| {
                        let l = crate::geom::to_local(e, p.x, p.y);
                        crate::widgets::hot_at(e, l.x, l.y).is_some()
                    });
                self.hover_handle = h;
                self.hover = hit.map(|e| e.id.clone());
            }
            return;
        };
        match g {
            Gesture::Pan { s0, cam } => {
                let c = Camera { x: cam.x + (s.x - s0.x) as f64, y: cam.y + (s.y - s0.y) as f64, z: cam.z };
                self.set_cam(c);
            }
            Gesture::Pinch { .. } => {}
            Gesture::Draw(_) => {
                let force = self.input.force;
                self.add_draw_point(s, t, force);
            }
            Gesture::Erase { last, .. } => {
                let a = *last;
                *last = p;
                self.erase_along(a, p);
            }
            Gesture::Lasso { poly, .. } => poly.extend([p.x, p.y]),
            Gesture::Marquee { start, cur, base } => {
                *cur = p;
                let mb = normalize_box(start.x, start.y, p.x, p.y);
                let base = base.clone();
                // Sections only when wholly inside, so a box drawn within one doesn't pick it up.
                let hits: Vec<String> = self
                    .board
                    .all()
                    .iter()
                    .filter(|el| editable(el) && (mb.w > 0.0 || mb.h > 0.0) && if el.is_section() { mb.holds(&frame_box(el)) } else { intersects(&frame_box(el), &mb) })
                    .map(|e| e.id.clone())
                    .collect();
                let mut next = base;
                for h in hits {
                    if !next.contains(&h) {
                        next.push(h);
                    }
                }
                if next != self.selection {
                    self.selection = next;
                }
            }
            Gesture::Move { start, orig, bbox, cands, moved, s0 } => {
                if !*moved && s.distance(*s0) < 3.0 {
                    return;
                }
                *moved = true;
                let (mut dx, mut dy) = (p.x - start.x, p.y - start.y);
                if m.shift {
                    if dx.abs() > dy.abs() { dy = 0.0 } else { dx = 0.0 }
                }
                let (orig, bbox, cands) = (orig.clone(), *bbox, std::mem::take(cands));
                self.guides.clear();
                if !m.ctrl && !m.command {
                    let (sx, sy) = self.snap_move(BBox { x: bbox.x + dx, y: bbox.y + dy, ..bbox }, &cands);
                    dx += sx;
                    dy += sy;
                }
                if let Some(Gesture::Move { cands: c, .. }) = &mut self.gesture {
                    *c = cands;
                }
                for el in &orig {
                    self.preview.insert(el.id.clone(), Arc::new(El { x: el.x + dx, y: el.y + dy, ..el.clone() }));
                }
                self.follow_connectors(orig.iter().map(|e| e.id.clone()).collect());
                self.schedule_flush();
            }
            Gesture::Resize { .. } => self.resize_to(p, m.shift, m.alt),
            Gesture::Rotate { .. } => self.rotate_to(p, m.shift),
            Gesture::Endpoint { which, orig } => {
                let (which, orig) = (*which, orig.clone());
                let (a, b) = ends_of(&orig);
                let fixed = if which == 0 { b } else { a };
                let mut q = p;
                if m.shift {
                    let ang = snap_angle((p.y - fixed.y).atan2(p.x - fixed.x), std::f64::consts::FRAC_PI_4);
                    let len = (p.x - fixed.x).hypot(p.y - fixed.y);
                    q = pt(fixed.x + ang.cos() * len, fixed.y + ang.sin() * len);
                }
                // Over a shape, sticky… the end attaches to it; elsewhere it is let go.
                let other = orig.line().and_then(|l| if which == 0 { l.to.clone() } else { l.from.clone() });
                let target = self.connect_target(p, other.as_deref());
                let moved = if which == 0 { set_line_ends(&orig, q, b) } else { set_line_ends(&orig, a, q) };
                let mut line = unbind(&moved, which == 0, which == 1);
                if let Some(tg) = target {
                    let l = line.line_mut().unwrap();
                    if which == 0 { l.from = Some(tg.id.clone()) } else { l.to = Some(tg.id.clone()) }
                    line = route_connector(&line, |id| self.preview.get(id).or_else(|| self.board.get(id)).map(|e| &**e)).unwrap_or(line);
                }
                self.preview.insert(orig.id.clone(), Arc::new(line));
                self.schedule_flush();
            }
            Gesture::Create { start, moved, spawn, .. } => {
                let (start, moved, spawn) = (*start, *moved, spawn.is_some());
                if spawn && !moved && s.distance(self.cam_screen(start)) < 4.0 {
                    return;
                }
                self.pending_add = None;
                self.update_create(p, m.shift, m.alt);
            }
            Gesture::Laser => {
                self.laser.push((p.x, p.y, t));
                if self.laser.len() > 80 {
                    self.laser.remove(0);
                }
                let flat: Vec<Value> = self.laser.iter().flat_map(|&(x, y, _)| [json!(r2(x)), json!(r2(y))]).collect();
                self.presence.set("laser", Value::Array(flat));
            }
            Gesture::Ruler { s0, x, y } => {
                self.ruler.x = *x + (s.x - s0.x) as f64;
                self.ruler.y = *y + (s.y - s0.y) as f64;
            }
            Gesture::RulerRotate => {
                let mut angle = (s.y as f64 - self.ruler.y).atan2(s.x as f64 - self.ruler.x);
                let right = snap_angle(angle, std::f64::consts::FRAC_PI_4);
                if (right - angle).abs() < 1.5_f64.to_radians() || m.shift {
                    angle = if m.shift { snap_angle(angle, std::f64::consts::PI / 12.0) } else { right };
                }
                self.ruler.angle = angle;
            }
        }
    }

    fn cam_screen(&self, p: Pt) -> Pos2 {
        self.to_screen(p.x, p.y)
    }

    fn on_up(&mut self, s: Pos2, m: Modifiers, cancel: bool) {
        self.press = None;
        let Some(g) = self.gesture.take() else { return };
        match g {
            Gesture::Pinch { .. } => {
                self.gesture = Some(g);
                return;
            }
            Gesture::Draw(d) => {
                if cancel {
                    self.presence.set("live", Value::Null);
                } else {
                    self.commit_draw(d);
                }
            }
            Gesture::Lasso { poly, tap } => {
                let z = self.cam.z;
                if let Some(tap) = tap
                    && poly.chunks_exact(2).all(|q| (q[0] - poly[0]).abs() * z < 8.0 && (q[1] - poly[1]).abs() * z < 8.0)
                {
                    self.open_menu(tap);
                    return;
                }
                if poly.len() >= 6 {
                    self.selection = self
                        .board
                        .all()
                        .iter()
                        .filter(|el| {
                            if !editable(el) {
                                return false;
                            }
                            let pts = sample_points(el);
                            let inside = pts.iter().filter(|q| point_in_polygon(q.x, q.y, &poly)).count();
                            inside as f64 / pts.len() as f64 >= 0.75
                        })
                        .map(|e| e.id.clone())
                        .collect();
                }
            }
            Gesture::Create { el, start, moved, spawn } => {
                if let (Some(side), Some(src)) = (self.pending_add.take(), &spawn) {
                    self.add_beside(src, side);
                } else {
                    self.finish_create(el, start, moved, spawn);
                }
            }
            Gesture::Erase { touched, .. } => self.finish_erase(touched),
            // Let the trail fade, then clear it for the others.
            Gesture::Laser => self.laser_clear = Some(now() + 950.0),
            Gesture::Move { moved, .. } => {
                let sel = self.selected();
                if !moved && sel.len() == 1 && sel[0].is_comment() {
                    self.comment = Some(sel[0].id.clone());
                }
                if !moved && !m.shift && self.selection.len() > 1 {
                    // Click (no drag) inside a multi-selection picks the element under the pointer.
                    let p = self.to_world(s);
                    if let Some(hit) = self.hit_element(p, 6.0) {
                        self.selection = vec![hit.id.clone()];
                    }
                }
            }
            _ => {}
        }
        self.finish_gesture();
    }

    fn on_double_click(&mut self, s: Pos2) {
        if self.read_only {
            return;
        }
        let p = self.to_world(s);
        if let Some(hit) = self.hit_element(p, 6.0).filter(|e| writable(e)) {
            self.set_tool_keep(Tool::Select);
            self.selection = vec![hit.id.clone()];
            self.edit_at(&hit, Some(p));
        } else if let Some(hit) = self.hit_element(p, 6.0).filter(|e| e.widget().is_some() && !e.locked) {
            // Widgets are written in the side panel.
            self.selection = vec![hit.id.clone()];
            self.prefs.right_panel = true;
            self.prefs.focus = false;
        }
    }

    fn on_leave(&mut self) {
        if self.input.kind != Some(PointerKind::Touch) {
            self.presence.set("cursor", Value::Null);
        }
        self.eraser_at = None;
        self.hover = None;
    }

    fn on_wheel(&mut self, s: Pos2, unit: MouseWheelUnit, delta: Vec2, m: Modifiers) {
        // Browser-like pixels: positive dy scrolls the view down.
        let k = match unit {
            MouseWheelUnit::Point => 1.0,
            MouseWheelUnit::Line => 40.0,
            MouseWheelUnit::Page => self.size.1,
        };
        let (mut dx, mut dy) = (-delta.x as f64 * k, -delta.y as f64 * k);
        if self.ruler.visible && on_ruler(&self.ruler, s.x as f64, s.y as f64) && !m.ctrl {
            self.ruler.angle += dy.signum() * if m.shift { 15.0_f64 } else { 1.0 }.to_radians();
            return;
        }
        self.stop_following();
        if m.ctrl || m.command || (self.prefs.wheel == crate::prefs::Wheel::Zoom && !m.shift) {
            let z = self.cam.z * (-dy.clamp(-60.0, 60.0) * 0.0085).exp();
            return self.zoom_at(s.x as f64, s.y as f64, z);
        }
        if m.shift && dx == 0.0 {
            (dx, dy) = (dy, 0.0);
        }
        let c = Camera { x: self.cam.x - dx, y: self.cam.y - dy, z: self.cam.z };
        self.set_cam(c);
    }

    fn on_key(&mut self, key: Key, physical: Option<Key>, m: Modifiers) {
        let cmd = m.ctrl || m.command;
        let ro = self.read_only;
        if key == Key::Space {
            self.space = true;
            return;
        }
        if cmd {
            match key {
                Key::Z if !ro => {
                    if m.shift { self.redo() } else { self.undo() }
                }
                Key::Y if !ro => self.redo(),
                Key::D if !ro => self.duplicate(),
                Key::A => self.select_all(),
                Key::Equals | Key::Plus => self.zoom_by(1.25),
                Key::Minus => self.zoom_by(0.8),
                Key::Num0 => self.zoom_to(1.0),
                Key::CloseBracket if !ro => self.order(if m.shift { Order::Front } else { Order::Forward }),
                Key::OpenBracket if !ro => self.order(if m.shift { Order::Back } else { Order::Backward }),
                Key::Backslash => self.toggle_focus(),
                Key::G if !ro => {
                    if m.shift { self.ungroup() } else { self.group() }
                }
                Key::L if m.shift => self.toggle_lock(),
                Key::H if m.shift => self.toggle_hide(),
                Key::E if m.shift => self.requests.push(Request::Export { selection: false }),
                Key::Slash => self.requests.push(Request::Shortcuts),
                _ => {}
            }
            return;
        }
        if m.alt {
            return;
        }
        let digit = |n: Key| key == n || physical == Some(n);
        if m.shift && (key == Key::Exclamationmark || digit(Key::Num1)) {
            return self.fit();
        }
        if m.shift && digit(Key::Num2) {
            return self.fit_selection();
        }
        if m.shift && (digit(Key::Num0) || key == Key::Equals) {
            return self.zoom_to(1.0);
        }
        if key == Key::Questionmark {
            return self.requests.push(Request::Shortcuts);
        }
        if key == Key::Escape {
            if self.gesture.is_some() {
                self.cancel_gesture();
            } else if !self.selection.is_empty() {
                self.selection.clear();
            } else {
                self.set_tool(Tool::Select);
            }
            return;
        }
        let tool_key = match key {
            Key::V => Some(Tool::Select),
            Key::H => Some(Tool::Hand),
            Key::K => Some(Tool::Laser),
            Key::P => Some(Tool::Pen),
            Key::M => Some(Tool::Highlighter),
            Key::E => Some(Tool::Eraser),
            Key::Q => Some(Tool::Lasso),
            Key::L => Some(Tool::Line),
            Key::T => Some(Tool::Text),
            Key::S => Some(Tool::Sticky),
            _ => None,
        };
        if ro {
            if let Some(t @ (Tool::Select | Tool::Hand | Tool::Laser)) = tool_key {
                self.set_tool(t);
            }
            return;
        }
        match key {
            Key::Delete | Key::Backspace => return self.remove(),
            Key::Enter if self.selection.len() == 1 => {
                if let Some(el) = self.board.get(&self.selection[0]).cloned() {
                    if el.is_comment() {
                        self.comment = Some(el.id.clone());
                    } else if writable(&el) {
                        self.edit_at(&el, None);
                    }
                }
                return;
            }
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown if !self.selection.is_empty() => {
                let step = if m.shift { 10.0 } else { 1.0 } / self.cam.z.max(1.0);
                let (dx, dy) = match key {
                    Key::ArrowLeft => (-step, 0.0),
                    Key::ArrowRight => (step, 0.0),
                    Key::ArrowUp => (0.0, -step),
                    _ => (0.0, step),
                };
                let sel: Vec<Arc<El>> = self.selected().into_iter().filter(|e| editable(e)).collect();
                let els = self.with_content(&sel);
                self.board.put(els.iter().map(|e| El { x: e.x + dx, y: e.y + dy, ..(**e).clone() }).collect::<Vec<_>>());
                return;
            }
            _ => {}
        }
        if m.shift {
            match key {
                Key::L => self.set_tool(Tool::Arrow),
                Key::S => self.set_tool(Tool::Section),
                _ => {}
            }
            return;
        }
        match key {
            Key::X => self.set_tool(Tool::Arrow),
            Key::C => self.set_tool(Tool::Comment),
            Key::W => self.set_tool(Tool::Tape),
            Key::Slash => self.requests.push(Request::Chat),
            Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 if self.tool == Tool::Pen => {
                let n = match key {
                    Key::Num1 => 0,
                    Key::Num2 => 1,
                    Key::Num3 => 2,
                    _ => 3,
                };
                if n < self.prefs.pens.len() {
                    self.pen = n;
                }
            }
            Key::R | Key::O => {
                self.prefs.last_shape = ShapeTool::Kind(if key == Key::R { ShapeKind::Rect } else { ShapeKind::Ellipse });
                self.set_tool(Tool::Shape);
            }
            Key::U => self.toggle_ruler(),
            Key::I => self.requests.push(Request::InsertImage),
            _ => {
                if let Some(t) = tool_key {
                    self.set_tool(t);
                }
            }
        }
    }

    pub fn toggle_ruler(&mut self) {
        if self.ruler.visible {
            self.ruler.visible = false;
        } else {
            self.ruler = Ruler { visible: true, x: self.size.0 / 2.0, y: self.size.1 / 2.0 - 60.0, angle: self.ruler.angle };
        }
    }

    /// Ctrl+\: hides both panels, or brings both back when none is showing.
    pub fn toggle_focus(&mut self) {
        let p = &mut self.prefs;
        let showing = !p.focus && (p.left_panel || p.right_panel);
        if showing {
            p.focus = true;
        } else {
            p.focus = false;
            p.left_panel = true;
            p.right_panel = true;
        }
    }

    fn cursor(&self) -> CursorIcon {
        if matches!(self.gesture, Some(Gesture::Pan { .. })) {
            return CursorIcon::Grabbing;
        }
        if self.space || self.tool == Tool::Hand {
            return CursorIcon::Grab;
        }
        if self.hot && self.tool == Tool::Select {
            return CursorIcon::PointingHand;
        }
        if let Some(h) = self.hover_handle {
            return match h {
                Handle::N | Handle::S => CursorIcon::ResizeVertical,
                Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
                Handle::Nw | Handle::Se => CursorIcon::ResizeNwSe,
                Handle::Ne | Handle::Sw => CursorIcon::ResizeNeSw,
                Handle::Rot => CursorIcon::Grab,
                Handle::P0 | Handle::P1 => CursorIcon::Move,
                _ => CursorIcon::Copy,
            };
        }
        match self.tool {
            Tool::Pen | Tool::Highlighter | Tool::Shape | Tool::Line | Tool::Arrow | Tool::Lasso | Tool::Laser | Tool::Stamp | Tool::Sticky | Tool::Section | Tool::Tape | Tool::Table => CursorIcon::Crosshair,
            Tool::Comment => CursorIcon::Cell,
            Tool::Eraser => CursorIcon::None,
            Tool::Text => CursorIcon::Text,
            _ => CursorIcon::Default,
        }
    }
}
