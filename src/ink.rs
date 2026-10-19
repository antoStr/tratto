//! Freehand strokes: their shape, smoothing, pressure, ink-to-shape recognition and the ruler.

use crate::geom::{Pt, dist_to_segment, pt};
use crate::model::Brush;
use crate::prims::Path;

/// A stroke's shape. `pts` is flat [x, y, pressure, …]; pressure < 0 means the device had no
/// pressure and the line keeps an even width.
///
/// Round dabs of the pen at its points and the bands joining them, all filled together
/// (non-zero): their union, which can never fold over itself. A single outline round the
/// stroke (perfect-freehand's) did, in the tight turns of thick pens and highlighters, and
/// left hairs and notches like a dry brush.
pub fn stroke_path(pts: &[f32], size: f64, highlighter: bool) -> Path {
    if size <= 0.0 || !size.is_finite() || pts.len() < 3 {
        return Path::default();
    }
    let real = pts.chunks_exact(3).any(|p| p[2] >= 0.0);
    let thinning = if highlighter || !real { 0.0 } else { 0.42 };
    let radius = |p: f32, _: f64, _: f64| size * (0.5 - thinning * (0.5 - if p < 0.0 { 0.5 } else { p.clamp(0.0, 1.0) as f64 }));
    let c = centre_line(pts, if highlighter { 0.75 } else { 0.85 }, radius);
    union_path(&smooth3(&thin3(&c, size * 0.08), size), size)
}

/// The pen's centre line as (x, y, radius): mouse and pen jitter smoothed out (the last point
/// stays where the pen was lifted), the radius from `radius(pressure, distance from the start,
/// distance to the end)`.
fn centre_line(pts: &[f32], t: f64, radius: impl Fn(f32, f64, f64) -> f64) -> Vec<[f64; 3]> {
    let n = pts.len() / 3;
    let mut along = Vec::with_capacity(n);
    let mut s = 0.0;
    for (i, p) in pts.chunks_exact(3).enumerate() {
        if i > 0 {
            let d = ((p[0] - pts[i * 3 - 3]) as f64).hypot((p[1] - pts[i * 3 - 2]) as f64);
            if d.is_finite() {
                s += d;
            }
        }
        along.push(s);
    }
    let mut c: Vec<[f64; 3]> = Vec::with_capacity(n);
    for (i, p) in pts.chunks_exact(3).enumerate() {
        // A pressure that is not a number counts as none.
        let q = [p[0] as f64, p[1] as f64, radius(if p[2].is_finite() { p[2] } else { -1.0 }, along[i], s - along[i])];
        if !q.iter().all(|v| v.is_finite()) {
            continue;
        }
        c.push(match c.last() {
            Some(a) if i + 1 < n => [a[0] + (q[0] - a[0]) * t, a[1] + (q[1] - a[1]) * t, a[2] + (q[2] - a[2]) * t],
            _ => q,
        });
    }
    c
}

/// How wide a brush draws at a point, as a share of its size: from the pressure (below 0: the
/// device has none) and, for the brush pen, how far the point is from either end.
fn brush_width(brush: Brush, p: f32, from_start: f64, to_end: f64, size: f64) -> f64 {
    let p = (p >= 0.0).then(|| p.clamp(0.0, 1.0) as f64);
    match brush {
        Brush::Pen => p.map_or(1.0, |p| 0.14 + 1.36 * p),
        Brush::Pencil => p.map_or(0.9, |p| 0.28 + 0.92 * p),
        Brush::Marker => p.map_or(1.0, |p| 0.68 + 0.44 * p),
        Brush::Calligraphy => p.map_or(1.0, |p| 0.24 + 1.06 * p),
        Brush::Brush => {
            // Pointed ends: it narrows over the first and last two and a half widths.
            let l = size * 2.5;
            let end = |d: f64| {
                let t = (d / l).clamp(0.0, 1.0);
                0.12 + 0.88 * (1.0 - (1.0 - t) * (1.0 - t))
            };
            p.map_or(1.0, |p| 0.06 + 1.54 * p) * end(from_start).min(end(to_end))
        }
        Brush::Airbrush => p.map_or(1.0, |p| 0.35 + 0.95 * p),
        Brush::Chalk => p.map_or(1.0, |p| 0.38 + 0.87 * p),
        Brush::Smudge => p.map_or(1.0, |p| 0.45 + 0.75 * p),
    }
}

