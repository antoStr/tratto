//! Software drawing with tiny-skia: elements cut by the pixel eraser, exports, thumbnails,
//! the minimap and template previews.

use std::sync::Arc;

use egui::Color32;
use tiny_skia::{BlendMode, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, StrokeDash, Transform};

use crate::geom::{BBox, aabb};
use crate::model::El;
use crate::prims::{Affine, Cmd, Env, ImageKey, Path, Prim, prims_of};

/// Pictures for image and reaction elements.
pub type Images<'a> = &'a dyn Fn(&ImageKey) -> Option<Arc<Pixmap>>;

pub fn sk_path(p: &Path) -> Option<tiny_skia::Path> {
    let mut b = PathBuilder::new();
    for c in &p.cmds {
        match *c {
            Cmd::Move(x, y) => b.move_to(x, y),
            Cmd::Line(x, y) => b.line_to(x, y),
            Cmd::Quad(x1, y1, x, y) => b.quad_to(x1, y1, x, y),
            Cmd::Cubic(x1, y1, x2, y2, x, y) => b.cubic_to(x1, y1, x2, y2, x, y),
            Cmd::Close => b.close(),
        }
    }
    b.finish()
}

pub fn paint_of(c: Color32) -> Paint<'static> {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, a);
    p.anti_alias = true;
    p
}

pub fn transform_of(t: &Affine) -> Transform {
    Transform::from_row(t.a as f32, t.b as f32, t.c as f32, t.d as f32, t.e as f32, t.f as f32)
}

/// An RGBA picture (straight alpha) as a premultiplied pixmap.
pub fn pixmap_from_rgba(w: u32, h: u32, rgba: &[u8]) -> Option<Pixmap> {
    let data = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as u16;
            [(p[0] as u16 * a / 255) as u8, (p[1] as u16 * a / 255) as u8, (p[2] as u16 * a / 255) as u8, p[3]]
        })
        .collect();
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

fn draw_body(pm: &mut Pixmap, el: &El, env: &Env, t: Transform, images: Images) {
    let et = t.pre_concat(transform_of(&Affine::of(el)));
    for prim in prims_of(el, env) {
        match prim {
            Prim::Fill { path, color } => {
                if let Some(p) = sk_path(&path) {
                    pm.fill_path(&p, &paint_of(color), FillRule::Winding, et, None);
                }
            }
            Prim::Stroke { path, width, color, round, dash } => {
                let Some(p) = sk_path(&path) else { continue };
                let stroke = Stroke {
                    width,
                    line_cap: if round { LineCap::Round } else { LineCap::Butt },
                    line_join: if round { LineJoin::Round } else { LineJoin::Bevel },
                    dash: dash.and_then(|[on, off]| StrokeDash::new(vec![on, off.max(1e-3)], 0.0)),
                    ..Stroke::default()
                };
                pm.stroke_path(&p, &paint_of(color), &stroke, et, None);
            }
            Prim::Text { placed, color } => {
                let paint = paint_of(color);
                for &(g, gx, gy) in &placed.glyphs {
                    let glyph = crate::text::glyph_path(placed.face, g);
                    if let Some(p) = sk_path(&glyph) {
                        let gt = Transform::from_row(placed.k, 0.0, placed.lean * placed.k, -placed.k, gx, gy);
                        pm.fill_path(&p, &paint, FillRule::Winding, et.pre_concat(gt), None);
                    }
                }
            }
            Prim::Image { key, x, y, w, h, alpha } => match images(&key) {
                Some(img) => {
                    let it = Transform::from_row(w / img.width() as f32, 0.0, 0.0, h / img.height() as f32, x, y);
                    let paint = PixmapPaint { opacity: alpha, quality: tiny_skia::FilterQuality::Bilinear, ..PixmapPaint::default() };
                    pm.draw_pixmap(0, 0, img.as_ref().as_ref(), &paint, et.pre_concat(it), None);
                }
                None => {
                    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                        pm.fill_rect(r, &paint_of(Color32::from_rgba_unmultiplied(128, 128, 128, 31)), et, None);
                    }
                }
            },
            Prim::Shadow { x, y, w, h, radius, blur, dy, color } => {
                // A few widening rounded rectangles stand in for the blur.
                let ring = crate::prims::with_alpha(color, 0.25);
                for i in 0..4 {
                    let e = blur * (i as f32 / 3.0 - 0.5);
                    if let Some(p) = sk_path(&Path::round_rect(x - e, y + dy - e, w + 2.0 * e, h + 2.0 * e, [radius + e.max(0.0); 4])) {
                        pm.fill_path(&p, &paint_of(ring), FillRule::Winding, et, None);
                    }
                }
            }
        }
    }
}

