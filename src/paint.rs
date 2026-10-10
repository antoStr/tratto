//! The board on screen: each element's primitives are tessellated (lyon) once into meshes in
//! board coordinates and kept while the element and the zoom level stay the same; every frame
//! only places them. Antialiasing comes from multisampling (desktop) or WebGL (browser).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, mpsc};

use egui::epaint::{Vertex, WHITE_UV};
use egui::{Color32, ColorImage, Mesh, Pos2, Shape, TextureHandle, TextureId, TextureOptions, pos2};
use lyon_tessellation::math::point;
use lyon_tessellation::path::iterator::PathIterator;
use lyon_tessellation::path::{Path as LPath, PathEvent};
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers};

use crate::geom::{BBox, Camera, aabb, intersects};
use crate::model::{El, Kind};
use crate::prims::{Affine, Cmd, Env, ImageKey, Path, Prim, prims_of};
use crate::text;

/// Converts a path, mapping every point through `t`.
pub fn to_lyon(p: &Path, t: impl Fn(f32, f32) -> (f32, f32)) -> LPath {
    let mut b = LPath::builder();
    let mut open = false;
    let mut start = (0.0, 0.0);
    let ensure = |b: &mut lyon_tessellation::path::path::Builder, open: &mut bool, at: (f32, f32)| {
        if !*open {
            b.begin(point(at.0, at.1));
            *open = true;
        }
    };
    for c in &p.cmds {
        match *c {
            Cmd::Move(x, y) => {
                if open {
                    b.end(false);
                }
                let q = t(x, y);
                b.begin(point(q.0, q.1));
                start = q;
                open = true;
            }
            Cmd::Line(x, y) => {
                ensure(&mut b, &mut open, start);
                let q = t(x, y);
                b.line_to(point(q.0, q.1));
            }
            Cmd::Quad(x1, y1, x, y) => {
                ensure(&mut b, &mut open, start);
                let (c, q) = (t(x1, y1), t(x, y));
                b.quadratic_bezier_to(point(c.0, c.1), point(q.0, q.1));
            }
            Cmd::Cubic(x1, y1, x2, y2, x, y) => {
                ensure(&mut b, &mut open, start);
                let (c1, c2, q) = (t(x1, y1), t(x2, y2), t(x, y));
                b.cubic_bezier_to(point(c1.0, c1.1), point(c2.0, c2.1), point(q.0, q.1));
            }
            Cmd::Close => {
                if open {
                    b.end(true);
                    open = false;
                }
            }
        }
    }
    if open {
        b.end(false);
    }
    b.build()
}

/// The path cut into dashes (`on` drawn, `off` skipped), restarting on every subpath.
fn dashed(path: &LPath, on: f32, off: f32, tol: f32) -> LPath {
    let mut b = LPath::builder();
    if on <= 0.0 {
        return b.build();
    }
    let mut open = false;
    let (mut drawing, mut remaining) = (true, on);
    let seg = |b: &mut lyon_tessellation::path::path::Builder, open: &mut bool, drawing: &mut bool, remaining: &mut f32, from: lyon_tessellation::math::Point, to: lyon_tessellation::math::Point| {
        let mut len = (to - from).length();
        if len <= 0.0 {
            return;
        }
        let dir = (to - from) / len;
        let mut at = from;
        while len > 0.0 {
            let step = remaining.min(len);
            let end = at + dir * step;
            if *drawing {
                if !*open {
                    b.begin(at);
                    *open = true;
                }
                b.line_to(end);
            }
            *remaining -= step;
            len -= step;
            at = end;
            if *remaining <= 1e-6 {
                if *drawing && *open {
                    b.end(false);
                    *open = false;
                }
                *drawing = !*drawing;
                *remaining = if *drawing { on } else { off.max(1e-3) };
            }
        }
    };
    for e in path.iter().flattened(tol) {
        match e {
            PathEvent::Begin { .. } => {
                (drawing, remaining) = (true, on);
            }
            PathEvent::Line { from, to } => seg(&mut b, &mut open, &mut drawing, &mut remaining, from, to),
            PathEvent::End { last, first, close } => {
                if close {
                    seg(&mut b, &mut open, &mut drawing, &mut remaining, last, first);
                }
                if open {
                    b.end(false);
                    open = false;
                }
            }
            _ => {}
        }
    }
    b.build()
}

/// One mesh of an element: coloured, or an image.
struct Part {
    image: Option<(ImageKey, f32)>,
    pos: Vec<[f32; 2]>,
    col: Vec<Color32>,
    idx: Vec<u32>,
}

impl Part {
    fn colored() -> Part {
        Part { image: None, pos: Vec::new(), col: Vec::new(), idx: Vec::new() }
    }
    fn append(&mut self, buf: &VertexBuffers<[f32; 2], u32>, color: Color32) {
        let base = self.pos.len() as u32;
        self.pos.extend_from_slice(&buf.vertices);
        self.col.extend(std::iter::repeat_n(color, buf.vertices.len()));
        self.idx.extend(buf.indices.iter().map(|i| i + base));
    }
    /// Quad with full `inner` colour fading to transparent at `outer` (both as 4 corners).
    fn feathered(&mut self, inner: [[f32; 2]; 4], outer: [[f32; 2]; 4], color: Color32) {
        let base = self.pos.len() as u32;
        self.pos.extend(inner);
        self.pos.extend(outer);
        self.col.extend([color; 4]);
        self.col.extend([Color32::TRANSPARENT; 4]);
        self.idx.extend([0, 1, 2, 0, 2, 3].map(|i| i + base));
        for i in 0..4 {
            let j = (i + 1) % 4;
            self.idx.extend([i, j, 4 + j, i, 4 + j, 4 + i].map(|k| k + base));
        }
    }
}

struct Built {
    ox: f64,
    oy: f64,
    parts: Vec<Part>,
}

#[derive(Clone, Copy, PartialEq)]
struct Key {
    bucket: i32,
    editing: bool,
    cell: Option<(usize, usize)>,
    /// Exact zoom, for elements that keep a size on screen (section titles, comment pins).
    zoom: u64,
    thin: bool,
}

struct Entry {
    el: Arc<El>,
    key: Key,
    built: Rc<Built>,
    used: u64,
}

/// What the screen shows: screen = board · z + (cam.x, cam.y) + origin.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub cam: Camera,
    pub origin: Pos2,
    /// Visible part of the board.
    pub view: BBox,
    pub pixels_per_point: f32,
    pub editing: Option<&'a str>,
    pub editing_cell: Option<(usize, usize)>,
}

type GlyphMesh = Rc<(Vec<[f32; 2]>, Vec<u32>)>;

/// An element cut by the pixel eraser, drawn in software at screen resolution.
struct Erased {
    el: Arc<El>,
    zoom: f64,
    /// Area drawn, in device pixels of the board at this zoom (board · z · ppp).
    area: [f64; 4],
    tex: TextureHandle,
    used: u64,
    /// While it is being erased: the picture and how many points of each mark it shows, so each
    /// move of the eraser redraws (and sends to the graphics card) only the bit it passed over.
    live: Option<(tiny_skia::Pixmap, Vec<usize>)>,
    changed_at: f64,
}

