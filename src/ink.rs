//! Freehand strokes: their shape, smoothing, pressure, ink-to-shape recognition and the ruler.

use crate::geom::{Pt, dist_to_segment, pt};
use crate::prims::Path;

/// A stroke's shape. `pts` is flat [x, y, pressure, …]; pressure < 0 means the device had no
/// pressure and the line keeps an even width.
///
/// Round dabs of the pen at its points and the bands joining them, all filled together
/// (non-zero): their union, which can never fold over itself. A single outline round the
/// stroke (perfect-freehand's) did, in the tight turns of thick pens and highlighters, and
/// left hairs and notches like a dry brush.
pub fn stroke_path(pts: &[f32], size: f64, highlighter: bool) -> Path {
    let mut path = Path::default();
    if size <= 0.0 || !size.is_finite() || pts.len() < 3 {
        return path;
    }
    let real = pts.chunks_exact(3).any(|p| p[2] >= 0.0);
    let thinning = if highlighter || !real { 0.0 } else { 0.42 };
    let radius = |p: f32| size * (0.5 - thinning * (0.5 - if p < 0.0 { 0.5 } else { p.clamp(0.0, 1.0) as f64 }));
    // Mouse and pen jitter smoothed out; the last point stays where the pen was lifted.
    let t = if highlighter { 0.75 } else { 0.85 };
    let n = pts.len() / 3;
    let mut c: Vec<[f64; 3]> = Vec::with_capacity(n);
    for (i, p) in pts.chunks_exact(3).enumerate() {
        let q = [p[0] as f64, p[1] as f64, radius(p[2])];
        if !q.iter().all(|v| v.is_finite()) {
            continue;
        }
        c.push(match c.last() {
            Some(a) if i + 1 < n => [a[0] + (q[0] - a[0]) * t, a[1] + (q[1] - a[1]) * t, a[2] + (q[2] - a[2]) * t],
            _ => q,
        });
    }
    let c = simplify3(&c, size * 0.02);
    for &[x, y, r] in &c {
        circle(&mut path, x, y, r);
    }
    for w in c.windows(2) {
        let ([x0, y0, r0], [x1, y1, r1]) = (w[0], w[1]);
        let d = (x1 - x0).hypot(y1 - y0);
        let s = (r0 - r1) / d;
        // One dab inside the other: nothing to join.
        if !(s.abs() < 1.0) {
            continue;
        }
        // The two lines touching both dabs, and the band between them (turning the same way
        // as the dabs, or the union would have holes).
        let (ux, uy) = ((x1 - x0) / d, (y1 - y0) / d);
        let k = (1.0 - s * s).sqrt();
        let (ax, ay) = (-uy * k + ux * s, ux * k + uy * s);
        let (bx, by) = (uy * k + ux * s, -ux * k + uy * s);
        path.add_polygon(&[x0 + r0 * bx, y0 + r0 * by, x1 + r1 * bx, y1 + r1 * by, x1 + r1 * ax, y1 + r1 * ay, x0 + r0 * ax, y0 + r0 * ay]);
    }
    path
}

/// A circle of cubic arcs, turning as `Path::ellipse` does.
fn circle(p: &mut Path, x: f64, y: f64, r: f64) {
    p.cmds.extend(Path::ellipse(x as f32, y as f32, r as f32, r as f32).cmds);
}

