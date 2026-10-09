//! Exports (PNG, JPG, SVG, PDF, .tratto), thumbnails and the pictures put on boards.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use tiny_skia::Pixmap;

use crate::geom::{BBox, aabb, union};
use crate::model::{BoardMeta, El, color_or};
use crate::prims::{Affine, Cmd, Env, ImageKey, Path, Prim, prims_of};
use crate::raster::{self, pixmap_from_rgba};

pub struct Options {
    /// Only these elements; None = the whole board (hidden elements are always skipped).
    pub ids: Option<Vec<String>>,
    /// Pixel ratio of PNG and PDF.
    pub scale: f64,
    pub background: bool,
    /// PDF: an A4 page for printing instead of one the size of the content.
    pub a4: bool,
}

const MAX_SIDE: f64 = 16_384.0;
const MAX_AREA: f64 = 120_000_000.0;
const MIN_SIDE: f64 = 512.0;
const EMPTY: &str = "La lavagna è vuota: non c'è niente da esportare.";

/// What to draw and the board box around it, with a margin.
fn collect(els: &[Arc<El>], ids: Option<&[String]>, padding: Option<f64>) -> Result<(Vec<Arc<El>>, BBox), String> {
    // Comments are notes for the people on the board, not part of the picture.
    let els: Vec<Arc<El>> = els.iter().filter(|e| !e.hidden && !e.is_comment() && ids.is_none_or(|ids| ids.contains(&e.id))).cloned().collect();
    let b = union(els.iter().map(|e| {
        let a = aabb(e);
        if e.is_section() {
            let t = crate::text::section_title_box(e, 1.0);
            union([a, BBox { x: t.x, y: t.y, w: t.w, h: t.h }]).unwrap()
        } else {
            a
        }
    }))
    .ok_or(EMPTY)?;
    // Content drawn zoomed in is small in board units: a fixed margin would dwarf it.
    let p = padding.unwrap_or_else(|| (b.w.max(b.h) * 0.08).min(32.0));
    Ok((els, BBox { x: b.x - p, y: b.y - p, w: (b.w + p * 2.0).max(1.0), h: (b.h + p * 2.0).max(1.0) }))
}

fn raster_of(els: &[Arc<El>], meta: &BoardMeta, opts: &Options, images: raster::Images, opaque: bool) -> Result<(Pixmap, BBox), String> {
    let (els, b) = collect(els, opts.ids.as_deref(), None)?;
    // Small content (typically written zoomed in) is enlarged to at least MIN_SIDE px per 1×.
    let scale = opts.scale * (MIN_SIDE / b.w.max(b.h)).max(1.0);
    let k = scale.min(MAX_SIDE / b.w.max(b.h)).min((MAX_AREA / (b.w * b.h)).sqrt());
    let (w, h) = ((b.w * k).round().max(1.0) as u32, (b.h * k).round().max(1.0) as u32);
    let bg = (opts.background || opaque).then(|| color_or(&meta.background, egui::Color32::from_gray(0xF5)));
    let pm = raster::render(&els, bg, w, h, b, &Env::default(), images).ok_or("Impossibile creare l'immagine.")?;
    Ok((pm, b))
}

