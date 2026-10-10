//! Lucide icons drawn sharp at the exact pixel size, white, and tinted when painted.

use std::collections::HashMap;

use egui::{Color32, ColorImage, Id, Mesh, Pos2, Rect, TextureHandle, TextureOptions, Ui, pos2, vec2};

use super::icon_data::ICONS;

#[derive(Clone, Default)]
struct Cache(HashMap<(String, u32), Option<TextureHandle>>);

fn texture(ctx: &egui::Context, name: &str, px: u32, svg: impl FnOnce() -> Option<Vec<u8>>) -> Option<TextureHandle> {
    let key = (name.to_string(), px);
    if let Some(hit) = ctx.data(|d| d.get_temp::<Cache>(Id::new("icons")).and_then(|c| c.0.get(&key).cloned())) {
        return hit;
    }
    let tex = svg().and_then(|svg| crate::assets::rasterize_svg(&svg, px)).map(|pm| {
        let img = ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
        ctx.load_texture(format!("icon:{name}:{px}"), img, TextureOptions::LINEAR)
    });
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(Id::new("icons")).0.insert(key, tex.clone()));
    tex
}

/// Device pixels for an icon `size` points wide.
fn pixels(ui: &Ui, size: f32) -> u32 {
    (size * ui.ctx().pixels_per_point()).round().max(1.0) as u32
}

/// One texel per device pixel: the picture is `px` device pixels wide and its corner sits on the
/// pixel grid, so nothing is stretched or resampled (that is what blurred icons at 115 or 130%).
fn paint(ui: &Ui, tex: &TextureHandle, center: Pos2, px: u32, color: Color32) {
    let ppp = ui.ctx().pixels_per_point();
    let side = px as f32;
    let min = ((center.to_vec2() * ppp) - vec2(side, side) / 2.0).round() / ppp;
    let rect = Rect::from_min_size(min.to_pos2(), vec2(side, side) / ppp);
    let mut mesh = Mesh::with_texture(tex.id());
    mesh.add_rect_with_uv(rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), color);
    ui.painter().add(mesh);
}

/// Paints a Lucide icon centred on `center`, `size` points wide, in `color`.
pub fn icon(ui: &Ui, name: &str, center: Pos2, size: f32, color: Color32) {
    let px = pixels(ui, size);
    let svg = || ICONS.iter().find(|(n, _)| *n == name).map(|(_, s)| s.as_bytes().to_vec());
    if let Some(tex) = texture(ui.ctx(), name, px, svg) {
        paint(ui, &tex, center, px, color);
    }
}

/// An SVG picture (reaction, logo) in its own colours.
pub fn picture(ui: &Ui, key: &str, svg: &'static [u8], rect: Rect) {
    let px = pixels(ui, rect.width().max(rect.height()));
    if let Some(tex) = texture(ui.ctx(), key, px, || Some(svg.to_vec())) {
        paint(ui, &tex, rect.center(), px, Color32::WHITE);
    }
}

pub fn logo(ui: &Ui, rect: Rect) {
    picture(ui, "logo", crate::assets::LOGO, rect);
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_icon_draws() {
        for (name, svg) in super::ICONS {
            let p = crate::assets::rasterize_svg(svg.as_bytes(), 24).unwrap_or_else(|| panic!("{name}"));
            assert!(p.pixels().iter().any(|px| px.alpha() > 0), "{name}");
        }
    }
}