/// Same element apart from its eraser marks.
fn same_body(a: &El, b: &El) -> bool {
    a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h && a.rotation == b.rotation && a.opacity == b.opacity && a.hidden == b.hidden && a.kind == b.kind
}

impl Erased {
    /// The element got new eraser marks (or longer ones) and nothing else: redraws only the area
    /// they cover. False when that is not the case and the whole picture must be drawn again.
    fn grow(&mut self, el: &Arc<El>, k: f64, env: &Env, images: crate::raster::Images) -> bool {
        let Some((pm, lens)) = &mut self.live else { return false };
        if !same_body(&self.el, el) || el.erase.len() < lens.len() {
            return false;
        }
        for (i, n) in lens.iter().enumerate() {
            let (m, o) = (&el.erase[i], &self.el.erase[i]);
            if m.p.len() < *n || o.p.len() != *n || m.s != o.s || m.a != o.a || m.p[..*n] != o.p[..] {
                return false;
            }
        }
        let t = crate::prims::Affine::of(el);
        let mut dirty: Option<[f64; 4]> = None;
        for (i, m) in el.erase.iter().enumerate() {
            let had = lens.get(i).copied();
            if had == Some(m.p.len()) {
                continue;
            }
            // From the last point already drawn, so the join is covered too.
            let from = had.map_or(0, |n| n.saturating_sub(2));
            let r = m.s * k / 2.0 + 2.0;
            for q in m.p[from..].chunks_exact(2) {
                let (wx, wy) = t.apply(q[0] as f64, q[1] as f64);
                let (px, py) = (wx * k - self.area[0], wy * k - self.area[1]);
                let b = [px - r, py - r, px + r, py + r];
                dirty = Some(match dirty {
                    Some(d) => [d[0].min(b[0]), d[1].min(b[1]), d[2].max(b[2]), d[3].max(b[3])],
                    None => b,
                });
            }
        }
        let (w, h) = (pm.width() as f64, pm.height() as f64);
        if let Some(d) = dirty {
            let (x0, y0) = (d[0].floor().clamp(0.0, w), d[1].floor().clamp(0.0, h));
            let (x1, y1) = (d[2].ceil().clamp(0.0, w), d[3].ceil().clamp(0.0, h));
            if x1 > x0 && y1 > y0 {
                // The whole element with every mark, but only this piece of it: what a full
                // redraw gives there. Drawn a little larger, since shapes cut at the edge of a
                // picture come out a shade different along that edge; only the inside is kept.
                let m = 6.0;
                let (rx0, ry0) = ((x0 - m).max(0.0), (y0 - m).max(0.0));
                let (rx1, ry1) = ((x1 + m).min(w), (y1 + m).min(h));
                let Some(mut piece) = tiny_skia::Pixmap::new((rx1 - rx0) as u32, (ry1 - ry0) as u32) else { return false };
                let tr = tiny_skia::Transform::from_row(k as f32, 0.0, 0.0, k as f32, (-(self.area[0] + rx0)) as f32, (-(self.area[1] + ry0)) as f32);
                crate::raster::draw_element(&mut piece, el, env, tr, images);
                let (pw, ph) = ((x1 - x0) as usize, (y1 - y0) as usize);
                let (ox, oy) = ((x0 - rx0) as usize, (y0 - ry0) as usize);
                let (stride, src_stride, row) = (pm.width() as usize * 4, piece.width() as usize * 4, pw * 4);
                let mut out = Vec::with_capacity(row * ph);
                let src = piece.data();
                let data = pm.data_mut();
                for j in 0..ph {
                    let s = (oy + j) * src_stride + ox * 4;
                    let at = (y0 as usize + j) * stride + x0 as usize * 4;
                    data[at..at + row].copy_from_slice(&src[s..s + row]);
                    out.extend_from_slice(&src[s..s + row]);
                }
                let img = ColorImage::from_rgba_premultiplied([pw, ph], &out);
                self.tex.set_partial([x0 as usize, y0 as usize], img, TEXTURE);
            }
        }
        *lens = el.erase.iter().map(|m| m.p.len()).collect();
        self.el = el.clone();
        true
    }
}

pub struct Painter {
    cache: HashMap<String, Entry>,
    erased: HashMap<String, Erased>,
    /// When the zoom last changed: erased elements are redrawn once it settles.
    zoom_at: (f64, f64),
    /// Glyph outlines by face, glyph and level of detail.
    glyphs: HashMap<(usize, u16, u8), GlyphMesh>,
    fill: FillTessellator,
    stroke: StrokeTessellator,
    pub images: Images,
    frame: u64,
    /// Vertices and indices of the last frame: the next one reserves them at once instead of
    /// growing (and copying) a big buffer many times.
    pub last_len: (usize, usize),
    /// Every element id seen so far, and when the new ones arrived: they pop in.
    known: std::collections::HashSet<String>,
    born: HashMap<String, f64>,
}

impl Painter {
    pub fn new(images: Images) -> Painter {
        Painter { cache: HashMap::new(), erased: HashMap::new(), zoom_at: (0.0, 0.0), glyphs: HashMap::new(), fill: FillTessellator::new(), stroke: StrokeTessellator::new(), images, frame: 0, last_len: (0, 0), known: Default::default(), born: HashMap::new() }
    }