/// How much stylus tilt (`0.0..=1.0`) broadens a stroke for shading brushes.
pub fn tilt_scale(brush: Brush, tilt: f32) -> f64 {
    let t = tilt.clamp(0.0, 1.0) as f64;
    if t <= 0.05 {
        return 1.0;
    }
    match brush {
        Brush::Pencil | Brush::Chalk => 1.0 + 0.65 * t,
        Brush::Brush | Brush::Airbrush | Brush::Smudge => 1.0 + 0.45 * t,
        Brush::Marker => 1.0 + 0.25 * t,
        Brush::Pen | Brush::Calligraphy => 1.0 + 0.12 * t,
    }
}

/// How far a stroke can reach from its centre line, as a share of its size: what its box must
/// leave around the points.
pub fn reach(ink: &crate::model::Ink) -> f64 {
    match ink.brush {
        // Older strokes: up to 0.71 with pressure (asked often, so not checked point by point:
        // a stroke without pressure just gets a little more room).
        None => 0.71,
        Some(Brush::Pen) => 0.85,
        Some(Brush::Brush) => 0.95,
        Some(Brush::Pencil) => 0.85,
        Some(Brush::Marker) => 0.65,
        Some(Brush::Calligraphy | Brush::Airbrush | Brush::Chalk | Brush::Smudge) => 0.85,
    }
}

/// Points of a stroke's centre line in each piece it is drawn in (see `union_parts`).
pub const PIECE: usize = 64;

/// The shape of a stroke drawn with a brush (all but the airbrush, which is dabs), in pieces:
/// the union of round dabs and bands as for the pen, a piece every `PIECE` points, or the sweep
/// of a flat nib for calligraphy (in one piece).
pub fn brush_paths(pts: &[f32], size: f64, brush: Brush) -> Vec<Path> {
    brush_paths_tilted(pts, size, brush, 0.0)
}

/// Same as `brush_paths`, with optional stylus tilt (`0.0..=1.0`) broadening shading brushes.
pub fn brush_paths_tilted(pts: &[f32], size: f64, brush: Brush, tilt: f32) -> Vec<Path> {
    let size = size * tilt_scale(brush, tilt);
    if size <= 0.0 || !size.is_finite() || pts.len() < 3 {
        return Vec::new();
    }
    let c = centre_line(pts, 0.85, |p, a, b| size / 2.0 * brush_width(brush, p, a, b, size));
    match brush {
        Brush::Calligraphy => calligraphy_parts(&smooth3(&thin3(&c, size * 0.04), size), size, PIECE),
        _ => union_parts(&smooth3(&thin3(&c, size * 0.08), size), size, PIECE),
    }
}