fn encode(pm: &Pixmap, jpeg: bool) -> Result<Vec<u8>, String> {
    let rgba = unpremultiply(pm);
    let img = image::RgbaImage::from_raw(pm.width(), pm.height(), rgba).ok_or("Immagine non valida")?;
    let mut out = Vec::new();
    let mut cur = std::io::Cursor::new(&mut out);
    if jpeg {
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cur, 92).encode_image(&rgb).map_err(|e| e.to_string())?;
    } else {
        image::DynamicImage::ImageRgba8(img).write_to(&mut cur, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn unpremultiply(pm: &Pixmap) -> Vec<u8> {
    pm.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect()
}

pub fn png(els: &[Arc<El>], meta: &BoardMeta, opts: &Options, images: raster::Images, jpeg: bool) -> Result<Vec<u8>, String> {
    let (pm, _) = raster_of(els, meta, opts, images, jpeg)?;
    encode(&pm, jpeg)
}

/// The picture of the board list: the content fitted in 480×300.
pub fn thumbnail(els: &[Arc<El>], meta: &BoardMeta, images: raster::Images) -> Option<Vec<u8>> {
    let (w, h) = (480.0, 300.0);
    let (els, c) = collect(els, None, Some(0.0)).ok()?;
    let k = ((w - 48.0) / c.w.max(1.0)).min((h - 48.0) / c.h.max(1.0)).min(crate::editor::MAX_ZOOM);
    let b = BBox { x: c.x + c.w / 2.0 - w / 2.0 / k, y: c.y + c.h / 2.0 - h / 2.0 / k, w: w / k, h: h / k };
    let pm = raster::render(&els, Some(color_or(&meta.background, egui::Color32::from_gray(0xF5))), w as u32, h as u32, b, &Env::default(), images)?;
    encode(&pm, false).ok()
}

/* ---------------- PDF ---------------- */

/// A PDF with the board as one picture (RGB plus a soft mask for transparency).
pub fn pdf(els: &[Arc<El>], meta: &BoardMeta, opts: &Options, images: raster::Images) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let (pm, b) = raster_of(els, meta, opts, images, false)?;
    let rgba = unpremultiply(&pm);
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let alpha: Vec<u8> = rgba.chunks_exact(4).map(|p| p[3]).collect();
    let deflate = |data: &[u8]| {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    };
    let (rgb, alpha) = (deflate(&rgb), deflate(&alpha));
    // Points: 0.75 per board unit (CSS px), like the first Tratto.
    let (wpt, hpt) = (b.w * 0.75, b.h * 0.75);
    let (pw, ph, x, y, dw, dh) = if opts.a4 {
        let (pw, ph) = if wpt >= hpt { (841.89, 595.28) } else { (595.28, 841.89) };
        let k = ((pw - 85.0) / wpt).min((ph - 85.0) / hpt);
        (pw, ph, (pw - wpt * k) / 2.0, (ph - hpt * k) / 2.0, wpt * k, hpt * k)
    } else {
        (wpt, hpt, 0.0, 0.0, wpt, hpt)
    };
    let content = format!("q {dw:.2} 0 0 {dh:.2} {x:.2} {y:.2} cm /Im0 Do Q");
    let (iw, ih) = (pm.width(), pm.height());
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>").into_bytes(),
        [format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(), content.into_bytes(), b"\nendstream".to_vec()].concat(),
        [format!("<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /SMask 6 0 R /Length {} >>\nstream\n", rgb.len()).into_bytes(), rgb, b"\nendstream".to_vec()].concat(),
        [format!("<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", alpha.len()).into_bytes(), alpha, b"\nendstream".to_vec()].concat(),
    ];
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").into_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).into_bytes());
    Ok(out)
}

/* ---------------- SVG ---------------- */

