//! Ports of `UsageBreakdownCardTests.swift` plus the card layout: legend
//! percents, bar slices and chip placement.
#![allow(missing_docs, clippy::assert_is_empty)]

use tokenmon_core::Srgb;
use tokenmon_ui::kit::breakdown::{BreakdownItem, build_card, pool_percents, segment_percents};

fn item(id: &str, amount: f64, value_text: Option<&str>) -> BreakdownItem {
    BreakdownItem {
        id: id.to_owned(),
        label: id.to_owned(),
        color: Srgb::rgb(0.4, 0.5, 0.6),
        amount,
        value_text: value_text.map(str::to_owned),
    }
}

/// SuperGrok products at 25 / 12 / 6 / 4 % of the pool (47% used) stay
/// percents of that bar. They are not stretched to fill 100%.
#[test]
fn test_percents_stay_shares_of_the_pool() {
    let percents = pool_percents(&[25.0, 12.0, 6.0, 4.0]);
    assert_eq!(percents, vec![25, 12, 6, 4]);
    assert_eq!(percents.iter().sum::<u32>(), 47);
}

/// Rounding each slice alone can miss the consumed total. The largest
/// remainders take the missing points so the legend matches that total.
#[test]
fn test_percents_sum_to_the_consumed_total() {
    let thirds = pool_percents(&[1.0, 1.0, 1.0]);
    assert_eq!(thirds.iter().sum::<u32>(), 3);
    assert_eq!(thirds, vec![1, 1, 1]);
    let full = pool_percents(&[50.31, 28.09, 21.6]);
    assert_eq!(full.iter().sum::<u32>(), 100);
    assert_eq!(full, vec![50, 28, 22]);
    let drifted = pool_percents(&[10.4, 10.4, 10.2]);
    assert_eq!(drifted.iter().sum::<u32>(), 31);
}

/// Nothing used gives zero percents; negative and non-finite amounts count as
/// zero.
#[test]
fn test_nothing_used_gives_zero_percents() {
    assert_eq!(pool_percents(&[0.0, 0.0]), vec![0, 0]);
    assert!(pool_percents(&[]).is_empty());
    assert_eq!(pool_percents(&[-5.0, 10.0]), vec![0, 10]);
    assert_eq!(pool_percents(&[f64::NAN, f64::INFINITY]), vec![0, 0]);
    // A sum that rounds to zero leaves every percent at zero.
    assert_eq!(pool_percents(&[0.2, 0.1]), vec![0, 0]);
}

/// The breakdown bar is the consumed use filling its full width. 25 / 12 of
/// the pool become about 67.6 / 32.4 of that bar. Dollars split the same way.
#[test]
fn test_slices_fill_the_whole_bar() {
    let slices = segment_percents(&[25.0, 12.0, 6.0, 4.0]);
    assert!((slices.iter().sum::<f64>() - 100.0).abs() < 0.001);
    assert!((slices[0] - 25.0 / 47.0 * 100.0).abs() < 0.001);
    assert!((slices[3] - 4.0 / 47.0 * 100.0).abs() < 0.001);
    let dollars = segment_percents(&[2.0, 1.0]);
    assert!((dollars[0] - 200.0 / 3.0).abs() < 0.001);
    assert!((dollars[1] - 100.0 / 3.0).abs() < 0.001);
    assert_eq!(segment_percents(&[0.0, 0.0]), vec![0.0, 0.0]);
    assert!(segment_percents(&[]).is_empty());
}

/// Percent items show the pool percents; an item with text shows the text and
/// the bar splits by the quantities.
#[test]
fn test_card_legend_text_and_bar_fill() {
    let pool = build_card(
        &[item("chat", 25.0, None), item("build", 22.0, None)],
        372.0,
    );
    let texts: Vec<&str> = pool.chips.iter().map(|c| c.value_text.as_str()).collect();
    assert_eq!(texts, ["25%", "22%"]);
    let widths: f64 = pool.segments.iter().map(|s| s.width).sum();
    assert!(
        (widths + 2.0 - 372.0).abs() < 0.01,
        "bar fills the track: {widths}"
    );
    assert!(pool.segments.iter().all(|s| !s.remainder));

    let dollars = build_card(
        &[
            item("user", 2.0, Some("$2.00")),
            item("agent", 1.0, Some("$1.00")),
        ],
        372.0,
    );
    let texts: Vec<&str> = dollars
        .chips
        .iter()
        .map(|c| c.value_text.as_str())
        .collect();
    assert_eq!(texts, ["$2.00", "$1.00"]);
}

/// A long legend wraps onto more rows instead of truncating, and a short one
/// stays on a single row.
#[test]
fn test_card_chips_wrap_when_the_legend_is_long() {
    let short = build_card(&[item("Chat", 10.0, None), item("Build", 5.0, None)], 372.0);
    assert_eq!(short.chips_height, 16.0);
    let long: Vec<BreakdownItem> = (0..8)
        .map(|i| item(&format!("Category number {i}"), 10.0, None))
        .collect();
    let card = build_card(&long, 372.0);
    assert!(card.chips_height > 16.0);
    assert!(card.chips.iter().any(|c| c.y > 0.0));
}

/// No items: an empty bar slot and no chips.
#[test]
fn test_card_without_items_has_no_chips() {
    let card = build_card(&[], 372.0);
    assert!(card.chips.is_empty());
    assert_eq!(card.chips_height, 0.0);
}