    /// Paints the elements that are on screen; returns whether any visible element is.
    pub fn paint(&mut self, ctx: &egui::Context, out: &mut Vec<Shape>, els: &[Arc<El>], f: &Frame) -> bool {
        self.frame += 1;
        self.images.poll(ctx);
        if std::mem::take(&mut self.images.changed) {
            self.erased.clear();
        }
        let z = f.cam.z;
        let pixel = 1.0 / (z * f.pixels_per_point as f64);
        let bucket = (2.0 * z.log2()).floor() as i32;
        let mut mesh = Mesh::default();
        mesh.vertices.reserve(self.last_len.0);
        mesh.indices.reserve(self.last_len.1);
        let mut len = (0, 0);
        let mut on_screen = false;
        let now = crate::platform::now_ms();
        if self.zoom_at.0 != z {
            self.zoom_at = (z, now);
        }
        let settled = now - self.zoom_at.1 > 150.0;
        // Things added after the board opened (here or by someone else) pop in, like in FigJam.
        // Strokes and lines are left alone: they were already seen while being drawn.
        for el in els {
            if !self.known.contains(&el.id) {
                self.known.insert(el.id.clone());
                if self.frame > 2 && !matches!(el.kind, Kind::Ink(_) | Kind::Highlighter(_) | Kind::Line(_) | Kind::Comment { .. }) {
                    self.born.insert(el.id.clone(), now);
                }
            }
        }
        if !self.born.is_empty() {
            self.born.retain(|_, t| now - *t < POP_MS);
            ctx.request_repaint();
        }
        for el in els {
            if el.hidden || !intersects(&aabb(el), &f.view) {
                continue;
            }
            on_screen = true;
            if !el.erase.is_empty() {
                flush(out, &mut mesh);
                self.paint_erased(ctx, out, el, f, settled);
                continue;
            }
            let zoom_bound = matches!(el.kind, Kind::Section { .. } | Kind::Comment { .. });
            let editing = f.editing == Some(el.id.as_str());
            let key = Key { bucket, editing, cell: if editing { f.editing_cell } else { None }, zoom: if zoom_bound { z.to_bits() } else { 0 }, thin: el.ink().is_some_and(|i| i.size <= pixel) };
            let built = match self.cache.get_mut(&el.id) {
                Some(e) if Arc::ptr_eq(&e.el, el) && e.key == key => {
                    e.used = self.frame;
                    e.built.clone()
                }
                _ => {
                    let env = Env { zoom: z, pixel: Some(pixel), hairline: false, editing: f.editing, comments: true, editing_cell: f.editing_cell };
                    let built = Rc::new(self.build(el, &env, tolerance(bucket, f.pixels_per_point)));
                    self.cache.insert(el.id.clone(), Entry { el: el.clone(), key, built: built.clone(), used: self.frame });
                    built
                }
            };
            // screen = (o + v) · z + cam + origin, computed in f64 so far-away boards stay exact.
            let (ax, ay) = (built.ox * z + f.cam.x + f.origin.x as f64, built.oy * z + f.cam.y + f.origin.y as f64);
            // Popping in: a quick grow with a little overshoot, and a fade, about its centre.
            let (k, fade) = self.born.get(&el.id).map_or((1.0, 1.0), |t| {
                let t = ((now - t) / POP_MS).clamp(0.0, 1.0) as f32;
                (0.86 + 0.14 * egui::emath::easing::back_out(t), egui::emath::easing::cubic_out((t / 0.6).min(1.0)))
            });
            let (cx, cy) = ((el.x + el.w / 2.0 - built.ox) * z + ax, (el.y + el.h / 2.0 - built.oy) * z + ay);
            let place = |p: [f32; 2]| {
                let (x, y) = (p[0] as f64 * z + ax, p[1] as f64 * z + ay);
                if k == 1.0 { pos2(x as f32, y as f32) } else { pos2((cx + (x - cx) * k as f64) as f32, (cy + (y - cy) * k as f64) as f32) }
            };
            for part in &built.parts {
                match &part.image {
                    None => {
                        let base = mesh.vertices.len() as u32;
                        mesh.vertices.extend(part.pos.iter().zip(&part.col).map(|(&p, &c)| Vertex { pos: place(p), uv: WHITE_UV, color: if fade < 1.0 { c.gamma_multiply(fade) } else { c } }));
                        mesh.indices.extend(part.idx.iter().map(|i| i + base));
                        len = (len.0 + part.pos.len(), len.1 + part.idx.len());
                    }
                    Some((key, alpha)) => {
                        let corners: Vec<Pos2> = part.pos.iter().map(|&p| place(p)).collect();
                        match self.images.texture(ctx, key) {
                            Some(Ok(tex)) => {
                                flush(out, &mut mesh);
                                let tint = Color32::from_white_alpha((alpha * 255.0) as u8);
                                let mut m = Mesh::with_texture(tex);
                                for (c, uv) in corners.iter().zip([pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)]) {
                                    m.vertices.push(Vertex { pos: *c, uv, color: tint });
                                }
                                m.indices.extend([0, 1, 2, 0, 2, 3]);
                                out.push(Shape::mesh(m));
                            }
                            // Loading, or broken: a soft placeholder of the same size.
                            other => {
                                let c = if other.is_some() { Color32::from_rgba_unmultiplied(242, 72, 34, 20) } else { Color32::from_rgba_unmultiplied(128, 128, 128, 31) };
                                let base = mesh.vertices.len() as u32;
                                mesh.vertices.extend(corners.iter().map(|&p| Vertex { pos: p, uv: WHITE_UV, color: c }));
                                mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| i + base));
                            }
                        }
                    }
                }
            }
        }
        flush(out, &mut mesh);
        self.last_len = len;
        // The software copy of an erased element is kept only while it is being erased: the
        // graphics card has the picture, and each copy can take tens of megabytes.
        for e in self.erased.values_mut() {
            if e.live.is_some() && now - e.changed_at > 2500.0 {
                e.live = None;
            }
        }
        // Forget meshes not drawn for a while (elements deleted or long off screen).
        if self.frame % 240 == 0 {
            let now = self.frame;
            self.cache.retain(|_, e| now - e.used < 600);
            self.erased.retain(|_, e| now - e.used < 600);
            let present: std::collections::HashSet<&str> = els.iter().map(|e| e.id.as_str()).collect();
            self.known.retain(|id| present.contains(id.as_str()));
        }
        if !settled && self.erased.values().any(|e| e.zoom != z) {
            ctx.request_repaint_after(std::time::Duration::from_millis(160));
        }
        on_screen
    }

    fn paint_erased(&mut self, ctx: &egui::Context, out: &mut Vec<Shape>, el: &Arc<El>, f: &Frame, settled: bool) {
        let (z, ppp) = (f.cam.z, f.pixels_per_point as f64);
        let k = z * ppp;
        let b = aabb(el);
        let full = [b.x * k, b.y * k, b.right() * k, b.bottom() * k];
        // What is visible, plus half a screen around it so panning doesn't redraw at once.
        let v = crate::geom::expand(f.view, f.view.w.max(f.view.h) * 0.5);
        let want = [full[0].max(v.x * k).floor(), full[1].max(v.y * k).floor(), full[2].min(v.right() * k).ceil(), full[3].min(v.bottom() * k).ceil()];
        if want[2] <= want[0] || want[3] <= want[1] {
            return;
        }
        let seen = |area: [f64; 4]| [area[0].max(f.view.x * k), area[1].max(f.view.y * k), area[2].min(f.view.right() * k), area[3].min(f.view.bottom() * k)];
        let covers = |e: &Erased| {
            let s = seen(want);
            e.area[0] <= s[0] && e.area[1] <= s[1] && e.area[2] >= s[2] && e.area[3] >= s[3]
        };
        let fresh = self.erased.get(&el.id).is_some_and(|e| Arc::ptr_eq(&e.el, el) && (e.zoom == z || !settled) && (e.zoom != z || covers(e)));
        if !fresh {
            let env = Env { zoom: z, pixel: Some(1.0 / k), hairline: false, editing: f.editing, comments: true, editing_cell: f.editing_cell };
            let images = &self.images;
            let fetch = |key: &ImageKey| images.pixmap(key);
            let now = crate::platform::now_ms();
            // While erasing, only what the eraser just passed over is drawn again.
            let grown = self.erased.get_mut(&el.id).is_some_and(|e| e.zoom == z && covers(e) && e.grow(el, k, &env, &fetch));
            if grown {
                if let Some(e) = self.erased.get_mut(&el.id) {
                    e.changed_at = now;
                }
            } else {
                // Never bigger than 4096 px a side: far zoomed in, only what is around the screen.
                let (w, h) = ((want[2] - want[0]).min(4096.0), (want[3] - want[1]).min(4096.0));
                let area = [want[0], want[1], want[0] + w, want[1] + h];
                let t = tiny_skia::Transform::from_row(k as f32, 0.0, 0.0, k as f32, (-area[0]) as f32, (-area[1]) as f32);
                if let Some(mut pm) = tiny_skia::Pixmap::new(w as u32, h as u32) {
                    crate::raster::draw_element(&mut pm, el, &env, t, &fetch);
                    let img = ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
                    let live = Some((pm, el.erase.iter().map(|m| m.p.len()).collect()));
                    match self.erased.get_mut(&el.id) {
                        Some(e) => {
                            e.tex.set(img, TEXTURE);
                            (e.el, e.zoom, e.area, e.live, e.changed_at) = (el.clone(), z, area, live, now);
                        }
                        None => {
                            let tex = ctx.load_texture(format!("erased:{}", el.id), img, TEXTURE);
                            self.erased.insert(el.id.clone(), Erased { el: el.clone(), zoom: z, area, tex, used: self.frame, live, changed_at: now });
                        }
                    }
                }
            }
        }
        let Some(e) = self.erased.get_mut(&el.id) else { return };
        e.used = self.frame;
        // Area back to screen points (scaled if drawn at another zoom).
        let s = z / e.zoom;
        let to = |x: f64, y: f64| pos2((x * s / ppp + f.cam.x + f.origin.x as f64) as f32, (y * s / ppp + f.cam.y + f.origin.y as f64) as f32);
        let rect = egui::Rect::from_min_max(to(e.area[0], e.area[1]), to(e.area[2], e.area[3]));
        let mut m = Mesh::with_texture(e.tex.id());
        m.add_rect_with_uv(rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        out.push(Shape::mesh(m));
    }

    fn build(&mut self, el: &El, env: &Env, tol: f32) -> Built {
        let t = Affine::of(el);
        let (ox, oy) = (el.x, el.y);
        let tf = |x: f32, y: f32| {
            let (wx, wy) = t.apply(x as f64, y as f64);
            ((wx - ox) as f32, (wy - oy) as f32)
        };
        let mut parts = vec![Part::colored()];
        for prim in prims_of(el, env) {
            match prim {
                Prim::Fill { path, color } => {
                    let lp = to_lyon(&path, tf);
                    let mut buf = VertexBuffers::new();
                    let _ = self.fill.tessellate_path(&lp, &FillOptions::tolerance(tol).with_fill_rule(FillRule::NonZero), &mut BuffersBuilder::new(&mut buf, |v: FillVertex| v.position().to_array()));
                    last_colored(&mut parts).append(&buf, color);
                }
                Prim::Stroke { path, width, color, round, dash } => {
                    if width <= 0.0 {
                        continue;
                    }
                    let mut lp = to_lyon(&path, tf);
                    if let Some([on, off]) = dash {
                        lp = dashed(&lp, on, off, tol);
                    }
                    let (cap, join) = if round { (LineCap::Round, LineJoin::Round) } else { (LineCap::Butt, LineJoin::Bevel) };
                    let opts = StrokeOptions::tolerance(tol).with_line_width(width).with_line_cap(cap).with_line_join(join);
                    let mut buf = VertexBuffers::new();
                    let _ = self.stroke.tessellate_path(&lp, &opts, &mut BuffersBuilder::new(&mut buf, |v: StrokeVertex| v.position().to_array()));
                    last_colored(&mut parts).append(&buf, color);
                }
                Prim::Text { placed, color } => {
                    let part = last_colored(&mut parts);
                    let size = placed.k * text::faces()[placed.face].upem;
                    // Text a few pixels tall cannot be read: a faint bar per line shows it is there,
                    // for a handful of vertices instead of hundreds per word.
                    if size * 0.33 / tol < 6.0 {
                        let faint = color.gamma_multiply(0.45);
                        let mut lines: Vec<(f32, f32, f32)> = Vec::new();
                        for &(_, gx, gy) in &placed.glyphs {
                            match lines.last_mut() {
                                Some(l) if l.2 == gy => l.1 = l.1.max(gx),
                                _ => lines.push((gx, gx, gy)),
                            }
                        }
                        for (x0, x1, y) in lines {
                            let corners = [(x0, y - 0.5 * size), (x1 + 0.5 * size, y - 0.5 * size), (x1 + 0.5 * size, y - 0.1 * size), (x0, y - 0.1 * size)].map(|(cx, cy)| {
                                let (px, py) = tf(cx, cy);
                                [px, py]
                            });
                            let base = part.pos.len() as u32;
                            part.pos.extend(corners);
                            part.col.extend([faint; 4]);
                            part.idx.extend([0, 1, 2, 0, 2, 3].map(|i| i + base));
                        }
                        continue;
                    }
                    for &(g, gx, gy) in &placed.glyphs {
                        let mesh = self.glyph(placed.face, g, tol / placed.k);
                        let base = part.pos.len() as u32;
                        part.pos.extend(mesh.0.iter().map(|&[px, py]| {
                            let (x, y) = tf(gx + (px + placed.lean * py) * placed.k, gy - py * placed.k);
                            [x, y]
                        }));
                        part.col.extend(std::iter::repeat_n(color, mesh.0.len()));
                        part.idx.extend(mesh.1.iter().map(|i| i + base));
                    }
                }
                Prim::Image { key, x, y, w, h, alpha } => {
                    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].map(|(cx, cy)| {
                        let (px, py) = tf(cx, cy);
                        [px, py]
                    });
                    parts.push(Part { image: Some((key, alpha)), pos: corners.to_vec(), col: Vec::new(), idx: Vec::new() });
                }
                Prim::Shadow { x, y, w, h, blur, dy, color, .. } => {
                    let b = blur / 2.0;
                    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(px, py)| {
                        let (qx, qy) = tf(px, py);
                        [qx, qy]
                    });
                    let (ib, jb) = (b.min(w / 2.0), b.min(h / 2.0));
                    let inner = rect(x + ib, y + dy + jb, x + w - ib, y + dy + h - jb);
                    let outer = rect(x - b, y + dy - b, x + w + b, y + dy + h + b);
                    last_colored(&mut parts).feathered(inner, outer, color);
                }
            }
        }
        parts.retain(|p| p.image.is_some() || !p.idx.is_empty());
        Built { ox, oy, parts }
    }

    /// A glyph flattened to within `tol` font units. Small text on screen gets coarser outlines:
    /// the finest level is needed only when zoomed in a lot, and costs ten times the vertices.
    fn glyph(&mut self, face: usize, g: u16, tol: f32) -> GlyphMesh {
        let finest = text::faces()[face].upem * 0.0008;
        let level = (tol / finest).log2().floor().clamp(0.0, 8.0) as u8;
        self.glyphs
            .entry((face, g, level))
            .or_insert_with(|| {
                let path = text::glyph_path(face, g);
                let lp = to_lyon(&path, |x, y| (x, y));
                let mut buf: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
                let tol = finest * (1u32 << level) as f32;
                let _ = FillTessellator::new().tessellate_path(&lp, &FillOptions::tolerance(tol).with_fill_rule(FillRule::NonZero), &mut BuffersBuilder::new(&mut buf, |v: FillVertex| v.position().to_array()));
                Rc::new((buf.vertices, buf.indices))
            })
            .clone()
    }

    /// Drops every cached mesh (fonts or images changed).
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

