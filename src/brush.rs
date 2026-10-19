//! What a plain fill can't give a brush: the grain of pencil and chalk (a texture repeating
//! across the stroke's own coordinates, so it stays put on the paper as the stroke moves), and
//! the soft dabs of the airbrush. Made here once, for the screen and for software drawing.

use std::sync::{Arc, Mutex, OnceLock};

use egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grain {
    /// The fine tooth of drawing paper.
    Pencil,
    /// Coarse and patchy, as chalk on a board or pastel on rough paper.
    Chalk,
}

/// Texels a side of a grain tile.
pub const GRAIN_SIZE: usize = 128;
/// Board units a grain tile covers.
pub const GRAIN_TILE: f32 = 96.0;

/// Rings of a soft dab from the centre out: (distance as a share of the radius, strength), a
/// bell that fades to nothing at the rim.
pub const DAB_RINGS: [(f32, f32); 3] = [(0.0, 1.0), (0.50, 0.50), (1.0, 0.0)];
/// Sides of each ring.
pub const DAB_SIDES: usize = 14;

fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFF_FFFF) as f32 / 16_777_216.0
}

/// Smooth noise with features `cell` texels apart, repeating every tile.
fn noise(x: usize, y: usize, cell: usize, seed: u32) -> f32 {
    let period = (GRAIN_SIZE / cell) as u32;
    let (fx, fy) = (x as f32 / cell as f32, y as f32 / cell as f32);
    let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
    let (tx, ty) = (fx - ix as f32, fy - iy as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let v = |i: u32, j: u32| hash(i % period, j % period, seed);
    let a = v(ix, iy) + (v(ix + 1, iy) - v(ix, iy)) * s(tx);
    let b = v(ix, iy + 1) + (v(ix + 1, iy + 1) - v(ix, iy + 1)) * s(tx);
    a + (b - a) * s(ty)
}

/// How much of the colour each texel of the grain lets through (0–255).
pub fn grain_alpha(g: Grain) -> &'static [u8] {
    static PENCIL: OnceLock<Vec<u8>> = OnceLock::new();
    static CHALK: OnceLock<Vec<u8>> = OnceLock::new();
    let make = move || -> Vec<u8> {
        let mut out = Vec::with_capacity(GRAIN_SIZE * GRAIN_SIZE);
        for y in 0..GRAIN_SIZE {
            for x in 0..GRAIN_SIZE {
                let a = match g {
                    Grain::Pencil => {
                        // The paper's tooth, a little mottling, and a speckle per texel.
                        let n = 0.5 * noise(x, y, 2, 1) + 0.3 * noise(x, y, 4, 2) + 0.2 * noise(x, y, 16, 3);
                        // Graphite lies on most of the paper, thinner in its hollows.
                        0.3 + 0.7 * ((n - 0.1) / 0.5).clamp(0.0, 1.0) * (0.8 + 0.2 * hash(x as u32, y as u32, 9))
                    }
                    Grain::Chalk => {
                        // Fine gaps where the chalk skipped over the board, in patches.
                        let n = 0.4 * noise(x, y, 2, 4) + 0.35 * noise(x, y, 4, 5) + 0.25 * noise(x, y, 16, 6);
                        let t = ((n - 0.3) / 0.28).clamp(0.0, 1.0);
                        t * t * (3.0 - 2.0 * t) * (0.85 + 0.15 * hash(x as u32, y as u32, 7))
                    }
                };
                out.push((a * 255.0).round() as u8);
            }
        }
        out
    };
    match g {
        Grain::Pencil => PENCIL.get_or_init(make),
        Grain::Chalk => CHALK.get_or_init(make),
    }
}