/// Draws one element; `t` maps board coordinates to the pixmap's pixels. Elements cut by the
/// pixel eraser are painted on their own layer, the marks cut out of it, the layer copied over.
pub fn draw_element(pm: &mut Pixmap, el: &El, env: &Env, t: Transform, images: Images) {
    if el.hidden {
        return;
    }
    if el.erase.is_empty() {
        return draw_body(pm, el, env, t, images);
    }
    let b = aabb(el);
    let mut pts = [tiny_skia::Point::from_xy(b.x as f32, b.y as f32), tiny_skia::Point::from_xy(b.right() as f32, b.y as f32), tiny_skia::Point::from_xy(b.x as f32, b.bottom() as f32), tiny_skia::Point::from_xy(b.right() as f32, b.bottom() as f32)];
    t.map_points(&mut pts);
    let x0 = pts.iter().map(|p| p.x).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32 - 1;
    let y0 = pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32 - 1;
    let x1 = (pts.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max).ceil() as i32 + 1).min(pm.width() as i32);
    let y1 = (pts.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max).ceil() as i32 + 1).min(pm.height() as i32);
    let (x0, y0) = (x0.max(0), y0.max(0));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let Some(mut layer) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else { return };
    let lt = t.post_translate(-x0 as f32, -y0 as f32);
    draw_body(&mut layer, el, env, lt, images);
    let et = lt.pre_concat(transform_of(&Affine::of(el)));
    for m in &el.erase {
        let mut p = Path::default();
        for (i, q) in m.p.chunks_exact(2).enumerate() {
            if i == 0 { p.move_to(q[0], q[1]) } else { p.line_to(q[0], q[1]) }
        }
        // A tap is a zero-length line, which isn't stroked: give it a hair of length.
        if m.p.chunks_exact(2).all(|q| q[0] == m.p[0] && q[1] == m.p[1]) {
            p.line_to(m.p[0] + (m.s as f32 * 0.001).max(1e-4), m.p[1]);
        }
        let Some(path) = sk_path(&p) else { continue };
        let mut paint = paint_of(Color32::from_black_alpha((m.a.clamp(0.0, 1.0) * 255.0) as u8));
        paint.blend_mode = BlendMode::DestinationOut;
        let stroke = Stroke { width: m.s as f32, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        layer.stroke_path(&path, &paint, &stroke, et, None);
    }
    pm.draw_pixmap(x0, y0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

/// Draws elements so that board box `b` fills a `w`×`h` picture.
pub fn render(els: &[Arc<El>], background: Option<Color32>, w: u32, h: u32, b: BBox, env: &Env, images: Images) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w.max(1), h.max(1))?;
    if let Some(bg) = background {
        let [r, g, bl, a] = bg.to_srgba_unmultiplied();
        pm.fill(tiny_skia::Color::from_rgba8(r, g, bl, a));
    }
    let t = Transform::from_row((w as f64 / b.w) as f32, 0.0, 0.0, (h as f64 / b.h) as f32, (-b.x * w as f64 / b.w) as f32, (-b.y * h as f64 / b.h) as f32);
    for el in els {
        draw_element(&mut pm, el, env, t, images);
    }
    Some(pm)
}

/// Whether the eraser marks hide the whole element: drawn small, is any pixel still there?
pub fn nothing_left(el: &El, images: Images) -> bool {
    if !el.erase.iter().any(|m| m.a >= 0.99) {
        return false;
    }
    let b = aabb(el);
    let k = 160.0 / b.w.max(b.h).max(1e-6);
    let (w, h) = ((b.w * k).ceil().max(1.0) as u32, (b.h * k).ceil().max(1.0) as u32);
    let Some(mut pm) = Pixmap::new(w, h) else { return false };
    let el = El { opacity: 1.0, ..el.clone() };
    draw_element(&mut pm, &el, &Env::default(), Transform::from_row(k as f32, 0.0, 0.0, k as f32, (-b.x * k) as f32, (-b.y * k) as f32), images);
    !pm.pixels().iter().any(|p| p.alpha() > 24)
}

/// Reactions as pixmaps, drawn once.
pub fn stamp_pixmap(emoji: &str) -> Option<Arc<Pixmap>> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<String, Arc<Pixmap>>>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    let map = c.get_or_insert_with(HashMap::new);
    if let Some(p) = map.get(emoji) {
        return Some(p.clone());
    }
    let p = Arc::new(crate::assets::rasterize_svg(crate::assets::stamp(emoji)?.svg, 512)?);
    map.insert(emoji.to_string(), p.clone());
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EraseMark, Ink, Kind};

    fn stroke() -> El {
        let mut el = El::new(Kind::Ink(Ink { points: vec![0.0, 0.0, -1.0, 100.0, 0.0, -1.0], color: "#000000".into(), size: 10.0 }));
        el.w = 100.0;
        el
    }

    #[test]
    fn a_stroke_erased_end_to_end_is_gone() {
        let none = |_: &ImageKey| None;
        let mut el = stroke();
        assert!(!nothing_left(&el, &none));
        el.erase = vec![EraseMark { p: vec![-10.0, 0.0, 50.0, 0.0], s: 30.0, a: 1.0 }];
        assert!(!nothing_left(&el, &none), "half is still there");
        el.erase.push(EraseMark { p: vec![40.0, 0.0, 110.0, 0.0], s: 30.0, a: 1.0 });
        assert!(nothing_left(&el, &none));
    }

    #[test]
    fn render_paints_the_background_and_elements() {
        let none = |_: &ImageKey| None;
        let el = Arc::new(stroke());
        let pm = render(&[el], Some(Color32::WHITE), 120, 40, BBox { x: -10.0, y: -20.0, w: 120.0, h: 40.0 }, &Env::default(), &none).unwrap();
        let px = |x: u32, y: u32| pm.pixel(x, y).unwrap();
        assert_eq!(px(1, 1).red(), 255, "background");
        assert!(px(60, 20).red() < 40, "ink in the middle");
    }
}
