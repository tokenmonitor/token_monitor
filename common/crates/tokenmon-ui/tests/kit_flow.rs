//! Ports of `FlowLayoutTests.swift`: row breaking and spreading for the
//! wrapping legend layout, plus the chip placement built on them.
#![allow(missing_docs, clippy::assert_is_empty)]

use tokenmon_ui::kit::flow::{ROW_GAP, estimate_text_width, place, rows, x_positions};

/// Chips share a row while they fit.
#[test]
fn test_items_share_a_row_while_they_fit() {
    assert_eq!(rows(&[80.0, 90.0, 100.0], 300.0, 14.0), vec![vec![0, 1, 2]]);
}

/// 80 + 14 + 90 + 14 + 100 = 298 > 290, so the third wraps.
#[test]
fn test_items_wrap_when_the_next_would_overflow() {
    assert_eq!(
        rows(&[80.0, 90.0, 100.0, 60.0], 290.0, 14.0),
        vec![vec![0, 1], vec![2, 3]]
    );
}

/// A chip wider than the row gets its own row.
#[test]
fn test_an_item_wider_than_the_row_gets_its_own_row() {
    assert_eq!(rows(&[400.0, 50.0], 300.0, 14.0), vec![vec![0], vec![1]]);
}

/// A spread row fills the width with equal gaps: 300 - (60 + 80 + 100) = 60
/// free, so two gaps of 30.
#[test]
fn test_spread_row_has_equal_gaps_and_fills_the_width() {
    let xs = x_positions(&[60.0, 80.0, 100.0], 300.0, 14.0, true);
    assert_eq!(xs, vec![0.0, 90.0, 200.0]);
    assert_eq!(xs[2] + 100.0, 300.0);
}

/// A row that is not spread keeps the minimum gap from the leading edge.
#[test]
fn test_unspread_row_keeps_the_minimum_gap() {
    assert_eq!(
        x_positions(&[60.0, 80.0], 300.0, 14.0, false),
        vec![0.0, 74.0]
    );
}

/// One chip, or a row too full to spread, stays at the minimum gap; an
/// unbounded width never spreads.
#[test]
fn test_spread_never_goes_below_the_minimum_gap() {
    assert_eq!(x_positions(&[60.0], 300.0, 14.0, true), vec![0.0]);
    assert_eq!(
        x_positions(&[150.0, 150.0], 300.0, 14.0, true),
        vec![0.0, 164.0]
    );
    assert_eq!(
        x_positions(&[60.0, 80.0], f64::INFINITY, 14.0, true),
        vec![0.0, 74.0]
    );
}

/// No chips, no rows.
#[test]
fn test_no_items_means_no_rows() {
    assert!(rows(&[], 300.0, 14.0).is_empty());
    let (placed, height) = place(&[], 16.0, 300.0);
    assert!(placed.is_empty());
    assert_eq!(height, 0.0);
}

/// Placement: full rows spread, the wrapped last row keeps the minimum gap,
/// rows stack with the row gap, and the height covers every row.
#[test]
fn test_place_spreads_full_rows_and_leaves_the_last_row_packed() {
    let (placed, height) = place(&[80.0, 90.0, 100.0, 60.0], 14.0, 290.0);
    let xs: Vec<f64> = placed.iter().map(|p| p.x).collect();
    // Row one: 290 - 170 = 120 free in one gap. Row two keeps the 14 gap.
    assert_eq!(xs, vec![0.0, 200.0, 0.0, 114.0]);
    assert_eq!(placed[2].y, 14.0 + ROW_GAP);
    assert_eq!(height, 2.0 * 14.0 + ROW_GAP);
}

/// A single row spreads too (the Swift rule: `rows.count == 1`).
#[test]
fn test_place_spreads_a_single_row() {
    let (placed, _) = place(&[60.0, 80.0, 100.0], 14.0, 300.0);
    let xs: Vec<f64> = placed.iter().map(|p| p.x).collect();
    assert_eq!(xs, vec![0.0, 90.0, 200.0]);
}

/// The width estimate grows with the text and is zero when empty.
#[test]
fn test_estimated_text_width_grows_with_the_text() {
    assert_eq!(estimate_text_width("", 12.0), 0.0);
    assert!(estimate_text_width("Group chats", 12.0) > estimate_text_width("You", 12.0));
    assert!(estimate_text_width("MWM", 12.0) > estimate_text_width("iii", 12.0));
}
