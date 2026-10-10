//! What is drawn over the board, in screen space: other people's cursors, selections, strokes
//! and lasers; the hover outline; the selection frame and its handles; size labels; votes; the
//! marquee and lasso; the stroke being drawn; snap guides; the laser; the eraser; the ruler.

use egui::epaint::TextShape;
use egui::{Color32, FontId, Painter, Pos2, Shape, Stroke, Vec2, pos2, vec2};

use super::*;
use crate::ink::{RULER_HEIGHT, RULER_LENGTH, stroke_outline};
use crate::paint::fill_mesh;
use crate::prims::Path;

const LASER: Color32 = Color32::from_rgb(0xFF, 0x3B, 0x30);
const GUIDE: Color32 = Color32::from_rgb(0xF2, 0x48, 0x22);
const PX_PER_MM: f32 = 96.0 / 25.4;

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn circle(center: Pos2, radius: f32, fill: Color32, stroke: Stroke) -> Shape {
    Shape::Circle(egui::epaint::CircleShape { center, radius, fill, stroke })
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("medium".into()))
}

impl Editor {
    /// Board point to screen (absolute, in points).
    fn sp(&self, x: f64, y: f64) -> Pos2 {
        let p = self.cam.to_screen(x, y);
        pos2(p.x as f32 + self.origin.x, p.y as f32 + self.origin.y)
    }

    fn outline_el(&self, out: &mut Vec<Shape>, el: &El, color: Color32, width: f32) {
        if el.is_line() {
            let (a, b) = ends_of(el);
            out.push(Shape::line_segment([self.sp(a.x, a.y), self.sp(b.x, b.y)], Stroke::new(width, color)));
            return;
        }
        let pts: Vec<Pos2> = corners(el, 0.0).iter().map(|p| self.sp(p.x, p.y)).collect();
        out.push(Shape::closed_line(pts, Stroke::new(width, color)));
    }

    /// A stroke (board coordinates, flat [x, y, pressure, …]) drawn on screen.
    fn stroke_shape(&self, out: &mut Vec<Shape>, pts: &[f64], size: f64, hl: bool, color: Color32, opacity: f64) {
        if pts.len() < 3 {
            return;
        }
        let (x0, y0) = (pts[0], pts[1]);
        let rel: Vec<f32> = pts.chunks_exact(3).flat_map(|p| [(p[0] - x0) as f32, (p[1] - y0) as f32, p[2] as f32]).collect();
        let outline: Vec<crate::geom::Pt> = stroke_outline(&rel, size, hl)
            .into_iter()
            .map(|q| {
                let s = self.sp(q.x + x0, q.y + y0);
                pt(s.x as f64, s.y as f64)
            })
            .collect();
        let a = if hl { 0.45 } else { 1.0 } * opacity.clamp(0.05, 1.0) as f32;
        out.push(Shape::mesh(fill_mesh(&Path::smooth_outline(&outline), alpha(color, a * color.a() as f32 / 255.0))));
    }

    fn laser_shape(&self, out: &mut Vec<Shape>, flat: &[f64], color: Color32) {
        let n = flat.len() / 2;
        if n == 0 {
            return;
        }
        let at = |i: usize| self.sp(flat[i * 2], flat[i * 2 + 1]);
        for i in 1..n {
            let k = i as f32 / n as f32;
            let seg = [at(i - 1), at(i)];
            // A soft glow under the line.
            out.push(Shape::line_segment(seg, Stroke::new(8.0 + k * 4.0, alpha(color, k * 0.18))));
            out.push(Shape::line_segment(seg, Stroke::new(2.0 + k * 4.0, alpha(color, k * 0.9))));
            out.push(Shape::circle_filled(at(i), (1.0 + k * 2.0) * 0.98, alpha(color, k * 0.9)));
        }
        out.push(Shape::circle_filled(at(n - 1), 3.0, Color32::WHITE));
    }

    fn label(&self, painter: &Painter, out: &mut Vec<Shape>, text: String, x: f32, y: f32, accent: Color32) {
        let galley = painter.layout_no_wrap(text, medium(11.0), Color32::WHITE);
        let w = galley.size().x + 10.0;
        let r = egui::Rect::from_center_size(pos2(x.round(), y.round()), vec2(w.round(), 18.0));
        out.push(Shape::rect_filled(r, 4.0, accent));
        out.push(Shape::galley(r.center() - galley.size() / 2.0, galley, Color32::WHITE));
    }

