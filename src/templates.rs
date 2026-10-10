//! Ready-made boards: brainstorming, kanban, SWOT, retrospective, mind map, week planner, flow chart.

use crate::model::*;
use crate::text::{self, layout_text};

pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub build: fn() -> Vec<El>,
}

const DARK_C: &str = "#1E1E1E";

fn el(kind: Kind, x: f64, y: f64, w: f64, h: f64) -> El {
    let mut e = El::new(kind);
    (e.x, e.y, e.w, e.h) = (x, y, w, h);
    e
}

fn text(x: f64, y: f64, s: &str, size: f64, bold: bool, color: &str, align: Align, w: Option<f64>) -> El {
    let f = text::font(FontKind::Sans, bold, false).face;
    let lay = layout_text(s, f, size, w);
    el(Kind::Text(Text { text: s.into(), color: color.into(), font_size: size, font: FontKind::Sans, align, bold, italic: false, fixed_width: w.is_some(), strike: false }), x, y, w.unwrap_or(lay.width).ceil() + 1.0, lay.height)
}

/// Text whose box is centred on (cx, cy).
fn label(cx: f64, cy: f64, s: &str, size: f64, bold: bool, color: &str) -> El {
    let mut t = text(0.0, 0.0, s, size, bold, color, Align::Center, None);
    t.x = cx - t.w / 2.0;
    t.y = cy - t.h / 2.0;
    t
}

#[allow(clippy::too_many_arguments)]
fn shape(k: ShapeKind, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, width: f64, radius: f64) -> El {
    el(Kind::Shape(Shape { shape: k, fill: fill.into(), stroke: stroke.into(), stroke_width: width, radius, ..Default::default() }), x, y, w, h)
}

/// Figma-style white card.
fn card(x: f64, y: f64, w: f64, h: f64) -> El {
    shape(ShapeKind::Rect, x, y, w, h, "#FFFFFF", "#E6E6E6", 1.0, 12.0)
}

/// A shape with its label written inside it, so they move together.
fn with_text(mut e: El, s: &str) -> El {
    if let Kind::Shape(sh) = &mut e.kind {
        sh.text = Some(s.into());
        sh.font = Some(FontKind::Sans);
    }
    e
}

fn sticky(x: f64, y: f64, color: &str, s: &str) -> El {
    el(Kind::Sticky(Sticky { text: s.into(), color: color.into(), font: FontKind::Hand, ..Default::default() }), x, y, 200.0, 200.0)
}

fn line(x1: f64, y1: f64, x2: f64, y2: f64, arrow: bool) -> El {
    let (x0, y0) = (x1.min(x2), y1.min(y2));
    let l = Line { points: vec![(x1 - x0) as f32, (y1 - y0) as f32, (x2 - x0) as f32, (y2 - y0) as f32], stroke: "#757575".into(), stroke_width: 2.0, dash: false, arrow_start: false, arrow_end: arrow, from: None, to: None, tape: false, route: crate::model::Route::Straight, label: None };
    el(Kind::Line(l), x0, y0, (x2 - x1).abs(), (y2 - y1).abs())
}

fn link(mut l: El, a: &El, b: &El) -> El {
    if let Kind::Line(x) = &mut l.kind {
        x.from = Some(a.id.clone());
        x.to = Some(b.id.clone());
    }
    l
}

/// Increasing z in paint order.
fn finish(els: Vec<El>) -> Vec<El> {
    els.into_iter().enumerate().map(|(i, mut e)| {
        e.z = i as f64;
        e
    }).collect()
}

/// Cards side by side with a bold header each, centred on the origin.
fn columns(titles: &[&str], col_w: f64, col_h: f64, gap: f64, header: f64, per_column: impl Fn(usize, f64, f64) -> Vec<El>) -> Vec<El> {
    let mut out = Vec::new();
    let n = titles.len() as f64;
    let x0 = -(n * col_w + (n - 1.0) * gap) / 2.0;
    let y0 = -col_h / 2.0;
    for (i, title) in titles.iter().enumerate() {
        let x = x0 + i as f64 * (col_w + gap);
        out.push(card(x, y0, col_w, col_h));
        out.push(text(x + 20.0, y0 + 20.0, title, header, true, DARK_C, Align::Left, None));
        out.extend(per_column(i, x, y0));
    }
    out
}

