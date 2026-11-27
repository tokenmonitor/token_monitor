//! Composites rendered status images onto a menu-bar-colored backdrop, enlarged
//! with nearest-neighbor sampling, so two renderers can be compared by eye.

use tiny_skia::{FilterQuality, Paint, Pixmap, PixmapPaint, Transform};

/// Stacks `images` (PNG bytes, each one row of the output, top to bottom) on a
/// solid `background`, enlarged by `factor`, with `pad` points around each.
/// Returns the PNG, or `None` if an input is not a decodable PNG.
///
/// ```
/// use tokenmon_macos::compare::stack;
/// use tokenmon_macos::skia_status::{Chrome, render};
/// use tokenmon_core::ProviderId;
/// use tokenmon_ui::tray_icon::BarSegment;
/// let bmp = render(&[BarSegment { provider: ProviderId::Grok, used: Some(10.0) }], Chrome::Dark, 2);
/// let out = stack(&[bmp.to_png(), bmp.to_png()], [30, 30, 30], 3, 4).unwrap();
/// assert!(out.starts_with(&[0x89, b'P', b'N', b'G']));
/// ```
pub fn stack(images: &[Vec<u8>], background: [u8; 3], factor: u32, pad: u32) -> Option<Vec<u8>> {
    let decoded: Vec<Pixmap> = images
        .iter()
        .map(|png| Pixmap::decode_png(png).ok())
        .collect::<Option<_>>()?;
    let f = factor.max(1);
    let width = decoded.iter().map(Pixmap::width).max()? * f + 2 * pad * f;
    let height: u32 = decoded
        .iter()
        .map(|p| p.height() * f + pad * f)
        .sum::<u32>()
        + pad * f;
    let mut out = Pixmap::new(width, height)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(background[0], background[1], background[2], 255);
    out.fill_rect(
        tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32)?,
        &paint,
        Transform::identity(),
        None,
    );
    let pixmap_paint = PixmapPaint {
        quality: FilterQuality::Nearest,
        ..PixmapPaint::default()
    };
    // The transform scales the whole draw, position included.
    let mut y = pad;
    for pm in &decoded {
        out.draw_pixmap(
            pad as i32,
            y as i32,
            pm.as_ref(),
            &pixmap_paint,
            Transform::from_scale(f as f32, f as f32),
            None,
        );
        y += pm.height() + pad;
    }
    out.encode_png().ok()
}
