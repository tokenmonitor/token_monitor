//! The "Usage Breakdown" card shared by the provider panels (upstream
//! `UsageBreakdownCard`): one bar split by what spent the pool, then a legend
//! of the same items, most used first.
//!
//! The slices fill the bar. An item with no `value_text` is a percent of the
//! usage pool, and the legend shows that percent, so the figures add up to what
//! was used. An item with `value_text` is a quantity (dollars); the bar splits
//! by those quantities and the legend shows the text.

use tokenmon_core::Srgb;

use crate::kit::flow::{self, Placed};
use crate::kit::segments::{ProductShare, SegmentSpec, layout_segments};

/// Legend chip text size, in px (12pt semibold).
pub const CHIP_FONT_SIZE: f64 = 12.0;
/// Height of one legend chip row, in px.
pub const CHIP_HEIGHT: f64 = 16.0;
/// Diameter of the color mark, in px.
pub const CHIP_MARK: f64 = 7.0;
/// Gap between the mark, the name and the value, in px.
pub const CHIP_GAP: f64 = 5.0;
/// Bar height, in px.
pub const BAR_HEIGHT: f64 = 8.0;

/// One slice of the breakdown.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakdownItem {
    /// Stable id.
    pub id: String,
    /// Legend name.
    pub label: String,
    /// Slice and mark color.
    pub color: Srgb,
    /// Percent of the usage pool, or a quantity when `value_text` is set.
    pub amount: f64,
    /// The legend's figure (e.g. `$1.25`); `None` shows `amount` as a percent
    /// of the pool.
    pub value_text: Option<String>,
}

/// One placed legend chip.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakdownChip {
    /// Legend name.
    pub name: String,
    /// The figure, e.g. `25%` or `$1.25`.
    pub value_text: String,
    /// Mark color.
    pub color: Srgb,
    /// Left edge in the legend area, in px.
    pub x: f64,
    /// Top edge in the legend area, in px.
    pub y: f64,
}

/// Everything the card draws.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakdownCardVm {
    /// Laid-out bar slots (the slices fill the bar, so there is no remainder).
    pub segments: Vec<SegmentSpec>,
    /// Legend chips, placed.
    pub chips: Vec<BreakdownChip>,
    /// Height of the legend area, in px.
    pub chips_height: f64,
}

/// Width of each slice as a percent of the breakdown bar. The slices fill the
/// bar. A zero total leaves it empty. Negative amounts count as zero.
///
/// # Examples
///
/// ```
/// use tokenmon_ui::kit::breakdown::segment_percents;
/// let slices = segment_percents(&[25.0, 12.0, 6.0, 4.0]);
/// assert!((slices.iter().sum::<f64>() - 100.0).abs() < 1e-9);
/// assert_eq!(segment_percents(&[0.0, 0.0]), vec![0.0, 0.0]);
/// ```
pub fn segment_percents(amounts: &[f64]) -> Vec<f64> {
    let clamped: Vec<f64> = amounts.iter().map(|a| clamp_amount(*a)).collect();
    let total: f64 = clamped.iter().sum();
    if total > 0.0 && total.is_finite() {
        clamped.iter().map(|a| a / total * 100.0).collect()
    } else {
        vec![0.0; clamped.len()]
    }
}

