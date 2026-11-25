//! Vector silhouettes standing in for the provider logos, plus a 3x5 digit
//! font. The brand SVGs (owned by ws-ui-kit) are too detailed for 16 px; these
//! are deliberately simple template masks tinted with the chrome color.

use tiny_skia::{FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};
use tokenmon_core::ProviderId;

/// 3x5 glyph rows, MSB of the low 3 bits is the left pixel. Index 10 is `-`.
const DIGITS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b000, 0b111, 0b000, 0b000],
];

/// Pixel width of `n` digits at `scale` (3 px per glyph, 1 px gap, scaled).
pub fn digits_width(n: usize, scale: u32) -> u32 {
    let n = n as u32;
    n * 3 * scale + n.saturating_sub(1) * scale
}

/// Pixel height of the digit font at `scale`.
pub fn digits_height(scale: u32) -> u32 {
    5 * scale
}

/// Draws decimal `text` (digits only) with its top-left at `(x, y)`.
pub fn draw_digits(pm: &mut Pixmap, text: &str, x: u32, y: u32, scale: u32, paint: &Paint<'_>) {
    let mut cx = x;
    for ch in text.chars() {
        let Some(d) = ch.to_digit(10) else { continue };
        for (row, bits) in DIGITS[d as usize].iter().enumerate() {
            for col in 0..3u32 {
                if bits >> (2 - col) & 1 == 1 {
                    fill_rect(
                        pm,
                        (cx + col * scale) as f32,
                        (y + row as u32 * scale) as f32,
                        scale as f32,
                        scale as f32,
                        paint,
                    );
                }
            }
        }
        cx += 4 * scale;
    }
}

/// Fills an axis-aligned rectangle (no-op when degenerate).
pub fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, paint: &Paint<'_>) {
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, paint, Transform::identity(), None);
    }
}

/// Fills a rounded rectangle with corner radius clamped to half the short side.
#[allow(clippy::many_single_char_names)]
pub fn fill_rrect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, paint: &Paint<'_>) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    if let Some(p) = pb.finish() {
        pm.fill_path(&p, paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn stroke(pm: &mut Pixmap, path: &Path, width: f32, paint: &Paint<'_>, t: Transform) {
    let s = Stroke {
        width,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    pm.stroke_path(path, paint, &s, t, None);
}

/// Strokes the polyline through `points` (a degenerate path draws nothing).
fn stroke_poly(
    pm: &mut Pixmap,
    points: &[(f32, f32)],
    closed: bool,
    width: f32,
    paint: &Paint<'_>,
    t: Transform,
) {
    let mut pb = PathBuilder::new();
    for (i, (px, py)) in points.iter().enumerate() {
        if i == 0 {
            pb.move_to(*px, *py);
        } else {
            pb.line_to(*px, *py);
        }
    }
    if closed {
        pb.close();
    }
    if let Some(path) = pb.finish() {
        stroke(pm, &path, width, paint, t);
    }
}

/// Strokes (`width = Some`) or fills (`None`) a circle in unit coordinates.
fn circle(
    pm: &mut Pixmap,
    (cx, cy, r): (f32, f32, f32),
    width: Option<f32>,
    paint: &Paint<'_>,
    t: Transform,
) {
    let Some(path) = PathBuilder::from_circle(cx, cy, r) else {
        return;
    };
    match width {
        Some(w) => stroke(pm, &path, w, paint, t),
        None => pm.fill_path(&path, paint, FillRule::Winding, t, None),
    }
}

/// Draws the silhouette for `provider` (Overview = the TokenMon mark) in a
/// `side` x `side` box at `(x, y)`.
pub fn draw_logo(
    pm: &mut Pixmap,
    provider: ProviderId,
    x: f32,
    y: f32,
    side: f32,
    paint: &Paint<'_>,
) {
    let t = Transform::from_translate(x, y).pre_scale(side, side);
    let w = 0.16; // stroke width in unit coordinates
    let line = |pm: &mut Pixmap, pts: &[(f32, f32)], closed: bool| {
        stroke_poly(pm, pts, closed, w, paint, t);
    };
    match provider {
        ProviderId::Grok => {
            circle(pm, (0.5, 0.5, 0.38), Some(w), paint, t);
            line(pm, &[(0.18, 0.88), (0.82, 0.12)], false);
        }
        ProviderId::Cursor => line(
            pm,
            &[
                (0.5, 0.04),
                (0.92, 0.28),
                (0.92, 0.72),
                (0.5, 0.96),
                (0.08, 0.72),
                (0.08, 0.28),
            ],
            true,
        ),
        ProviderId::OpenCode => {
            line(
                pm,
                &[(0.34, 0.12), (0.12, 0.12), (0.12, 0.88), (0.34, 0.88)],
                false,
            );
            line(
                pm,
                &[(0.66, 0.12), (0.88, 0.12), (0.88, 0.88), (0.66, 0.88)],
                false,
            );
        }
        ProviderId::Claude => {
            line(pm, &[(0.5, 0.06), (0.5, 0.94)], false);
            line(pm, &[(0.1, 0.28), (0.9, 0.72)], false);
            line(pm, &[(0.1, 0.72), (0.9, 0.28)], false);
        }
        ProviderId::ChatGpt => {
            line(
                pm,
                &[
                    (0.5, 0.04),
                    (0.9, 0.27),
                    (0.9, 0.73),
                    (0.5, 0.96),
                    (0.1, 0.73),
                    (0.1, 0.27),
                ],
                true,
            );
            circle(pm, (0.5, 0.5, 0.12), None, paint, t);
        }
        ProviderId::OpenRouter => {
            line(pm, &[(0.08, 0.3), (0.5, 0.3), (0.5, 0.3)], false);
            line(pm, &[(0.08, 0.7), (0.5, 0.7), (0.5, 0.7)], false);
            line(pm, &[(0.55, 0.12), (0.92, 0.5), (0.55, 0.88)], false);
        }
        ProviderId::Grokbot => {
            line(
                pm,
                &[(0.12, 0.25), (0.88, 0.25), (0.88, 0.9), (0.12, 0.9)],
                true,
            );
            line(pm, &[(0.5, 0.25), (0.5, 0.05)], false);
            circle(pm, (0.34, 0.55, 0.07), None, paint, t);
            circle(pm, (0.66, 0.55, 0.07), None, paint, t);
        }
        ProviderId::Overview => {
            fill_rrect(pm, x, y, side, side, side * 0.28, paint);
        }
    }
}