    pub fn paint_overlay(&mut self, painter: &Painter, dark_ui: bool) {
        let mut out: Vec<Shape> = Vec::new();
        let accent = self.prefs.accent();
        let o = self.origin.to_vec2();
        let k = if self.prefs.big_handles { 1.6 } else { 1.0 };

        // Other people: their selection, the stroke they are drawing, their laser.
        for (_, s) in &self.presence.peers {
            let color = peer_color(s);
            if let Some(sel) = s["sel"].as_array() {
                for id in sel.iter().take(200).filter_map(|v| v.as_str()) {
                    if let Some(el) = self.current(id) {
                        self.outline_el(&mut out, &el, color, 1.5);
                    }
                }
            }
            let live = &s["live"];
            if let (Some(pts), Some(size)) = (live["pts"].as_array(), live["size"].as_f64()) {
                let pts: Vec<f64> = pts.iter().take(3000).filter_map(|v| v.as_f64()).collect();
                let c = live["color"].as_str().filter(|c| c.len() == 7).and_then(parse_color).unwrap_or(color);
                if pts.len() >= 3 && pts.len() % 3 == 0 && size > 0.0 {
                    self.stroke_shape(&mut out, &pts, size, live["hl"].as_bool().unwrap_or(false), c, live["op"].as_f64().unwrap_or(1.0));
                }
            }
            if let Some(l) = s["laser"].as_array() {
                let flat: Vec<f64> = l.iter().take(200).filter_map(|v| v.as_f64()).collect();
                self.laser_shape(&mut out, &flat[..flat.len() / 2 * 2], color);
            }
        }

        // Hover outline (Figma style), fading in.
        let ctx = painter.ctx().clone();
        let g = &self.gesture;
        let quiet = g.is_none();
        if let Some(h) = &self.hover
            && (quiet || matches!(g, Some(Gesture::Create { .. } | Gesture::Endpoint { .. })))
            && !self.selection.contains(h)
            && let Some(el) = self.current(h)
        {
            let a = crate::ui::motion::entering(&ctx, egui::Id::new("hover-outline"), h).clamp(0.0, 1.0);
            self.outline_el(&mut out, &el, accent.gamma_multiply(a), if quiet { 1.5 } else { 2.0 });
        }
        // A new selection: its frame fades in and the handles grow into place with a little give.
        let ks = crate::ui::motion::entering(&ctx, egui::Id::new("selection-in"), &self.selection);
        let k = k * (0.5 + 0.5 * ks);
        let accent_in = accent.gamma_multiply(ks.clamp(0.0, 1.0));

        // Selection.
        let sel = self.selected();
        if !sel.is_empty() && !matches!(g, Some(Gesture::Draw(_))) {
            if sel.len() > 1 {
                for el in &sel {
                    self.outline_el(&mut out, el, accent, 1.0);
                }
            }
            let handles = self.handle_positions();
            let frame = self.selection_frame();
            let single_line = sel.len() == 1 && sel[0].is_line();
            if let Some((b, rot)) = frame
                && !single_line
            {
                let pts: Vec<Pos2> = corners_of(b, rot, 0.0).iter().map(|p| self.sp(p.x, p.y)).collect();
                out.push(Shape::closed_line(pts, Stroke::new(1.0, accent_in)));
                if let Some(&(_, r)) = handles.iter().find(|(h, _)| *h == Handle::Rot) {
                    // The knob hangs from the top edge, or from the top-right corner when the "+" take the top.
                    let from = if handles.iter().any(|(h, _)| *h == Handle::AddN) { Handle::Ne } else { Handle::N };
                    if let Some(&(_, t)) = handles.iter().find(|(h, _)| *h == from) {
                        out.push(Shape::line_segment([t + o, r + o], Stroke::new(1.0, accent)));
                    }
                }
            }
            if single_line {
                self.outline_el(&mut out, &sel[0], accent, 1.0);
            }
            if quiet || matches!(g, Some(Gesture::Resize { .. } | Gesture::Rotate { .. } | Gesture::Endpoint { .. })) {
                for &(h, p) in &handles {
                    let p = p + o;
                    if h.is_add() {
                        if !quiet {
                            continue;
                        }
                        // FigJam's "+": a filled dot that grows on a spring when the pointer is on it.
                        let r = crate::ui::motion::spring(&ctx, egui::Id::new(("add-handle", h as u8)), if self.hover_handle == Some(h) { 9.0 } else { 7.0 }, 0.25, 0.6) * k;
                        out.push(Shape::circle_filled(p, r, accent));
                        let st = Stroke::new(1.5, Color32::WHITE);
                        out.push(Shape::line_segment([p - vec2(r * 0.45, 0.0), p + vec2(r * 0.45, 0.0)], st));
                        out.push(Shape::line_segment([p - vec2(0.0, r * 0.45), p + vec2(0.0, r * 0.45)], st));
                        continue;
                    }
                    let st = Stroke::new(if k > 1.0 { 1.5 } else { 1.0 }, accent);
                    if matches!(h, Handle::Rot | Handle::P0 | Handle::P1) {
                        out.push(circle(p, 4.5 * k, Color32::WHITE, st));
                    } else {
                        let r = egui::Rect::from_center_size(pos2(p.x.round(), p.y.round()), vec2(7.0 * k, 7.0 * k));
                        out.push(Shape::rect_filled(r, 0.0, Color32::WHITE));
                        out.push(Shape::rect_stroke(r, 0.0, st, egui::StrokeKind::Middle));
                    }
                }
            }
            // Size label under the selection while transforming.
            if let Some((b, rot)) = frame {
                if matches!(g, Some(Gesture::Resize { .. })) {
                    let bottom = self.sp(b.x + b.w / 2.0, b.bottom());
                    self.label(painter, &mut out, format!("{} × {}", b.w.round(), b.h.round()), bottom.x, bottom.y + 18.0 + if rot != 0.0 { 12.0 } else { 0.0 }, accent);
                }
                if matches!(g, Some(Gesture::Rotate { .. })) && sel.len() == 1 {
                    let f = frame_box(&sel[0]);
                    let bottom = self.sp(f.x + f.w / 2.0, f.bottom());
                    let deg = (sel[0].rotation.to_degrees() % 360.0 + 360.0) % 360.0;
                    self.label(painter, &mut out, format!("{}°", deg.round()), bottom.x, bottom.y + 18.0, accent);
                }
            }
        }
        if let Some(Gesture::Create { el, .. }) = &self.gesture {
            let b = frame_box(el);
            let bottom = self.sp(b.x + b.w / 2.0, b.bottom());
            self.label(painter, &mut out, format!("{} × {}", b.w.round(), b.h.round()), bottom.x, bottom.y + 18.0, accent);
        }

        // Votes: during the session only your own, everyone's once it has ended.
        if let Some((_, ended)) = self.board.voting() {
            let counts = if ended { self.board.tally(None) } else { self.board.tally(Some(&self.voter)) };
            for (id, n) in counts {
                let Some(el) = self.current(&id).filter(|e| !e.hidden) else { continue };
                let b = frame_box(&el);
                let at = self.sp(b.right(), b.y) + vec2(-4.0, 4.0);
                out.push(circle(at, 11.0, if ended { Color32::from_rgb(0xE8, 0x59, 0x0C) } else { accent }, Stroke::new(2.0, Color32::WHITE)));
                let g = painter.layout_no_wrap(n.to_string(), medium(11.0), Color32::WHITE);
                out.push(Shape::galley(at + vec2(0.0, 0.5) - g.size() / 2.0, g, Color32::WHITE));
            }
        }

        match &self.gesture {
            Some(Gesture::Marquee { start, cur, .. }) => {
                let r = egui::Rect::from_two_pos(self.sp(start.x, start.y), self.sp(cur.x, cur.y));
                out.push(Shape::rect_filled(r, 0.0, alpha(accent, 0.08)));
                out.push(Shape::rect_stroke(r, 0.0, Stroke::new(1.0, accent), egui::StrokeKind::Inside));
            }
            Some(Gesture::Lasso { poly, .. }) if poly.len() > 4 => {
                let mut pts: Vec<Pos2> = poly.chunks_exact(2).map(|q| self.sp(q[0], q[1])).collect();
                out.push(Shape::mesh(fill_mesh(&Path::polygon(&pts.iter().flat_map(|p| [p.x as f64, p.y as f64]).collect::<Vec<_>>()), alpha(accent, 0.06))));
                pts.push(pts[0]);
                out.extend(Shape::dashed_line(&pts, Stroke::new(1.5, accent), 5.0, 4.0));
            }
            Some(Gesture::Draw(d)) => self.stroke_shape(&mut out, &d.pts, d.size, d.hl, parse_color(&d.color).unwrap_or(DARK), d.opacity),
            _ => {}
        }

        for l in &self.guides {
            let (a, b) = (self.sp(l[0], l[1]), self.sp(l[2], l[3]));
            out.push(Shape::line_segment([pos2(a.x.round() + 0.5, a.y.round() + 0.5), pos2(b.x.round() + 0.5, b.y.round() + 0.5)], Stroke::new(1.0, GUIDE)));
        }

        if !self.laser.is_empty() {
            let t = now();
            self.laser.retain(|p| t - p.2 < 900.0);
            let flat: Vec<f64> = self.laser.iter().flat_map(|p| [p.0, p.1]).collect();
            self.laser_shape(&mut out, &flat, LASER);
        }

        if let Some(e) = self.eraser_at
            && (self.tool == Tool::Eraser || matches!(self.gesture, Some(Gesture::Erase { .. })))
        {
            out.push(circle(e + o, (self.prefs.eraser.size / 2.0) as f32, Color32::from_white_alpha(89), Stroke::new(1.0, Color32::from_black_alpha(140))));
        }

        if self.ruler.visible {
            self.ruler_shapes(painter, &mut out, dark_ui);
        }
        self.cursor_shapes(painter, &mut out);
        painter.extend(out);
    }

