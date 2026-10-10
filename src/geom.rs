//! Board geometry: boxes, rotation, hit testing, scaling and connectors.

use crate::model::{El, EraseMark, Kind, Line, ShapeKind};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

pub const fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl BBox {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn center(&self) -> Pt {
        pt(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.y >= self.y && p.x <= self.right() && p.y <= self.bottom()
    }
    /// Whether `inner` lies wholly inside.
    pub fn holds(&self, inner: &BBox) -> bool {
        inner.x >= self.x && inner.y >= self.y && inner.right() <= self.right() && inner.bottom() <= self.bottom()
    }
}

/// screen = world * z + (x, y)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Camera {
    pub fn to_world(&self, sx: f64, sy: f64) -> Pt {
        pt((sx - self.x) / self.z, (sy - self.y) / self.z)
    }
    pub fn to_screen(&self, wx: f64, wy: f64) -> Pt {
        pt(wx * self.z + self.x, wy * self.z + self.y)
    }
}

pub fn rotate(px: f64, py: f64, cx: f64, cy: f64, angle: f64) -> Pt {
    if angle == 0.0 {
        return pt(px, py);
    }
    let (s, c) = angle.sin_cos();
    let dx = px - cx;
    let dy = py - cy;
    pt(cx + dx * c - dy * s, cy + dx * s + dy * c)
}

pub fn center(el: &El) -> Pt {
    pt(el.x + el.w / 2.0, el.y + el.h / 2.0)
}

pub fn frame(el: &El) -> BBox {
    BBox { x: el.x, y: el.y, w: el.w, h: el.h }
}

/// How far the visible stroke reaches past the element's box.
pub fn stroke_pad(el: &El) -> f64 {
    match &el.kind {
        Kind::Ink(i) | Kind::Highlighter(i) => i.size / 2.0 + 1.0,
        Kind::Line(l) => l.stroke_width * if l.arrow_end || l.arrow_start { 3.0 } else { 0.5 } + 1.0,
        Kind::Shape(s) => s.stroke_width / 2.0 + 1.0,
        _ => 0.0,
    }
}

pub fn corners_of(b: BBox, rotation: f64, pad: f64) -> [Pt; 4] {
    let c = b.center();
    [(b.x - pad, b.y - pad), (b.right() + pad, b.y - pad), (b.right() + pad, b.bottom() + pad), (b.x - pad, b.bottom() + pad)].map(|(x, y)| rotate(x, y, c.x, c.y, rotation))
}

pub fn corners(el: &El, pad: f64) -> [Pt; 4] {
    corners_of(frame(el), el.rotation, pad)
}

pub fn bounds_of(points: impl IntoIterator<Item = Pt>) -> BBox {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    BBox { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// Axis-aligned box around everything the element paints.
pub fn aabb(el: &El) -> BBox {
    bounds_of(corners(el, stroke_pad(el)))
}

/// Axis-aligned box around the element's own frame (no stroke padding): what selection handles use.
pub fn frame_box(el: &El) -> BBox {
    bounds_of(corners(el, 0.0))
}

pub fn union(boxes: impl IntoIterator<Item = BBox>) -> Option<BBox> {
    let mut any = false;
    let b = bounds_of(boxes.into_iter().flat_map(|b| {
        any = true;
        [pt(b.x, b.y), pt(b.right(), b.bottom())]
    }));
    any.then_some(b)
}

pub fn intersects(a: &BBox, b: &BBox) -> bool {
    a.x <= b.right() && b.x <= a.right() && a.y <= b.bottom() && b.y <= a.bottom()
}

pub fn expand(b: BBox, d: f64) -> BBox {
    BBox { x: b.x - d, y: b.y - d, w: b.w + d * 2.0, h: b.h + d * 2.0 }
}

pub fn normalize_box(x1: f64, y1: f64, x2: f64, y2: f64) -> BBox {
    BBox { x: x1.min(x2), y: y1.min(y2), w: (x2 - x1).abs(), h: (y2 - y1).abs() }
}

pub fn dist_to_segment(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len).clamp(0.0, 1.0) } else { 0.0 };
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// Shortest distance between segments AB and CD.
#[allow(clippy::too_many_arguments)]
pub fn segment_distance(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, dx: f64, dy: f64) -> f64 {
    let cross = |ox: f64, oy: f64, px: f64, py: f64, qx: f64, qy: f64| (px - ox) * (qy - oy) - (py - oy) * (qx - ox);
    let d1 = cross(ax, ay, bx, by, cx, cy);
    let d2 = cross(ax, ay, bx, by, dx, dy);
    let d3 = cross(cx, cy, dx, dy, ax, ay);
    let d4 = cross(cx, cy, dx, dy, bx, by);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return 0.0;
    }
    dist_to_segment(ax, ay, cx, cy, dx, dy).min(dist_to_segment(bx, by, cx, cy, dx, dy)).min(dist_to_segment(cx, cy, ax, ay, bx, by)).min(dist_to_segment(dx, dy, ax, ay, bx, by))
}