fn f(v: f64) -> String {
    if v.is_finite() { format!("{}", (v * 100.0).round() / 100.0) } else { "0".into() }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn rgba_attr(c: egui::Color32) -> (String, String) {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    (format!("#{r:02X}{g:02X}{b:02X}"), f(a as f64 / 255.0))
}

/// Path data in board coordinates, through the element transform `t`.
fn path_data(p: &Path, t: impl Fn(f32, f32) -> (f64, f64)) -> String {
    let mut d = String::new();
    let pt = |x: f32, y: f32| {
        let (a, b) = t(x, y);
        format!("{} {}", f(a), f(b))
    };
    for c in &p.cmds {
        match *c {
            Cmd::Move(x, y) => d += &format!("M{} ", pt(x, y)),
            Cmd::Line(x, y) => d += &format!("L{} ", pt(x, y)),
            Cmd::Quad(a, b, x, y) => d += &format!("Q{} {} ", pt(a, b), pt(x, y)),
            Cmd::Cubic(a, b, c2, d2, x, y) => d += &format!("C{} {} {} ", pt(a, b), pt(c2, d2), pt(x, y)),
            Cmd::Close => d += "Z ",
        }
    }
    d
}

fn png_data_url(pm: &Pixmap) -> Option<String> {
    let png = encode(pm, false).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

/// Vector picture of the board; text is drawn as outlines so it looks the same anywhere.
pub fn svg(els: &[Arc<El>], meta: &BoardMeta, opts: &Options, images: raster::Images) -> Result<String, String> {
    let (els, b) = collect(els, opts.ids.as_deref(), None)?;
    let mut parts = Vec::new();
    let mut defs = Vec::new();
    for (n, el) in els.iter().enumerate() {
        let tr = Affine::of(el);
        let t = |x: f32, y: f32| tr.apply(x as f64, y as f64);
        let mut body = String::new();
        for prim in prims_of(el, &Env::default()) {
            match prim {
                Prim::Fill { path, color } => {
                    let (c, a) = rgba_attr(color);
                    body += &format!(r#"<path d="{}" fill="{c}" fill-opacity="{a}" fill-rule="nonzero"/>"#, path_data(&path, t));
                }
                Prim::Stroke { path, width, color, round, dash } => {
                    let (c, a) = rgba_attr(color);
                    let cap = if round { "round" } else { "butt" };
                    let join = if round { "round" } else { "bevel" };
                    let dash = dash.map(|[on, off]| format!(r#" stroke-dasharray="{} {}""#, f(on as f64), f(off as f64))).unwrap_or_default();
                    body += &format!(r#"<path d="{}" fill="none" stroke="{c}" stroke-opacity="{a}" stroke-width="{}" stroke-linecap="{cap}" stroke-linejoin="{join}"{dash}/>"#, path_data(&path, t), f(width as f64));
                }
                Prim::Text { placed, color } => {
                    let (c, a) = rgba_attr(color);
                    let mut d = String::new();
                    for &(g, gx, gy) in &placed.glyphs {
                        let glyph = crate::text::glyph_path(placed.face, g);
                        d += &path_data(&glyph, |px, py| t(gx + (px + placed.lean * py) * placed.k, gy - py * placed.k));
                    }
                    if !d.is_empty() {
                        body += &format!(r#"<path d="{d}" fill="{c}" fill-opacity="{a}"/>"#);
                    }
                }
                Prim::Image { key, x, y, w, h, alpha } => {
                    if let Some(url) = images(&key).and_then(|pm| png_data_url(&pm)) {
                        let (cx, cy) = t(x + w / 2.0, y + h / 2.0);
                        body += &format!(
                            r#"<image x="{}" y="{}" width="{}" height="{}" opacity="{}" preserveAspectRatio="none" transform="rotate({} {} {})" href="{url}"/>"#,
                            f(cx - w as f64 / 2.0),
                            f(cy - h as f64 / 2.0),
                            f(w as f64),
                            f(h as f64),
                            f(alpha as f64),
                            f(el.rotation.to_degrees()),
                            f(cx),
                            f(cy)
                        );
                    }
                }
                Prim::Shadow { x, y, w, h, radius, blur, dy, color } => {
                    let (c, a) = rgba_attr(color);
                    if defs.is_empty() || !defs.iter().any(|d: &String| d.contains("tratto-blur")) {
                        defs.push(r#"<filter id="tratto-blur" x="-20%" y="-20%" width="140%" height="150%"><feGaussianBlur stdDeviation="4"/></filter>"#.to_string());
                    }
                    let _ = blur;
                    body += &format!(r#"<path d="{}" fill="{c}" fill-opacity="{a}" filter="url(#tratto-blur)"/>"#, path_data(&Path::round_rect(x, y + dy, w, h, [radius; 4]), t));
                }
            }
        }
        if body.is_empty() {
            continue;
        }
        // Pixel eraser marks as a mask: white keeps, black strokes cut (as strong as the eraser was).
        if !el.erase.is_empty() {
            let a = aabb(el);
            let pad = el.erase.iter().map(|m| m.s).fold(0.0, f64::max);
            let id = format!("tratto-erase-{n}");
            let mut mask = format!(r##"<mask id="{id}" maskUnits="userSpaceOnUse" x="{}" y="{}" width="{}" height="{}"><rect x="{}" y="{}" width="{}" height="{}" fill="#fff"/>"##, f(a.x - pad), f(a.y - pad), f(a.w + pad * 2.0), f(a.h + pad * 2.0), f(a.x - pad), f(a.y - pad), f(a.w + pad * 2.0), f(a.h + pad * 2.0));
            for m in &el.erase {
                let pts: Vec<f64> = m.p.iter().map(|&v| v as f64).collect();
                let mut p = Path::polyline(&pts);
                if pts.len() < 4 {
                    p.line_to(m.p[0] + 0.01, m.p[1]);
                }
                mask += &format!(r##"<path d="{}" fill="none" stroke="#000" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"##, path_data(&p, t), f(m.a.clamp(0.0, 1.0)), f(m.s));
            }
            mask += "</mask>";
            defs.push(mask);
            parts.push(format!(r#"<g mask="url(#{id})">{body}</g>"#));
        } else {
            parts.push(body);
        }
    }
    let bg = if opts.background { format!(r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#, f(b.x), f(b.y), f(b.w), f(b.h), esc(&meta.background)) } else { String::new() };
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\"><defs>{}</defs>{bg}{}</svg>",
        f(b.w.ceil()),
        f(b.h.ceil()),
        f(b.x),
        f(b.y),
        f(b.w),
        f(b.h),
        defs.join(""),
        parts.join("")
    ))
}

/* ---------------- .tratto files ---------------- */

#[cfg(not(target_arch = "wasm32"))]
pub fn tratto(store: &crate::store::Store, id: &str, title: &str, state: &[u8]) -> Result<String, String> {
    let b64 = |d: &[u8]| base64::engine::general_purpose::STANDARD.encode(d);
    let files: Vec<serde_json::Value> = store.files(id)?.into_iter().map(|(fid, mime, data)| serde_json::json!({ "id": fid, "mime": mime, "data": b64(&data) })).collect();
    Ok(serde_json::json!({ "app": "tratto", "version": 1, "title": title, "doc": b64(state), "files": files }).to_string())
}

/// A .tratto file read back: title, document and images. Refuses anything else.
pub fn read_tratto(text: &str) -> Result<(String, Vec<u8>, Vec<(String, Vec<u8>)>), String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|_| "Questo file non è una lavagna di Tratto.")?;
    if v["app"] != "tratto" || !v["doc"].is_string() {
        return Err("Questo file non è una lavagna di Tratto.".into());
    }
    let dec = |s: &str| base64::engine::general_purpose::STANDARD.decode(s.trim()).ok();
    let state = dec(v["doc"].as_str().unwrap()).ok_or("Il file della lavagna è danneggiato.")?;
    crate::doc::apply_update(&yrs::Doc::new(), &state, crate::doc::REMOTE).map_err(|_| "Il file della lavagna è danneggiato.")?;
    let files = v["files"].as_array().map(|a| a.iter().filter_map(|f| Some((f["id"].as_str()?.to_string(), dec(f["data"].as_str()?)?))).collect()).unwrap_or_default();
    Ok((v["title"].as_str().unwrap_or("Lavagna").to_string(), state, files))
}

/* ---------------- pictures ---------------- */

/// Pictures of a board read from the database, decoded once, for software drawing.
#[cfg(not(target_arch = "wasm32"))]
pub fn store_images(store: &crate::store::Store, board: &str) -> impl Fn(&ImageKey) -> Option<Arc<Pixmap>> + use<> {
    let (store, board) = (store.clone(), board.to_string());
    let cache: RefCell<HashMap<String, Option<Arc<Pixmap>>>> = RefCell::new(HashMap::new());
    move |key: &ImageKey| match key {
        ImageKey::Stamp(e) => raster::stamp_pixmap(e),
        ImageKey::File(id) => cache
            .borrow_mut()
            .entry(id.clone())
            .or_insert_with(|| {
                let bytes = store.file(&board, id).ok().flatten()?.1;
                let img = crate::paint::decode_image(&bytes)?;
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                pixmap_from_rgba(img.size[0] as u32, img.size[1] as u32, &rgba).map(Arc::new)
            })
            .clone(),
    }
}

/// A picture to put on a board: at most 2560 px a side, re-encoded (JPEG when opaque, PNG
/// otherwise) so boards stay light and only plain raster images are stored.
pub fn prepare_image(bytes: &[u8]) -> Option<(Vec<u8>, egui::ColorImage)> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width().max(img.height()) > 2560 { img.resize(2560, 2560, image::imageops::FilterType::Lanczos3) } else { img };
    let rgba = img.to_rgba8();
    let opaque = rgba.pixels().all(|p| p[3] == 255);
    let mut out = Vec::new();
    let mut cur = std::io::Cursor::new(&mut out);
    if opaque {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cur, 90).encode_image(&image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8()).ok()?;
    } else {
        image::DynamicImage::ImageRgba8(rgba.clone()).write_to(&mut cur, image::ImageFormat::Png).ok()?;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba);
    Some((out, color))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn copy_png(png: &[u8]) -> Result<(), String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
    let mut cb = arboard::Clipboard::new().map_err(|_| "Non riesco a copiare l'immagine: usa Esporta.")?;
    cb.set_image(arboard::ImageData { width: img.width() as usize, height: img.height() as usize, bytes: img.into_raw().into() }).map_err(|_| "Non riesco a copiare l'immagine: usa Esporta.".to_string())
}

pub fn safe_filename(title: &str) -> String {
    let s: String = title.chars().filter(|c| !"<>:\"/\\|?*".contains(*c) && !c.is_control()).collect::<String>().trim().chars().take(80).collect();
    let s = s.trim_end_matches(['.', ' ']).to_string();
    if s.is_empty() { "Lavagna".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Arc<El>> {
        crate::paint::picture::sample()
    }

    #[test]
    fn every_format_exports() {
        let els = sample();
        let meta = BoardMeta::default();
        let opts = Options { ids: None, scale: 1.0, background: true, a4: false };
        let none = |_: &ImageKey| None;
        let png = png(&els, &meta, &opts, &|k| raster::stamp_pixmap(match k { ImageKey::Stamp(e) => e, _ => "" }).or_else(|| none(k)), false).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let pdf = pdf(&els, &meta, &Options { a4: true, ..opts }, &none).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let opts = Options { ids: None, scale: 1.0, background: true, a4: false };
        let svg = svg(&els, &meta, &opts, &none).unwrap();
        assert!(svg.contains("<svg") && svg.contains("<path"));
        assert!(thumbnail(&els, &meta, &none).is_some());
        assert_eq!(safe_filename("a/b:c?. "), "abc");
        assert!(collect(&[], None, None).is_err());
    }
}
