//! Freehand strokes: outline (a port of perfect-freehand 1.2), smoothing, pressure,
//! ink-to-shape recognition and the ruler.

use crate::geom::{Pt, dist_to_segment, pt};

type V = [f64; 2];

const FIXED_PI: f64 = std::f64::consts::PI + 1e-4;

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k]
}
fn per(a: V) -> V {
    [a[1], -a[0]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: V) -> f64 {
    a[0].hypot(a[1])
}
fn uni(a: V) -> V {
    let l = len(a);
    [a[0] / l, a[1] / l]
}
fn dist2(a: V, b: V) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dy * dy
}
fn lrp(a: V, b: V, t: f64) -> V {
    add(a, mul(sub(b, a), t))
}
fn rot_around(a: V, c: V, r: f64) -> V {
    let (s, co) = r.sin_cos();
    let (x, y) = (a[0] - c[0], a[1] - c[1]);
    [x * co - y * s + c[0], x * s + y * co + c[1]]
}

struct StrokePoint {
    point: V,
    pressure: f64,
    vector: V,
    running_length: f64,
}

struct Options {
    size: f64,
    thinning: f64,
    smoothing: f64,
    streamline: f64,
    last: bool,
}

fn stroke_points(input: &[[f64; 3]], o: &Options) -> Vec<StrokePoint> {
    if input.is_empty() {
        return Vec::new();
    }
    let t = 0.15 + (1.0 - o.streamline) * 0.85;
    let mut pts: Vec<[f64; 3]> = input.to_vec();
    // Interpolated points lose their pressure (it becomes the default), as in the original.
    if pts.len() == 2 {
        let last = pts[1];
        pts.truncate(1);
        for i in 1..5 {
            let p = lrp([pts[0][0], pts[0][1]], [last[0], last[1]], i as f64 / 4.0);
            pts.push([p[0], p[1], f64::NAN]);
        }
    }
    if pts.len() == 1 {
        pts.push([pts[0][0] + 1.0, pts[0][1] + 1.0, pts[0][2]]);
    }
    let valid = |p: f64| p >= 0.0;
    let mut out = vec![StrokePoint { point: [pts[0][0], pts[0][1]], pressure: if valid(pts[0][2]) { pts[0][2] } else { 0.25 }, vector: [1.0, 1.0], running_length: 0.0 }];
    let mut reached = false;
    let mut running = 0.0;
    let max = pts.len() - 1;
    for (i, p) in pts.iter().enumerate().skip(1) {
        let prev = out.last().unwrap().point;
        let point = if o.last && i == max { [p[0], p[1]] } else { lrp(prev, [p[0], p[1]], t) };
        if point == prev {
            continue;
        }
        let distance = len(sub(point, prev));
        running += distance;
        if i < max && !reached {
            if running < o.size {
                continue;
            }
            reached = true;
        }
        out.push(StrokePoint { point, pressure: if valid(p[2]) { p[2] } else { 0.5 }, vector: uni(sub(prev, point)), running_length: running });
    }
    out[0].vector = out.get(1).map(|p| p.vector).unwrap_or([0.0, 0.0]);
    out
}