/// Even-odd test; `poly` is flat [x, y, …].
pub fn point_in_polygon(x: f64, y: f64, poly: &[f64]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 6 {
        return false;
    }
    let mut j = n - 2;
    let mut i = 0;
    while i < n {
        let (xi, yi, xj, yj) = (poly[i], poly[i + 1], poly[j], poly[j + 1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
        i += 2;
    }
    inside
}

/// Regular polygon with a point at the top, stretched to fill the box.
fn regular(n: usize, w: f64, h: f64) -> Vec<f64> {
    let raw: Vec<(f64, f64)> = (0..n).map(|i| (-std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / n as f64).sin_cos()).map(|(s, c)| (c, s)).collect();
    let left = raw.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let top = raw.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let sx = raw.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max) - left;
    let sy = raw.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) - top;
    raw.iter().flat_map(|&(x, y)| [(x - left) / sx * w, (y - top) / sy * h]).collect()
}

/// Outline of a shape as flat [x, y, …] in local coordinates; None for rect and ellipse.
pub fn shape_polygon(kind: ShapeKind, w: f64, h: f64, custom: Option<&[f32]>) -> Option<Vec<f64>> {
    use ShapeKind::*;
    Some(match kind {
        Triangle => vec![w / 2.0, 0.0, w, h, 0.0, h],
        TriangleDown => vec![0.0, 0.0, w, 0.0, w / 2.0, h],
        Diamond => vec![w / 2.0, 0.0, w, h / 2.0, w / 2.0, h, 0.0, h / 2.0],
        Parallelogram => vec![w * 0.25, 0.0, w, 0.0, w * 0.75, h, 0.0, h],
        Pentagon => regular(5, w, h),
        Hexagon => vec![w * 0.25, 0.0, w * 0.75, 0.0, w, h / 2.0, w * 0.75, h, w * 0.25, h, 0.0, h / 2.0],
        Octagon => vec![w * 0.3, 0.0, w * 0.7, 0.0, w, h * 0.3, w, h * 0.7, w * 0.7, h, w * 0.3, h, 0.0, h * 0.7, 0.0, h * 0.3],
        Plus => {
            let (a, b) = (1.0 / 3.0, 2.0 / 3.0);
            vec![w * a, 0.0, w * b, 0.0, w * b, h * a, w, h * a, w, h * b, w * b, h * b, w * b, h, w * a, h, w * a, h * b, 0.0, h * b, 0.0, h * a, w * a, h * a]
        }
        ArrowRight => vec![0.0, h * 0.28, w * 0.6, h * 0.28, w * 0.6, 0.0, w, h / 2.0, w * 0.6, h, w * 0.6, h * 0.72, 0.0, h * 0.72],
        ArrowLeft => vec![w, h * 0.28, w * 0.4, h * 0.28, w * 0.4, 0.0, 0.0, h / 2.0, w * 0.4, h, w * 0.4, h * 0.72, w, h * 0.72],
        Star => (0..10)
            .flat_map(|i| {
                let r = if i % 2 == 1 { 0.4 } else { 1.0 };
                let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
                [w / 2.0 + a.cos() * r * w / 2.0, h / 2.0 + a.sin() * r * h / 2.0 + h * 0.05]
            })
            .collect(),
        Polygon => custom.unwrap_or(&[]).iter().enumerate().map(|(i, &v)| v as f64 * if i % 2 == 1 { h } else { w }).collect(),
        Rect | Ellipse => return None,
    })
}