/// Douglas–Peucker on (x, y, radius): the points a stroke needs, within `eps`.
fn simplify3(pts: &[[f64; 3]], eps: f64) -> Vec<[f64; 3]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let far = |a: [f64; 3], b: [f64; 3], p: [f64; 3]| {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
        let l2 = ab.iter().map(|v| v * v).sum::<f64>();
        let t = if l2 > 0.0 { (ap.iter().zip(&ab).map(|(x, y)| x * y).sum::<f64>() / l2).clamp(0.0, 1.0) } else { 0.0 };
        ap.iter().zip(&ab).map(|(x, y)| (x - y * t).powi(2)).sum::<f64>()
    };
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut todo = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = todo.pop() {
        let (i, d) = (a + 1..b).map(|i| (i, far(pts[a], pts[b], pts[i]))).fold((a, 0.0), |m, x| if x.1 > m.1 { x } else { m });
        if d > eps * eps {
            keep[i] = true;
            todo.extend([(a, i), (i, b)]);
        }
    }
    pts.iter().zip(keep).filter(|x| x.1).map(|x| *x.0).collect()
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
    /// A straight line with a head drawn at its end (in the same stroke).
    Arrow { x1: f64, y1: f64, x2: f64, y2: f64 },
    Rect { x: f64, y: f64, w: f64, h: f64 },
    /// A rectangle drawn askew: centre, size and angle (radians).
    Turned { cx: f64, cy: f64, w: f64, h: f64, angle: f64 },
    Ellipse { x: f64, y: f64, w: f64, h: f64 },
    Diamond { x: f64, y: f64, w: f64, h: f64 },
    Triangle([Pt; 3]),
    Star { x: f64, y: f64, w: f64, h: f64 },
    /// Any other closed shape with straight sides (pentagon, hexagon, trapezoid…): its corners.
    Polygon(Vec<Pt>),
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

fn dist(a: Pt, b: Pt) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// The stroke as points `step` apart along it, so fast and slow parts weigh the same.
fn resample(pts: &[Pt], step: f64) -> Vec<Pt> {
    let mut out = vec![pts[0]];
    let mut carry = 0.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = dist(a, b);
        if d <= 0.0 {
            continue;
        }
        let mut t = step - carry;
        while t <= d {
            out.push(pt(a.x + (b.x - a.x) * t / d, a.y + (b.y - a.y) * t / d));
            t += step;
        }
        carry = d - (t - step);
    }
    if dist(*out.last().unwrap(), *pts.last().unwrap()) > step * 0.3 {
        out.push(*pts.last().unwrap());
    }
    out
}

/// Ramer–Douglas–Peucker: the corners of a polyline, within `eps`.
fn simplify(pts: &[Pt], eps: f64) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (mut best, mut at) = (0.0, 0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = dist_to_segment(p.x, p.y, a.x, a.y, b.x, b.y);
        if d > best {
            (best, at) = (d, i);
        }
    }
    if best <= eps {
        return vec![a, b];
    }
    let mut left = simplify(&pts[..=at], eps);
    left.pop();
    left.extend(simplify(&pts[at..], eps));
    left
}

/// Turn at `b` going from `a` to `c`, 0 (straight on) to π (back).
fn turn(a: Pt, b: Pt, c: Pt) -> f64 {
    let (u, v) = ((b.x - a.x, b.y - a.y), (c.x - b.x, c.y - b.y));
    let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
    if lu <= 0.0 || lv <= 0.0 {
        return 0.0;
    }
    ((u.0 * v.0 + u.1 * v.1) / (lu * lv)).clamp(-1.0, 1.0).acos()
}

/// The corners of a closed stroke: its outline simplified, nearly straight corners and corners
/// crowded together dropped.
fn corners(ring: &[Pt], size: f64) -> Vec<Pt> {
    // Split where the outline is farthest from its start, so no corner is cut in two.
    let start = ring[0];
    let far = (0..ring.len()).max_by(|&i, &j| dist(ring[i], start).total_cmp(&dist(ring[j], start))).unwrap_or(0);
    let mut c = simplify(&ring[..=far], size * 0.045);
    c.pop();
    let mut back = ring[far..].to_vec();
    back.push(start);
    c.extend(simplify(&back, size * 0.045));
    c.pop();
    loop {
        let n = c.len();
        if n < 3 {
            return c;
        }
        let perimeter: f64 = (0..n).map(|i| dist(c[i], c[(i + 1) % n])).sum();
        // The weakest corner: the straightest, or one on a very short side.
        let score = |i: usize| {
            let (a, b, d) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
            let short = dist(a, b).min(dist(b, d)) / perimeter;
            (turn(a, b, d), short)
        };
        let weakest = (0..n).min_by(|&i, &j| score(i).0.total_cmp(&score(j).0)).unwrap();
        let (angle, _) = score(weakest);
        let tiny = (0..n).find(|&i| dist(c[i], c[(i + 1) % n]) < perimeter * 0.06);
        if angle < 0.42 {
            c.remove(weakest);
        } else if let Some(i) = tiny {
            // Two corners almost on top of each other: one, half-way.
            let j = (i + 1) % n;
            let m = pt((c[i].x + c[j].x) / 2.0, (c[i].y + c[j].y) / 2.0);
            c[i] = m;
            c.remove(j);
        } else {
            return c;
        }
    }
}

