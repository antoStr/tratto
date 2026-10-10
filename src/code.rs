//! Code blocks: a small tokenizer that colours comments, strings, numbers, keywords, types and
//! function names in the common languages, and the block's layout (header, line numbers, soft
//! wrapping). Enough to read code at a glance, and light enough for the guests' page.

use egui::Color32;

use crate::model::{Code, El, FontKind};
use crate::prims::{Path, Prim, with_alpha};
use crate::text::{self, place_lines};

pub const LANGUAGES: [(&str, &str); 20] = [
    ("", "Testo semplice"),
    ("bash", "Bash"),
    ("c", "C"),
    ("cpp", "C++"),
    ("csharp", "C#"),
    ("css", "CSS"),
    ("go", "Go"),
    ("html", "HTML"),
    ("java", "Java"),
    ("javascript", "JavaScript"),
    ("json", "JSON"),
    ("kotlin", "Kotlin"),
    ("php", "PHP"),
    ("python", "Python"),
    ("ruby", "Ruby"),
    ("rust", "Rust"),
    ("sql", "SQL"),
    ("swift", "Swift"),
    ("typescript", "TypeScript"),
    ("yaml", "YAML"),
];

pub fn language_name(id: &str) -> &'static str {
    LANGUAGES.iter().find(|l| l.0 == id).map_or("Testo semplice", |l| l.1)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tok {
    Plain,
    Keyword,
    Type,
    Str,
    Number,
    Comment,
    Function,
}

fn keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while"],
        "javascript" | "typescript" => &["async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete", "do", "else", "export", "extends", "false", "finally", "for", "from", "function", "if", "import", "in", "instanceof", "interface", "let", "new", "null", "of", "return", "static", "super", "switch", "this", "throw", "true", "try", "type", "typeof", "undefined", "var", "void", "while", "yield", "enum", "implements", "private", "public", "readonly"],
        "python" => &["and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "self", "True", "try", "while", "with", "yield"],
        "go" => &["break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "false", "for", "func", "go", "goto", "if", "import", "interface", "map", "nil", "package", "range", "return", "select", "struct", "switch", "true", "type", "var"],
        "sql" => &["select", "from", "where", "and", "or", "not", "insert", "into", "values", "update", "set", "delete", "create", "table", "drop", "alter", "join", "left", "right", "inner", "outer", "on", "as", "group", "by", "order", "having", "limit", "null", "is", "in", "primary", "key", "foreign", "references", "index", "distinct", "union", "case", "when", "then", "else", "end", "asc", "desc"],
        "bash" => &["if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac", "function", "in", "return", "export", "local", "echo", "exit", "true", "false"],
        "ruby" => &["begin", "class", "def", "do", "else", "elsif", "end", "ensure", "false", "for", "if", "in", "module", "nil", "rescue", "return", "self", "then", "true", "unless", "until", "when", "while", "yield", "require"],
        "php" => &["abstract", "array", "as", "break", "case", "catch", "class", "const", "continue", "default", "do", "echo", "else", "elseif", "extends", "false", "final", "for", "foreach", "function", "if", "implements", "interface", "namespace", "new", "null", "private", "protected", "public", "return", "static", "switch", "this", "throw", "true", "try", "use", "while"],
        "json" | "yaml" => &["true", "false", "null", "yes", "no"],
        "css" | "html" => &[],
        // C, C++, C#, Java, Kotlin, Swift and anything else C-like.
        _ => &["abstract", "auto", "bool", "break", "case", "catch", "char", "class", "const", "continue", "default", "delete", "do", "double", "else", "enum", "extends", "extern", "false", "final", "float", "for", "fun", "func", "goto", "if", "implements", "import", "include", "int", "interface", "let", "long", "namespace", "new", "null", "nullptr", "override", "package", "private", "protected", "public", "return", "short", "signed", "sizeof", "static", "struct", "super", "switch", "template", "this", "throw", "true", "try", "typedef", "typename", "union", "unsigned", "using", "val", "var", "virtual", "void", "volatile", "while"],
    }
}