/// Point in the element's unrotated local space, with (0, 0) at its top-left.
pub fn to_local(el: &El, x: f64, y: f64) -> Pt {
    let c = center(el);
    let p = rotate(x, y, c.x, c.y, -el.rotation);
    pt(p.x - el.x, p.y - el.y)
}

/// Local point back to board coordinates.
pub fn to_world_of(el: &El, lx: f64, ly: f64) -> Pt {
    let c = center(el);
    rotate(el.x + lx, el.y + ly, c.x, c.y, el.rotation)
}

fn near_polyline<T: Copy + Into<f64>>(pts: &[T], stride: usize, x: f64, y: f64, tol: f64, closed: bool) -> bool {
    let n = pts.len();
    let at = |i: usize| (pts[i].into(), pts[i + 1].into());
    if n < stride {
        return false;
    }
    if n == stride {
        let (px, py) = at(0);
        return (px - x).hypot(py - y) <= tol;
    }
    let mut i = 0;
    while i + stride < n {
        let (ax, ay) = at(i);
        let (bx, by) = at(i + stride);
        if dist_to_segment(x, y, ax, ay, bx, by) <= tol {
            return true;
        }
        i += stride;
    }
    if closed && n > stride * 2 {
        let (ax, ay) = at(n - stride);
        let (bx, by) = at(0);
        return dist_to_segment(x, y, ax, ay, bx, by) <= tol;
    }
    false
}

/// Whether a local point lies under a full-strength pixel eraser mark.
pub fn erased_at(el: &El, x: f64, y: f64) -> bool {
    el.erase.iter().any(|m| m.a >= 0.99 && near_polyline(&m.p, 2, x, y, m.s / 2.0, false))
}

/// Whether a world point touches the element; `tol` is in world units. Points the pixel
/// eraser removed don't count, unless `ignore_erase`.
pub fn hit_test(el: &El, x: f64, y: f64, tol: f64, ignore_erase: bool) -> bool {
    if el.hidden {
        return false;
    }
    let p = to_local(el, x, y);
    if !ignore_erase && !el.erase.is_empty() && erased_at(el, p.x, p.y) {
        return false;
    }
    let in_box = |pad: f64| p.x >= -pad && p.y >= -pad && p.x <= el.w + pad && p.y <= el.h + pad;
    match &el.kind {
        // Only the border: the inside is free for selecting and drawing what the section holds.
        Kind::Section { .. } => in_box(tol) && !(p.x > tol && p.y > tol && p.x < el.w - tol && p.y < el.h - tol),
        Kind::Ink(i) | Kind::Highlighter(i) => in_box(i.size + tol) && near_polyline(&i.points, 3, p.x, p.y, i.size / 2.0 + tol, false),
        Kind::Line(l) => near_polyline(&l.points, 2, p.x, p.y, l.stroke_width / 2.0 + tol, false),
        Kind::Shape(s) => {
            // Text inside makes the whole shape clickable, as if it were filled.
            let filled = s.fill != "transparent" || s.text.as_ref().is_some_and(|t| !t.is_empty());
            let edge = s.stroke_width / 2.0 + tol;
            match s.shape {
                ShapeKind::Rect => in_box(edge) && (filled || !(p.x > edge && p.y > edge && p.x < el.w - edge && p.y < el.h - edge)),
                ShapeKind::Ellipse => {
                    let (rx, ry) = (el.w / 2.0, el.h / 2.0);
                    let d = ((p.x - rx) / rx.max(1e-6)).hypot((p.y - ry) / ry.max(1e-6));
                    let band = edge / rx.min(ry).max(1e-6);
                    if filled { d <= 1.0 + band } else { (d - 1.0).abs() <= band }
                }
                k => {
                    let poly = shape_polygon(k, el.w, el.h, s.points.as_deref()).unwrap_or_default();
                    (filled && point_in_polygon(p.x, p.y, &poly)) || near_polyline(&poly, 2, p.x, p.y, edge, true)
                }
            }
        }
        _ => in_box(tol),
    }
}