/// How long something new takes to pop in.
const POP_MS: f64 = 320.0;

/// Curves are flattened to within a third of a device pixel at the deepest zoom of the level.
fn tolerance(bucket: i32, ppp: f32) -> f32 {
    let z_hi = 2f64.powf((bucket + 1) as f64 / 2.0);
    (0.33 / (z_hi * ppp as f64)) as f32
}

fn last_colored(parts: &mut Vec<Part>) -> &mut Part {
    if parts.last().is_none_or(|p| p.image.is_some()) {
        parts.push(Part::colored());
    }
    parts.last_mut().unwrap()
}

fn flush(out: &mut Vec<Shape>, mesh: &mut Mesh) {
    if !mesh.indices.is_empty() {
        out.push(Shape::mesh(std::mem::take(mesh)));
    }
}

/// A path already in screen coordinates, filled.
pub fn fill_mesh(path: &Path, color: Color32) -> Mesh {
    let lp = to_lyon(path, |x, y| (x, y));
    let mut buf: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let _ = FillTessellator::new().tessellate_path(&lp, &FillOptions::tolerance(0.15).with_fill_rule(FillRule::NonZero), &mut BuffersBuilder::new(&mut buf, |v: FillVertex| v.position().to_array()));
    let mut m = Mesh::default();
    m.vertices = buf.vertices.iter().map(|p| Vertex { pos: pos2(p[0], p[1]), uv: WHITE_UV, color }).collect();
    m.indices = buf.indices;
    m
}