/// The grain as a texture repeating across the board, white with the grain as its alpha (the
/// vertices give the colour). Made once per window. (The pictures of the tests don't repeat it:
/// egui_kittest filters textures by hand, clamped, so there the grain smears past one tile.)
pub fn grain_texture(ctx: &egui::Context, g: Grain) -> egui::TextureId {
    let id = egui::Id::new(("tratto-grain", g));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t.id();
    }
    let rgba: Vec<u8> = grain_alpha(g).iter().flat_map(|&a| [255, 255, 255, a]).collect();
    let img = egui::ColorImage::from_rgba_unmultiplied([GRAIN_SIZE, GRAIN_SIZE], &rgba);
    let options = egui::TextureOptions { magnification: egui::TextureFilter::Linear, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::Repeat, mipmap_mode: Some(egui::TextureFilter::Linear) };
    let tex = ctx.load_texture(format!("grain-{g:?}"), img, options);
    let tid = tex.id();
    ctx.data_mut(|d| d.insert_temp(id, tex));
    tid
}

/// The grain in a colour, for software drawing (exports, erased strokes); the last few colours
/// are kept.
pub fn grain_pixmap(g: Grain, c: Color32) -> Option<Arc<tiny_skia::Pixmap>> {
    type Cache = Vec<((Grain, [u8; 4]), Arc<tiny_skia::Pixmap>)>;
    static CACHE: Mutex<Cache> = Mutex::new(Vec::new());
    let key = (g, c.to_array());
    if let Ok(cache) = CACHE.lock()
        && let Some((_, p)) = cache.iter().find(|(k, _)| *k == key)
    {
        return Some(p.clone());
    }
    let [r, gr, b, a] = c.to_srgba_unmultiplied();
    let data: Vec<u8> = grain_alpha(g)
        .iter()
        .flat_map(|&t| {
            let al = a as u32 * t as u32 / 255;
            [(r as u32 * al / 255) as u8, (gr as u32 * al / 255) as u8, (b as u32 * al / 255) as u8, al as u8]
        })
        .collect();
    let pm = Arc::new(tiny_skia::Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(GRAIN_SIZE as u32, GRAIN_SIZE as u32)?)?);
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= 24 {
            cache.remove(0);
        }
        cache.push((key, pm.clone()));
    }
    Some(pm)
}

/// The vertices of a soft dab (centre first, then each ring), positions through `place`, each
/// in the colour faded as the rings say; and the triangles joining them (indices from 0).
pub fn dab_mesh(x: f32, y: f32, r: f32, color: Color32, place: impl Fn(f32, f32) -> [f32; 2], pos: &mut Vec<[f32; 2]>, col: &mut Vec<Color32>, idx: &mut Vec<u32>) {
    let base = pos.len() as u32;
    pos.push(place(x, y));
    col.push(color.gamma_multiply(DAB_RINGS[0].1));
    for &(d, k) in &DAB_RINGS[1..] {
        for i in 0..DAB_SIDES {
            let a = i as f32 / DAB_SIDES as f32 * std::f32::consts::TAU;
            pos.push(place(x + a.cos() * r * d, y + a.sin() * r * d));
            col.push(color.gamma_multiply(k));
        }
    }
    let n = DAB_SIDES as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        idx.extend([base, base + 1 + i, base + 1 + j]);
    }
    for ring in 0..(DAB_RINGS.len() as u32 - 2) {
        let (a, b) = (base + 1 + ring * n, base + 1 + (ring + 1) * n);
        for i in 0..n {
            let j = (i + 1) % n;
            idx.extend([a + i, b + i, b + j, a + i, b + j, a + j]);
        }
    }
}