/// World-space sample points used by lasso selection.
pub fn sample_points(el: &El) -> Vec<Pt> {
    match &el.kind {
        Kind::Ink(i) | Kind::Highlighter(i) => {
            let step = (i.points.len() / 3 / 24).max(3);
            i.points.chunks_exact(3).step_by(step).map(|p| to_world_of(el, p[0] as f64, p[1] as f64)).collect()
        }
        Kind::Line(l) => vec![to_world_of(el, l.points[0] as f64, l.points[1] as f64), to_world_of(el, l.points[2] as f64, l.points[3] as f64), center(el)],
        _ => {
            let mut v = corners(el, 0.0).to_vec();
            v.push(center(el));
            v
        }
    }
}

/// Smallest stroke width or font size, in board units (a 2 px pen at the deepest zoom is 0.06).
pub const MIN_SIZE: f64 = 0.01;

/// The element after its selection box goes from `from` to `to`. Rotated elements keep their
/// rotation; their centre and size follow the scale.
pub fn scale_element(el: &El, from: BBox, to: BBox) -> El {
    let sx = if from.w > 0.01 { to.w / from.w } else { 1.0 };
    let sy = if from.h > 0.01 { to.h / from.h } else { 1.0 };
    let c = center(el);
    let ncx = to.x + (c.x - from.x) * sx;
    let ncy = to.y + (c.y - from.y) * sy;
    // For a rotated element, project the box scale onto its own axes.
    let (cos, sin) = (el.rotation.cos().abs(), el.rotation.sin().abs());
    let ex = sx * cos + sy * sin;
    let ey = sx * sin + sy * cos;
    let w = (el.w * ex).max(0.0);
    let h = (el.h * ey).max(0.0);
    let mut next = El { x: ncx - w / 2.0, y: ncy - h / 2.0, w, h, ..el.clone() };
    match &mut next.kind {
        Kind::Ink(i) | Kind::Highlighter(i) => {
            let (kx, ky) = (if el.w > 0.01 { w / el.w } else { ex }, if el.h > 0.01 { h / el.h } else { ey });
            for p in i.points.chunks_exact_mut(3) {
                p[0] = (p[0] as f64 * kx) as f32;
                p[1] = (p[1] as f64 * ky) as f32;
            }
            // Only a guard against zero: strokes drawn zoomed in are legitimately very thin.
            i.size = (i.size * (ex * ey).abs().sqrt()).max(MIN_SIZE);
        }
        Kind::Line(l) => {
            let (kx, ky) = (if el.w > 0.01 { w / el.w } else { 1.0 }, if el.h > 0.01 { h / el.h } else { 1.0 });
            for p in l.points.chunks_exact_mut(2) {
                p[0] = (p[0] as f64 * kx) as f32;
                p[1] = (p[1] as f64 * ky) as f32;
            }
        }
        Kind::Stamp { .. } => {
            let s = w.max(h);
            next.w = s;
            next.h = s;
            next.x = ncx - s / 2.0;
            next.y = ncy - s / 2.0;
        }
        // Tables widen their columns; code and widgets take the new width; all of them then
        // grow or shrink in height to fit what they hold.
        Kind::Table(t) => {
            let k = if el.w > 0.01 { w / el.w } else { 1.0 };
            for c in &mut t.cols {
                *c = (*c * k).max(24.0);
            }
            crate::text::fit_text(&mut next);
            next.y = ncy - next.h / 2.0;
            next.x = ncx - next.w / 2.0;
        }
        Kind::Code(_) | Kind::Widget(_) => {
            next.w = w.max(160.0);
            crate::text::fit_text(&mut next);
            next.y = ncy - next.h / 2.0;
            next.x = ncx - next.w / 2.0;
        }
        _ => {}
    }
    if !el.erase.is_empty() {
        let kx = if el.w > 0.01 { next.w / el.w } else { ex };
        let ky = if el.h > 0.01 { next.h / el.h } else { ey };
        next.erase = scale_marks(&el.erase, kx, ky);
    }
    next
}

