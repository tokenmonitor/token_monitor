//! `decode_ico_frame` on synthetic `.ico` files: the happy path (bottom-up BGRA
//! to top-down RGBA, `0` meaning 256 px) and every header check that must
//! reject a malformed frame instead of reading out of bounds.

use tokenmon_ui::tray_icon::decode_ico_frame;

/// Builds a one-entry ICO whose DIB is described by the parameters, with
/// `pixels` as the BGRA payload (bottom row first).
struct Ico {
    entry_width: u8,
    dib_width: u32,
    dib_height2: u32,
    bpp: u16,
    header: u32,
    offset_shift: u32,
    pixels: Vec<u8>,
}

impl Ico {
    fn valid(size: u32, pixels: Vec<u8>) -> Self {
        Self {
            entry_width: if size == 256 { 0 } else { size as u8 },
            dib_width: size,
            dib_height2: size * 2,
            bpp: 32,
            header: 40,
            offset_shift: 0,
            pixels,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let mut dib = Vec::new();
        dib.extend_from_slice(&self.header.to_le_bytes());
        dib.extend_from_slice(&self.dib_width.to_le_bytes());
        dib.extend_from_slice(&self.dib_height2.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&self.bpp.to_le_bytes());
        dib.resize(self.header as usize, 0);
        dib.extend_from_slice(&self.pixels);

        let mut out = vec![0, 0, 1, 0, 1, 0];
        out.extend_from_slice(&[self.entry_width, self.entry_width, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(dib.len() as u32).to_le_bytes());
        out.extend_from_slice(&(22 + self.offset_shift).to_le_bytes());
        out.extend_from_slice(&dib);
        out
    }
}

/// Two rows of two BGRA pixels, bottom row first.
fn two_by_two() -> Vec<u8> {
    vec![
        // bottom row: (b,g,r,a)
        1, 2, 3, 4, 5, 6, 7, 8, //
        // top row
        9, 10, 11, 12, 13, 14, 15, 16,
    ]
}

/// A valid frame is flipped to top-down and channel-swapped to RGBA.
#[test]
fn test_valid_frame_decodes_top_down_rgba() {
    let rgba = decode_ico_frame(&Ico::valid(2, two_by_two()).bytes(), 2).expect("valid frame");
    assert_eq!(
        rgba,
        vec![
            11, 10, 9, 12, 15, 14, 13, 16, // top row first, RGBA
            3, 2, 1, 4, 7, 6, 5, 8,
        ]
    );
}

/// A width byte of `0` means a 256 px frame.
#[test]
fn test_zero_width_byte_means_256() {
    let pixels = vec![0u8; 256 * 256 * 4];
    let rgba = decode_ico_frame(&Ico::valid(256, pixels).bytes(), 256).expect("256 px frame");
    assert_eq!(rgba.len(), 256 * 256 * 4);
}

/// Asking for a size the file does not contain finds no entry.
#[test]
fn test_missing_size_is_none() {
    assert!(decode_ico_frame(&Ico::valid(2, two_by_two()).bytes(), 4).is_none());
}

/// A non-32-bit DIB is rejected.
#[test]
fn test_wrong_bit_depth_is_rejected() {
    let mut ico = Ico::valid(2, two_by_two());
    ico.bpp = 24;
    assert!(decode_ico_frame(&ico.bytes(), 2).is_none());
}

/// A DIB whose width disagrees with the requested size is rejected.
#[test]
fn test_dib_width_mismatch_is_rejected() {
    let mut ico = Ico::valid(2, two_by_two());
    ico.dib_width = 3;
    assert!(decode_ico_frame(&ico.bytes(), 2).is_none());
}

/// The DIB height must be twice the size (XOR plus AND mask rows).
#[test]
fn test_dib_height_mismatch_is_rejected() {
    let mut ico = Ico::valid(2, two_by_two());
    ico.dib_height2 = 2;
    assert!(decode_ico_frame(&ico.bytes(), 2).is_none());
}

/// A header smaller than a BITMAPINFOHEADER is rejected.
#[test]
fn test_short_dib_header_is_rejected() {
    let mut ico = Ico::valid(2, two_by_two());
    ico.header = 20;
    assert!(decode_ico_frame(&ico.bytes(), 2).is_none());
}

/// Fewer pixel bytes than the size implies is rejected, not read past.
#[test]
fn test_truncated_pixels_are_rejected() {
    let mut pixels = two_by_two();
    pixels.truncate(12);
    assert!(decode_ico_frame(&Ico::valid(2, pixels).bytes(), 2).is_none());
}

/// An entry pointing outside the file is rejected.
#[test]
fn test_out_of_range_offset_is_rejected() {
    let mut ico = Ico::valid(2, two_by_two());
    ico.offset_shift = 10_000;
    assert!(decode_ico_frame(&ico.bytes(), 2).is_none());
}

/// An entry table that runs past the end of the file is rejected.
#[test]
fn test_truncated_entry_table_is_rejected() {
    let bytes = Ico::valid(2, two_by_two()).bytes();
    assert!(decode_ico_frame(&bytes[..10], 2).is_none());
}