/// Smallest rectangle around the points, turned to any angle: centre, width, height, angle.
fn min_rect(h: &[Pt]) -> (Pt, f64, f64, f64) {
    let mut best = (pt(0.0, 0.0), 0.0, 0.0, 0.0, f64::INFINITY);
    for i in 0..h.len() {
        let (a, b) = (h[i], h[(i + 1) % h.len()]);
        let ang = (b.y - a.y).atan2(b.x - a.x);
        let (s, c) = ang.sin_cos();
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in h {
            let (u, v) = (p.x * c + p.y * s, -p.x * s + p.y * c);
            (x0, y0, x1, y1) = (x0.min(u), y0.min(v), x1.max(u), y1.max(v));
        }
        let a = (x1 - x0) * (y1 - y0);
        if a < best.4 {
            let (u, v) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            best = (pt(u * c - v * s, u * s + v * c), x1 - x0, y1 - y0, ang, a);
        }
    }
    (best.0, best.1, best.2, best.3)
}

/// Guesses which shape a freehand stroke was meant to be; None keeps the ink as drawn. `px` is
/// one screen pixel in board units: thresholds are on screen, so it works the same at any zoom,
/// and strokes smaller than 48 px are handwriting, never shapes.
pub fn recognize(flat: &[f32], px: f64) -> Option<Recognized> {
    let raw: Vec<Pt> = flat.chunks_exact(3).map(|p| pt(p[0] as f64, p[1] as f64)).collect();
    if raw.len() < 4 {
        return None;
    }
    let length: f64 = raw.windows(2).map(|w| dist(w[0], w[1])).sum();
    if length < 96.0 * px {
        return None;
    }
    let pts = resample(&raw, (length / 160.0).max(px * 0.5));
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let gap = dist(first, last);
    let b = crate::geom::bounds_of(pts.iter().copied());
    let size = b.w.hypot(b.h);
    // Open strokes: a line, or an arrow (a line and its head, drawn in one go).
    if gap > length * 0.2 && gap > size * 0.3 {
        if gap > length * 0.85 {
            let dev = pts.iter().map(|p| dist_to_segment(p.x, p.y, first.x, first.y, last.x, last.y)).fold(0.0, f64::max);
            if dev < (6.0 * px).max(length * 0.06) {
                return Some(Recognized::Line { x1: first.x, y1: first.y, x2: last.x, y2: last.y });
            }
        }
        let s = simplify(&pts, size * 0.05);
        // The shaft is the first and longest leg; what follows stays near its end and is short.
        if s.len() >= 3 && s.len() <= 6 {
            let shaft = dist(s[0], s[1]);
            let rest: f64 = s.windows(2).skip(1).map(|w| dist(w[0], w[1])).sum();
            let near = s[2..].iter().all(|p| dist(*p, s[1]) < shaft * 0.45);
            if shaft > rest * 1.1 && near && rest > shaft * 0.12 && turn(s[0], s[1], s[2]) > 2.0 {
                return Some(Recognized::Arrow { x1: s[0].x, y1: s[0].y, x2: s[1].x, y2: s[1].y });
            }
        }
        return None;
    }
    if b.w.max(b.h) < 48.0 * px || b.w.min(b.h) < 12.0 * px {
        return None;
    }
    // Closed: the outline once round (the overlap where it was closed cut off).
    let mut ring = pts.clone();
    if let Some(i) = (pts.len() * 3 / 4..pts.len()).min_by(|&i, &j| dist(pts[i], first).total_cmp(&dist(pts[j], first))) {
        if dist(pts[i], first) < size * 0.15 {
            ring.truncate(i + 1);
        }
    }
    let h = hull(&ring);
    let hull_area = area(&h);
    if hull_area <= 0.0 {
        return None;
    }
    let (x, y, w, hh) = (b.x, b.y, b.w, b.h);
    // An ellipse: every point near the ellipse in its box.
    let (rx, ry) = (w / 2.0, hh / 2.0);
    let (cx, cy) = (x + rx, y + ry);
    let err = ring.iter().map(|p| (((p.x - cx) / rx).hypot((p.y - cy) / ry) - 1.0).abs()).sum::<f64>() / ring.len() as f64;
    let c = corners(&ring, size);
    // Whichever fits the stroke better: the ellipse, or the polygon through its corners.
    let ellipse_off = err * (rx + ry) / 2.0;
    let polygon_off = if c.len() >= 3 {
        ring.iter().map(|p| (0..c.len()).map(|i| {
            let (a, d) = (c[i], c[(i + 1) % c.len()]);
            dist_to_segment(p.x, p.y, a.x, a.y, d.x, d.y)
        }).fold(f64::INFINITY, f64::min)).sum::<f64>() / ring.len() as f64
    } else {
        f64::INFINITY
    };
    if err < 0.16 && (c.len() < 3 || c.len() > 10 || ellipse_off < polygon_off * 0.9) {
        return Some(Recognized::Ellipse { x, y, w, h: hh });
    }
    match c.len() {
        3 => Some(Recognized::Triangle([c[0], c[1], c[2]])),
        4 => {
            let (center, rw, rh, angle) = min_rect(&h);
            let fill = hull_area / (rw * rh);
            // Corners in the middle of the box's sides: a diamond.
            let mid = [pt(cx, y), pt(x + w, cy), pt(cx, y + hh), pt(x, cy)];
            let diamond = c.iter().all(|p| mid.iter().any(|m| dist(*p, *m) < size * 0.14));
            if diamond && hull_area / (w * hh) < 0.68 {
                return Some(Recognized::Diamond { x, y, w, h: hh });
            }
            if fill > 0.86 {
                // Square to the page, or turned.
                let a = angle.rem_euclid(std::f64::consts::FRAC_PI_2);
                let off = a.min(std::f64::consts::FRAC_PI_2 - a);
                if off < 0.12 {
                    return Some(Recognized::Rect { x, y, w, h: hh });
                }
                return Some(Recognized::Turned { cx: center.x, cy: center.y, w: rw, h: rh, angle });
            }
            Some(Recognized::Polygon(c))
        }
        10 => {
            // A star: points far out and corners deep in, one after the other.
            let ctr = pt(c.iter().map(|p| p.x).sum::<f64>() / 10.0, c.iter().map(|p| p.y).sum::<f64>() / 10.0);
            let r: Vec<f64> = c.iter().map(|p| dist(*p, ctr)).collect();
            let (odd, even): (f64, f64) = (r.iter().skip(1).step_by(2).sum::<f64>() / 5.0, r.iter().step_by(2).sum::<f64>() / 5.0);
            if odd.min(even) < odd.max(even) * 0.75 {
                return Some(Recognized::Star { x, y, w, h: hh });
            }
            None
        }
        5..=8 => Some(Recognized::Polygon(c)),
        _ => None,
    }
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
    // On the ruler itself: along the nearer edge, as with a real one.
    if across.abs() <= RULER_HEIGHT / 2.0 {
        let side = if across < 0.0 { -1.0 } else { 1.0 };
        return Some(RulerSnap { r: *r, offset: side * (RULER_HEIGHT / 2.0 + radius) });
    }
    for side in [-1.0, 1.0] {
        let edge = side * RULER_HEIGHT / 2.0;
        let d = (across - edge) * side;
        // Start within 36 px outside the edge.
        if d > -10.0 && d < 36.0 {
            return Some(RulerSnap { r: *r, offset: edge + side * radius });
        }
    }
    None
}