pub fn scale_marks(marks: &[EraseMark], kx: f64, ky: f64) -> Vec<EraseMark> {
    marks
        .iter()
        .map(|m| EraseMark { p: m.p.iter().enumerate().map(|(i, &v)| (v as f64 * if i % 2 == 1 { ky } else { kx }) as f32).collect(), s: m.s * (kx * ky).abs().sqrt(), a: m.a })
        .collect()
}

/* ---------------- lines and connectors ---------------- */

/// The two ends of a line in board coordinates.
pub fn line_world_ends(el: &El, l: &Line) -> (Pt, Pt) {
    (to_world_of(el, l.points[0] as f64, l.points[1] as f64), to_world_of(el, l.points[2] as f64, l.points[3] as f64))
}

pub fn ends_of(el: &El) -> (Pt, Pt) {
    el.line().map(|l| line_world_ends(el, l)).unwrap_or_default()
}

/// The line moved to new ends; its eraser marks follow (turned and stretched with it).
pub fn set_line_ends(el: &El, a: Pt, b: Pt) -> El {
    let (oa, ob) = ends_of(el);
    let (min_x, min_y) = (a.x.min(b.x), a.y.min(b.y));
    let mut next = El { rotation: 0.0, x: min_x, y: min_y, w: (b.x - a.x).abs(), h: (b.y - a.y).abs(), ..el.clone() };
    if let Some(l) = next.line_mut() {
        l.points = vec![(a.x - min_x) as f32, (a.y - min_y) as f32, (b.x - min_x) as f32, (b.y - min_y) as f32];
    }
    if !el.erase.is_empty() {
        let u = (ob.x - oa.x).hypot(ob.y - oa.y);
        let k = if u > 1e-6 { (b.x - a.x).hypot(b.y - a.y) / u } else { 1.0 };
        let turn = if u > 1e-6 { (b.y - a.y).atan2(b.x - a.x) - (ob.y - oa.y).atan2(ob.x - oa.x) } else { 0.0 };
        next.erase = el
            .erase
            .iter()
            .map(|m| {
                let p = m
                    .p
                    .chunks_exact(2)
                    .flat_map(|q| {
                        let w = to_world_of(el, q[0] as f64, q[1] as f64);
                        let r = rotate(a.x + (w.x - oa.x) * k, a.y + (w.y - oa.y) * k, a.x, a.y, turn);
                        [(r.x - min_x) as f32, (r.y - min_y) as f32]
                    })
                    .collect();
                EraseMark { p, s: m.s * k, a: m.a }
            })
            .collect();
    }
    next
}

