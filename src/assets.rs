//! Pictures bundled with the app. Reactions are Microsoft Fluent Emoji (MIT, assets/emoji/LICENSE),
//! so they look the same on every computer and in exports.

pub struct Stamp {
    pub emoji: &'static str,
    pub name: &'static str,
    pub svg: &'static [u8],
}

macro_rules! stamp {
    ($emoji:literal, $name:literal, $file:literal) => {
        Stamp { emoji: $emoji, name: $name, svg: include_bytes!(concat!("../assets/emoji/", $file, ".svg")) }
    };
}

pub static STAMPS: [Stamp; 16] = [
    stamp!("👍", "Mi piace", "thumbs-up"),
    stamp!("❤️", "Cuore", "heart"),
    stamp!("⭐", "Stella", "star"),
    stamp!("✅", "Fatto", "check"),
    stamp!("❌", "No", "cross"),
    stamp!("❓", "Domanda", "question"),
    stamp!("❗", "Importante", "exclamation"),
    stamp!("💡", "Idea", "bulb"),
    stamp!("🎉", "Festa", "party"),
    stamp!("🔥", "Fuoco", "fire"),
    stamp!("👀", "Da guardare", "eyes"),
    stamp!("😂", "Risata", "laugh"),
    stamp!("👏", "Applauso", "clap"),
    stamp!("💯", "Perfetto", "hundred"),
    stamp!("🚀", "Via!", "rocket"),
    stamp!("📌", "Puntina", "pin"),
];

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
    #[test]
    fn every_stamp_draws() {
        for s in &super::STAMPS {
            let p = super::rasterize_svg(s.svg, 64).unwrap_or_else(|| panic!("{}", s.name));
            assert!(p.pixels().iter().any(|px| px.alpha() > 0), "{}", s.name);
        }
        assert!(super::rasterize_svg(super::LOGO, 32).is_some());
    }
}
