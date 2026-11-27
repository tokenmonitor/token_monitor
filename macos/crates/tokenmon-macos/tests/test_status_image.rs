//! Status-image layout, the tiny-skia renderer and the comparison backdrop.

use tokenmon_core::ProviderId;
use tokenmon_macos::compare::stack;
use tokenmon_macos::skia_status::{Chrome, render, text_width};
use tokenmon_macos::status_layout::{
    BAR_WIDTH, MIN_FILL, fill_width, percent_text, segment_layouts,
};
use tokenmon_ui::tray_icon::BarSegment;

fn seg(provider: ProviderId, used: Option<f64>) -> BarSegment {
    BarSegment { provider, used }
}

/// Percent text rounds, clamps and shows a dash for no data.
#[test]
fn test_percent_text_rounds_clamps_and_dashes() {
    assert_eq!(percent_text(Some(62.5)), "63%");
    assert_eq!(percent_text(Some(-4.0)), "0%");
    assert_eq!(percent_text(Some(101.0)), "100%");
    assert_eq!(percent_text(None), "\u{2014}");
}

/// A bar is proportional but never thinner than the sliver, and never wider
/// than the bar.
#[test]
fn test_fill_width_has_a_sliver_floor_and_a_cap() {
    assert_eq!(fill_width(Some(100.0), BAR_WIDTH), BAR_WIDTH);
    assert_eq!(fill_width(Some(500.0), BAR_WIDTH), BAR_WIDTH);
    assert_eq!(fill_width(Some(0.1), BAR_WIDTH), MIN_FILL);
    assert_eq!(fill_width(None, BAR_WIDTH), MIN_FILL);
}

/// Segments advance by a constant step and the empty layout keeps a minimum width.
#[test]
fn test_segment_layouts_advance_and_minimum_width() {
    let (three, width) = segment_layouts(3, 30.0);
    let step = three[1].icon_x - three[0].icon_x;
    assert_eq!(three[2].icon_x - three[1].icon_x, step);
    assert!(width >= three[2].end_x);
    let (none, min_width) = segment_layouts(0, 30.0);
    assert_eq!(none.len(), 0);
    assert_eq!(min_width, 20.0);
}

/// The renderer returns a bitmap of the layout's size with ink in it, and the
/// light and dark chromes differ.
#[test]
fn test_render_is_deterministic_and_chrome_dependent() {
    let segments = [
        seg(ProviderId::Claude, Some(63.0)),
        seg(ProviderId::Grok, None),
    ];
    let light = render(&segments, Chrome::Light, 2);
    assert_eq!(light, render(&segments, Chrome::Light, 2));
    assert_eq!(light.height_px, 44);
    assert_eq!(light.height_pt(), 22.0);
    assert_eq!(
        light.rgba.len() as u32,
        light.width_px * light.height_px * 4
    );
    assert!(
        light.rgba.chunks(4).any(|p| p[3] > 0),
        "something was drawn"
    );
    let dark = render(&segments, Chrome::Dark, 2);
    assert_ne!(light.rgba, dark.rgba);
    assert_eq!(dark.width_px, light.width_px);
}

/// Different percentages change the bitmap but not its size (fixed text slot).
#[test]
fn test_width_does_not_depend_on_the_value() {
    let a = render(&[seg(ProviderId::Claude, Some(5.0))], Chrome::Dark, 2);
    let b = render(&[seg(ProviderId::Claude, Some(100.0))], Chrome::Dark, 2);
    assert_eq!(a.width_px, b.width_px);
    assert_ne!(a.rgba, b.rgba);
}

/// Text measurement is positive and grows with the string.
#[test]
fn test_text_width_grows_with_text() {
    assert!(text_width("100%") > text_width("1%"));
    assert!(text_width("1%") > 0.0);
}

/// The status bitmap encodes to a PNG, and a zero-segment render still works.
#[test]
fn test_png_encoding_and_empty_render() {
    let bmp = render(&[], Chrome::Light, 1);
    assert!(bmp.to_png().starts_with(&[0x89, b'P', b'N', b'G']));
}

/// Stacking composes decodable PNGs and rejects non-PNG input.
#[test]
fn test_stack_composites_pngs_and_rejects_garbage() {
    let png = render(&[seg(ProviderId::Grok, Some(10.0))], Chrome::Dark, 2).to_png();
    let both = stack(&[png.clone(), png], [30, 30, 30], 2, 3).expect("stacked");
    assert!(both.starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(stack(&[b"not a png".to_vec()], [0, 0, 0], 2, 3).is_none());
    assert!(stack(&[], [0, 0, 0], 2, 3).is_none());
}