/// Point where the ray from the element's centre towards `toward` crosses its outline, pushed out by `gap`.
pub fn edge_point(el: &El, toward: Pt, gap: f64) -> Pt {
    let l = to_local(el, toward.x, toward.y);
    let (hw, hh) = (el.w / 2.0, el.h / 2.0);
    let dx = l.x - hw;
    let mut dy = l.y - hh;
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        dy = -1.0;
    }
    let inv = |d: f64, half: f64| if d.abs() > 1e-9 { half / d.abs() } else { f64::INFINITY };
    let mut t = inv(dx, hw).min(inv(dy, hh));
    if let Some(s) = el.shape() {
        if s.shape == ShapeKind::Ellipse {
            t = 1.0 / (dx * dx / (hw * hw).max(1e-9) + dy * dy / (hh * hh).max(1e-9)).sqrt();
        } else if let Some(poly) = shape_polygon(s.shape, el.w, el.h, s.points.as_deref()) {
            t = ray_polygon(hw, hh, dx, dy, &poly).unwrap_or(t);
        }
    }
    let len = dx.hypot(dy);
    let k = t + gap / len;
    to_world_of(el, hw + dx * k, hh + dy * k)
}

/// Smallest t > 0 where (ox, oy) + t·(dx, dy) crosses the closed polygon.
fn ray_polygon(ox: f64, oy: f64, dx: f64, dy: f64, poly: &[f64]) -> Option<f64> {
    let mut best: Option<f64> = None;
    let n = poly.len();
    let mut i = 0;
    while i + 1 < n {
        let (ax, ay, bx, by) = (poly[i], poly[i + 1], poly[(i + 2) % n], poly[(i + 3) % n]);
        let (ex, ey) = (bx - ax, by - ay);
        let den = dx * ey - dy * ex;
        if den.abs() >= 1e-12 {
            let t = ((ax - ox) * ey - (ay - oy) * ex) / den;
            let u = ((ax - ox) * dy - (ay - oy) * dx) / den;
            if t > 1e-9 && (-1e-9..=1.0 + 1e-9).contains(&u) && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
        i += 2;
    }
    best
}

/// Elements a connector can attach to.
pub fn connectable(el: Option<&El>) -> bool {
    el.is_some_and(|el| !matches!(el.kind, Kind::Line(_) | Kind::Ink(_) | Kind::Highlighter(_) | Kind::Comment { .. }) && !el.hidden)
}

/// A connector with its ends re-attached to the elements they belong to, or None if it has none.
pub fn route_connector<'a>(el: &El, get: impl Fn(&str) -> Option<&'a El>) -> Option<El> {
    let l = el.line()?;
    let a = l.from.as_deref().and_then(&get).filter(|e| connectable(Some(e)));
    let b = l.to.as_deref().and_then(&get).filter(|e| connectable(Some(e)));
    if a.is_none() && b.is_none() {
        return None;
    }
    let (pa, pb) = line_world_ends(el, l);
    let ca = a.map(center).unwrap_or(pa);
    let cb = b.map(center).unwrap_or(pb);
    let gap = l.stroke_width * 1.5;
    Some(set_line_ends(el, a.map(|a| edge_point(a, cb, gap)).unwrap_or(pa), b.map(|b| edge_point(b, ca, gap)).unwrap_or(pb)))
}

/// Lets go of one end (or both) of a connector.
pub fn unbind(el: &El, from: bool, to: bool) -> El {
    let mut next = el.clone();
    if let Some(l) = next.line_mut() {
        if from {
            l.from = None;
        }
        if to {
            l.to = None;
        }
    }
    next
}