fn outline_points(points: &[StrokePoint], o: &Options) -> Vec<V> {
    let size = o.size;
    if points.is_empty() || size <= 0.0 {
        return Vec::new();
    }
    let n = points.len();
    let total = points[n - 1].running_length;
    let min_distance = (size * o.smoothing).powi(2);
    let radius_of = |pressure: f64| size * (0.5 - o.thinning * (0.5 - pressure));
    let mut left: Vec<V> = Vec::new();
    let mut right: Vec<V> = Vec::new();
    let mut radius = radius_of(points[n - 1].pressure);
    let mut first_radius: Option<f64> = None;
    let mut prev_vector = points[0].vector;
    let mut pl = points[0].point;
    let mut pr = pl;
    let mut prev_sharp = false;
    for (i, sp) in points.iter().enumerate() {
        let is_last = i == n - 1;
        if !is_last && total - sp.running_length < 3.0 {
            continue;
        }
        radius = if o.thinning != 0.0 { radius_of(sp.pressure) } else { size / 2.0 };
        if first_radius.is_none() {
            first_radius = Some(radius);
        }
        radius = radius.max(0.01);
        let next_vector = if is_last { sp.vector } else { points[i + 1].vector };
        let next_dpr = if is_last { 1.0 } else { dot(sp.vector, next_vector) };
        let prev_dpr = dot(sp.vector, prev_vector);
        let sharp = prev_dpr < 0.0 && !prev_sharp;
        let next_sharp = next_dpr < 0.0;
        if sharp || next_sharp {
            let e = mul(per(prev_vector), radius);
            let mut t = 0.0;
            while t <= 1.0 {
                let tl = rot_around(sub(sp.point, e), sp.point, FIXED_PI * t);
                left.push(tl);
                let tr = rot_around(add(sp.point, e), sp.point, -FIXED_PI * t);
                right.push(tr);
                pl = tl;
                pr = tr;
                t += 0.07692307692307693;
            }
            if next_sharp {
                prev_sharp = true;
            }
            continue;
        }
        prev_sharp = false;
        if is_last {
            let e = mul(per(sp.vector), radius);
            left.push(sub(sp.point, e));
            right.push(add(sp.point, e));
            continue;
        }
        let e = mul(per(lrp(next_vector, sp.vector, next_dpr)), radius);
        let tl = sub(sp.point, e);
        if i <= 1 || dist2(pl, tl) > min_distance {
            left.push(tl);
            pl = tl;
        }
        let tr = add(sp.point, e);
        if i <= 1 || dist2(pr, tr) > min_distance {
            right.push(tr);
            pr = tr;
        }
        prev_vector = sp.vector;
    }
    let first = points[0].point;
    let last = if n > 1 { points[n - 1].point } else { add(points[0].point, [1.0, 1.0]) };
    if n == 1 {
        // A dot.
        let r = first_radius.unwrap_or(radius);
        let start = add(first, mul(uni(per(sub(first, add(first, [1.0, 1.0])))), -r));
        let mut out = Vec::new();
        let step = 1.0 / 13.0;
        let mut t = step;
        while t <= 1.0 {
            out.push(rot_around(start, first, FIXED_PI * 2.0 * t));
            t += step;
        }
        return out;
    }
    let mut start_cap = Vec::new();
    if let Some(&r0) = right.first() {
        let step = 1.0 / 13.0;
        let mut t = step;
        while t <= 1.0 {
            start_cap.push(rot_around(r0, first, FIXED_PI * t));
            t += step;
        }
    }
    let direction = per(mul(points[n - 1].vector, -1.0));
    let mut end_cap = Vec::new();
    let start = add(last, mul(direction, radius));
    let step = 1.0 / 29.0;
    let mut t = step;
    while t < 1.0 {
        end_cap.push(rot_around(start, last, FIXED_PI * 3.0 * t));
        t += step;
    }
    right.reverse();
    left.extend(end_cap);
    left.extend(right);
    left.extend(start_cap);
    left
}

/// perfect-freehand's default pen size, the scale its constants were tuned for.
const PEN_UNIT: f64 = 16.0;

/// Outline polygon of a stroke. `pts` is flat [x, y, pressure, …]; pressure < 0 means the
/// device had no pressure and the line keeps an even width.
///
/// perfect-freehand has constants in absolute units that assume screen pixels; strokes are in
/// board units, tiny when drawn zoomed in. So the outline is computed with the pen scaled to a
/// fixed size, then scaled back.
pub fn stroke_outline(pts: &[f32], size: f64, highlighter: bool) -> Vec<Pt> {
    if size <= 0.0 || !size.is_finite() || pts.len() < 3 {
        return Vec::new();
    }
    let k = PEN_UNIT / size;
    let mut real = false;
    let input: Vec<[f64; 3]> = pts
        .chunks_exact(3)
        .map(|p| {
            if p[2] >= 0.0 {
                real = true;
            }
            [p[0] as f64 * k, p[1] as f64 * k, if p[2] < 0.0 { 0.5 } else { p[2] as f64 }]
        })
        .collect();
    // Never derive width from speed: slow movements would swell into round blobs.
    let o = Options { size: PEN_UNIT, thinning: if highlighter || !real { 0.0 } else { 0.42 }, smoothing: 0.5, streamline: if highlighter { 0.3 } else { 0.18 }, last: true };
    outline_points(&stroke_points(&input, &o), &o).into_iter().map(|[x, y]| pt(x / k, y / k)).collect()
}

/// The "1€ filter" (Casiez et al.): heavy smoothing when the pen moves slowly, where tremor is
/// visible, and almost none when it moves fast. Works in screen pixels.
pub struct OneEuro {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    t: f64,
    min_cutoff: f64,
    beta: f64,
}