/// A flat nib held at 45° (a thin rectangle, so moving along its edge still leaves a hairline):
/// broad going down to the right, thin going up to the right. What it sweeps is built a run at a
/// time: while the pen keeps crossing the nib the same way, the same two corners lead and trail,
/// and the run is one polygon between the path shifted to those corners; the nib itself is laid
/// where the runs meet and at the ends. A few large polygons instead of one per step: a long
/// stroke is built ten times faster. In pieces of whole runs, each at least `every` points long
/// (where they meet, the nib laid there overlaps the next run by its hairline thickness).
fn calligraphy_parts(c: &[[f64; 3]], size: f64, every: usize) -> Vec<Path> {
    let mut parts: Vec<Path> = vec![Path::default()];
    let mut since = 0;
    let (nx, ny) = (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2);
    let (mx, my) = (-ny, nx);
    let h = size * 0.05;
    // All turned the same way as the round dabs of the other brushes, so overlaps add up (non-zero)
    // instead of cancelling out.
    let add = |path: &mut Path, mut poly: Vec<f64>| {
        let n = poly.len() / 2;
        let area: f64 = (0..n).map(|i| poly[i * 2] * poly[(i * 2 + 3) % (n * 2)] - poly[(i * 2 + 2) % (n * 2)] * poly[i * 2 + 1]).sum();
        if area.abs() < 1e-12 {
            return;
        }
        if area < 0.0 {
            poly = poly.chunks_exact(2).rev().flatten().copied().collect();
        }
        path.add_polygon(&poly);
    };
    let nib = |p: &[f64; 3]| -> Vec<f64> {
        let (ax, ay, bx, by) = (nx * p[2], ny * p[2], mx * h, my * h);
        vec![p[0] - ax - bx, p[1] - ay - by, p[0] + ax - bx, p[1] + ay - by, p[0] + ax + bx, p[1] + ay + by, p[0] - ax + bx, p[1] - ay + by]
    };
    if c.is_empty() {
        return parts;
    }
    // Which way each step crosses the nib (along it and across it): the leading corner.
    let lead = |i: usize| {
        let (dx, dy) = (c[i + 1][0] - c[i][0], c[i + 1][1] - c[i][1]);
        let s = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v.signum() };
        (s(dx * ny - dy * nx), s(dx * my - dy * mx))
    };
    let mut runs: Vec<(usize, usize, (f64, f64))> = Vec::new();
    let mut start = 0;
    let mut cur = (0.0, 0.0);
    for i in 0..c.len() - 1 {
        let (a, b) = lead(i);
        let next = (if a == 0.0 { cur.0 } else { a }, if b == 0.0 { cur.1 } else { b });
        if i > start && next != cur && cur != (0.0, 0.0) {
            runs.push((start, i, cur));
            start = i;
        }
        cur = next;
    }
    runs.push((start, c.len() - 1, cur));
    for &(a, b, (sa, sb)) in &runs {
        // A new piece once this one is long enough, at the start of a run.
        if since >= every {
            parts.push(Path::default());
            since = 0;
        }
        since += b - a + 1;
        let path = parts.last_mut().unwrap();
        if b > a {
            let k = |p: &[f64; 3]| (sa * nx * p[2] + sb * mx * h, sa * ny * p[2] + sb * my * h);
            let mut poly = Vec::with_capacity((b - a + 1) * 4);
            for p in &c[a..=b] {
                let (kx, ky) = k(p);
                poly.extend([p[0] - kx, p[1] - ky]);
            }
            for p in c[a..=b].iter().rev() {
                let (kx, ky) = k(p);
                poly.extend([p[0] + kx, p[1] + ky]);
            }
            add(path, poly);
        }
        add(path, nib(&c[a]));
    }
    add(parts.last_mut().unwrap(), nib(&c[c.len() - 1]));
    parts
}

/// How strong each airbrush dab is at full pressure: many overlap, so each adds a little.
const FLOW: f64 = 0.17;
/// How far apart airbrush dabs are, as a share of their width: close enough that the soft
/// edge shows no ripple.
const SPACING: f64 = 0.1;

/// The airbrush's dabs along a stroke, `[x, y, radius, alpha]`: soft round spots a small share
/// of their width apart, building up where they overlap. Never closer than `min_step` (on
/// screen, about a pixel): fewer, stronger dabs then give the same build-up.
pub fn brush_dabs(pts: &[f32], size: f64, min_step: f64) -> Vec<[f32; 4]> {
    brush_dabs_tilted(pts, size, min_step, 0.0)
}