    fn ruler_shapes(&self, painter: &Painter, out: &mut Vec<Shape>, dark: bool) {
        let r = self.ruler;
        let (l, h) = (RULER_LENGTH as f32, RULER_HEIGHT as f32);
        let (sin, cos) = (r.angle as f32).sin_cos();
        let c = pos2(r.x as f32, r.y as f32) + self.origin.to_vec2();
        let at = |x: f32, y: f32| {
            let (dx, dy) = (x - l / 2.0, y - h / 2.0);
            c + vec2(dx * cos - dy * sin, dx * sin + dy * cos)
        };
        // Body: a rounded rectangle.
        let mut body = Vec::new();
        let rad = 6.0;
        for (cx, cy, a0) in [(l - rad, rad, -90.0_f32), (l - rad, h - rad, 0.0), (rad, h - rad, 90.0), (rad, rad, 180.0)] {
            for i in 0..=6 {
                let a = (a0 + i as f32 * 15.0).to_radians();
                body.push(at(cx + a.cos() * rad, cy + a.sin() * rad));
            }
        }
        let (fill, edge, tick, text) = if dark {
            (Color32::from_rgba_unmultiplied(44, 44, 44, 219), Color32::from_white_alpha(41), Color32::from_white_alpha(153), Color32::from_white_alpha(179))
        } else {
            (Color32::from_rgba_unmultiplied(255, 255, 255, 209), Color32::from_black_alpha(41), Color32::from_black_alpha(140), Color32::from_black_alpha(153))
        };
        out.push(Shape::convex_polygon(body, fill, Stroke::new(1.0, edge)));
        let mm_count = ((l - 80.0) / PX_PER_MM) as i32;
                for mm in 0..=mm_count {
            let x = 40.0 + mm as f32 * PX_PER_MM;
            let len = if mm % 10 == 0 { 18.0 } else if mm % 5 == 0 { 12.0 } else { 7.0 };
            out.push(Shape::line_segment([at(x, 0.0), at(x, len)], Stroke::new(1.0, tick)));
            out.push(Shape::line_segment([at(x, h), at(x, h - len)], Stroke::new(1.0, tick)));
            if mm % 10 == 0 {
                rotated_text(out, painter, at(x, 30.0), &(mm / 10).to_string(), medium(9.0), text, r.angle as f32);
            }
        }
        // Angle, in the middle.
        let deg = (r.angle.to_degrees() % 360.0 + 360.0) % 360.0;
        let shown = if deg > 180.0 { 360.0 - deg } else { deg }.round();
        let mut pill = Vec::new();
        for (cx, a0) in [(l / 2.0 + 15.0, -90.0_f32), (l / 2.0 - 15.0, 90.0)] {
            for i in 0..=12 {
                let a = (a0 + i as f32 * 15.0).to_radians();
                pill.push(at(cx + a.cos() * 11.0, h / 2.0 + 7.0 + a.sin() * 11.0));
            }
        }
        out.push(Shape::convex_polygon(pill, DARK, Stroke::NONE));
        rotated_text(out, painter, at(l / 2.0, h / 2.0 + 7.0), &format!("{shown}°"), medium(11.0), Color32::WHITE, r.angle as f32);
        // Rotation knob.
        let knob = at(l - 26.0, h / 2.0);
        out.push(circle(knob, 13.0, Color32::WHITE, Stroke::new(1.0, Color32::from_black_alpha(46))));
        let arc: Vec<Pos2> = (0..=10).map(|i| (-60.0 + i as f32 * 27.0_f32).to_radians()).map(|a| knob + vec2(a.cos() * 5.0, a.sin() * 5.0)).collect();
        let end = *arc.last().unwrap();
        out.push(Shape::line(arc, Stroke::new(1.6, DARK)));
        out.push(Shape::line(vec![end + vec2(-3.5, -1.5), end, end + vec2(1.0, -3.8)], Stroke::new(1.6, DARK)));
    }

