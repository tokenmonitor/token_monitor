//! Bridges between raw RGBA bitmaps (tiny-skia) and AppKit images.
#![allow(unsafe_code)]

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::skia_status::StatusBitmap;

/// A zeroed 8-bit RGBA bitmap rep of `width` x `height` pixels, shown at
/// `size` points (so 2 pixels per point on Retina).
pub fn blank_rep(width: usize, height: usize, size: NSSize) -> Option<Retained<NSBitmapImageRep>> {
    // SAFETY: a null plane pointer asks AppKit to allocate the pixel storage;
    // all other arguments are plain values within the documented ranges.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }?;
    rep.setSize(size);
    Some(rep)
}

/// Copies premultiplied RGBA `rgba` (`width` x `height`) into a new rep.
pub fn rep_from_premultiplied(
    rgba: &[u8],
    width: usize,
    height: usize,
    size: NSSize,
) -> Option<Retained<NSBitmapImageRep>> {
    let rep = blank_rep(width, height, size)?;
    let row_bytes = rep.bytesPerRow() as usize;
    // SAFETY: `bitmapData` points at `bytesPerRow * height` bytes owned by the
    // rep, which stays alive for this scope; each row copy stays inside it.
    unsafe {
        let dst = rep.bitmapData();
        if dst.is_null() || rgba.len() < width * height * 4 {
            return None;
        }
        for y in 0..height {
            std::ptr::copy_nonoverlapping(
                rgba.as_ptr().add(y * width * 4),
                dst.add(y * row_bytes),
                width * 4,
            );
        }
    }
    Some(rep)
}

/// The tiny-skia status bitmap as an `NSImage` (not a template: the colors
/// are the brand accents).
pub fn status_image(bitmap: &StatusBitmap) -> Option<Retained<NSImage>> {
    let size = NSSize::new(f64::from(bitmap.width_pt()), f64::from(bitmap.height_pt()));
    let rep = rep_from_premultiplied(
        &bitmap.rgba,
        bitmap.width_px as usize,
        bitmap.height_px as usize,
        size,
    )?;
    Some(image_from_rep(&rep, size))
}

/// Wraps a rep in an `NSImage` of `size` points.
pub fn image_from_rep(rep: &NSBitmapImageRep, size: NSSize) -> Retained<NSImage> {
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.addRepresentation(rep);
    image.setTemplate(false);
    image
}

/// Convenience for a point-based rect.
pub fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// PNG bytes of an image's first bitmap representation (screenshots).
pub fn png_of(rep: &NSBitmapImageRep) -> Option<Vec<u8>> {
    use objc2_app_kit::NSBitmapImageFileType;
    // SAFETY: a valid type constant and an empty properties dictionary.
    let data = unsafe {
        rep.representationUsingType_properties(
            NSBitmapImageFileType::PNG,
            &objc2_foundation::NSDictionary::new(),
        )
    }?;
    Some(data.to_vec())
}