/// A 4-vertex quad for a dab when viewed at low screen-space resolution (1.0 <= R_screen < 3.0 px).
/// Generates 4 vertices and 6 indices (2 triangles) instead of 29 vertices and 126 indices.
pub fn dab_quad(
    x: f32,
    y: f32,
    r: f32,
    color: Color32,
    place: impl Fn(f32, f32) -> [f32; 2],
    pos: &mut Vec<[f32; 2]>,
    col: &mut Vec<Color32>,
    idx: &mut Vec<u32>,
) {
    let base = pos.len() as u32;
    let c = color.gamma_multiply(0.48);
    let s = r * 0.88;
    pos.extend([
        place(x - s, y - s),
        place(x + s, y - s),
        place(x + s, y + s),
        place(x - s, y + s),
    ]);
    col.extend([c, c, c, c]);
    idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A simplified thin line ribbon connecting consecutive dabs when screen radius R_screen < 1.0 px.
pub fn dab_strip(
    dabs: &[[f32; 4]],
    color: Color32,
    tints: &[Color32],
    half_width: f32,
    place: impl Fn(f32, f32) -> [f32; 2],
    pos: &mut Vec<[f32; 2]>,
    col: &mut Vec<Color32>,
    idx: &mut Vec<u32>,
) {
    if dabs.is_empty() {
        return;
    }
    if dabs.len() == 1 {
        let d = dabs[0];
        let c = tints.first().copied().unwrap_or(color).gamma_multiply(d[3]);
        let r = half_width.max(d[2]);
        dab_quad(d[0], d[1], r, c, place, pos, col, idx);
        return;
    }
    for i in 1..dabs.len() {
        let (d0, d1) = (dabs[i - 1], dabs[i]);
        let (dx, dy) = (d1[0] - d0[0], d1[1] - d0[1]);
        let dist = dx.hypot(dy);
        let c0 = tints.get(i - 1).copied().unwrap_or(color).gamma_multiply(d0[3]);
        let c1 = tints.get(i).copied().unwrap_or(color).gamma_multiply(d1[3]);
        let hw = half_width.max(0.5 * (d0[2] + d1[2]));
        if dist < 1e-4 {
            let base = pos.len() as u32;
            pos.extend([
                place(d0[0] - hw, d0[1] - hw),
                place(d0[0] + hw, d0[1] - hw),
                place(d0[0] + hw, d0[1] + hw),
                place(d0[0] - hw, d0[1] + hw),
            ]);
            col.extend([c0, c0, c0, c0]);
            idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            continue;
        }
        let (nx, ny) = (-dy / dist * hw, dx / dist * hw);
        let base = pos.len() as u32;
        pos.extend([
            place(d0[0] - nx, d0[1] - ny),
            place(d0[0] + nx, d0[1] + ny),
            place(d1[0] + nx, d1[1] + ny),
            place(d1[0] - nx, d1[1] - ny),
        ]);
        col.extend([c0, c0, c1, c1]);
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}


/// A soft dab for software drawing: a radial gradient fading as the rings do, in `color` (its
/// own alpha too) times `alpha`. Kept apart: a faint dab's colour, made premultiplied and back,
/// would come out a few shades off, and many of them show it.
pub fn dab_shader(x: f32, y: f32, r: f32, color: Color32, alpha: f32) -> Option<tiny_skia::Shader<'static>> {
    let [cr, cg, cb, ca] = color.to_srgba_unmultiplied();
    let a = ca as f32 / 255.0 * alpha.clamp(0.0, 1.0);
    let stops = DAB_RINGS.iter().map(|&(d, k)| tiny_skia::GradientStop::new(d, tiny_skia::Color::from_rgba(cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0, (a * k).clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::TRANSPARENT))).collect();
    let c = tiny_skia::Point::from_xy(x, y);
    tiny_skia::RadialGradient::new(c, 0.0, c, r.max(1e-3), stops, tiny_skia::SpreadMode::Pad, tiny_skia::Transform::identity())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How long each brush takes to build a long stroke (as while it is being drawn, every frame)
    /// and how many vertices it makes: `cargo test --release brush_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn brush_cost() {
        use crate::model::BRUSHES;
        // A long, wiggly stroke with pressure: 2000 points over 3000 board units.
        let pts: Vec<f32> = (0..2000)
            .flat_map(|i| {
                let t = i as f32 / 2000.0;
                [t * 3000.0, 80.0 * (t * 60.0).sin(), 0.3 + 0.6 * (t * 20.0).sin().abs()]
            })
            .collect();
        for (brush, name) in BRUSHES {
            let size = if brush == crate::model::Brush::Airbrush { 36.0 } else { 8.0 };
            let t0 = std::time::Instant::now();
            let mut verts = 0;
            for _ in 0..10 {
                let prims = crate::prims::brush_prims(&pts, size, Some(brush), false, Color32::BLACK, (0.0, 0.0), Some(1.0));
                let ctx = egui::Context::default();
                for m in crate::paint::brush_meshes(&ctx, prims, 1.0, |x, y| egui::pos2(x, y)) {
                    verts = m.vertices.len();
                }
            }
            println!("{name:<14} {:6.2} ms  {verts} vertices", t0.elapsed().as_secs_f64() * 100.0);
        }
    }

    /// Every brush with a pen (pressure rising and falling) and with a mouse:
    /// `TRATTO_SHOT=brushes.png cargo test brush_picture -- --nocapture`.
    #[test]
    fn brush_picture() {
        use crate::model::{BRUSHES, El, Ink, Kind};
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let mut els = Vec::new();
        for (row, (brush, _)) in BRUSHES.iter().enumerate() {
            for (col, pen) in [true, false].into_iter().enumerate() {
                let size = if *brush == crate::model::Brush::Airbrush { 34.0 } else { 12.0 };
                let mut pts = Vec::new();
                for i in 0..=160 {
                    let t = i as f32 / 160.0;
                    let p = if pen { (t * std::f32::consts::PI).sin().max(0.0).powf(0.8) } else { -1.0 };
                    pts.extend([t * 420.0, 30.0 + 26.0 * (t * std::f32::consts::TAU * 1.5).sin(), p]);
                }
                let mut el = El::new(Kind::Ink(Ink { colors: Vec::new(), points: pts, color: if *brush == crate::model::Brush::Chalk { "#2F6FD6".into() } else { "#1E1E1E".into() }, size, brush: Some(*brush) }));
                (el.x, el.y, el.w, el.h) = (40.0 + col as f64 * 480.0, 30.0 + row as f64 * 100.0, 420.0, 60.0);
                els.push(std::sync::Arc::new(el));
            }
        }
        let none = |_: &crate::prims::ImageKey| None;
        let b = crate::geom::BBox { x: 0.0, y: 0.0, w: 1000.0, h: 760.0 };
        let pm = crate::raster::render(&els, Some(Color32::from_rgb(0xFA, 0xF7, 0xF0)), 2000, 1520, b, &crate::prims::Env::default(), &none).unwrap();
        pm.save_png(path).unwrap();
    }

    #[test]
    fn grains_tile_without_a_seam_and_let_the_colour_through() {
        for g in [Grain::Pencil, Grain::Chalk] {
            let a = grain_alpha(g);
            assert_eq!(a.len(), GRAIN_SIZE * GRAIN_SIZE);
            let mean = a.iter().map(|&v| v as f64).sum::<f64>() / a.len() as f64;
            // Mostly colour with gaps (paper showing), never blank or solid.
            assert!((70.0..215.0).contains(&mean), "{g:?} mean {mean}");
            // The last column leads into the first as smoothly as any two neighbours do.
            let step = |x0: usize, x1: usize| (0..GRAIN_SIZE).map(|y| (a[y * GRAIN_SIZE + x0] as f64 - a[y * GRAIN_SIZE + x1] as f64).abs()).sum::<f64>() / GRAIN_SIZE as f64;
            let inside = (1..GRAIN_SIZE).map(|x| step(x - 1, x)).sum::<f64>() / (GRAIN_SIZE - 1) as f64;
            assert!(step(GRAIN_SIZE - 1, 0) < inside * 1.6, "{g:?}: seam {} vs {inside}", step(GRAIN_SIZE - 1, 0));
        }
    }

    #[test]
    fn dab_quad_produces_four_vertices_and_two_triangles() {
        let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
        dab_quad(10.0, 20.0, 2.5, Color32::RED, |x, y| [x, y], &mut pos, &mut col, &mut idx);
        assert_eq!(pos.len(), 4);
        assert_eq!(col.len(), 4);
        assert_eq!(idx.len(), 6);
        assert_eq!(idx, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn dab_strip_connects_consecutive_dabs() {
        let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
        let dabs = vec![[0.0, 0.0, 1.0, 1.0], [10.0, 0.0, 1.0, 1.0], [20.0, 0.0, 1.0, 1.0]];
        dab_strip(&dabs, Color32::BLUE, &[], 0.5, |x, y| [x, y], &mut pos, &mut col, &mut idx);
        assert_eq!(pos.len(), 8);
        assert_eq!(idx.len(), 12);
    }
}
