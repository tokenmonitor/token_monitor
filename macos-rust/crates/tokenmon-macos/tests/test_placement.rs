//! Panel placement under the status item (AppKit to winit coordinates).

use tokenmon_macos::placement::{AppKitRect, EDGE_MARGIN, GAP, place_under};

const VISIBLE: AppKitRect = AppKitRect {
    x: 0.0,
    y: 0.0,
    width: 1920.0,
    height: 1050.0,
};

fn anchor_at(x: f64) -> AppKitRect {
    AppKitRect {
        x,
        y: 1050.0,
        width: 100.0,
        height: 30.0,
    }
}

/// The panel is centred under the item, GAP below its bottom edge, and the
/// y flips to winit's top-left origin.
#[test]
fn test_panel_is_centred_under_the_item_and_flipped() {
    let p = place_under(anchor_at(800.0), VISIBLE, 1080.0, 420.0);
    assert_eq!(p.x, 850.0 - 210.0);
    assert_eq!(p.y, 1080.0 - (1050.0 - GAP));
    assert_eq!(p.max_height, 1050.0 - GAP - EDGE_MARGIN);
}

/// An item at the right edge pushes the panel left to stay on screen.
#[test]
fn test_panel_is_clamped_to_the_right_edge() {
    let p = place_under(anchor_at(1900.0), VISIBLE, 1080.0, 420.0);
    assert_eq!(p.x, 1920.0 - 420.0 - EDGE_MARGIN);
}

/// An item at the left edge pushes the panel right.
#[test]
fn test_panel_is_clamped_to_the_left_edge() {
    let p = place_under(anchor_at(0.0), VISIBLE, 1080.0, 420.0);
    assert_eq!(p.x, EDGE_MARGIN);
}

/// A panel wider than the screen sits at the left margin instead of panicking.
#[test]
fn test_panel_wider_than_the_screen_does_not_panic() {
    let p = place_under(anchor_at(100.0), VISIBLE, 1080.0, 5000.0);
    assert_eq!(p.x, EDGE_MARGIN);
}

/// An anchor below the visible area leaves no height rather than a negative one.
#[test]
fn test_max_height_never_goes_negative() {
    let low = AppKitRect {
        y: 2.0,
        ..anchor_at(0.0)
    };
    assert_eq!(place_under(low, VISIBLE, 1080.0, 420.0).max_height, 0.0);
    assert_eq!(
        AppKitRect {
            x: 5.0,
            y: 0.0,
            width: 10.0,
            height: 1.0
        }
        .max_x(),
        15.0
    );
}