/* ---------------- images ---------------- */

enum Img {
    Loading,
    Ready(TextureHandle),
    Failed,
}

/// Starts fetching an uploaded image; calls back with its bytes (or None) from any thread.
pub type Fetch = Arc<dyn Fn(String, Box<dyn FnOnce(Option<Vec<u8>>) + Send>) + Send + Sync>;

/// Textures of the board's images and reactions, loaded on first use.
pub struct Images {
    tex: HashMap<ImageKey, Img>,
    /// Pictures also kept in memory for software drawing (erased images, exports).
    pixmaps: std::cell::RefCell<HashMap<String, Option<Arc<tiny_skia::Pixmap>>>>,
    /// A picture for software drawing arrived: erased elements must be drawn again.
    changed: bool,
    tx: mpsc::Sender<(ImageKey, Option<ColorImage>)>,
    rx: mpsc::Receiver<(ImageKey, Option<ColorImage>)>,
    fetch: Option<Fetch>,
}

const TEXTURE: TextureOptions = TextureOptions {
    magnification: egui::TextureFilter::Linear,
    minification: egui::TextureFilter::Linear,
    wrap_mode: egui::TextureWrapMode::ClampToEdge,
    mipmap_mode: Some(egui::TextureFilter::Linear),
};

/// Pictures bigger than this are scaled down when decoded.
const MAX_SIDE: u32 = 4096;

pub fn decode_image(bytes: &[u8]) -> Option<ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut img = reader.decode().ok()?;
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba))
}

impl Images {
    pub fn new(fetch: Option<Fetch>) -> Images {
        let (tx, rx) = mpsc::channel();
        Images { tex: HashMap::new(), pixmaps: Default::default(), changed: false, tx, rx, fetch }
    }

    /// Puts a just-added image in the cache, so it shows without a round trip.
    pub fn prime(&mut self, ctx: &egui::Context, file_id: &str, img: ColorImage) {
        let h = ctx.load_texture(format!("img:{file_id}"), img, TEXTURE);
        self.tex.insert(ImageKey::File(file_id.into()), Img::Ready(h));
    }

