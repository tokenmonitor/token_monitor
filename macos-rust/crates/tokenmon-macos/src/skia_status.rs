//! The shared tiny-skia status-image renderer (candidate A of the M0 text
//! decision).
//!
//! It draws the same layout as the Swift renderer (see [`crate::status_layout`])
//! into a premultiplied RGBA bitmap: provider logo from `tokenmon-ui`, percent
//! text rasterized from a bundled Inter font with `ab_glyph` (tiny-skia has no
//! text support of its own), and the track and fill bars with a rounded clip.

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Transform};
use tokenmon_core::ProviderId;
use tokenmon_ui::tray_icon::{BarSegment, draw_logo};

use crate::status_layout::{
    BAR_HEIGHT, BAR_WIDTH, FONT_SIZE, HEIGHT, ICON, SLOT_SAMPLE, TRACK_ALPHA, fill_width,
    percent_text, segment_layouts,
};

static FONT_BYTES: &[u8] = include_bytes!("../assets/Inter-SemiBold.ttf");

/// The menu bar chrome the image is drawn against: black ink on a light bar,
/// white on a dark one (the status image is not a template, so a light/dark
/// change needs a re-render, as in the Swift app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// Light menu bar.
    Light,
    /// Dark menu bar.
    Dark,
}

impl Chrome {
    /// Ink color.
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Light => [0, 0, 0],
            Self::Dark => [255, 255, 255],
        }
    }
}

/// A rendered status image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBitmap {
    /// Width in device pixels.
    pub width_px: u32,
    /// Height in device pixels.
    pub height_px: u32,
    /// Pixels per point (2 for Retina).
    pub scale: u32,
    /// Premultiplied RGBA, row-major.
    pub rgba: Vec<u8>,
}

impl StatusBitmap {
    /// Width in points.
    pub fn width_pt(&self) -> f32 {
        self.width_px as f32 / self.scale as f32
    }

    /// Height in points.
    pub fn height_pt(&self) -> f32 {
        self.height_px as f32 / self.scale as f32
    }

    /// PNG encoding (screenshots and tests); empty if encoding fails.
    pub fn to_png(&self) -> Vec<u8> {
        tiny_skia::IntSize::from_wh(self.width_px, self.height_px)
            .and_then(|size| Pixmap::from_vec(self.rgba.clone(), size))
            .and_then(|pm| pm.encode_png().ok())
            .unwrap_or_default()
    }
}

fn font() -> Option<&'static FontRef<'static>> {
    static FONT: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(FONT_BYTES).ok())
        .as_ref()
}

fn paint(rgb: [u8; 3], alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(
        rgb[0],
        rgb[1],
        rgb[2],
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ));
    p.anti_alias = true;
    p
}

/// Width in points of `text` at the status font size.
pub fn text_width(text: &str) -> f32 {
    let Some(font) = font() else {
        return 0.0;
    };
    let scaled = font.as_scaled(PxScale::from(FONT_SIZE));
    text.chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum()
}

/// Draws `text` with its left edge at `x` and baseline at `baseline` (pixels).
fn draw_text(pm: &mut Pixmap, text: &str, x: f32, baseline: f32, px: f32, rgb: [u8; 3]) {
    let Some(font) = font() else {
        return;
    };
    let scaled = font.as_scaled(PxScale::from(px));
    let (width, height) = (pm.width() as i32, pm.height() as i32);
    let stride = pm.width() as usize * 4;
    let data = pm.data_mut();
    let mut pen = x;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        let glyph = id.with_scale_and_position(PxScale::from(px), point(pen, baseline));
        pen += scaled.h_advance(id);
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let (dx, dy) = (
                bounds.min.x as i32 + gx as i32,
                bounds.min.y as i32 + gy as i32,
            );
            if dx < 0 || dy < 0 || dx >= width || dy >= height {
                return;
            }
            let i = dy as usize * stride + dx as usize * 4;
            let inv = 1.0 - coverage;
            for (k, ink) in rgb.iter().enumerate() {
                let src = f32::from(*ink) * coverage;
                data[i + k] = (src + f32::from(data[i + k]) * inv).round().min(255.0) as u8;
            }
            data[i + 3] = (255.0 * coverage + f32::from(data[i + 3]) * inv)
                .round()
                .min(255.0) as u8;
        });
    }
}