pub fn snap_angle(angle: f64, step: f64) -> f64 {
    (angle / step).round() * step
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model::*;

    pub fn base(kind: Kind) -> El {
        El { id: "a".into(), x: 0.0, y: 0.0, w: 0.0, h: 0.0, rotation: 0.0, z: 0.0, opacity: 1.0, locked: false, hidden: false, name: None, group_id: None, erase: vec![], kind }
    }
    pub fn rect(id: &str, x: f64, w: f64, h: f64, shape: ShapeKind) -> El {
        let mut el = base(Kind::Shape(Shape { shape, fill: "transparent".into(), stroke: "#000000".into(), stroke_width: 2.0, radius: 0.0, dash: false, points: None, text: None, font: None }));
        el.id = id.into();
        el.x = x;
        el.w = w;
        el.h = h;
        el
    }
    fn ink(points: Vec<f32>, w: f64, h: f64, size: f64) -> El {
        El { w, h, ..base(Kind::Ink(Ink { points, color: "#000".into(), size })) }
    }
    pub fn line(points: Vec<f32>, width: f64) -> El {
        base(Kind::Line(Line { points, stroke: "#000000".into(), stroke_width: width, dash: false, arrow_start: false, arrow_end: false, from: None, to: None, tape: false }))
    }

    #[test]
    fn hit_testing_respects_stroke_width_fill_and_rotation() {
        let i = ink(vec![0.0, 0.0, 0.5, 100.0, 0.0, 0.5], 100.0, 0.0, 10.0);
        assert!(hit_test(&i, 50.0, 4.0, 0.0, false));
        assert!(!hit_test(&i, 50.0, 12.0, 0.0, false));
        let b = rect("b", 0.0, 100.0, 50.0, ShapeKind::Rect);
        assert!(hit_test(&b, 0.0, 25.0, 2.0, false), "edge of an empty rectangle");
        assert!(!hit_test(&b, 50.0, 25.0, 2.0, false), "inside of an empty rectangle is click-through");
        let mut filled = b.clone();
        if let Kind::Shape(s) = &mut filled.kind {
            s.fill = "#fff".into();
        }
        assert!(hit_test(&filled, 50.0, 25.0, 2.0, false));
        let turned = El { rotation: std::f64::consts::FRAC_PI_2, ..filled.clone() };
        assert!(hit_test(&turned, 50.0, 70.0, 1.0, false));
        assert!(!hit_test(&turned, 90.0, 25.0, 1.0, false));
        let bb = aabb(&El { rotation: std::f64::consts::FRAC_PI_2, ..b });
        assert!((bb.w - 54.0).abs() < 0.01 && (bb.h - 104.0).abs() < 0.01, "{bb:?}");
    }

    #[test]
    fn scaling_a_stroke_scales_its_points_and_thickness() {
        let i = El { x: 10.0, y: 10.0, ..ink(vec![0.0, 0.0, 0.5, 100.0, 50.0, 0.5], 100.0, 50.0, 4.0) };
        let p = scale_element(&i, BBox { x: 10.0, y: 10.0, w: 100.0, h: 50.0 }, BBox { x: 10.0, y: 10.0, w: 200.0, h: 100.0 });
        assert_eq!(p.ink().unwrap().points, vec![0.0, 0.0, 0.5, 200.0, 100.0, 0.5]);
        assert_eq!(p.ink().unwrap().size, 8.0);
        assert_eq!(p.x, 10.0);
        let mut thin = i.clone();
        thin.ink_mut().unwrap().size = 0.06;
        let t = scale_element(&thin, BBox { x: 10.0, y: 10.0, w: 100.0, h: 50.0 }, BBox { x: 10.0, y: 10.0, w: 110.0, h: 55.0 });
        assert!((t.ink().unwrap().size - 0.066).abs() < 1e-9);
    }

    #[test]
    fn helpers() {
        assert_eq!(segment_distance(0.0, 0.0, 10.0, 10.0, 0.0, 10.0, 10.0, 0.0), 0.0);
        assert_eq!(segment_distance(0.0, 0.0, 10.0, 0.0, 0.0, 5.0, 10.0, 5.0), 5.0);
        let sq = [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0];
        assert!(point_in_polygon(5.0, 5.0, &sq));
        assert!(!point_in_polygon(15.0, 5.0, &sq));
    }

    #[test]
    fn connectors_attach_to_the_outline_toward_each_other() {
        let a = rect("a", 0.0, 100.0, 100.0, ShapeKind::Rect);
        let b = rect("b", 300.0, 100.0, 100.0, ShapeKind::Ellipse);
        let mut l = line(vec![0.0; 4], 0.0);
        if let Some(ln) = l.line_mut() {
            ln.from = Some("a".into());
            ln.to = Some("b".into());
        }
        let get = |id: &str| match id {
            "a" => Some(&a),
            "b" => Some(&b),
            _ => None,
        };
        let r = route_connector(&l, get).unwrap();
        let (p, q) = ends_of(&r);
        assert!((p.x - 100.0).abs() < 1e-6 && (p.y - 50.0).abs() < 1e-6, "{p:?}");
        assert!((q.x - 300.0).abs() < 1e-6 && (q.y - 50.0).abs() < 1e-6, "{q:?}");
        let d = rect("d", 0.0, 100.0, 100.0, ShapeKind::Diamond);
        let e = edge_point(&d, pt(1000.0, 1000.0), 0.0);
        assert!((e.x - 75.0).abs() < 1e-6 && (e.y - 75.0).abs() < 1e-6, "{e:?}");
        assert!(route_connector(&line(vec![0.0; 4], 0.0), |_| None).is_none());
    }

    #[test]
    fn pixel_eraser_marks_move_turn_and_scale_with_what_they_cut() {
        let mut i = ink(vec![0.0, 0.0, 0.5, 100.0, 0.0, 0.5], 100.0, 0.0, 10.0);
        i.erase = vec![EraseMark { p: vec![50.0, -20.0, 50.0, 20.0], s: 10.0, a: 1.0 }];
        assert!(!hit_test(&i, 50.0, 0.0, 1.0, false));
        assert!(hit_test(&i, 50.0, 0.0, 1.0, true));
        assert!(hit_test(&i, 20.0, 0.0, 1.0, false));
        let wide = scale_element(&i, BBox { x: 0.0, y: -5.0, w: 100.0, h: 10.0 }, BBox { x: 0.0, y: -5.0, w: 200.0, h: 10.0 });
        assert_eq!(wide.erase[0].p.iter().step_by(2).copied().collect::<Vec<_>>(), vec![100.0, 100.0]);
        let mut l = line(vec![0.0, 0.0, 100.0, 0.0], 4.0);
        l.w = 100.0;
        l.erase = vec![EraseMark { p: vec![50.0, 0.0], s: 6.0, a: 1.0 }];
        let up = set_line_ends(&l, pt(0.0, 0.0), pt(0.0, -100.0));
        let m = &up.erase[0].p;
        assert!((up.x + m[0] as f64).abs() < 1e-5 && (up.y + m[1] as f64 + 50.0).abs() < 1e-5, "{} {}", up.x + m[0] as f64, up.y + m[1] as f64);
    }

    #[test]
    fn every_shape_has_an_outline_inside_its_box() {
        use ShapeKind::*;
        for kind in [TriangleDown, Parallelogram, Pentagon, Octagon, Plus, ArrowRight, ArrowLeft] {
            let poly = shape_polygon(kind, 80.0, 40.0, None).unwrap();
            assert!(poly.len() >= 6);
            let xs: Vec<f64> = poly.iter().step_by(2).copied().collect();
            let ys: Vec<f64> = poly.iter().skip(1).step_by(2).copied().collect();
            let (x0, x1) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
            let (y0, y1) = (ys.iter().copied().fold(f64::MAX, f64::min), ys.iter().copied().fold(f64::MIN, f64::max));
            assert!(x0 >= -1e-9 && x1 <= 80.0 + 1e-9 && y0 >= -1e-9 && y1 <= 40.0 + 1e-9, "{kind:?}");
            assert!(x1 - x0 > 79.0 && y1 - y0 > 39.0, "{kind:?} fills its box");
        }
    }
}
