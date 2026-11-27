//! Geometry and text of the menu bar status image, shared by both renderers.
//!
//! The numbers mirror the Swift `MenuBarStatusRenderer`: a 22 pt tall image
//! made of one segment per provider, each segment an icon (16 pt), a fixed
//! `100%`-wide text slot and a 48 x 8 pt bar, separated by 7 pt gaps and 10 pt
//! between segments. Keeping the text slot fixed keeps the status item from
//! resizing (and dragging the panel along) whenever a percentage changes.

/// Image height in points.
pub const HEIGHT: f32 = 22.0;
/// Provider icon edge in points.
pub const ICON: f32 = 16.0;
/// Gap between icon, text and bar.
pub const GAP: f32 = 7.0;
/// Bar width.
pub const BAR_WIDTH: f32 = 48.0;
/// Bar height.
pub const BAR_HEIGHT: f32 = 8.0;
/// Gap between two segments.
pub const SEGMENT_GAP: f32 = 10.0;
/// Text size in points (SF monospaced digits, medium, in the Swift app).
pub const FONT_SIZE: f32 = 12.5;
/// Smallest visible fill in points, so a 0 % bar still shows its brand color.
pub const MIN_FILL: f32 = 2.0;
/// Opacity of the empty part of the bar relative to the chrome color.
pub const TRACK_ALPHA: f32 = 0.22;
/// The widest text a slot must hold.
pub const SLOT_SAMPLE: &str = "100%";

/// Where one segment's parts sit, in points from the image's left edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentLayout {
    /// Left edge of the icon.
    pub icon_x: f32,
    /// Left edge of the text slot.
    pub text_x: f32,
    /// Left edge of the bar.
    pub bar_x: f32,
    /// Right edge of the segment.
    pub end_x: f32,
}

/// `NN%` for a used percentage, or an em dash when there is no data.
///
/// ```
/// use tokenmon_macos::status_layout::percent_text;
/// assert_eq!(percent_text(Some(63.4)), "63%");
/// assert_eq!(percent_text(Some(250.0)), "100%");
/// assert_eq!(percent_text(None), "\u{2014}");
/// ```
pub fn percent_text(used: Option<f64>) -> String {
    used.map_or_else(
        || "\u{2014}".to_owned(),
        |u| format!("{}%", u.clamp(0.0, 100.0).round() as i32),
    )
}

/// Width of the filled part of a bar: proportional, never under [`MIN_FILL`].
///
/// ```
/// use tokenmon_macos::status_layout::fill_width;
/// assert_eq!(fill_width(Some(50.0), 48.0), 24.0);
/// assert_eq!(fill_width(Some(0.0), 48.0), 2.0);
/// assert_eq!(fill_width(None, 48.0), 2.0);
/// ```
pub fn fill_width(used: Option<f64>, bar_width: f32) -> f32 {
    let frac = used.map_or(0.0, |u| u.clamp(0.0, 100.0) as f32 / 100.0);
    (bar_width * frac).max(MIN_FILL).min(bar_width)
}

/// Lays out `count` segments whose text slot is `slot_width` wide. Returns the
/// per-segment layout and the total image width (rounded up, plus 2 pt).
///
/// ```
/// use tokenmon_macos::status_layout::segment_layouts;
/// let (segments, width) = segment_layouts(2, 30.0);
/// assert_eq!(segments.len(), 2);
/// assert_eq!(segments[0].icon_x, 0.0);
/// assert!(segments[1].icon_x > segments[0].end_x);
/// assert!(width >= segments[1].end_x);
/// ```
pub fn segment_layouts(count: usize, slot_width: f32) -> (Vec<SegmentLayout>, f32) {
    let step = ICON + GAP + slot_width + GAP + BAR_WIDTH;
    let layouts: Vec<SegmentLayout> = (0..count)
        .map(|i| {
            let x = i as f32 * (step + SEGMENT_GAP);
            SegmentLayout {
                icon_x: x,
                text_x: x + ICON + GAP,
                bar_x: x + ICON + GAP + slot_width + GAP,
                end_x: x + step,
            }
        })
        .collect();
    let end = layouts.last().map_or(0.0, |l| l.end_x);
    (layouts, (end + 2.0).ceil().max(20.0))
}