impl OneEuro {
    pub fn new(min_cutoff: f64) -> Self {
        OneEuro { x: 0.0, y: 0.0, dx: 0.0, dy: 0.0, t: -1.0, min_cutoff, beta: 0.012 }
    }
    fn alpha(cutoff: f64, dt: f64) -> f64 {
        let tau = 1.0 / (std::f64::consts::TAU * cutoff);
        1.0 / (1.0 + tau / dt)
    }
    pub fn filter(&mut self, x: f64, y: f64, t_ms: f64) -> Pt {
        if self.t < 0.0 {
            (self.x, self.y, self.t) = (x, y, t_ms);
            return pt(x, y);
        }
        let dt = ((t_ms - self.t) / 1000.0).max(0.001);
        self.t = t_ms;
        let ad = Self::alpha(1.0, dt);
        self.dx += ad * ((x - self.x) / dt - self.dx);
        self.dy += ad * ((y - self.y) / dt - self.dy);
        let a = Self::alpha(self.min_cutoff + self.beta * self.dx.hypot(self.dy), dt);
        self.x += a * (x - self.x);
        self.y += a * (y - self.y);
        pt(self.x, self.y)
    }
}

/// Pen pressure smoothed (pens report noisy values) and lifted so a light touch still shows.
pub fn next_pressure(prev: Option<f64>, raw: f64) -> f64 {
    let p = raw.clamp(0.0, 1.0);
    match prev {
        None => p.max(0.35),
        Some(prev) => prev + (p - prev) * 0.3,
    }
}

pub fn stored_pressure(smoothed: f64) -> f32 {
    ((0.2 + 0.8 * smoothed) * 1000.0).round() as f32 / 1000.0
}

/// Drops points closer than `min_dist` to the previous kept one; keeps the last point.
pub fn thin_points(pts: &[f32], min_dist: f64) -> Vec<f32> {
    if pts.len() <= 6 {
        return pts.to_vec();
    }
    let mut out = pts[..3].to_vec();
    let n = pts.len();
    let mut i = 3;
    while i < n - 3 {
        let (lx, ly) = (out[out.len() - 3], out[out.len() - 2]);
        if ((pts[i] - lx) as f64).hypot((pts[i + 1] - ly) as f64) >= min_dist {
            out.extend_from_slice(&pts[i..i + 3]);
        }
        i += 3;
    }
    out.extend_from_slice(&pts[n - 3..]);
    out
}

/* ---------------- ink to shape ---------------- */

#[derive(Debug, PartialEq)]
pub enum Recognized {
    Line { x1: f64, y1: f64, x2: f64, y2: f64 },
    Rect { x: f64, y: f64, w: f64, h: f64 },
    Ellipse { x: f64, y: f64, w: f64, h: f64 },
    Diamond { x: f64, y: f64, w: f64, h: f64 },
    Triangle([Pt; 3]),
}

fn hull(points: &[Pt]) -> Vec<Pt> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Pt, a: Pt, b: Pt| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut lower: Vec<Pt> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Pt> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].x * poly[(i + 1) % n].y - poly[(i + 1) % n].x * poly[i].y).sum::<f64>().abs() / 2.0
}