/// Same as `brush_dabs`, with optional stylus tilt (`0.0..=1.0`) widening and softening the spray.
pub fn brush_dabs_tilted(pts: &[f32], size: f64, min_step: f64, tilt: f32) -> Vec<[f32; 4]> {
    let t = tilt.clamp(0.0, 1.0) as f64;
    let size = size * tilt_scale(Brush::Airbrush, tilt);
    let mut out = Vec::new();
    if size <= 0.0 || !size.is_finite() || pts.len() < 3 {
        return out;
    }
    // The centre line with the alpha in place of the radius, then the radius alongside.
    let flow_scale = 1.0 - 0.25 * t;
    let flow = |p: f32| FLOW * flow_scale * if p < 0.0 { 0.75 } else { 0.18 + 0.82 * p.clamp(0.0, 1.0) as f64 };
    let c = centre_line(pts, 0.85, |p, _, _| size / 2.0 * brush_width(Brush::Airbrush, p, 0.0, 0.0, size));
    let a = centre_line(pts, 0.85, |p, _, _| flow(p));
    if c.is_empty() || a.len() != c.len() {
        return out;
    }
    // The spacing depends only on where the dabs are (never on how long the stroke has become),
    // so the dabs already laid stay put as the stroke grows.
    let least = (size * 0.02).max(min_step);
    // A dab `k` times farther apart than it should be stands for `k` of them.
    let strength = |a: f64, k: f64| if k <= 1.0 { a } else { 1.0 - (1.0 - a).powf(k) };
    let first_k = least / (c[0][2] * 2.0 * SPACING).max(1e-9);
    out.push([c[0][0] as f32, c[0][1] as f32, c[0][2] as f32, strength(a[0][2], first_k) as f32]);
    let mut since = 0.0;
    for i in 1..c.len() {
        let (p0, p1) = (c[i - 1], c[i]);
        let d = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
        let mut at = 0.0;
        loop {
            let r = p0[2] + (p1[2] - p0[2]) * (at / d.max(1e-9));
            let nominal = (r * 2.0 * SPACING).max(1e-9);
            let step = nominal.max(least);
            if since + (d - at) < step {
                since += d - at;
                break;
            }
            at += step - since;
            since = 0.0;
            let k = at / d.max(1e-9);
            let lerp = |u: f64, v: f64| u + (v - u) * k;
            out.push([lerp(p0[0], p1[0]) as f32, lerp(p0[1], p1[1]) as f32, lerp(p0[2], p1[2]) as f32, strength(lerp(a[i - 1][2], a[i][2]), step / nominal) as f32]);
        }
    }
    out
}

/// A colour (0xRRGGBB) in OKLab, where mixing two colours goes through what the eye sees
/// between them: red into blue passes through purple, not through a dull grey as in RGB.
pub fn to_oklab(c: u32) -> [f64; 3] {
    let lin = |v: u32| {
        let s = (v & 0xFF) as f64 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(c >> 16), lin(c >> 8), lin(c));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s]
}

/// Back from OKLab to 0xRRGGBB.
pub fn from_oklab(c: [f64; 3]) -> u32 {
    let l = (c[0] + 0.3963377774 * c[1] + 0.2158037573 * c[2]).powi(3);
    let m = (c[0] - 0.1055613458 * c[1] - 0.0638541728 * c[2]).powi(3);
    let s = (c[0] - 0.0894841775 * c[1] - 1.2914855480 * c[2]).powi(3);
    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    let srgb = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round().clamp(0.0, 255.0) as u32
    };
    (srgb(r) << 16) | (srgb(g) << 8) | srgb(b)
}

/// Two OKLab colours mixed, `k` of the way from `a` to `b`, round the colour wheel (lightness,
/// colourfulness and hue each in turn) as paints mix: red and blue make purple, yellow and blue
/// green, and nothing goes grey half way. A grey takes on the hue of the other colour.
pub fn mix_lab(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    let (ca, cb) = (a[1].hypot(a[2]), b[1].hypot(b[2]));
    let (ha, hb) = (a[2].atan2(a[1]), b[2].atan2(b[1]));
    let (ha, hb) = match (ca < 0.02, cb < 0.02) {
        (true, false) => (hb, hb),
        (false, true) => (ha, ha),
        _ => (ha, hb),
    };
    let mut dh = hb - ha;
    if dh > std::f64::consts::PI {
        dh -= std::f64::consts::TAU;
    } else if dh < -std::f64::consts::PI {
        dh += std::f64::consts::TAU;
    }
    let (l, c, h) = (a[0] + (b[0] - a[0]) * k, ca + (cb - ca) * k, ha + dh * k);
    [l, c * h.cos(), c * h.sin()]
}

/// Two colours (0xRRGGBB) mixed, `k` of the way from `u` to `v` (see `mix_lab`).
pub fn mix_colors(u: u32, v: u32, k: f64) -> u32 {
    if u == v || k <= 0.0 {
        return u;
    }
    if k >= 1.0 {
        return v;
    }
    from_oklab(mix_lab(to_oklab(u), to_oklab(v), k))
}

