//! Tables: cell layout (rows grow to their tallest cell), drawing and the cell under a point.

use egui::Color32;

use crate::model::{Align, CELL_PAD, DARK, El, TABLE_ROW, Table, color_or};
use crate::prims::{Path, Prim, with_alpha};
use crate::text::{self, layout_text, place_lines};

pub fn cell_font(t: &Table, row: usize) -> text::FontRef {
    text::font(t.font, t.header && row == 0, false)
}

/// Wrapped lines of a cell.
pub fn cell_lines(t: &Table, row: usize, col: usize) -> Vec<String> {
    let w = (t.cols[col] - CELL_PAD * 2.0).max(4.0);
    layout_text(&t.cells[row][col].text, cell_font(t, row).face, t.font_size, Some(w)).lines
}

/// Row heights that fit every cell, and the table's size to match.
pub fn fit(el: &mut El) {
    let crate::model::Kind::Table(t) = &mut el.kind else { return };
    for r in 0..t.rows.len() {
        let tallest = (0..t.cols.len()).map(|c| cell_lines(t, r, c).len().max(1)).max().unwrap_or(1);
        t.rows[r] = (tallest as f64 * t.font_size * text::LINE_HEIGHT + CELL_PAD * 2.0).max(TABLE_ROW);
    }
    el.w = t.cols.iter().sum();
    el.h = t.rows.iter().sum();
}

/// A cell's box in the table's own coordinates.
pub fn cell_box(t: &Table, row: usize, col: usize) -> (f64, f64, f64, f64) {
    let x: f64 = t.cols[..col].iter().sum();
    let y: f64 = t.rows[..row].iter().sum();
    (x, y, t.cols[col], t.rows[row])
}

const GRID: Color32 = Color32::from_rgb(0xD9, 0xD9, 0xD9);
const HEAD: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF5);

pub fn prims(el: &El, t: &Table, alpha: f64, editing: Option<(usize, usize)>, out: &mut Vec<Prim>) {
    let (w, h) = (el.w as f32, el.h as f32);
    let r = 6.0f32;
    out.push(Prim::Fill { path: Path::round_rect(0.0, 0.0, w, h, [r; 4]), color: with_alpha(Color32::WHITE, alpha) });
    let rows = t.rows.len();
    let cols = t.cols.len();
    for row in 0..rows {
        for col in 0..cols {
            let (x, y, cw, ch) = cell_box(t, row, col);
            let fill = t.cells[row][col].fill.as_deref().map(|f| color_or(f, Color32::WHITE)).or((t.header && row == 0).then_some(HEAD));
            if let Some(f) = fill {
                // Corner cells keep the table's rounded corners.
                let radii = [
                    if row == 0 && col == 0 { r } else { 0.0 },
                    if row == 0 && col == cols - 1 { r } else { 0.0 },
                    if row == rows - 1 && col == cols - 1 { r } else { 0.0 },
                    if row == rows - 1 && col == 0 { r } else { 0.0 },
                ];
                out.push(Prim::Fill { path: Path::round_rect(x as f32, y as f32, cw as f32, ch as f32, radii), color: with_alpha(f, alpha) });
            }
            if editing == Some((row, col)) || t.cells[row][col].text.is_empty() {
                continue;
            }
            let dark = t.cells[row][col].fill.as_deref().is_some_and(crate::model::is_dark);
            let color = if dark { Color32::WHITE } else { DARK };
            let lines = cell_lines(t, row, col);
            out.push(Prim::Text { placed: place_lines(&lines, cell_font(t, row), t.font_size, Align::Left, cw - CELL_PAD * 2.0, y + CELL_PAD, x + CELL_PAD), color: with_alpha(color, alpha) });
        }
    }
    // Grid: inner lines, then the rounded border on top.
    let mut grid = Path::default();
    let mut x = 0.0f32;
    for cw in &t.cols[..cols - 1] {
        x += *cw as f32;
        grid.move_to(x, 0.0);
        grid.line_to(x, h);
    }
    let mut y = 0.0f32;
    for rh in &t.rows[..rows - 1] {
        y += *rh as f32;
        grid.move_to(0.0, y);
        grid.line_to(w, y);
    }
    out.push(Prim::Stroke { path: grid, width: 1.0, color: with_alpha(GRID, alpha), round: false, dash: None });
    out.push(Prim::Stroke { path: Path::round_rect(0.5, 0.5, w - 1.0, h - 1.0, [r; 4]), width: 1.0, color: with_alpha(GRID, alpha), round: true, dash: None });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    #[test]
    fn rows_grow_with_their_text() {
        let mut el = El::new(Kind::Table(Table::new(2, 2)));
        fit(&mut el);
        assert_eq!((el.w, el.h), (320.0, 88.0));
        if let Kind::Table(t) = &mut el.kind {
            t.cells[1][0].text = "una cella con parecchio testo che deve andare a capo più volte".into();
        }
        fit(&mut el);
        let t = el.table().unwrap();
        assert!(t.rows[1] > TABLE_ROW && t.rows[0] == TABLE_ROW);
        assert_eq!(el.h, t.rows.iter().sum::<f64>());
        assert_eq!(t.cell_at(170.0, t.rows[0] + 2.0), Some((1, 1)));
    }
}