/// Whole-number percents of the pool that add up to the rounded sum of
/// `amounts` (largest remainder), so a 25 / 12 / 6 / 4 split stays 47 rather
/// than being stretched to 100. All zeros when nothing was used. Negative
/// amounts count as zero.
///
/// # Examples
///
/// ```
/// use tokenmon_ui::kit::breakdown::pool_percents;
/// assert_eq!(pool_percents(&[25.0, 12.0, 6.0, 4.0]), vec![25, 12, 6, 4]);
/// assert_eq!(pool_percents(&[50.31, 28.09, 21.6]), vec![50, 28, 22]);
/// assert_eq!(pool_percents(&[-5.0, 10.0]), vec![0, 10]);
/// ```
pub fn pool_percents(amounts: &[f64]) -> Vec<u32> {
    let clamped: Vec<f64> = amounts.iter().map(|a| clamp_amount(*a)).collect();
    let total: f64 = clamped.iter().sum();
    let zeros = || vec![0; clamped.len()];
    if !(total > 0.0 && total.is_finite()) {
        return zeros();
    }
    let target = total.round();
    if target < 1.0 || target > f64::from(u32::MAX) {
        return zeros();
    }
    let exact: Vec<f64> = clamped.iter().map(|a| a / total * target).collect();
    let mut shares: Vec<u32> = exact.iter().map(|e| e.floor() as u32).collect();
    let missing = (target as i64 - shares.iter().map(|s| i64::from(*s)).sum::<i64>()).max(0);
    let mut by_remainder: Vec<usize> = (0..exact.len()).collect();
    by_remainder.sort_by(|a, b| {
        let left = exact[*a] - f64::from(shares[*a]);
        let right = exact[*b] - f64::from(shares[*b]);
        right
            .partial_cmp(&left)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });
    for index in by_remainder.into_iter().take(missing as usize) {
        shares[index] += 1;
    }
    shares
}

/// Negative and non-finite amounts count as zero.
fn clamp_amount(amount: f64) -> f64 {
    if amount.is_finite() {
        amount.max(0.0)
    } else {
        0.0
    }
}

/// Width of one chip: mark, name, value and the two gaps.
fn chip_width(label: &str, value: &str) -> f64 {
    CHIP_MARK
        + CHIP_GAP
        + flow::estimate_text_width(label, CHIP_FONT_SIZE)
        + CHIP_GAP
        + flow::estimate_text_width(value, CHIP_FONT_SIZE)
}

/// Lays out the card for a track `width` px wide.
///
/// # Examples
///
/// ```
/// use tokenmon_core::Srgb;
/// use tokenmon_ui::kit::breakdown::{build_card, BreakdownItem};
/// let item = |id: &str, amount: f64| BreakdownItem {
///     id: id.into(), label: id.into(), color: Srgb::rgb(0.5, 0.5, 0.5),
///     amount, value_text: None,
/// };
/// let card = build_card(&[item("chat", 30.0), item("build", 10.0)], 372.0);
/// assert_eq!(card.chips[0].value_text, "30%");
/// assert_eq!(card.segments.len(), 2);
/// ```
pub fn build_card(items: &[BreakdownItem], width: f64) -> BreakdownCardVm {
    let amounts: Vec<f64> = items.iter().map(|i| i.amount).collect();
    let all_percent = items.iter().all(|i| i.value_text.is_none());
    let pool = all_percent.then(|| pool_percents(&amounts));
    let shares: Vec<ProductShare> = items
        .iter()
        .zip(segment_percents(&amounts))
        .map(|(item, percent)| ProductShare {
            name: item.label.clone(),
            percent_of_pool: percent,
            color: item.color,
        })
        .collect();
    let segments = layout_segments(&shares, width, Srgb::rgb(0.5, 0.5, 0.5));
    let texts: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.value_text
                .clone()
                .unwrap_or_else(|| format!("{}%", pool.as_ref().map_or(0, |p| p[index])))
        })
        .collect();
    let widths: Vec<f64> = items
        .iter()
        .zip(&texts)
        .map(|(item, text)| chip_width(&item.label, text))
        .collect();
    let (placed, chips_height) = flow::place(&widths, CHIP_HEIGHT, width);
    let chips = placed
        .iter()
        .map(|Placed { index, x, y }| BreakdownChip {
            name: items[*index].label.clone(),
            value_text: texts[*index].clone(),
            color: items[*index].color,
            x: *x,
            y: *y,
        })
        .collect();
    BreakdownCardVm {
        segments,
        chips,
        chips_height,
    }
}