fn line_comment(lang: &str) -> &'static [&'static str] {
    match lang {
        "python" | "ruby" | "bash" | "yaml" => &["#"],
        "php" => &["//", "#"],
        "sql" => &["--"],
        "json" | "html" | "css" | "" => &[],
        _ => &["//"],
    }
}

fn block_comment(lang: &str) -> Option<(&'static str, &'static str)> {
    match lang {
        "python" | "ruby" | "bash" | "yaml" | "json" | "" => None,
        "html" => Some(("<!--", "-->")),
        _ => Some(("/*", "*/")),
    }
}

/// Splits one line into coloured pieces. `in_block` carries an open block comment over lines.
pub fn tokens<'a>(lang: &str, line: &'a str, in_block: &mut bool) -> Vec<(Tok, &'a str)> {
    let mut out: Vec<(Tok, &'a str)> = Vec::new();
    if lang.is_empty() {
        out.push((Tok::Plain, line));
        return out;
    }
    let kw = keywords(lang);
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &line[i..];
        if *in_block {
            let (_, end) = block_comment(lang).unwrap_or(("", "\u{0}"));
            let j = rest.find(end).map_or(bytes.len(), |k| {
                *in_block = false;
                i + k + end.len()
            });
            out.push((Tok::Comment, &line[i..j]));
            i = j;
            continue;
        }
        if line_comment(lang).iter().any(|m| rest.starts_with(m)) {
            out.push((Tok::Comment, rest));
            break;
        }
        if let Some((start, _)) = block_comment(lang)
            && rest.starts_with(start)
        {
            *in_block = true;
            out.push((Tok::Comment, &line[i..i + start.len()]));
            i += start.len();
            continue;
        }
        let c = rest.chars().next().unwrap();
        if c == '"' || c == '\'' || c == '`' {
            // Up to the closing quote, skipping escaped ones; an unclosed string runs to the end.
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
                if bytes[j - 1] == c as u8 {
                    break;
                }
            }
            let j = j.min(bytes.len());
            // JSON and YAML keys read as names, not as text.
            let key = matches!(lang, "json" | "yaml") && line[j..].trim_start().starts_with(':');
            out.push((if key { Tok::Type } else { Tok::Str }, &line[i..j]));
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let j = i + rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_')).unwrap_or(rest.len());
            out.push((Tok::Number, &line[i..j]));
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' || (c == '$' && matches!(lang, "php" | "bash")) {
            let n = c.len_utf8();
            let j = i + n + rest[n..].find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || (ch == '-' && lang == "css"))).unwrap_or(rest.len() - n);
            let word = &line[i..j];
            let is_kw = if lang == "sql" { kw.iter().any(|k| k.eq_ignore_ascii_case(word)) } else { kw.contains(&word) };
            let after = line[j..].trim_start();
            let tag = lang == "html" && (line[..i].ends_with('<') || line[..i].ends_with("</"));
            let t = if is_kw || tag {
                Tok::Keyword
            } else if after.starts_with('(') {
                Tok::Function
            } else if (matches!(lang, "css" | "yaml") && after.starts_with(':')) || (lang == "html" && after.starts_with('=')) {
                Tok::Type
            } else if c.is_uppercase() && !matches!(lang, "sql" | "bash" | "css" | "html" | "yaml") {
                Tok::Type
            } else {
                Tok::Plain
            };
            out.push((t, word));
            i = j;
            continue;
        }
        out.push((Tok::Plain, &line[i..i + c.len_utf8()]));
        i += c.len_utf8();
    }
    out
}