/// Whether a screen point is on the ruler's middle pill (the angle), where a pen grabs it to
/// move it instead of drawing along it.
pub fn on_ruler_pill(r: &Ruler, sx: f64, sy: f64) -> bool {
    if !r.visible {
        return false;
    }
    let (dx, dy) = (sx - r.x, sy - r.y);
    let (s, c) = r.angle.sin_cos();
    let (along, across) = (dx * c + dy * s, -dx * s + dy * c);
    along.abs() <= 34.0 && (across - 7.0).abs() <= 18.0
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
    fn kind(r: Option<Recognized>) -> String {
        match r {
            Some(Recognized::Line { .. }) => "line".into(),
            Some(Recognized::Arrow { .. }) => "arrow".into(),
            Some(Recognized::Rect { .. }) => "rect".into(),
            Some(Recognized::Turned { .. }) => "turned".into(),
            Some(Recognized::Ellipse { .. }) => "ellipse".into(),
            Some(Recognized::Diamond { .. }) => "diamond".into(),
            Some(Recognized::Triangle(_)) => "triangle".into(),
            Some(Recognized::Star { .. }) => "star".into(),
            Some(Recognized::Polygon(p)) => format!("polygon{}", p.len()),
            None => "none".into(),
        }
    }

    /// A hand's wobble: a little sideways noise, the same every run.
    fn shaky(pts: Vec<(f64, f64)>, amount: f64) -> Vec<(f64, f64)> {
        pts.into_iter().enumerate().map(|(i, (x, y))| (x + (i as f64 * 1.7).sin() * amount, y + (i as f64 * 2.3).cos() * amount)).collect()
    }

    fn regular(n: usize, r: f64, inner: Option<f64>) -> Vec<(f64, f64)> {
        let k = if inner.is_some() { n * 2 } else { n };
        let mut corners: Vec<(f64, f64)> = (0..k)
            .map(|i| {
                let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / k as f64;
                let rr = if i % 2 == 1 { inner.unwrap_or(r) } else { r };
                (200.0 + a.cos() * rr, 200.0 + a.sin() * rr)
            })
            .collect();
        corners.push(corners[0]);
        polyline(&corners)
    }

    #[test]
    fn more_shapes_drawn_by_hand_are_recognised() {
        let k = |p: Vec<(f64, f64)>| kind(recognize(&flat(&p), 1.0));
        assert_eq!(k(shaky(circle(150.0, 150.0, 90.0), 2.0)), "ellipse");
        assert_eq!(k(shaky(polyline(&[(0.0, 0.0), (240.0, 0.0), (240.0, 150.0), (0.0, 150.0), (0.0, 4.0)]), 2.0)), "rect");
        // A rectangle drawn at an angle.
        let turned: Vec<(f64, f64)> = [(0.0, 0.0), (220.0, 0.0), (220.0, 120.0), (0.0, 120.0), (0.0, 3.0)].iter().map(|&(x, y): &(f64, f64)| {
            let (s, c) = 0.5f64.sin_cos();
            (x * c - y * s + 300.0, x * s + y * c + 100.0)
        }).collect();
        assert_eq!(k(shaky(polyline(&turned), 1.5)), "turned");
        assert_eq!(k(shaky(polyline(&[(100.0, 0.0), (200.0, 100.0), (100.0, 200.0), (0.0, 100.0), (98.0, 2.0)]), 1.5)), "diamond");
        assert_eq!(k(shaky(regular(5, 110.0, None), 1.5)), "polygon5");
        assert_eq!(k(shaky(regular(6, 110.0, None), 1.5)), "polygon6");
        assert_eq!(k(shaky(regular(5, 120.0, Some(50.0)), 1.0)), "star");
        // An arrow: the shaft, then the two barbs in the same stroke.
        assert_eq!(k(polyline(&[(0.0, 100.0), (260.0, 100.0), (225.0, 75.0), (260.0, 100.0), (225.0, 125.0)])), "arrow");
        assert_eq!(k(shaky(polyline(&[(0.0, 0.0), (300.0, 60.0)]), 1.0)), "line");
        // Handwriting stays handwriting.
        let word: Vec<(f64, f64)> = (0..300).map(|i| {
            let t = i as f64 / 300.0 * std::f64::consts::TAU * 6.0;
            (i as f64 * 1.2 - 25.0 * t.sin(), 25.0 * (1.0 - t.cos()))
        }).collect();
        assert_eq!(k(word), "none");
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

    /// The points a path passes through (ends of lines and curves), for comparisons.
    fn ends(p: &Path) -> Vec<(f64, f64)> {
        p.cmds
            .iter()
            .filter_map(|c| match *c {
                crate::prims::Cmd::Move(x, y) | crate::prims::Cmd::Line(x, y) | crate::prims::Cmd::Quad(_, _, x, y) | crate::prims::Cmd::Cubic(_, _, _, _, x, y) => Some((x as f64, y as f64)),
                crate::prims::Cmd::Close => None,
            })
            .collect()
    }

    #[test]
    fn a_stroke_written_zoomed_in_looks_the_same_on_screen_as_at_100() {
        let stroke = |zoom: f64| {
            let pts: Vec<f32> = (0..=20).flat_map(|i| [(i as f64 * 2.0 / zoom) as f32, (((i as f64 / 20.0) * std::f64::consts::PI).sin() * 10.0 / zoom) as f32, -1.0]).collect();
            ends(&stroke_path(&pts, 4.0 / zoom, false)).into_iter().map(|p| (p.0 * zoom, p.1 * zoom)).collect::<Vec<_>>()
        };
        let base = stroke(1.0);
        assert!(base.len() > 8);
        for zoom in [4.0, 16.0, 32.0] {
            let out = stroke(zoom);
            assert_eq!(out.len(), base.len(), "zoom {zoom}");
            for (a, b) in out.iter().zip(&base) {
                // f32 storage of the scaled-down input limits the match.
                assert!((a.0 - b.0).hypot(a.1 - b.1) < 2e-3, "zoom {zoom}");
            }
        }
        // A tap is a dot the size of the pen, not a dash.
        let dot = ends(&stroke_path(&[10.0, 10.0, -1.0], 4.0 / 32.0, false));
        assert!(!dot.is_empty());
        assert!(dot.iter().map(|p| (p.0 - 10.0).hypot(p.1 - 10.0)).fold(0.0, f64::max) * 32.0 < 3.0);
        let small: Vec<f32> = flat(&circle(100.0, 100.0, 80.0)).iter().enumerate().map(|(i, &v)| if i % 3 == 2 { v } else { v / 16.0 }).collect();
        assert_eq!(kind(recognize(&small, 1.0 / 16.0)), "ellipse");
    }

    #[test]
    fn a_thick_stroke_is_one_clean_band_even_drawn_with_a_shaky_mouse() {
        // A tight wave drawn with a mouse (positions rounded to screen pixels, zoomed out) with a
        // 96 px highlighter: every pixel on the stroke's centre line is inside, every pixel
        // farther than its half width is outside: no notches, holes or hairs.
        let z = 0.56;
        let mut pts = Vec::new();
        let mut t: f64 = 0.0;
        while t < 1.0 {
            let (sx, sy) = (60.0 + t * 520.0 + 40.0 * (t * 19.0).sin(), 160.0 + 70.0 * (t * 9.0).sin() + 30.0 * (t * 23.0).cos());
            pts.extend([(sx.round() / z) as f32, (sy.round() / z) as f32, -1.0]);
            t += 0.0012 + 0.0016 * (t * 97.0).sin().abs();
        }
        let thin = thin_points(&pts, 0.4 / z);
        let mesh = crate::paint::fill_mesh(&stroke_path(&thin, 96.0, true), egui::Color32::WHITE);
        let inside = |x: f64, y: f64| {
            mesh.indices.chunks_exact(3).any(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize].pos);
                let s = |p: egui::Pos2, q: egui::Pos2| (q.x - p.x) as f64 * (y - p.y as f64) - (q.y - p.y) as f64 * (x - p.x as f64);
                let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
                !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
            })
        };
        let line: Vec<Pt> = thin.chunks_exact(3).map(|p| pt(p[0] as f64, p[1] as f64)).collect();
        let away = |x: f64, y: f64| line.windows(2).map(|w| dist_to_segment(x, y, w[0].x, w[0].y, w[1].x, w[1].y)).fold(f64::INFINITY, f64::min);
        let (mut holes, mut hairs) = (0, 0);
        let mut y = 100.0;
        while y < 520.0 {
            let mut x = 40.0;
            while x < 1140.0 {
                let d = away(x, y);
                if d < 40.0 && !inside(x, y) {
                    holes += 1;
                }
                if d > 52.0 && d < 110.0 && inside(x, y) {
                    hairs += 1;
                }
                x += 5.0;
            }
            y += 5.0;
        }
        assert_eq!((holes, hairs), (0, 0));
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
        // Pressed on the ruler: along the nearer edge.
        let on = ruler_snapper(&ruler, 450.0, 320.0, 2.0).unwrap();
        assert_eq!(on.apply(500.0, 330.0).y, 344.0);
        assert!(ruler_snapper(&Ruler { visible: false, ..ruler }, 400.0, 248.0, 2.0).is_none());
    }
}

