//! The text and the corners of any element, seen the same way: font, size, weight, slant,
//! strike, alignment and colour of its text, and the radius of its corners. The panels show and
//! change whatever is selected through this, all at once.

use crate::model::{Align, El, FontKind, Kind, ShapeKind};

/// What an element's text looks like. A setting the element doesn't have is None.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub font: Option<FontKind>,
    /// Text size; None for notes and shapes that fit their text automatically.
    pub size: Option<f64>,
    /// The size can be left automatic (notes and shapes).
    pub auto: bool,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub align: Option<Align>,
    /// Text colour; None when automatic (dark, or white on a dark background).
    pub color: Option<Option<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Font(FontKind),
    /// A size, or None for automatic.
    Size(Option<f64>),
    /// Bigger or smaller by a factor (the context bar's + and −).
    Scale(f64),
    Bold(bool),
    Italic(bool),
    Strike(bool),
    Align(Align),
    Color(String),
}

pub fn look(el: &El) -> Option<Look> {
    Some(match &el.kind {
        Kind::Text(t) => Look { font: Some(t.font), size: Some(t.font_size), auto: false, bold: Some(t.bold), italic: Some(t.italic), strike: Some(t.strike), align: Some(t.align), color: Some(Some(t.color.clone())) },
        Kind::Sticky(s) => Look { font: Some(s.font), size: s.font_size, auto: true, bold: Some(s.bold), italic: Some(s.italic), strike: None, align: Some(s.align), color: Some(s.text_color.clone()) },
        Kind::Shape(s) => Look { font: Some(s.font.unwrap_or_default()), size: s.font_size, auto: true, bold: Some(s.bold), italic: Some(s.italic), strike: None, align: Some(s.align.unwrap_or(Align::Center)), color: Some(s.text_color.clone()) },
        Kind::Table(t) => Look { font: Some(t.font), size: Some(t.font_size), auto: false, bold: Some(t.bold), italic: Some(t.italic), strike: None, align: Some(t.align.unwrap_or(Align::Left)), color: Some(t.color.clone()) },
        Kind::Code(c) => Look { font: None, size: Some(c.font_size), auto: false, bold: None, italic: None, strike: None, align: None, color: None },
        _ => return None,
    })
}

/// The size shown for an automatic text: what it is drawn at now.
pub fn shown_size(el: &El) -> Option<f64> {
    match &el.kind {
        Kind::Sticky(s) => Some(crate::text::sticky_layout(el, s).1),
        Kind::Shape(s) => Some(crate::text::shape_text_layout(el, s).size),
        _ => look(el).and_then(|l| l.size),
    }
}