/// GitHub's colours, dark and light.
pub fn color(t: Tok, light: bool) -> Color32 {
    let hex = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    hex(match (t, light) {
        (Tok::Plain, false) => 0xE6EDF3,
        (Tok::Keyword, false) => 0xFF7B72,
        (Tok::Type, false) => 0xFFA657,
        (Tok::Str, false) => 0xA5D6FF,
        (Tok::Number, false) => 0x79C0FF,
        (Tok::Comment, false) => 0x8B949E,
        (Tok::Function, false) => 0xD2A8FF,
        (Tok::Plain, true) => 0x1F2328,
        (Tok::Keyword, true) => 0xCF222E,
        (Tok::Type, true) => 0x953800,
        (Tok::Str, true) => 0x0A3069,
        (Tok::Number, true) => 0x0550AE,
        (Tok::Comment, true) => 0x6E7781,
        (Tok::Function, true) => 0x8250DF,
    })
}

/// Where everything goes in a code block, in the block's own units.
pub struct Layout {
    pub pad: f64,
    pub header: f64,
    pub line_h: f64,
    pub gutter: f64,
    pub advance: f64,
    /// Screen lines: (number of the source line if it starts here, coloured pieces).
    pub lines: Vec<(Option<usize>, Vec<(Tok, String)>)>,
    pub height: f64,
}

pub fn layout(el: &El, c: &Code) -> Layout {
    let size = c.font_size;
    let face = text::font(FontKind::Mono, false, false).face;
    let advance = text::measure(face, "M", size).max(1.0);
    let pad = size * 1.15;
    let header = size * 2.4;
    let line_h = size * 1.55;
    let count = c.code.split('\n').count();
    let gutter = advance * (count.to_string().len() as f64 + 2.0);
    let cols = (((el.w - pad * 2.0 - gutter) / advance).floor() as usize).max(8);
    let mut lines = Vec::new();
    let mut in_block = false;
    for (n, src) in c.code.split('\n').enumerate() {
        let src = src.replace('\t', "    ");
        let mut row: Vec<(Tok, String)> = Vec::new();
        let mut col = 0;
        let mut first = true;
        // Long lines wrap at the block's width, carrying their colours over.
        for (t, piece) in tokens(&c.language, &src, &mut in_block) {
            let mut piece = piece;
            while !piece.is_empty() {
                if col >= cols {
                    lines.push((first.then_some(n + 1), std::mem::take(&mut row)));
                    first = false;
                    col = 0;
                }
                let take = piece.char_indices().nth(cols - col).map_or(piece.len(), |(k, _)| k);
                row.push((t, piece[..take].to_string()));
                col += piece[..take].chars().count();
                piece = &piece[take..];
            }
        }
        lines.push((first.then_some(n + 1), row));
    }
    let height = header + pad + lines.len() as f64 * line_h + pad * 0.8;
    Layout { pad, header, line_h, gutter, advance, lines, height }
}

pub fn fit(el: &mut El) {
    let Some(c) = el.code() else { return };
    el.h = layout(el, c).height;
}