/// The sfumino: soft dabs along the stroke, each in the colour it carried there (`colors`, one
/// a point, 0xRRGGBB), denser and stronger than the airbrush's so it smears what is under it.
pub fn smudge_dabs(pts: &[f32], colors: &[u32], size: f64, min_step: f64) -> Vec<([f32; 4], u32)> {
    let mut out = Vec::new();
    if size <= 0.0 || !size.is_finite() || pts.len() < 3 {
        return out;
    }
    // Only the points that are numbers, each with its colour (the centre line skips the others).
    let mut flat = Vec::with_capacity(pts.len());
    let mut cols = Vec::with_capacity(pts.len() / 3);
    for (i, p) in pts.chunks_exact(3).enumerate() {
        if p[0].is_finite() && p[1].is_finite() {
            flat.extend_from_slice(p);
            cols.push(colors.get(i).or(colors.last()).copied().unwrap_or(0));
        }
    }
    let flow = |p: f32| if p < 0.0 { 0.38 } else { 0.15 + 0.32 * p.clamp(0.0, 1.0) as f64 };
    let c = centre_line(&flat, 0.85, |p, _, _| size / 2.0 * brush_width(Brush::Smudge, p, 0.0, 0.0, size));
    let a = centre_line(&flat, 0.85, |p, _, _| flow(p));
    if c.len() != cols.len() || a.len() != c.len() {
        return out;
    }
    const STEP: f64 = 0.14;
    let least = (size * 0.03).max(min_step);
    let strength = |a: f64, k: f64| if k <= 1.0 { a } else { 1.0 - (1.0 - a).powf(k) };
    let lab_cols: Vec<[f32; 3]> = cols.iter().map(|&c| {
        let lab = to_oklab(c);
        [lab[0] as f32, lab[1] as f32, lab[2] as f32]
    }).collect();
    out.push(([c[0][0] as f32, c[0][1] as f32, c[0][2] as f32, strength(a[0][2], least / (c[0][2] * 2.0 * STEP).max(1e-9)) as f32], cols[0]));
    let mut since = 0.0;
    for i in 1..c.len() {
        let (p0, p1) = (c[i - 1], c[i]);
        let d = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
        let mut at = 0.0;
        loop {
            let r = p0[2] + (p1[2] - p0[2]) * (at / d.max(1e-9));
            let nominal = (r * 2.0 * STEP).max(1e-9);
            let step = nominal.max(least);
            if since + (d - at) < step {
                since += d - at;
                break;
            }
            at += step - since;
            since = 0.0;
            let k = at / d.max(1e-9);
            let lerp = |u: f64, v: f64| u + (v - u) * k;
            let dab = [lerp(p0[0], p1[0]) as f32, lerp(p0[1], p1[1]) as f32, lerp(p0[2], p1[2]) as f32, strength(lerp(a[i - 1][2], a[i][2]), step / nominal) as f32];
            let dab_col = if cols[i - 1] == cols[i] {
                cols[i - 1]
            } else {
                let c0 = lab_cols[i - 1];
                let c1 = lab_cols[i];
                let lab = [
                    c0[0] + (c1[0] - c0[0]) * k as f32,
                    c0[1] + (c1[1] - c0[1]) * k as f32,
                    c0[2] + (c1[2] - c0[2]) * k as f32,
                ];
                from_oklab([lab[0] as f64, lab[1] as f64, lab[2] as f64])
            };
            out.push((dab, dab_col));
        }
    }
    out
}

/// The union of round dabs along the centre line and the bands joining them.
fn union_path(c: &[[f64; 3]], size: f64) -> Path {
    union_parts(c, size, usize::MAX).pop().unwrap_or_default()
}