/// Applies a change of text style; the box then fits its text again.
pub fn apply(el: &mut El, c: &Change) {
    let current = shown_size(el);
    let scaled = |k: f64| current.map(|v| (v * k).round().clamp(4.0, 2000.0));
    match (&mut el.kind, c) {
        (Kind::Text(t), Change::Font(f)) => t.font = *f,
        (Kind::Text(t), Change::Size(Some(v))) => t.font_size = v.clamp(0.1, 2000.0),
        (Kind::Text(t), Change::Scale(k)) => t.font_size = (t.font_size * k).round().clamp(4.0, 2000.0),
        (Kind::Text(t), Change::Bold(b)) => t.bold = *b,
        (Kind::Text(t), Change::Italic(b)) => t.italic = *b,
        (Kind::Text(t), Change::Strike(b)) => t.strike = *b,
        (Kind::Text(t), Change::Align(a)) => t.align = *a,
        (Kind::Text(t), Change::Color(c)) => t.color = c.clone(),
        (Kind::Sticky(s), Change::Font(f)) => s.font = *f,
        (Kind::Sticky(s), Change::Size(v)) => s.font_size = v.map(|v| v.clamp(4.0, 400.0)),
        (Kind::Sticky(s), Change::Scale(k)) => s.font_size = scaled(*k).map(|v| v.min(400.0)),
        (Kind::Sticky(s), Change::Bold(b)) => s.bold = *b,
        (Kind::Sticky(s), Change::Italic(b)) => s.italic = *b,
        (Kind::Sticky(s), Change::Align(a)) => s.align = *a,
        (Kind::Sticky(s), Change::Color(c)) => s.text_color = Some(c.clone()),
        (Kind::Shape(s), Change::Font(f)) => s.font = Some(*f),
        (Kind::Shape(s), Change::Size(v)) => s.font_size = v.map(|v| v.clamp(1.0, 400.0)),
        (Kind::Shape(s), Change::Scale(k)) => s.font_size = scaled(*k).map(|v| v.min(400.0)),
        (Kind::Shape(s), Change::Bold(b)) => s.bold = *b,
        (Kind::Shape(s), Change::Italic(b)) => s.italic = *b,
        (Kind::Shape(s), Change::Align(a)) => s.align = Some(*a),
        (Kind::Shape(s), Change::Color(c)) => s.text_color = Some(c.clone()),
        (Kind::Table(t), Change::Font(f)) => t.font = *f,
        (Kind::Table(t), Change::Size(Some(v))) => t.font_size = v.clamp(4.0, 400.0),
        (Kind::Table(t), Change::Scale(k)) => t.font_size = (t.font_size * k).round().clamp(4.0, 400.0),
        (Kind::Table(t), Change::Bold(b)) => t.bold = *b,
        (Kind::Table(t), Change::Italic(b)) => t.italic = *b,
        (Kind::Table(t), Change::Align(a)) => t.align = Some(*a),
        (Kind::Table(t), Change::Color(c)) => t.color = Some(c.clone()),
        (Kind::Code(c), Change::Size(Some(v))) => c.font_size = v.clamp(4.0, 400.0),
        (Kind::Code(c), Change::Scale(k)) => c.font_size = (c.font_size * k).round().clamp(4.0, 400.0),
        _ => return,
    }
    crate::text::fit_text(el);
}

/// The radius of an element's corners, if it has corners to round.
pub fn radius(el: &El) -> Option<f64> {
    match &el.kind {
        Kind::Shape(s) if crate::prims::has_corners(s.shape) => Some(s.radius),
        Kind::Sticky(s) => Some(s.radius.unwrap_or(4.0)),
        Kind::Table(t) => Some(crate::table::radius(t)),
        Kind::Code(c) => Some(c.radius.unwrap_or(c.font_size * 0.7)),
        _ => None,
    }
}

/// The largest radius that still changes something: half the shorter side.
pub fn max_radius(el: &El) -> f64 {
    let half = el.w.min(el.h) / 2.0;
    match &el.kind {
        // A polygon's corners stop rounding before that: let the slider go a bit further.
        Kind::Shape(s) if s.shape != ShapeKind::Rect => half * 1.5,
        _ => half,
    }
    .max(1.0)
}

pub fn set_radius(el: &mut El, v: f64) {
    let v = v.max(0.0);
    match &mut el.kind {
        Kind::Shape(s) if crate::prims::has_corners(s.shape) => s.radius = v,
        Kind::Sticky(s) => s.radius = Some(v),
        Kind::Table(t) => t.radius = Some(v),
        Kind::Code(c) => c.radius = Some(v),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn one_change_reaches_every_kind_of_text() {
        let mut els = vec![
            El::new(Kind::Sticky(Sticky::default())),
            El::new(Kind::Shape(Shape { text: Some("Ciao".into()), ..Default::default() })),
            El::new(Kind::Table(Table::new(2, 2))),
        ];
        for el in &mut els {
            (el.w, el.h) = (200.0, 200.0);
            apply(el, &Change::Bold(true));
            apply(el, &Change::Size(Some(40.0)));
            apply(el, &Change::Align(Align::Right));
            let l = look(el).unwrap();
            assert_eq!((l.bold, l.size, l.align), (Some(true), Some(40.0), Some(Align::Right)), "{}", el.type_name());
        }
        // Automatic again: the note fits its text as before.
        apply(&mut els[0], &Change::Size(None));
        assert_eq!(look(&els[0]).unwrap().size, None);
        // Corners: a table and a note take a radius, an ellipse has none.
        set_radius(&mut els[2], 18.0);
        assert_eq!(radius(&els[2]), Some(18.0));
        let round = El::new(Kind::Shape(Shape { shape: ShapeKind::Ellipse, ..Default::default() }));
        assert_eq!(radius(&round), None);
    }
}
