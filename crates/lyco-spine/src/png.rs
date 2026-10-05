//! Read a PNG's dimensions from its header.
//!
//! Deliberately does not decode the image. An atlas page is megabytes of pixels
//! and the only thing wanted here is two integers that sit in the first 24 bytes.

use anyhow::{bail, Result};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Width and height from the IHDR chunk of a PNG.
pub fn png_size(bytes: &[u8]) -> Result<(u32, u32)> {
    // 8 signature + 4 length + 4 type + 13 data is the minimum to hold IHDR.
    if bytes.len() < 33 {
        bail!("too short to be a PNG: {} bytes", bytes.len());
    }
    if bytes[..8] != SIGNATURE {
        bail!("not a PNG: the signature does not match");
    }

    let length = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let kind = &bytes[12..16];
    if kind != b"IHDR" {
        bail!(
            "the first chunk is {:?}, and IHDR must come first",
            String::from_utf8_lossy(kind)
        );
    }
    if length != 13 {
        bail!("IHDR must be 13 bytes, got {length}");
    }

    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 0 || height == 0 {
        bail!("a PNG cannot be {width}x{height}");
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG header with the given dimensions and a placeholder CRC.
    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE);
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    }

    #[test]
    fn reads_the_dimensions_from_the_header() {
        assert_eq!(png_size(&header(512, 256)).expect("size"), (512, 256));
        assert_eq!(png_size(&header(1, 1)).expect("size"), (1, 1));
    }

    #[test]
    fn a_non_png_is_refused_by_its_signature() {
        let mut bytes = header(8, 8);
        bytes[1] = b'X';
        assert!(png_size(&bytes).unwrap_err().to_string().contains("signature"));
    }

    #[test]
    fn a_truncated_file_is_refused() {
        let bytes = header(8, 8);
        for cut in [0usize, 8, 16, 24, 32] {
            let err = png_size(&bytes[..cut]);
            assert!(err.is_err(), "{cut} bytes was accepted");
        }
    }

    #[test]
    fn a_missing_ihdr_is_refused() {
        let mut bytes = header(8, 8);
        bytes[12..16].copy_from_slice(b"IDAT");
        assert!(png_size(&bytes).unwrap_err().to_string().contains("IHDR"));
    }

    #[test]
    fn zero_dimensions_are_refused() {
        assert!(png_size(&header(0, 8)).is_err());
        assert!(png_size(&header(8, 0)).is_err());
    }

    #[test]
    fn reads_a_real_generated_png() {
        // The head of a skin that a real model produced, so this is not only
        // ever tested against bytes this test wrote itself.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/blockv06_seed7.png"
        );
        let bytes = std::fs::read(path).expect("fixture");
        assert_eq!(png_size(&bytes).expect("size"), (64, 64));
    }
}