/// The same union cut into pieces of about `every` points, each piece's own union, the pieces
/// meeting edge to edge where a stretch runs smooth (a point without a dab, where the bands on
/// either side share their cross-section): together they cover exactly what the whole does, only
/// where the stroke crosses itself from one piece to another is it covered twice. A stroke being
/// drawn then redraws only its last piece, the others staying exactly as they were.
fn union_parts(c: &[[f64; 3]], size: f64, every: usize) -> Vec<Path> {
    if c.is_empty() {
        return vec![Path::default()];
    }
    // A dab where the band turns sharply or changes width fast, at the ends, and on both sides of
    // a band left out; along a smooth stretch the bands meet edge to edge along the bisector.
    let mut dab = vec![false; c.len()];
    let heading = |a: [f64; 3], b: [f64; 3]| (b[1] - a[1]).atan2(b[0] - a[0]);
    for i in 0..c.len() {
        dab[i] = i == 0 || i + 1 == c.len() || {
            let d0 = (c[i][0] - c[i - 1][0]).hypot(c[i][1] - c[i - 1][1]);
            let d1 = (c[i + 1][0] - c[i][0]).hypot(c[i + 1][1] - c[i][1]);
            let turn = (heading(c[i], c[i + 1]) - heading(c[i - 1], c[i]) + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
            d0 <= 1e-6 || d1 <= 1e-6 || turn.abs() > 0.35 || (c[i + 1][2] - c[i][2]).abs() + (c[i][2] - c[i - 1][2]).abs() > size * 0.04
        };
    }
    let mut bands = Vec::with_capacity(c.len());
    for (i, w) in c.windows(2).enumerate() {
        let ([x0, y0, r0], [x1, y1, r1]) = (w[0], w[1]);
        let d = (x1 - x0).hypot(y1 - y0);
        let s = (r0 - r1) / d;
        // One dab inside the other: nothing to join, both dabs drawn.
        if !(s.abs() < 1.0) {
            (dab[i], dab[i + 1]) = (true, true);
            continue;
        }
        bands.push((i, d, s));
    }
    // Where the pieces meet: the first point without a dab once a piece is long enough.
    let mut bounds = vec![0];
    let mut next = every;
    for (i, &d) in dab.iter().enumerate().take(c.len().saturating_sub(1)).skip(1) {
        if i >= next && !d {
            bounds.push(i);
            next = i.saturating_add(every);
        }
    }
    let piece = |i: usize| bounds.partition_point(|&b| b <= i).saturating_sub(1);
    let mut paths = vec![Path::default(); bounds.len()];
    for (i, (&[x, y, r], _)) in c.iter().zip(&dab).enumerate().filter(|(_, (_, d))| **d) {
        circle(&mut paths[piece(i)], x, y, r);
    }
    let left = |i: usize| {
        let (dx, dy) = (c[i + 1][0] - c[i][0], c[i + 1][1] - c[i][1]);
        let d = dx.hypot(dy).max(1e-9);
        (-dy / d, dx / d)
    };
    // A point without a dab: the bands either side end on the same cross-section, along the
    // bisector (each square to its own way, they left a thin wedge open outside the turn).
    let shared = |i: usize| {
        let ((ax, ay), (bx, by)) = (left(i - 1), left(i));
        let (mx, my) = (ax + bx, ay + by);
        let k = c[i][2] / (mx * ax + my * ay).max(0.5);
        [c[i][0] - mx * k, c[i][1] - my * k, c[i][0] + mx * k, c[i][1] + my * k]
    };
    for (i, d, s) in bands {
        // The two lines touching both dabs, and the band between them (turning the same way
        // as the dabs, or the union would have holes).
        let ([x0, y0, r0], [x1, y1, r1]) = (c[i], c[i + 1]);
        let (ux, uy) = ((x1 - x0) / d, (y1 - y0) / d);
        let k = (1.0 - s * s).sqrt();
        let (ax, ay) = (-uy * k + ux * s, ux * k + uy * s);
        let (bx, by) = (uy * k + ux * s, -ux * k + uy * s);
        let mut ends = [[x0 + r0 * bx, y0 + r0 * by, x0 + r0 * ax, y0 + r0 * ay], [x1 + r1 * bx, y1 + r1 * by, x1 + r1 * ax, y1 + r1 * ay]];
        for (e, j) in ends.iter_mut().zip([i, i + 1]) {
            if !dab[j] {
                *e = shared(j);
            }
        }
        let [mut a, mut b] = ends;
        // On tight curves where consecutive bisectors cross on the inner side, prevent the inner
        // edge from reversing direction and forming a negative-winding bowtie hole.
        if (b[0] - a[0]) * ux + (b[1] - a[1]) * uy < 0.0 {
            let (mx, my) = ((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5);
            (a[0], a[1], b[0], b[1]) = (mx, my, mx, my);
        }
        if (b[2] - a[2]) * ux + (b[3] - a[3]) * uy < 0.0 {
            let (mx, my) = ((a[2] + b[2]) * 0.5, (a[3] + b[3]) * 0.5);
            (a[2], a[3], b[2], b[3]) = (mx, my, mx, my);
        }
        paths[piece(i)].add_polygon(&[a[0], a[1], b[0], b[1], b[2], b[3], a[2], a[3]]);
    }
    paths
}

/// A circle of cubic arcs, turning as `Path::ellipse` does.
fn circle(p: &mut Path, x: f64, y: f64, r: f64) {
    p.cmds.extend(Path::ellipse(x as f32, y as f32, r as f32, r as f32).cmds);
}

/// The points a stroke needs: each one at least `step` away from the one kept before it (the
/// last always kept). A point kept stays kept as the stroke grows, so while drawing only its tip
/// changes: a best-fit simplification picked other points at every move, and the part already
/// drawn jumped about.
fn thin3(pts: &[[f64; 3]], step: f64) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = Vec::with_capacity(pts.len());
    for (i, &p) in pts.iter().enumerate() {
        match out.last() {
            Some(a) if i + 1 < pts.len() && (p[0] - a[0]).hypot(p[1] - a[1]) < step && (p[2] - a[2]).abs() < step => {}
            _ => out.push(p),
        }
    }
    out
}

/// Rounds polygonal joints into local quadratic B-spline arcs so thin strokes stay smooth even
/// when zoomed in, while keeping the already-drawn part of a live stroke completely still.
fn smooth3(pts: &[[f64; 3]], size: f64) -> Vec<[f64; 3]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let lerp = |a: [f64; 3], b: [f64; 3], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    let mut out = Vec::with_capacity(pts.len() * 3);
    out.push(pts[0]);
    for i in 1..pts.len() - 1 {
        let (p0, p1, p2) = (pts[i - 1], pts[i], pts[i + 1]);
        let d0 = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
        let d1 = (p2[0] - p1[0]).hypot(p2[1] - p1[1]);
        if d0 <= size * 0.12 || d1 <= size * 0.12 {
            out.push(p1);
            continue;
        }
        let h0 = (p1[1] - p0[1]).atan2(p1[0] - p0[0]);
        let h1 = (p2[1] - p1[1]).atan2(p2[0] - p1[0]);
        let turn = ((h1 - h0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).abs();
        if !(0.03..2.6).contains(&turn) {
            out.push(p1);
            continue;
        }
        let a = lerp(p1, p0, 0.45);
        let b = lerp(p1, p2, 0.45);
        let steps = ((turn * 6.0).ceil() as usize).clamp(2, 6);
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let u = 1.0 - t;
            out.push([
                u * u * a[0] + 2.0 * u * t * p1[0] + t * t * b[0],
                u * u * a[1] + 2.0 * u * t * p1[1] + t * t * b[1],
                u * u * a[2] + 2.0 * u * t * p1[2] + t * t * b[2],
            ]);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// The "1€ filter" (Casiez et al.) plus an optional StreamLine rope/spring
/// stabilizer: heavy smoothing when the pen moves slowly, where tremor is visible, and smooth
/// calligraphic pull when `streamline > 0`. Works in screen pixels.
pub struct OneEuro {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    t: f64,
    min_cutoff: f64,
    beta: f64,
    streamline: f64,
}

impl OneEuro {
    pub fn new(min_cutoff: f64) -> Self {
        Self::with_streamline(min_cutoff, 0.0)
    }
    pub fn with_streamline(min_cutoff: f64, streamline: f64) -> Self {
        let s = streamline.clamp(0.0, 1.0);
        OneEuro {
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
            t: -1.0,
            min_cutoff: (min_cutoff * (1.0 - 0.65 * s)).max(0.25),
            beta: 0.012 * (1.0 - 0.75 * s),
            streamline: s,
        }
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
        // Virtual rope deadzone when StreamLine is active: absorbs tiny hand tremors before pulling.
        let ( mut tx, mut ty) = (x, y);
        if self.streamline > 0.01 {
            let rope = self.streamline * 8.0;
            let dist = (x - self.x).hypot(y - self.y);
            if dist <= rope {
                tx = self.x + (x - self.x) * 0.15;
                ty = self.y + (y - self.y) * 0.15;
            } else {
                let pull = (dist - rope) / dist;
                tx = self.x + (x - self.x) * pull;
                ty = self.y + (y - self.y) * pull;
            }
        }
        let ad = Self::alpha(1.0, dt);
        self.dx += ad * ((tx - self.x) / dt - self.dx);
        self.dy += ad * ((ty - self.y) / dt - self.dy);
        let a = Self::alpha(self.min_cutoff + self.beta * self.dx.hypot(self.dy), dt) * (1.0 - 0.62 * self.streamline);
        self.x += a * (tx - self.x);
        self.y += a * (ty - self.y);
        pt(self.x, self.y)
    }
}

/// Drops points closer than `min_dist` to the previous kept one; keeps the last point.
pub fn thin_points(pts: &[f32], min_dist: f64) -> Vec<f32> {
    thin_indices(pts, min_dist).into_iter().flat_map(|i| pts[i * 3..i * 3 + 3].iter().copied()).collect()
}

/// Which points `thin_points` keeps (their numbers), for what goes along with each point.
pub fn thin_indices(pts: &[f32], min_dist: f64) -> Vec<usize> {
    let n = pts.len() / 3;
    if n <= 2 {
        return (0..n).collect();
    }
    let mut out = vec![0];
    for i in 1..n - 1 {
        let l = out[out.len() - 1] * 3;
        if ((pts[i * 3] - pts[l]) as f64).hypot((pts[i * 3 + 1] - pts[l + 1]) as f64) >= min_dist {
            out.push(i);
        }
    }
    out.push(n - 1);
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
        // 96 px highlighter: everything near the stroke's centre line is inside, everything
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
                // (Sharp corners of the mouse's path are rounded off a little: an eighth of the width.)
                if d < 36.0 && !inside(x, y) {
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
    fn a_gentle_curve_has_no_cracks_along_its_edge() {
        // A wide mouse arc turning too little at each point for a dab: the bands either side of
        // a point used to leave a thin wedge open on the outer edge, showing the board.
        let big = 3000.0;
        let pts: Vec<f32> = (0..=60).flat_map(|i| {
            let a = -0.6 + i as f64 * 0.02;
            [(big * a.sin()) as f32, (big - big * a.cos()) as f32, -1.0]
        }).collect();
        let mesh = crate::paint::fill_mesh(&stroke_path(&pts, 300.0, false), egui::Color32::WHITE);
        let inside = |x: f64, y: f64| {
            mesh.indices.chunks_exact(3).any(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize].pos);
                let s = |p: egui::Pos2, q: egui::Pos2| (q.x - p.x) as f64 * (y - p.y as f64) - (q.y - p.y) as f64 * (x - p.x as f64);
                let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
                !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
            })
        };
        let cracks = (0..4000).map(|k| -0.5 + k as f64 * 0.00025).filter(|a| !inside((big + 130.0) * a.sin(), big - (big + 130.0) * a.cos())).count();
        assert_eq!(cracks, 0);
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
#[cfg(test)]
mod live {
    /// A 64 px pen being drawn: from one move to the next only the tip may change; what is
    /// already on screen stays exactly where it is (it used to jump about, looking jerky).
    #[test]
    fn a_stroke_being_drawn_only_changes_at_its_tip() {
        let mut pts = Vec::new();
        for i in 0..600 {
            let t = i as f64 / 600.0;
            pts.extend([(t * 1400.0 + 60.0 * (t * 31.0).sin()) as f32, (200.0 * (t * 9.0).sin() + 40.0 * (t * 53.0).cos()) as f32, -1.0]);
        }
        let dabs = |n: usize| -> Vec<(f32, f32)> {
            let p = super::stroke_path(&pts[..n * 3], 64.0, false);
            p.cmds.windows(2).filter_map(|w| if let (crate::prims::Cmd::Move(x, y), crate::prims::Cmd::Cubic(..)) = (w[0], w[1]) { Some((x, y)) } else { None }).collect()
        };
        let mut prev = dabs(30);
        for n in (35..600).step_by(5) {
            let now = dabs(n);
            let settled = prev.len().saturating_sub(3);
            assert_eq!(&prev[..settled], &now[..settled], "after {n} points");
            prev = now;
        }
    }
}