/// Points on an ellipse around the origin, starting at the top.
fn ring(n: usize, rx: f64, ry: f64) -> Vec<(f64, f64)> {
    (0..n).map(|i| -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / n as f64).map(|a| (a.cos() * rx, a.sin() * ry)).collect()
}

const YELLOW: &str = STICKY_COLORS[0];
const GREEN: &str = STICKY_COLORS[1];
const BLUE: &str = STICKY_COLORS[2];
const PINK: &str = STICKY_COLORS[3];
const VIOLET: &str = STICKY_COLORS[4];
const ORANGE: &str = STICKY_COLORS[5];

fn brainstorm() -> Vec<El> {
    let mut out = vec![text(-150.0, -560.0, "Brainstorming", 28.0, true, DARK_C, Align::Left, None), sticky(-100.0, -100.0, YELLOW, "Tema")];
    let colors = [GREEN, BLUE, PINK, VIOLET, ORANGE];
    for (i, (x, y)) in ring(8, 460.0, 340.0).into_iter().enumerate() {
        out.push(sticky(x - 100.0, y - 100.0, colors[i % colors.len()], ""));
    }
    finish(out)
}

fn kanban() -> Vec<El> {
    let colors = [vec![YELLOW, YELLOW, YELLOW], vec![BLUE, BLUE], vec![GREEN, GREEN]];
    let texts = [vec!["Idea 1", "Idea 2", "Idea 3"], vec!["Attività 1", "Attività 2"], vec!["Completata 1", "Completata 2"]];
    finish(columns(&["Da fare", "In corso", "Fatto"], 248.0, 740.0, 32.0, 24.0, |i, x, y| colors[i].iter().enumerate().map(|(j, c)| sticky(x + 24.0, y + 72.0 + j as f64 * 216.0, c, texts[i][j])).collect()))
}

fn swot() -> Vec<El> {
    let quads = [("Punti di forza", "#E6F6EA", "#B7E4C2"), ("Punti deboli", "#FDE8E8", "#F5BFBF"), ("Opportunità", "#E7F1FD", "#B9D7F7"), ("Minacce", "#FFF0DC", "#F7D3A3")];
    let (w, h, gap) = (440.0, 320.0, 24.0);
    let mut out = Vec::new();
    for (i, (title, fill, stroke)) in quads.into_iter().enumerate() {
        let x = -w - gap / 2.0 + (i % 2) as f64 * (w + gap);
        let y = -h - gap / 2.0 + (i / 2) as f64 * (h + gap);
        out.push(shape(ShapeKind::Rect, x, y, w, h, fill, stroke, 1.0, 16.0));
        out.push(text(x + 24.0, y + 20.0, title, 24.0, true, DARK_C, Align::Left, None));
    }
    finish(out)
}

fn retro() -> Vec<El> {
    finish(columns(&["Cosa è andato bene", "Cosa migliorare", "Azioni"], 300.0, 680.0, 32.0, 22.0, |_, _, _| Vec::new()))
}

fn mindmap() -> Vec<El> {
    let colors = [BLUE, GREEN, PINK, VIOLET, ORANGE, YELLOW];
    // Text inside the shapes and connectors attached to them: a branch dragged away keeps its line.
    let centre = with_text(shape(ShapeKind::Ellipse, -120.0, -64.0, 240.0, 128.0, "#E7F1FD", "#1971C2", 2.0, 0.0), "Idea centrale");
    let mut out = Vec::new();
    let mut branches = Vec::new();
    for (i, (x, y)) in ring(6, 440.0, 280.0).into_iter().enumerate() {
        let (w, h) = (170.0, 64.0);
        let b = if i % 2 == 1 { shape(ShapeKind::Ellipse, x - w / 2.0, y - h / 2.0, w, h, colors[i], "#757575", 1.5, 0.0) } else { shape(ShapeKind::Rect, x - w / 2.0, y - h / 2.0, w, h, colors[i], "#757575", 1.5, 16.0) };
        out.push(link(line(0.0, 0.0, x, y, false), &centre, &b));
        branches.push(with_text(b, &format!("Ramo {}", i + 1)));
    }
    out.push(centre);
    out.extend(branches);
    finish(out)
}

