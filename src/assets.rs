//! Pictures bundled with the app. Reactions are Fluent Emoji 3D (MIT,
//! assets/emoji/LICENSE), glossy like Apple's and the same on every computer and in exports.

pub struct Stamp {
    pub emoji: &'static str,
    pub name: &'static str,
    pub png: &'static [u8],
}

macro_rules! stamp {
    ($emoji:literal, $name:literal, $file:literal) => {
        Stamp { emoji: $emoji, name: $name, png: include_bytes!(concat!("../assets/emoji/", $file, ".png")) }
    };
}

pub static STAMPS: [Stamp; 24] = [
    stamp!("👍", "Mi piace", "thumbs-up"),
    stamp!("❤️", "Cuore", "heart"),
    stamp!("😂", "Risata", "laugh"),
    stamp!("😍", "Che bello", "heart-eyes"),
    stamp!("😮", "Wow", "wow"),
    stamp!("🤔", "Ci penso", "thinking"),
    stamp!("👏", "Applauso", "clap"),
    stamp!("🙌", "Evviva", "raising-hands"),
    stamp!("🎉", "Festa", "party"),
    stamp!("🔥", "Fuoco", "fire"),
    stamp!("⭐", "Stella", "star"),
    stamp!("💯", "Perfetto", "hundred"),
    stamp!("✅", "Fatto", "check"),
    stamp!("❌", "No", "cross"),
    stamp!("❓", "Domanda", "question"),
    stamp!("❗", "Importante", "exclamation"),
    stamp!("💡", "Idea", "bulb"),
    stamp!("👀", "Da guardare", "eyes"),
    stamp!("🚀", "Via!", "rocket"),
    stamp!("📌", "Puntina", "pin"),
    stamp!("👎", "Non mi piace", "thumbs-down"),
    stamp!("🎯", "Obiettivo", "target"),
    stamp!("⚡", "Veloce", "zap"),
    stamp!("🏆", "Vittoria", "trophy"),
];

/// A reaction's picture at `size`×`size` pixels (premultiplied, like the SVGs').
pub fn stamp_pixmap(s: &Stamp, size: u32) -> Option<tiny_skia::Pixmap> {
    let img = image::load_from_memory_with_format(s.png, image::ImageFormat::Png).ok()?;
    let img = if img.width() == size { img } else { img.resize_exact(size, size, image::imageops::FilterType::Triangle) };
    let rgba = img.to_rgba8();
    let mut pm = tiny_skia::Pixmap::new(size, size)?;
    for (dst, px) in pm.pixels_mut().iter_mut().zip(rgba.pixels()) {
        let [r, g, b, a] = px.0;
        *dst = tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    Some(pm)
}

pub fn stamp(emoji: &str) -> Option<&'static Stamp> {
    STAMPS.iter().find(|s| s.emoji == emoji)
}

pub const LOGO: &[u8] = include_bytes!("../assets/logo.svg");

/// An SVG drawn at `size`×`size` pixels (straight alpha, RGBA).
pub fn rasterize_svg(svg: &[u8], size: u32) -> Option<tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let s = tree.size();
    let k = size as f32 / s.width().max(s.height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    /// The reactions at the size bundled with the app, from Fluent's 256 px pictures:
    /// `TRATTO_EMOJI_IN=dir cargo test shrink_emoji -- --ignored`.
    #[test]
    #[ignore]
    fn shrink_emoji() {
        let dir = std::env::var("TRATTO_EMOJI_IN").expect("TRATTO_EMOJI_IN");
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let img = image::open(&p).unwrap().resize_exact(160, 160, image::imageops::FilterType::Lanczos3);
            img.save(std::path::Path::new("assets/emoji").join(p.file_name().unwrap())).unwrap();
        }
    }

    #[test]
    fn every_stamp_draws() {
        for s in &super::STAMPS {
            let p = super::stamp_pixmap(s, 64).unwrap_or_else(|| panic!("{}", s.name));
            assert!(p.pixels().iter().any(|px| px.alpha() > 0), "{}", s.name);
        }
        assert!(super::rasterize_svg(super::LOGO, 32).is_some());
    }
}