/// Guesses which simple shape a freehand stroke was meant to be; None keeps the ink as drawn.
/// `px` is one screen pixel in board units: thresholds are on screen, so it works the same at
/// any zoom, and strokes smaller than 48 px are handwriting, never shapes.
pub fn recognize(flat: &[f32], px: f64) -> Option<Recognized> {
    let pts: Vec<Pt> = flat.chunks_exact(3).map(|p| pt(p[0] as f64, p[1] as f64)).collect();
    if pts.len() < 4 {
        return None;
    }
    let length: f64 = pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
    if length < 96.0 * px {
        return None;
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let gap = (last.x - first.x).hypot(last.y - first.y);
    // Straight line: endpoints far apart and no point strays from the chord.
    if gap > length * 0.8 {
        let dev = pts.iter().map(|p| dist_to_segment(p.x, p.y, first.x, first.y, last.x, last.y)).fold(0.0, f64::max);
        if dev < (6.0 * px).max(length * 0.06) {
            return Some(Recognized::Line { x1: first.x, y1: first.y, x2: last.x, y2: last.y });
        }
        return None;
    }
    if gap > length * 0.25 {
        return None; // open curve, not a closed shape
    }
    let h = hull(&pts);
    let hull_area = area(&h);
    let b = crate::geom::bounds_of(pts.iter().copied());
    if b.w.max(b.h) < 48.0 * px || b.w.min(b.h) < 12.0 * px || hull_area == 0.0 {
        return None;
    }
    let fill = hull_area / (b.w * b.h);
    // Largest triangle inside the hull: close to the whole hull means it is a triangle.
    let mut best = [h[0]; 3];
    let mut best_area = 0.0;
    for i in 0..h.len() {
        for j in i + 1..h.len() {
            for k in j + 1..h.len() {
                let a = area(&[h[i], h[j], h[k]]);
                if a > best_area {
                    best_area = a;
                    best = [h[i], h[j], h[k]];
                }
            }
        }
    }
    if best_area / hull_area > 0.86 {
        return Some(Recognized::Triangle(best));
    }
    // Ellipse: points sit close to the ellipse inscribed in the box.
    let (rx, ry) = (b.w / 2.0, b.h / 2.0);
    let (cx, cy) = (b.x + rx, b.y + ry);
    let err = pts.iter().map(|p| (((p.x - cx) / rx).hypot((p.y - cy) / ry) - 1.0).abs()).sum::<f64>() / pts.len() as f64;
    let (x, y, w, h) = (b.x, b.y, b.w, b.h);
    if err < 0.11 && fill < 0.9 {
        return Some(Recognized::Ellipse { x, y, w, h });
    }
    if fill > 0.84 {
        return Some(Recognized::Rect { x, y, w, h });
    }
    if fill > 0.4 && fill < 0.62 {
        return Some(Recognized::Diamond { x, y, w, h });
    }
    None
}

/* ---------------- ruler ---------------- */

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ruler {
    pub visible: bool,
    /// Centre in screen pixels.
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

pub const RULER_LENGTH: f64 = 760.0;
pub const RULER_HEIGHT: f64 = 84.0;

/// A stroke starting at one of the ruler's long edges is drawn straight along it.
#[derive(Clone, Copy, Debug)]
pub struct RulerSnap {
    r: Ruler,
    offset: f64,
}

impl RulerSnap {
    pub fn apply(&self, x: f64, y: f64) -> Pt {
        let (uy, ux) = self.r.angle.sin_cos();
        let (nx, ny) = (-uy, ux);
        let t = (x - self.r.x) * ux + (y - self.r.y) * uy;
        pt(self.r.x + ux * t + nx * self.offset, self.r.y + uy * t + ny * self.offset)
    }
}

/// If a screen point starts within reach of one of the ruler's long edges, how later points
/// are projected onto that edge (offset by the pen radius).
pub fn ruler_snapper(r: &Ruler, sx: f64, sy: f64, radius: f64) -> Option<RulerSnap> {
    if !r.visible {
        return None;
    }
    let (uy, ux) = r.angle.sin_cos();
    let (nx, ny) = (-uy, ux);
    let (dx, dy) = (sx - r.x, sy - r.y);
    if (dx * ux + dy * uy).abs() > RULER_LENGTH / 2.0 + 30.0 {
        return None;
    }
    let across = dx * nx + dy * ny;
    for side in [-1.0, 1.0] {
        let edge = side * RULER_HEIGHT / 2.0;
        let d = (across - edge) * side;
        // Start within 36 px outside the edge (or a little inside it).
        if d > -10.0 && d < 36.0 {
            return Some(RulerSnap { r: *r, offset: edge + side * radius });
        }
    }
    None
}

/// Whether a screen point is on the ruler body (used to drag it).
pub fn on_ruler(r: &Ruler, sx: f64, sy: f64) -> bool {
    if !r.visible {
        return false;
    }
    let (dx, dy) = (sx - r.x, sy - r.y);
    let (s, c) = r.angle.sin_cos();
    (dx * c + dy * s).abs() <= RULER_LENGTH / 2.0 && (-dx * s + dy * c).abs() <= RULER_HEIGHT / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(pts: &[(f64, f64)]) -> Vec<f32> {
        pts.iter().flat_map(|&(x, y)| [x as f32, y as f32, 0.5]).collect()
    }
    fn circle(cx: f64, cy: f64, r: f64) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let mut a = 0.0;
        while a <= std::f64::consts::PI * 2.02 {
            out.push((cx + a.cos() * r, cy + a.sin() * r * 0.8));
            a += 0.06;
        }
        out
    }
    fn polyline(corners: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for w in corners.windows(2) {
            let ((x1, y1), (x2, y2)) = (w[0], w[1]);
            let n = (((x2 - x1).hypot(y2 - y1) / 4.0).round() as usize).max(1);
            for k in 0..n {
                out.push((x1 + (x2 - x1) * k as f64 / n as f64, y1 + (y2 - y1) * k as f64 / n as f64));
            }
        }
        out.push(*corners.last().unwrap());
        out
    }
    fn kind(r: Option<Recognized>) -> &'static str {
        match r {
            Some(Recognized::Line { .. }) => "line",
            Some(Recognized::Rect { .. }) => "rect",
            Some(Recognized::Ellipse { .. }) => "ellipse",
            Some(Recognized::Diamond { .. }) => "diamond",
            Some(Recognized::Triangle(_)) => "triangle",
            None => "none",
        }
    }

    #[test]
    fn ink_to_shape_recognises_the_basic_shapes_and_leaves_scribbles_alone() {
        assert_eq!(kind(recognize(&flat(&polyline(&[(0.0, 0.0), (200.0, 40.0)])), 1.0)), "line");
        assert_eq!(kind(recognize(&flat(&circle(100.0, 100.0, 80.0)), 1.0)), "ellipse");
        assert_eq!(kind(recognize(&flat(&polyline(&[(0.0, 0.0), (200.0, 0.0), (200.0, 120.0), (0.0, 120.0), (0.0, 2.0)])), 1.0)), "rect");
        assert_eq!(kind(recognize(&flat(&polyline(&[(100.0, 0.0), (200.0, 160.0), (0.0, 160.0), (98.0, 3.0)])), 1.0)), "triangle");
        let scribble: Vec<(f64, f64)> = (0..100).map(|i| i as f64 / 100.0).map(|t| (t * 300.0, (t * 40.0).sin() * 30.0)).collect();
        assert_eq!(kind(recognize(&flat(&scribble), 1.0)), "none");
    }

    #[test]
    fn a_stroke_written_zoomed_in_looks_the_same_on_screen_as_at_100() {
        let stroke = |zoom: f64| {
            let pts: Vec<f32> = (0..=20).flat_map(|i| [(i as f64 * 2.0 / zoom) as f32, (((i as f64 / 20.0) * std::f64::consts::PI).sin() * 10.0 / zoom) as f32, -1.0]).collect();
            stroke_outline(&pts, 4.0 / zoom, false).into_iter().map(|p| (p.x * zoom, p.y * zoom)).collect::<Vec<_>>()
        };
        let base = stroke(1.0);
        assert!(base.len() > 20);
        for zoom in [4.0, 16.0, 32.0] {
            let out = stroke(zoom);
            assert_eq!(out.len(), base.len(), "zoom {zoom}");
            for (a, b) in out.iter().zip(&base) {
                // f32 storage of the scaled-down input limits the match.
                assert!((a.0 - b.0).hypot(a.1 - b.1) < 1e-3, "zoom {zoom}");
            }
        }
        // A tap is a dot the size of the pen, not a dash.
        let dot = stroke_outline(&[10.0, 10.0, -1.0], 4.0 / 32.0, false);
        assert!(!dot.is_empty());
        assert!(dot.iter().map(|p| (p.x - 10.0).hypot(p.y - 10.0)).fold(0.0, f64::max) * 32.0 < 3.0);
        let small: Vec<f32> = flat(&circle(100.0, 100.0, 80.0)).iter().enumerate().map(|(i, &v)| if i % 3 == 2 { v } else { v / 16.0 }).collect();
        assert_eq!(kind(recognize(&small, 1.0 / 16.0)), "ellipse");
    }

    #[test]
    fn thinning_and_ruler() {
        assert_eq!(thin_points(&[0.0, 0.0, 1.0, 0.1, 0.0, 1.0, 0.2, 0.0, 1.0, 10.0, 0.0, 1.0], 1.0).len(), 6);
        let ruler = Ruler { visible: true, x: 500.0, y: 300.0, angle: 0.0 };
        let snap = ruler_snapper(&ruler, 400.0, 248.0, 2.0).unwrap();
        let (a, b) = (snap.apply(420.0, 230.0), snap.apply(700.0, 262.0));
        assert_eq!(a.y, b.y);
        assert_eq!(a.y, 256.0);
        assert!(ruler_snapper(&ruler, 400.0, 120.0, 2.0).is_none());
        assert!(ruler_snapper(&Ruler { visible: false, ..ruler }, 400.0, 248.0, 2.0).is_none());
    }
}
