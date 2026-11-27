//! Where the panel opens under the status item.
//!
//! AppKit measures from the bottom-left corner of the primary screen; winit
//! (and so Slint's `set_position`) from its top-left. This module takes
//! AppKit rectangles, which is what `NSStatusItem` and `NSScreen` report, and
//! returns the top-left origin and height cap for the Slint window. All in
//! points (logical pixels); no AppKit types, so it is tested everywhere.

/// An AppKit rectangle: `(x, y)` is the bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppKitRect {
    /// Left edge.
    pub x: f64,
    /// Bottom edge (AppKit y grows upward).
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl AppKitRect {
    /// Right edge.
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }

    /// Horizontal centre.
    pub fn mid_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
}

/// The outcome: where to put the Slint window and how tall it may be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPlacement {
    /// Left edge, in winit's top-left coordinates.
    pub x: f64,
    /// Top edge, in winit's top-left coordinates.
    pub y: f64,
    /// Tallest the panel may be before its content must scroll.
    pub max_height: f64,
}

/// Gap between the status item and the panel, in points.
pub const GAP: f64 = 4.0;
/// Minimum distance to the screen edges, in points.
pub const EDGE_MARGIN: f64 = 8.0;

/// Centres a panel `panel_width` wide under `anchor`, keeps it inside
/// `visible` (the screen minus menu bar and Dock) and converts to winit's
/// top-left coordinates using the primary screen's height.
///
/// ```
/// use tokenmon_macos::placement::*;
/// let anchor = AppKitRect { x: 1000.0, y: 1080.0, width: 90.0, height: 24.0 };
/// let visible = AppKitRect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 };
/// let p = place_under(anchor, visible, 1080.0, 420.0);
/// assert_eq!(p.x, 1045.0 - 210.0);
/// assert_eq!(p.y, 1080.0 - (1080.0 - GAP));
/// ```
pub fn place_under(
    anchor: AppKitRect,
    visible: AppKitRect,
    primary_height: f64,
    panel_width: f64,
) -> PanelPlacement {
    let lowest_x = visible.x + EDGE_MARGIN;
    let highest_x = (visible.max_x() - panel_width - EDGE_MARGIN).max(lowest_x);
    let x = (anchor.mid_x() - panel_width / 2.0).clamp(lowest_x, highest_x);
    let top_appkit = anchor.y - GAP;
    let max_height = (top_appkit - visible.y - EDGE_MARGIN).max(0.0);
    PanelPlacement {
        x,
        y: primary_height - top_appkit,
        max_height,
    }
}