/// Draws one bar: the translucent track, then the fill clipped to the track's
/// rounded shape (like the Swift clip path).
fn draw_bar(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    s: f32,
    used: Option<f64>,
    accent: [u8; 3],
    ink: [u8; 3],
) {
    let (w, h) = (BAR_WIDTH * s, BAR_HEIGHT * s);
    let Some(track) = rounded_rect(x, y, w, h, h / 2.0) else {
        return;
    };
    pm.fill_path(
        &track,
        &paint(ink, TRACK_ALPHA),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    let Some(mut clip) = Mask::new(pm.width(), pm.height()) else {
        return;
    };
    clip.fill_path(&track, FillRule::Winding, true, Transform::identity());
    if let Some(fill) = Rect::from_xywh(x, y, fill_width(used, BAR_WIDTH) * s, h) {
        pm.fill_rect(
            fill,
            &paint(accent, 1.0),
            Transform::identity(),
            Some(&clip),
        );
    }
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.quad_to(x + w, y, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.quad_to(x + w, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.quad_to(x, y + h, x, y + h - r);
    b.line_to(x, y + r);
    b.quad_to(x, y, x + r, y);
    b.close();
    b.finish()
}

/// Renders the status image for `segments` at `scale` pixels per point.
///
/// ```
/// use tokenmon_core::ProviderId;
/// use tokenmon_macos::skia_status::{Chrome, render};
/// use tokenmon_ui::tray_icon::BarSegment;
/// let bmp = render(&[BarSegment { provider: ProviderId::Claude, used: Some(40.0) }], Chrome::Dark, 2);
/// assert_eq!(bmp.height_px, 44);
/// assert_eq!(bmp.rgba.len() as u32, bmp.width_px * bmp.height_px * 4);
/// ```
pub fn render(segments: &[BarSegment], chrome: Chrome, scale: u32) -> StatusBitmap {
    let s = scale.max(1) as f32;
    let slot = text_width(SLOT_SAMPLE).ceil();
    let (layouts, width_pt) = segment_layouts(segments.len(), slot);
    let (w_px, h_px) = ((width_pt * s) as u32, (HEIGHT * s) as u32);
    let ink = chrome.rgb();
    let Some(mut pm) = Pixmap::new(w_px.max(1), h_px.max(1)) else {
        return StatusBitmap {
            width_px: 1,
            height_px: 1,
            scale,
            rgba: vec![0; 4],
        };
    };
    let mid = HEIGHT / 2.0;
    for (segment, at) in segments.iter().zip(&layouts) {
        draw_logo(
            &mut pm,
            segment.provider,
            at.icon_x * s,
            (mid - ICON / 2.0) * s,
            ICON * s,
            &paint(ink, 1.0),
        );
        let text = percent_text(segment.used);
        let centred = at.text_x + (slot - text_width(&text)) / 2.0;
        draw_text(
            &mut pm,
            &text,
            centred * s,
            (mid + FONT_SIZE * 0.3636) * s,
            FONT_SIZE * s,
            ink,
        );
        let (r, g, b) = accent(segment.provider);
        draw_bar(
            &mut pm,
            at.bar_x * s,
            (mid - BAR_HEIGHT / 2.0) * s,
            s,
            segment.used,
            [r, g, b],
            ink,
        );
    }
    StatusBitmap {
        width_px: pm.width(),
        height_px: pm.height(),
        scale,
        rgba: pm.data().to_vec(),
    }
}

fn accent(provider: ProviderId) -> (u8, u8, u8) {
    provider.accent().to_rgb8()
}