fn week() -> Vec<El> {
    finish(columns(&["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"], 220.0, 620.0, 16.0, 22.0, |_, _, _| Vec::new()))
}

fn flow() -> Vec<El> {
    let sw = 2.0;
    let start = shape(ShapeKind::Ellipse, -560.0, -40.0, 160.0, 80.0, GREEN, "#2F9E44", sw, 0.0);
    let step = shape(ShapeKind::Rect, -310.0, -40.0, 180.0, 80.0, BLUE, "#1971C2", sw, 12.0);
    let decision = shape(ShapeKind::Diamond, -60.0, -80.0, 240.0, 160.0, YELLOW, "#F5B700", sw, 0.0);
    let yes = shape(ShapeKind::Rect, 300.0, -210.0, 200.0, 80.0, GREEN, "#2F9E44", sw, 12.0);
    let no = shape(ShapeKind::Rect, 300.0, 130.0, 200.0, 80.0, PINK, "#C2255C", sw, 12.0);
    let cy = |s: &El| s.y + s.h / 2.0;
    finish(vec![
        link(line(start.x + start.w, 0.0, step.x, 0.0, true), &start, &step),
        link(line(step.x + step.w, 0.0, decision.x, 0.0, true), &step, &decision),
        link(line(decision.x + decision.w, 0.0, yes.x, cy(&yes), true), &decision, &yes),
        link(line(decision.x + decision.w, 0.0, no.x, cy(&no), true), &decision, &no),
        with_text(start, "Inizio"),
        with_text(step, "Passo"),
        with_text(decision, "Decisione?"),
        with_text(yes, "Esito A"),
        with_text(no, "Esito B"),
        label(250.0, -110.0, "Sì", 16.0, true, "#2F9E44"),
        label(250.0, 110.0, "No", 16.0, true, "#C2255C"),
    ])
}

/// A meeting: the agenda to tick, a quick poll, the actions in a table and room for notes.
fn meeting() -> Vec<El> {
    let fitted = |mut e: El| {
        text::fit_text(&mut e);
        e
    };
    let mut table = Table::new(4, 3);
    for (c, h) in ["Azione", "Chi", "Entro"].iter().enumerate() {
        table.cells[0][c].text = h.to_string();
    }
    table.cols = vec![240.0, 140.0, 120.0];
    let agenda = Widget::Checklist { title: "Ordine del giorno".into(), items: ["Com'è andata la settimana", "Novità", "Decisioni da prendere", "Prossimi passi"].iter().map(|s| CheckItem { text: s.to_string(), done: false }).collect() };
    let poll = Widget::Poll { question: "Quando ci rivediamo?".into(), options: ["Lunedì", "Mercoledì", "Venerdì"].iter().map(|s| PollOption { text: s.to_string(), votes: Vec::new() }).collect() };
    finish(vec![
        text(-560.0, -360.0, "Riunione", 40.0, true, DARK_C, Align::Left, None),
        fitted(el(Kind::Widget(agenda), -560.0, -280.0, WIDGET_W, 0.0)),
        fitted(el(Kind::Widget(poll), -560.0, 60.0, WIDGET_W, 0.0)),
        text(-200.0, -280.0, "Azioni", 20.0, true, DARK_C, Align::Left, None),
        fitted(el(Kind::Table(table), -200.0, -240.0, 0.0, 0.0)),
        text(-200.0, 20.0, "Appunti", 20.0, true, DARK_C, Align::Left, None),
        sticky(-200.0, 60.0, "#FFF3A3", ""),
        sticky(20.0, 60.0, "#C7E5FF", ""),
        sticky(240.0, 60.0, "#C9F2C7", ""),
    ])
}

