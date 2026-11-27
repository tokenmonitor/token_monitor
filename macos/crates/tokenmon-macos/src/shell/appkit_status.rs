//! The AppKit / Core Text status-image renderer (candidate B of the M0 text
//! decision): same layout as [`crate::skia_status`], but text is drawn with
//! `NSString` and the system font (SF, monospaced digits, medium), the track
//! and fill with `NSBezierPath`, into an `NSBitmapImageRep` like the Swift app.
//! The provider logo is the shared tiny-skia glyph in both renderers, so the
//! comparison isolates text, anti-aliasing and bar rendering.
#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBezierPath, NSBitmapImageRep, NSColor, NSFont, NSFontAttributeName, NSFontWeightMedium,
    NSForegroundColorAttributeName, NSGraphicsContext, NSImage, NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSPoint, NSSize, NSString};
use tiny_skia::{Color, Paint, Pixmap};
use tokenmon_ui::tray_icon::{BarSegment, draw_logo};

use super::bitmap::{blank_rep, image_from_rep, rect, rep_from_premultiplied};
use crate::skia_status::Chrome;
use crate::status_layout::{
    BAR_HEIGHT, BAR_WIDTH, FONT_SIZE, HEIGHT, ICON, SLOT_SAMPLE, TRACK_ALPHA, fill_width,
    percent_text, segment_layouts,
};

/// A rendered AppKit status image and its backing rep (for PNG export).
pub struct AppKitStatus {
    /// The image to hand to the status item button.
    pub image: Retained<NSImage>,
    /// The bitmap it was drawn into.
    pub rep: Retained<NSBitmapImageRep>,
}

fn color(chrome_rgb: [u8; 3], alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(chrome_rgb[0]) / 255.0,
        f64::from(chrome_rgb[1]) / 255.0,
        f64::from(chrome_rgb[2]) / 255.0,
        alpha,
    )
}

fn attributes(font: &NSFont, ink: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: the two statics are valid attribute-name constants.
    let (font_key, color_key) = unsafe { (NSFontAttributeName, NSForegroundColorAttributeName) };
    let font_obj: &AnyObject = font;
    let ink_obj: &AnyObject = ink;
    NSDictionary::from_slices(&[font_key, color_key], &[font_obj, ink_obj])
}

fn logo_rep(
    provider: tokenmon_core::ProviderId,
    scale: usize,
    ink: [u8; 3],
) -> Option<Retained<NSBitmapImageRep>> {
    let side = ICON as usize * scale;
    let mut pm = Pixmap::new(side as u32, side as u32)?;
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(ink[0], ink[1], ink[2], 255));
    paint.anti_alias = true;
    draw_logo(&mut pm, provider, 0.0, 0.0, side as f32, &paint);
    rep_from_premultiplied(
        pm.data(),
        side,
        side,
        NSSize::new(f64::from(ICON), f64::from(ICON)),
    )
}

/// Renders `segments` with AppKit at `scale` pixels per point.
pub fn render(segments: &[BarSegment], chrome: Chrome, scale: usize) -> Option<AppKitStatus> {
    let ink = chrome.rgb();
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(
        f64::from(FONT_SIZE),
        // SAFETY: a constant provided by AppKit.
        unsafe { NSFontWeightMedium },
    );
    let ink_color = color(ink, 1.0);
    let attrs = attributes(&font, &ink_color);
    // SAFETY: `attrs` holds a font and a color, the attributes sizing reads.
    let measure = |text: &str| unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attrs)) };
    let slot = measure(SLOT_SAMPLE).width.ceil() as f32;
    let (layouts, width_pt) = segment_layouts(segments.len(), slot);
    let size = NSSize::new(f64::from(width_pt), f64::from(HEIGHT));
    let rep = blank_rep(
        (width_pt as usize * scale).max(1),
        HEIGHT as usize * scale,
        size,
    )?;
    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));
    let mid = f64::from(HEIGHT) / 2.0;
    for (segment, at) in segments.iter().zip(&layouts) {
        if let Some(logo) = logo_rep(segment.provider, scale, ink) {
            logo.drawInRect(rect(
                f64::from(at.icon_x),
                mid - f64::from(ICON) / 2.0,
                f64::from(ICON),
                f64::from(ICON),
            ));
        }
        let text = percent_text(segment.used);
        let ns_text = NSString::from_str(&text);
        // SAFETY: as for `measure`.
        let text_size = unsafe { ns_text.sizeWithAttributes(Some(&attrs)) };
        // SAFETY: drawing into the bitmap context set current above.
        unsafe {
            ns_text.drawAtPoint_withAttributes(
                NSPoint::new(
                    f64::from(at.text_x) + (f64::from(slot) - text_size.width) / 2.0,
                    mid - text_size.height / 2.0 - 0.5,
                ),
                Some(&attrs),
            );
        }
        let (r, g, b) = segment.provider.accent().to_rgb8();
        draw_bar(
            rect(
                f64::from(at.bar_x),
                mid - f64::from(BAR_HEIGHT) / 2.0,
                f64::from(BAR_WIDTH),
                f64::from(BAR_HEIGHT),
            ),
            segment.used,
            [r, g, b],
            ink,
        );
    }
    NSGraphicsContext::restoreGraphicsState_class();
    let image = image_from_rep(&rep, size);
    Some(AppKitStatus { image, rep })
}

fn draw_bar(bar: objc2_foundation::NSRect, used: Option<f64>, accent: [u8; 3], ink: [u8; 3]) {
    let radius = bar.size.height / 2.0;
    let track = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bar, radius, radius);
    color(ink, f64::from(TRACK_ALPHA)).setFill();
    track.fill();
    NSGraphicsContext::saveGraphicsState_class();
    track.addClip();
    color(accent, 1.0).setFill();
    let fill = rect(
        bar.origin.x,
        bar.origin.y,
        f64::from(fill_width(used, BAR_WIDTH)),
        bar.size.height,
    );
    NSBezierPath::fillRect(fill);
    NSGraphicsContext::restoreGraphicsState_class();
}
