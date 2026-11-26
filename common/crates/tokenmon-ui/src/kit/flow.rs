//! Wrapping legend layout (upstream `FlowLayout`, e7b1a275 and b1d0afbe).
//!
//! Chips keep their natural width and wrap onto a new row when the next one
//! would pass the available width. Rows are spread across the full width with
//! equal gaps; a wrapped last row keeps the minimum gap from the leading edge
//! so a lone leftover chip is not pushed apart from the rest.
//!
//! SwiftUI measures each chip; Slint does not expose text measurement to a
//! view model, so [`estimate_text_width`] approximates it (see the parity
//! ledger's divergences).

/// Smallest gap between chips on a row, in px.
pub const MIN_GAP: f64 = 14.0;
/// Gap between rows, in px.
pub const ROW_GAP: f64 = 6.0;

/// Chip indices per row: each row takes chips until the next one would pass
/// `max_width`. A chip wider than `max_width` gets a row alone.
///
/// # Examples
///
/// ```
/// use tokenmon_ui::kit::flow::rows;
/// assert_eq!(rows(&[80.0, 90.0, 100.0, 60.0], 290.0, 14.0), vec![vec![0, 1], vec![2, 3]]);
/// assert!(rows(&[], 300.0, 14.0).is_empty());
/// ```
pub fn rows(widths: &[f64], max_width: f64, spacing: f64) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0.0_f64;
    for (index, width) in widths.iter().enumerate() {
        let needed = if current.is_empty() {
            *width
        } else {
            used + spacing + width
        };
        if !current.is_empty() && needed > max_width {
            rows.push(std::mem::take(&mut current));
            current.push(index);
            used = *width;
        } else {
            current.push(index);
            used = needed;
        }
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

/// Leading x of each chip on a row. When `spreads` and the row has more than
/// one chip, the gaps are equal and the row fills `max_width`, never closer
/// than `spacing`; otherwise chips sit `spacing` apart from 0.
///
/// # Examples
///
/// ```
/// use tokenmon_ui::kit::flow::x_positions;
/// assert_eq!(x_positions(&[60.0, 80.0, 100.0], 300.0, 14.0, true), vec![0.0, 90.0, 200.0]);
/// assert_eq!(x_positions(&[60.0, 80.0], 300.0, 14.0, false), vec![0.0, 74.0]);
/// ```
pub fn x_positions(widths: &[f64], max_width: f64, spacing: f64, spreads: bool) -> Vec<f64> {
    let free = max_width - widths.iter().sum::<f64>();
    let gap = if spreads && widths.len() > 1 && max_width.is_finite() {
        spacing.max(free / (widths.len() - 1) as f64)
    } else {
        spacing
    };
    widths
        .iter()
        .scan(0.0_f64, |x, width| {
            let at = *x;
            *x += width + gap;
            Some(at)
        })
        .collect()
}

/// One placed chip: its index in the input, and its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// Index of the chip in the input slice.
    pub index: usize,
    /// Left edge, in px.
    pub x: f64,
    /// Top edge, in px.
    pub y: f64,
}

/// Where every chip goes, and the total height, for chips of `widths` that
/// are all `height` tall inside `max_width`.
///
/// # Examples
///
/// ```
/// use tokenmon_ui::kit::flow::place;
/// let (placed, height) = place(&[80.0, 90.0, 100.0, 60.0], 14.0, 290.0);
/// assert_eq!(placed.len(), 4);
/// assert_eq!(placed[2].y, 20.0); // second row: 14 + 6
/// assert_eq!(height, 34.0);
/// ```
pub fn place(widths: &[f64], height: f64, max_width: f64) -> (Vec<Placed>, f64) {
    let rows = rows(widths, max_width, MIN_GAP);
    let last = rows.len().saturating_sub(1);
    let placed: Vec<Placed> = rows
        .iter()
        .enumerate()
        .flat_map(|(row_index, row)| {
            let row_widths: Vec<f64> = row.iter().map(|i| widths[*i]).collect();
            let spreads = rows.len() == 1 || row_index < last;
            let xs = x_positions(&row_widths, max_width, MIN_GAP, spreads);
            let y = row_index as f64 * (height + ROW_GAP);
            row.iter().zip(xs).map(move |(index, x)| Placed {
                index: *index,
                x,
                y,
            })
        })
        .collect();
    let total = if rows.is_empty() {
        0.0
    } else {
        rows.len() as f64 * height + (rows.len() - 1) as f64 * ROW_GAP
    };
    (placed, total)
}

/// Approximate advance width of `text` at `size` px in a semibold UI face,
/// in px. Per-class em fractions: narrow glyphs (`i l . , ' | : ; ! j t f r`)
/// 0.32, wide glyphs (`m w M W`) 0.88, digits 0.6, capitals 0.68, the rest
/// 0.57.
///
/// # Examples
///
/// ```
/// use tokenmon_ui::kit::flow::estimate_text_width;
/// assert!(estimate_text_width("Group chats", 12.0) > estimate_text_width("You", 12.0));
/// assert_eq!(estimate_text_width("", 12.0), 0.0);
/// ```
pub fn estimate_text_width(text: &str, size: f64) -> f64 {
    text.chars().map(|c| glyph_em(c) * size).sum()
}

fn glyph_em(c: char) -> f64 {
    match c {
        'i' | 'l' | '.' | ',' | '\'' | '|' | ':' | ';' | '!' | 'j' | 't' | 'f' | 'r' | ' ' => 0.32,
        'm' | 'w' | 'M' | 'W' => 0.88,
        '0'..='9' | '%' | '$' => 0.6,
        c if c.is_uppercase() => 0.68,
        _ => 0.57,
    }
}