pub static TEMPLATES: [Template; 8] = [
    Template { id: "brainstorm", name: "Brainstorming", description: "Un tema al centro e tante note attorno per raccogliere idee.", build: brainstorm },
    Template { id: "kanban", name: "Kanban", description: "Tre colonne: da fare, in corso, fatto.", build: kanban },
    Template { id: "swot", name: "Analisi SWOT", description: "Punti di forza, punti deboli, opportunità e minacce.", build: swot },
    Template { id: "retro", name: "Retrospettiva", description: "Cosa è andato bene, cosa migliorare e le azioni.", build: retro },
    Template { id: "mindmap", name: "Mappa mentale", description: "Un'idea centrale con sei rami da sviluppare.", build: mindmap },
    Template { id: "week", name: "Planner settimanale", description: "Sette colonne, da lunedì a domenica.", build: week },
    Template { id: "flow", name: "Diagramma di flusso", description: "Inizio, passo, decisione e due esiti collegati da frecce.", build: flow },
    Template { id: "meeting", name: "Riunione", description: "Ordine del giorno da spuntare, un sondaggio, le azioni in tabella.", build: meeting },
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_template_builds_valid_elements() {
        for t in super::TEMPLATES.iter() {
            let els = (t.build)();
            assert!(!els.is_empty(), "{}", t.id);
            assert!(els.iter().all(|e| e.is_valid()), "{}", t.id);
        }
    }
}

/// A heavy board for comparing speed and memory with the first Tratto: 1500 pen strokes,
/// 300 sticky notes, 200 shapes with text and 100 arrows, always the same.
#[cfg(test)]
pub fn bench() -> Vec<El> {
    let mut seed = 42u64;
    let mut rnd = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as f64 / (1u64 << 31) as f64
    };
    let colors = ["#1E1E1E", "#E03131", "#1971C2", "#2F9E44", "#F76707", "#6741D9"];
    let mut out = Vec::new();
    for i in 0..1500 {
        let (ox, oy) = (rnd() * 6000.0, rnd() * 4000.0);
        let (mut x, mut y, mut a) = (0.0f64, 0.0f64, rnd() * 6.28);
        let mut pts = Vec::new();
        for _ in 0..50 {
            a += (rnd() - 0.5) * 0.8;
            x += a.cos() * 6.0;
            y += a.sin() * 6.0;
            pts.extend([x as f32, y as f32, 0.5]);
        }
        let (minx, miny) = pts.chunks(3).fold((f32::MAX, f32::MAX), |m, p| (m.0.min(p[0]), m.1.min(p[1])));
        let (maxx, maxy) = pts.chunks(3).fold((f32::MIN, f32::MIN), |m, p| (m.0.max(p[0]), m.1.max(p[1])));
        for p in pts.chunks_mut(3) {
            p[0] -= minx;
            p[1] -= miny;
        }
        let ink = Ink { points: pts, color: colors[i % colors.len()].into(), size: 4.0 };
        out.push(el(Kind::Ink(ink), ox + minx as f64, oy + miny as f64, (maxx - minx) as f64, (maxy - miny) as f64));
    }
    for i in 0..300 {
        out.push(sticky(rnd() * 6000.0, rnd() * 4000.0, STICKY_COLORS[i % 6], &format!("Idea numero {i}")));
    }
    for i in 0..200 {
        let k = if i % 2 == 0 { ShapeKind::Rect } else { ShapeKind::Ellipse };
        out.push(with_text(shape(k, rnd() * 6000.0, rnd() * 4000.0, 220.0, 120.0, "#E3F1FF", "#1971C2", 2.0, 8.0), "Passaggio"));
    }
    for _ in 0..100 {
        let (x, y) = (rnd() * 6000.0, rnd() * 4000.0);
        out.push(line(x, y, x + rnd() * 400.0 - 200.0, y + rnd() * 300.0, true));
    }
    for (z, e) in out.iter_mut().enumerate() {
        e.z = z as f64;
    }
    out
}