pub fn prims(el: &El, c: &Code, alpha: f64, editing: bool, out: &mut Vec<Prim>) {
    let (w, h) = (el.w as f32, el.h as f32);
    let l = layout(el, c);
    let size = c.font_size;
    let (bg, head, border, muted) = if c.light {
        (Color32::from_rgb(0xF6, 0xF8, 0xFA), Color32::from_rgb(0xEA, 0xEE, 0xF2), Color32::from_rgb(0xD0, 0xD7, 0xDE), Color32::from_rgb(0x6E, 0x77, 0x81))
    } else {
        (Color32::from_rgb(0x0D, 0x11, 0x17), Color32::from_rgb(0x16, 0x1B, 0x22), Color32::from_rgb(0x30, 0x36, 0x3D), Color32::from_rgb(0x7D, 0x85, 0x90))
    };
    let r = (size * 0.7) as f32;
    out.push(Prim::Shadow { x: 0.0, y: 0.0, w, h, radius: r, blur: 8.0, dy: 2.0, color: with_alpha(Color32::from_black_alpha(30), alpha) });
    out.push(Prim::Fill { path: Path::round_rect(0.0, 0.0, w, h, [r; 4]), color: with_alpha(bg, alpha) });
    out.push(Prim::Fill { path: Path::round_rect(0.0, 0.0, w, l.header as f32, [r, r, 0.0, 0.0]), color: with_alpha(head, alpha) });
    out.push(Prim::Stroke { path: Path::round_rect(0.5, 0.5, w - 1.0, h - 1.0, [r; 4]), width: 1.0, color: with_alpha(border, alpha), round: true, dash: None });
    // Header: three dots like a window, and the language.
    let cy = (l.header / 2.0) as f32;
    for (i, col) in [0xFF5F57u32, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
        let x = (l.pad + i as f64 * size * 1.1) as f32;
        let d = (size * 0.32) as f32;
        out.push(Prim::Fill { path: Path::ellipse(x, cy, d, d), color: with_alpha(Color32::from_rgb((col >> 16) as u8, (col >> 8) as u8, col as u8), alpha) });
    }
    let lsize = size * 0.85;
    let label = language_name(&c.language).to_string();
    out.push(Prim::Text { placed: place_lines(&[label], text::font(FontKind::Sans, false, false), lsize, crate::model::Align::Right, 0.0, l.header / 2.0 - lsize * text::LINE_HEIGHT / 2.0, el.w - l.pad), color: with_alpha(muted, alpha) });
    if editing {
        return;
    }
    // The code, piece by piece, each in its colour.
    let mono = text::font(FontKind::Mono, false, false);
    for (i, (num, row)) in l.lines.iter().enumerate() {
        let top = code_top(&l, size, i);
        if let Some(n) = num {
            out.push(Prim::Text { placed: place_lines(&[n.to_string()], mono, size, crate::model::Align::Right, 0.0, top, l.pad + l.gutter - l.advance * 1.5), color: with_alpha(muted, alpha * 0.8) });
        }
        let mut col = 0usize;
        for (t, piece) in row {
            if !piece.trim().is_empty() {
                out.push(Prim::Text { placed: place_lines(&[piece.clone()], mono, size, crate::model::Align::Left, 0.0, top, l.pad + l.gutter + col as f64 * l.advance), color: with_alpha(color(*t, c.light), alpha) });
            }
            col += piece.chars().count();
        }
    }
}

/// Top of the text of screen line `i`.
pub fn code_top(l: &Layout, size: f64, i: usize) -> f64 {
    l.header + l.pad + i as f64 * l.line_h + (l.line_h - size * text::LINE_HEIGHT) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_the_usual_suspects() {
        let mut b = false;
        let t = tokens("rust", r#"fn main() { let s = "ciao"; // saluto"#, &mut b);
        assert_eq!(t[0], (Tok::Keyword, "fn"));
        assert!(t.contains(&(Tok::Function, "main")));
        assert!(t.contains(&(Tok::Str, "\"ciao\"")));
        assert_eq!(t.last().unwrap(), &(Tok::Comment, "// saluto"));
        // A block comment carries over to the next line.
        let mut b = false;
        tokens("javascript", "x = 1 /* inizio", &mut b);
        assert!(b);
        assert_eq!(tokens("javascript", "fine */ y", &mut b)[0], (Tok::Comment, "fine */"));
        assert!(!b);
        // SQL keywords in any case, JSON keys as names.
        assert_eq!(tokens("sql", "SELECT 1", &mut false)[0], (Tok::Keyword, "SELECT"));
        assert_eq!(tokens("json", r#"{"a": "b"}"#, &mut false)[1], (Tok::Type, "\"a\""));
        // Every byte of the line ends up in exactly one piece, unclosed strings included.
        for line in ["def f(x):  # 𝛑 ünïcode", "s = 'non chiusa", "a = \"fine\\\""] {
            assert_eq!(tokens("python", line, &mut false).iter().map(|p| p.1).collect::<String>(), line);
        }
    }
}