#[cfg(test)]
mod big_stroke_picture {
    /// `TRATTO_SHOT=x.png cargo test big_strokes_picture -- --ignored`: thick strokes drawn
    /// with a mouse (pixel-rounded positions, zoomed out), to look at their edges.
    #[test]
    #[ignore]
    fn big_strokes_picture() {
        let z = 0.56;
        let mut pts = Vec::new();
        let mut t: f64 = 0.0;
        while t < 1.0 {
            // A loop-the-loop wave, sampled like a mouse: screen pixels, uneven steps.
            let (sx, sy) = (60.0 + t * 520.0 + 40.0 * (t * 19.0).sin(), 160.0 + 70.0 * (t * 9.0).sin() + 30.0 * (t * 23.0).cos());
            pts.extend([(sx.round() / z) as f32, (sy.round() / z) as f32, -1.0]);
            t += 0.0012 + 0.0016 * ((t * 97.0).sin().abs());
        }
        let thin = super::thin_points(&pts, 0.4 / z);
        let mut pm = tiny_skia::Pixmap::new(1200, 700).unwrap();
        pm.fill(tiny_skia::Color::WHITE);
        // And a pen with pressure, zoomed in four times, on the right.
        let pen: Vec<f32> = (0..=200).flat_map(|i| {
            let t = i as f64 / 200.0;
            [(t * 120.0) as f32, (20.0 * (t * 9.0).sin()) as f32, (0.15 + 0.8 * (t * 3.1).sin().abs()) as f32]
        }).collect();
        for (dy, size, hl) in [(0.0f32, 64.0, false), (380.0, 96.0, true), (-1.0, 6.0, false)] {
            let path = if dy < 0.0 { super::stroke_path(&pen, size, hl).mapped(|x, y| ((x * 4.0 + 680.0) / z as f32, (y * 4.0 + 300.0) / z as f32)) } else { super::stroke_path(&thin, size, hl) };
            let dy = dy.max(0.0);
            let mut pb = tiny_skia::PathBuilder::new();
            for c in &path.cmds {
                match *c {
                    crate::prims::Cmd::Move(x, y) => pb.move_to(x, y),
                    crate::prims::Cmd::Line(x, y) => pb.line_to(x, y),
                    crate::prims::Cmd::Quad(a, b, x, y) => pb.quad_to(a, b, x, y),
                    crate::prims::Cmd::Cubic(a, b, c, d, x, y) => pb.cubic_to(a, b, c, d, x, y),
                    crate::prims::Cmd::Close => pb.close(),
                }
            }
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(if hl { 240 } else { 30 }, if hl { 200 } else { 30 }, if hl { 40 } else { 30 }, if hl { 115 } else { 255 });
            let tr = tiny_skia::Transform::from_scale(z as f32, z as f32).post_translate(0.0, dy);
            pm.fill_path(&pb.finish().unwrap(), &paint, tiny_skia::FillRule::Winding, tr, None);
        }
        pm.save_png(std::env::var("TRATTO_SHOT").unwrap()).unwrap();
    }
}