    /// Other people's cursors with their name, or what they are typing (cursor chat).
    fn cursor_shapes(&self, painter: &Painter, out: &mut Vec<Shape>) {
        let ctx = painter.ctx();
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1) as f64;
        for (key, s) in &self.presence.peers {
            let (Some(x), Some(y)) = (s["cursor"]["x"].as_f64(), s["cursor"]["y"].as_f64()) else { continue };
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            // Positions arrive a few times a second: the cursor glides to each one, as in Figma,
            // in board units so it does not lag behind when you move the view.
            let id = egui::Id::new(("peer-cursor", key));
            let (x, y) = match ctx.data(|d| d.get_temp::<(f64, f64)>(id)) {
                Some((px, py)) => {
                    let k = 1.0 - (-dt / 0.07).exp();
                    let (nx, ny) = (px + (x - px) * k, py + (y - py) * k);
                    if (x - nx).hypot(y - ny) * self.cam.z < 0.3 {
                        (x, y)
                    } else {
                        ctx.request_repaint();
                        (nx, ny)
                    }
                }
                None => (x, y),
            };
            ctx.data_mut(|d| d.insert_temp(id, (x, y)));
            let p = self.sp(x, y);
            let color = peer_color(s);
            let on = if is_dark(&hex(color)) { Color32::WHITE } else { DARK };
            let arrow = [(2.0, 1.5), (15.5, 7.6), (9.0, 9.4), (6.9, 16.0)].map(|(a, b)| p + vec2(a, b));
            out.push(Shape::mesh(fill_mesh(&Path::polygon(&arrow.iter().flat_map(|q| [q.x as f64, q.y as f64]).collect::<Vec<_>>()), color)));
            out.push(Shape::closed_line(arrow.to_vec(), Stroke::new(1.4, Color32::WHITE)));
            let name = peer_name(s);
            let chat = s["chat"].as_str().map(str::trim).filter(|c| !c.is_empty()).map(|c| c.chars().take(80).collect::<String>());
            let origin = p + vec2(14.0, 16.0);
            match chat {
                None => {
                    let g = painter.layout_no_wrap(name, medium(11.0), on);
                    let r = egui::Rect::from_min_size(origin, g.size() + vec2(12.0, 4.0));
                    out.push(Shape::rect_filled(r, egui::CornerRadius { nw: 4, ne: 12, sw: 12, se: 12 }, color));
                    out.push(Shape::galley(origin + vec2(6.0, 2.0), g, on));
                }
                Some(chat) => {
                    let small = painter.layout_no_wrap(name, medium(10.0), alpha(on, 0.8));
                    let body = painter.layout(chat, FontId::proportional(12.0), on, 240.0);
                    let size = vec2(small.size().x.max(body.size().x), small.size().y + body.size().y) + vec2(20.0, 10.0);
                    out.push(Shape::rect_filled(egui::Rect::from_min_size(origin, size), egui::CornerRadius { nw: 4, ne: 14, sw: 14, se: 14 }, color));
                    let h = small.size().y;
                    out.push(Shape::galley(origin + vec2(10.0, 4.0), small, on));
                    out.push(Shape::galley(origin + vec2(10.0, 4.0 + h), body, on));
                }
            }
        }
    }
}

/// Text centred on `at`, turned by `angle`.
fn rotated_text(out: &mut Vec<Shape>, painter: &Painter, at: Pos2, text: &str, font: FontId, color: Color32, angle: f32) {
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    let half: Vec2 = galley.size() / 2.0;
    let (s, c) = angle.sin_cos();
    let offset = vec2(half.x * c - half.y * s, half.x * s + half.y * c);
    out.push(Shape::Text(TextShape::new(at - offset, galley, color).with_angle(angle)));
}