    /// The picture for software drawing; starts loading it when missing.
    pub fn pixmap(&self, key: &ImageKey) -> Option<Arc<tiny_skia::Pixmap>> {
        match key {
            ImageKey::Stamp(emoji) => crate::raster::stamp_pixmap(emoji),
            ImageKey::File(id) => {
                if let Some(p) = self.pixmaps.borrow().get(id) {
                    return p.clone();
                }
                self.pixmaps.borrow_mut().insert(id.clone(), None);
                if let Some(fetch) = &self.fetch {
                    let (tx, k) = (self.tx.clone(), key.clone());
                    fetch(id.clone(), Box::new(move |bytes| {
                        let _ = tx.send((k, bytes.as_deref().and_then(decode_image)));
                    }));
                }
                None
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((key, img)) = self.rx.try_recv() {
            if let (ImageKey::File(id), Some(img)) = (&key, &img)
                && self.pixmaps.borrow().contains_key(id)
            {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                let pm = crate::raster::pixmap_from_rgba(img.size[0] as u32, img.size[1] as u32, &rgba).map(Arc::new);
                self.pixmaps.borrow_mut().insert(id.clone(), pm);
                self.changed = true;
            }
            let state = match img {
                Some(img) => Img::Ready(ctx.load_texture(format!("{key:?}"), img, TEXTURE)),
                None => Img::Failed,
            };
            self.tex.insert(key, state);
        }
    }

    /// The texture if ready; None while loading; Some(Err) when it can't be shown.
    pub fn texture(&mut self, ctx: &egui::Context, key: &ImageKey) -> Option<Result<TextureId, ()>> {
        if !self.tex.contains_key(key) {
            match key {
                ImageKey::Stamp(emoji) => {
                    let img = crate::assets::stamp(emoji).and_then(|s| crate::assets::rasterize_svg(s.svg, 256)).map(|p| ColorImage::from_rgba_premultiplied([p.width() as usize, p.height() as usize], p.data()));
                    let state = match img {
                        Some(img) => Img::Ready(ctx.load_texture(format!("stamp:{emoji}"), img, TEXTURE)),
                        None => Img::Failed,
                    };
                    self.tex.insert(key.clone(), state);
                }
                ImageKey::File(id) => {
                    self.tex.insert(key.clone(), Img::Loading);
                    match &self.fetch {
                        Some(fetch) => {
                            let (tx, k, ctx) = (self.tx.clone(), key.clone(), ctx.clone());
                            fetch(
                                id.clone(),
                                Box::new(move |bytes| {
                                    let _ = tx.send((k, bytes.as_deref().and_then(decode_image)));
                                    ctx.request_repaint();
                                }),
                            );
                        }
                        None => {
                            self.tex.insert(key.clone(), Img::Failed);
                        }
                    }
                }
            }
        }
        match self.tex.get(key) {
            Some(Img::Ready(h)) => Some(Ok(h.id())),
            Some(Img::Failed) => Some(Err(())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_cover_on_and_skip_off() {
        let p = to_lyon(&Path::polyline(&[0.0, 0.0, 10.0, 0.0]), |x, y| (x, y));
        let d = dashed(&p, 3.0, 2.0, 0.1);
        let begins = d.iter().filter(|e| matches!(e, PathEvent::Begin { .. })).count();
        assert_eq!(begins, 2, "0–3 and 5–8");
    }

    #[test]
    fn the_three_spacings_never_look_alike() {
        for z in [0.04, 0.1, 0.33, 0.5, 0.71, 1.0, 1.4, 2.0, 3.0, 7.5, 20.0, 32.0] {
            let s: Vec<f32> = [12.0, 24.0, 48.0].iter().map(|g| pattern_levels(*g, z, 2.0).0).collect();
            assert!(s[0] < s[1] && s[1] < s[2], "zoom {z}: {s:?}");
            for (g, v) in [12.0, 24.0, 48.0].iter().zip(&s) {
                assert!(*v >= *g as f32 - 1e-3 && *v < *g as f32 * 2.0 + 1e-3, "zoom {z}: {v} for {g}");
            }
        }
        // Just before a level change the finer dots are all there, just after it they are the
        // main ones: the picture doesn't jump.
        let (a, sub_a, fade_a) = pattern_levels(24.0, 0.999, 2.0);
        let (b, _, fade_b) = pattern_levels(24.0, 1.001, 2.0);
        assert!((sub_a - 24.0).abs() < 0.1 && fade_a > 0.99, "{a} {sub_a} {fade_a}");
        assert!((b - 24.0).abs() < 0.1 && fade_b < 0.01, "{b} {fade_b}");
    }

    /// Erasing redraws only around the eraser; the picture must be the same as drawing it all.
    #[test]
    fn erasing_bit_by_bit_matches_a_full_redraw() {
        use crate::model::{EraseMark, Ink};
        let ctx = egui::Context::default();
        let pts: Vec<f32> = (0..=60).flat_map(|i| [i as f32 * 5.0, (i as f32 * 0.4).sin() * 30.0 + 40.0, 0.6]).collect();
        let mut el = El::new(Kind::Ink(Ink { points: pts, color: "#1E1E1E".into(), size: 12.0 }));
        (el.x, el.y, el.w, el.h) = (10.0, 10.0, 300.0, 80.0);
        let k = 1.5;
        let env = Env { zoom: k, pixel: Some(1.0 / k), comments: true, ..Env::default() };
        let none = |_: &ImageKey| None;
        let area = [0.0, 0.0, 500.0, 200.0];
        let draw = |el: &El| {
            let mut pm = tiny_skia::Pixmap::new(500, 200).unwrap();
            crate::raster::draw_element(&mut pm, el, &env, tiny_skia::Transform::from_row(k as f32, 0.0, 0.0, k as f32, 0.0, 0.0), &none);
            pm
        };
        for strength in [1.0, 0.5] {
            let mut first = el.clone();
            first.erase = vec![EraseMark { p: vec![40.0, -10.0, 60.0, 30.0], s: 16.0, a: strength }];
            let first = Arc::new(first);
            let pm = draw(&first);
            let tex = ctx.load_texture("t", ColorImage::from_rgba_premultiplied([500, 200], pm.data()), TEXTURE);
            let mut e = Erased { el: first.clone(), zoom: k, area, tex, used: 0, live: Some((pm, vec![4])), changed_at: 0.0 };
            // The first pass goes on, and a second one starts elsewhere.
            let mut next = (*first).clone();
            next.erase[0].p.extend([90.0, 60.0, 140.0, 70.0]);
            next.erase.push(EraseMark { p: vec![200.0, 0.0, 230.0, 80.0], s: 10.0, a: strength });
            let next = Arc::new(next);
            assert!(e.grow(&next, k, &env, &none), "only marks were added");
            let whole = draw(&next);
            let piecewise = &e.live.as_ref().unwrap().0;
            let diffs: Vec<i32> = whole.data().chunks_exact(4).zip(piecewise.data().chunks_exact(4)).map(|(a, b)| a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).abs()).max().unwrap()).collect();
            let worst = diffs.iter().max().copied().unwrap();
            let off = diffs.iter().filter(|d| **d > 8).count();
            let inked = whole.data().chunks_exact(4).filter(|p| p[3] > 0).count();
            // Full strength: the same to the pixel. Partial: antialiasing may differ by a shade on
            // a pixel or two, where the redrawn piece cuts a pass.
            if strength == 1.0 {
                assert!(worst <= 2, "strength {strength}: differs by {worst}");
            } else {
                assert!(worst <= 48 && off * 1000 <= inked, "strength {strength}: worst {worst}, {off} of {inked} pixels off");
            }
            // Anything else changed (here the colour): drawn again from scratch.
            let mut moved = (*next).clone();
            moved.ink_mut().unwrap().color = "#E03131".into();
            assert!(!e.grow(&Arc::new(moved), k, &env, &none));
        }
    }

    #[test]
    fn every_element_kind_tessellates() {
        let doc = yrs::Doc::new();
        crate::doc::apply_update(&doc, include_bytes!("../testdata/legacy.ydoc"), crate::doc::REMOTE).unwrap();
        let mut b = crate::doc::Board::new(doc, Arc::new(|| {}));
        let mut p = Painter::new(Images::new(None));
        for el in b.paint_order().to_vec() {
            let built = p.build(&el, &Env { comments: true, ..Env::default() }, 0.1);
            let has_mesh = built.parts.iter().any(|q| !q.idx.is_empty() || q.image.is_some());
            assert!(has_mesh, "{} draws something", el.id);
        }
    }
}

/* ---------------- background ---------------- */

/// Board colour plus a pattern (dots, grid, lines, graph paper, isometric) readable at any zoom.
/// `rect` is the canvas on screen; the camera is relative to its top-left corner.
pub fn background(out: &mut Vec<Shape>, rect: egui::Rect, cam: &Camera, meta: &crate::model::BoardMeta, ppp: f32) {
    use crate::model::Pattern;
    let bg = crate::model::color_or(&meta.background, Color32::from_gray(0xF5));
    out.push(Shape::rect_filled(rect, 0.0, bg));
    if meta.pattern == Pattern::None {
        return;
    }
    let (step, sub, fade) = pattern_levels(meta.grid_size, cam.z, if meta.pattern == Pattern::Graph { 5.0 } else { 2.0 });
    let dark = crate::model::is_dark(&meta.background);
    let ink = |a: f32| if dark { Color32::from_white_alpha((a * 255.0).round() as u8) } else { Color32::from_black_alpha((a * 255.0).round() as u8) };
    let (w, h) = (rect.width(), rect.height());
    let mut mesh = Mesh::default();
    let mut quad = |x0: f32, y0: f32, x1: f32, y1: f32, c: Color32| {
        if c.a() > 0 {
            mesh.add_colored_rect(egui::Rect::from_min_max(pos2(rect.min.x + x0, rect.min.y + y0), pos2(rect.min.x + x1, rect.min.y + y1)), c);
        }
    };
    // One device pixel wide, on the pixel grid.
    let px = 1.0 / ppp;
    let crisp = |v: f32| ((v * ppp).round() + 0.5) / ppp;
    // Every line (or dot) of the finest level in view, with its index counted from the board's
    // origin: the index says which level it belongs to, so the pattern stays fixed on the board.
    let ticks = |origin: f64, len: f32| {
        let s = sub as f64;
        let first = ((-origin) / s).ceil() as i64;
        (first..).map(move |i| (i, (origin + i as f64 * s) as f32)).take_while(move |(_, v)| *v < len)
    };
    let r = 1.1;
    match meta.pattern {
        Pattern::Dots => {
            let (major, minor) = (ink(0.16), ink(0.16 * fade));
            for (i, x) in ticks(cam.x, w + r) {
                for (j, y) in ticks(cam.y, h + r) {
                    quad(x - r, y - r, x + r, y + r, if i % 2 == 0 && j % 2 == 0 { major } else { minor });
                }
            }
        }
        Pattern::Isometric => {
            // A triangular lattice: every other row shifted by half a step. The finer level's
            // rows sit half-way between, its dots half-way along.
            let (major, minor) = (ink(0.18), ink(0.18 * fade));
            let s = sub as f64;
            let row_h = s * 0.866;
            let mut row = (-cam.y / row_h).floor() as i64 - 1;
            while (row as f64 * row_h + cam.y) < h as f64 + row_h {
                let y = (row as f64 * row_h + cam.y) as f32;
                let shift = row.rem_euclid(2) as f64 * s / 2.0;
                let first = ((-(cam.x + shift)) / s).ceil() as i64;
                let mut k = first;
                loop {
                    let x = (cam.x + shift + k as f64 * s) as f32;
                    if x > w + r {
                        break;
                    }
                    let on_major = row.rem_euclid(2) == 0 && k.rem_euclid(2) == (row / 2).rem_euclid(2);
                    quad(x - r, y - r, x + r, y + r, if on_major { major } else { minor });
                    k += 1;
                }
                row += 1;
            }
        }
        Pattern::Graph | Pattern::Grid | Pattern::Lines => {
            // Graph paper: fine squares and a stronger line every five; grid and ruled lines:
            // one weight. The finer level fades in as the squares grow.
            let level = |i: i64| -> Color32 {
                match meta.pattern {
                    Pattern::Graph if i.rem_euclid(25) == 0 => ink(0.11),
                    Pattern::Graph if i.rem_euclid(5) == 0 => ink(0.05 + 0.06 * fade),
                    Pattern::Graph => ink(0.05 * fade),
                    Pattern::Grid if i.rem_euclid(2) == 0 => ink(0.07),
                    Pattern::Grid => ink(0.07 * fade),
                    _ if i.rem_euclid(2) == 0 => ink(0.08),
                    _ => ink(0.08 * fade),
                }
            };
            if meta.pattern != Pattern::Lines {
                for (i, x) in ticks(cam.x, w) {
                    let cx = crisp(x);
                    quad(cx - px / 2.0, 0.0, cx + px / 2.0, h, level(i));
                }
            }
            for (i, y) in ticks(cam.y, h) {
                let cy = crisp(y);
                quad(0.0, cy - px / 2.0, w, cy + px / 2.0, level(i));
            }
        }
        Pattern::None => {}
    }
    let _ = step;
    out.push(Shape::mesh(mesh));
}

/// The pattern's spacing on screen at zoom `z`: the chosen spacing `g` (board units) times a
/// power of `base`, so that it stays between g and g·base pixels whatever the zoom. Fitto, Medio
/// and Ampio (12, 24, 48) therefore never look alike. Returns that step, the finer level's step
/// (step / base) and how much of the finer level shows (0 → 1 as the step grows), so moving from
/// one level to the next never jumps.
pub fn pattern_levels(g: f64, z: f64, base: f64) -> (f32, f32, f32) {
    let n = (-z.ln() / base.ln()).ceil();
    let step = g * base.powf(n) * z;
    let t = ((step / g).ln() / base.ln()).clamp(0.0, 1.0);
    // Only in the second half, eased: the finer level stays out of the way most of the time.
    let k = ((t - 0.5) * 2.0).clamp(0.0, 1.0);
    let fade = k * k * (3.0 - 2.0 * k);
    (step as f32, (step / base) as f32, fade as f32)
}

#[cfg(test)]
pub mod picture {
    use super::*;

    use crate::model::*;

    /// One element of each kind side by side.
    pub fn sample() -> Vec<Arc<El>> {
        let mut out = Vec::new();
        let mut add = |x: f64, y: f64, w: f64, h: f64, kind: Kind| {
            let mut el = El::new(kind);
            (el.x, el.y, el.w, el.h, el.z) = (x, y, w, h, out.len() as f64);
            out.push(Arc::new(el));
        };
        let shape = |k: ShapeKind, fill: &str, text: Option<&str>| Kind::Shape(crate::model::Shape { shape: k, fill: fill.into(), stroke: "#1E1E1E".into(), stroke_width: 3.0, radius: 16.0, text: text.map(str::to_string), ..Default::default() });
        add(40.0, 40.0, 160.0, 100.0, shape(ShapeKind::Rect, "transparent", None));
        add(240.0, 40.0, 160.0, 100.0, shape(ShapeKind::Ellipse, "#C7E5FF", Some("Ellisse con testo")));
        add(440.0, 40.0, 120.0, 100.0, shape(ShapeKind::Star, "#FFE066", None));
        add(600.0, 40.0, 140.0, 100.0, shape(ShapeKind::Diamond, "transparent", Some("Sì?")));
        let pts: Vec<f32> = (0..=40).flat_map(|i| { let t = i as f32 / 40.0; [t * 180.0, (t * 9.0).sin() * 30.0 + 40.0, 0.3 + 0.6 * (t * 3.1).sin().abs()] }).collect();
        add(40.0, 180.0, 180.0, 80.0, Kind::Ink(Ink { points: pts.clone(), color: "#1971C2".into(), size: 6.0 }));
        add(240.0, 180.0, 180.0, 80.0, Kind::Highlighter(Ink { points: pts.iter().enumerate().map(|(i, v)| if i % 3 == 2 { -1.0 } else { *v }).collect(), color: "#FFE066".into(), size: 22.0 }));
        add(440.0, 200.0, 140.0, 40.0, Kind::Line(Line { points: vec![0.0, 40.0, 140.0, 0.0], stroke: "#E03131".into(), stroke_width: 3.0, dash: false, arrow_start: false, arrow_end: true, from: None, to: None, tape: false, route: crate::model::Route::Straight, label: None }));
        add(600.0, 200.0, 140.0, 40.0, Kind::Line(Line { points: vec![0.0, 20.0, 140.0, 20.0], stroke: "#FFB3C7".into(), stroke_width: 28.0, dash: false, arrow_start: false, arrow_end: false, from: None, to: None, tape: true, route: crate::model::Route::Straight, label: None }));
        add(40.0, 300.0, 220.0, 220.0, Kind::Sticky(Sticky { text: "Una nota adesiva con un po' di testo".into(), color: "#FFF3A3".into(), font: FontKind::Hand, author: Some("Anto".into()), ..Default::default() }));
        add(300.0, 300.0, 0.0, 0.0, Kind::Text(Text { text: "Titolo in grassetto\nseconda riga".into(), color: "#1E1E1E".into(), font_size: 24.0, font: FontKind::Sans, align: Align::Left, bold: true, italic: false, fixed_width: false, strike: false }));
        add(300.0, 400.0, 64.0, 64.0, Kind::Stamp { emoji: "🎉".into() });
        add(400.0, 400.0, 64.0, 64.0, Kind::Stamp { emoji: "👍".into() });
        add(520.0, 300.0, 220.0, 160.0, Kind::Section { fill: "#E3F1FF".into() });
        add(760.0, 80.0, 0.0, 0.0, Kind::Comment { thread: vec![CommentMsg { author: "Anto".into(), color: "#0D99FF".into(), text: "Ok".into(), t: 0.0 }, CommentMsg { author: "B".into(), color: "#9747FF".into(), text: "Sì".into(), t: 0.0 }] });
        out.into_iter().map(|el| {
            let mut el = (*el).clone();
            crate::text::fit_text(&mut el);
            Arc::new(el)
        }).collect()
    }

    /// Tables, code and widgets, to look at: `TRATTO_SHOT=out.png cargo test figjam_picture`.
    #[test]
    fn figjam_picture() {
        use crate::model::*;
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let mut els = Vec::new();
        let mut add = |x: f64, y: f64, w: f64, kind: Kind| {
            let mut el = El::new(kind);
            (el.x, el.y, el.w) = (x, y, w);
            crate::text::fit_text(&mut el);
            el.z = els.len() as f64;
            els.push(Arc::new(el));
        };
        let mut t = Table::new(3, 3);
        for (i, s) in ["Attività", "Chi", "Quando", "Bozza", "Anna", "Lunedì", "Revisione con il gruppo di lavoro", "Marco", "Giovedì"].iter().enumerate() {
            t.cells[i / 3][i % 3].text = s.to_string();
        }
        t.cells[2][2].fill = Some("#C9F2C7".into());
        add(20.0, 20.0, 0.0, Kind::Table(t));
        add(20.0, 250.0, 460.0, Kind::Code(Code { code: "// Somma dei primi n numeri\nfn somma(n: u64) -> u64 {\n    (1..=n).sum() // 1 + 2 + … + n\n}\n\nlet totale = somma(10);\nprintln!(\"{totale}\");".into(), language: "rust".into(), light: false, font_size: 14.0, radius: None }));
        add(520.0, 20.0, 300.0, Kind::Widget(Widget::Poll { question: "Dove andiamo a pranzo?".into(), options: vec![PollOption { text: "Pizzeria".into(), votes: vec!["a".into(), "b".into()] }, PollOption { text: "Sushi".into(), votes: vec!["c".into()] }, PollOption { text: "Insalate".into(), votes: vec![] }] }));
        add(520.0, 330.0, 300.0, Kind::Widget(Widget::Checklist { title: "Prima della riunione".into(), items: vec![CheckItem { text: "Preparare le slide".into(), done: true }, CheckItem { text: "Prenotare la sala".into(), done: false }, CheckItem { text: String::new(), done: false }] }));
        add(860.0, 20.0, 220.0, Kind::Widget(Widget::Counter { label: "Idee raccolte".into(), value: 12 }));
        add(860.0, 260.0, 300.0, Kind::Code(Code { code: "SELECT nome, COUNT(*)\nFROM idee\nWHERE voti > 3 -- le migliori\nGROUP BY nome;".into(), language: "sql".into(), light: true, font_size: 13.0, radius: None }));
        for (i, k) in [ShapeKind::Pill, ShapeKind::Process, ShapeKind::Document, ShapeKind::Cylinder, ShapeKind::Speech, ShapeKind::Chevron, ShapeKind::Trapezoid].into_iter().enumerate() {
            let shape = Shape { shape: k, fill: "#E3F1FF".into(), stroke: "#1971C2".into(), stroke_width: 2.0, text: Some(["Inizio", "Calcolo", "Report", "Dati", "Ciao!", "Fase", "Filtro"][i].into()), ..Default::default() };
            let mut el = El::new(Kind::Shape(shape));
            (el.x, el.y, el.w, el.h) = (20.0 + i as f64 * 115.0, 500.0, 100.0, 80.0);
            els.push(Arc::new(el));
        }
        for (i, route) in [Route::Straight, Route::Elbow, Route::Curved].into_iter().enumerate() {
            let y = 470.0 + i as f64 * 45.0;
            let line = Line { points: vec![0.0, 0.0, 300.0, 40.0], stroke: "#1E1E1E".into(), stroke_width: 2.0, dash: i == 2, arrow_start: i == 1, arrow_end: true, from: None, to: None, tape: false, route, label: Some(["Sì", "Poi", "Forse"][i].into()) };
            let mut el = El::new(Kind::Line(line));
            (el.x, el.y, el.w, el.h) = (860.0, y, 300.0, 40.0);
            els.push(Arc::new(el));
        }
        let meta = BoardMeta::default();
        let mut painter = Painter::new(Images::new(None));
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 620.0)).wgpu().build_ui(move |ui| {
            let rect = ui.max_rect();
            let cam = Camera { x: 0.0, y: 0.0, z: 1.0 };
            let view = BBox { x: 0.0, y: 0.0, w: rect.width() as f64, h: rect.height() as f64 };
            let mut shapes = Vec::new();
            background(&mut shapes, rect, &cam, &meta, 1.0);
            painter.paint(ui.ctx(), &mut shapes, &els, &Frame { cam, origin: rect.min, view, pixels_per_point: 1.0, editing: None, editing_cell: None });
            ui.painter().extend(shapes);
        });
        harness.run();
        harness.render().unwrap().save(path).unwrap();
    }

    /// Draws the sample to an image to look at: `TRATTO_SHOT=out.png cargo test picture -- --nocapture`.
    #[test]
    fn sample_board_picture() {
        let Ok(path) = std::env::var("TRATTO_SHOT") else { return };
        let els = sample();
        let meta = BoardMeta::default();
        let mut painter = Painter::new(Images::new(None));
        let zoom: f64 = std::env::var("TRATTO_ZOOM").ok().and_then(|z| z.parse().ok()).unwrap_or(1.0);
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(820.0, 560.0)).wgpu().build_ui(move |ui| {
            let rect = ui.max_rect();
            let cam = Camera { x: 0.0, y: 0.0, z: zoom };
            let view = BBox { x: -cam.x / cam.z, y: -cam.y / cam.z, w: rect.width() as f64 / cam.z, h: rect.height() as f64 / cam.z };
            let mut shapes = Vec::new();
            background(&mut shapes, rect, &cam, &meta, 1.0);
            painter.paint(ui.ctx(), &mut shapes, &els, &Frame { cam, origin: rect.min, view, pixels_per_point: 1.0, editing: None, editing_cell: None });
            ui.painter().extend(shapes);
        });
        harness.run();
        harness.render().unwrap().save(path).unwrap();
    }
}

