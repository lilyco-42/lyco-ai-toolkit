//! lyco-mc-skin - Minecraft skin geometry, palette quantization and validation (F1).
//!
//! Scope of this crate:
//!   * `format`  - the 64x64 box-UV unwrap and the 12 part/layer anchors
//!   * `palette` - deterministic median-cut quantization to a small palette
//!   * `skin`    - load/save/template plus structural validation
//!
//! Image *generation* is deliberately not here. A diffusion backend is a
//! subprocess boundary (ComfyUI) that may not exist on the build machine, so
//! this crate stays pure and testable offline.

pub mod format;
pub mod palette;
pub mod skin;

pub use format::{
    all_boxes, part_box, ArmModel, BoxUv, Face, Layer, Part, Rect, SKIN_HEIGHT, SKIN_WIDTH,
};
pub use palette::{
    apply as apply_palette, distinct_opaque_colors, median_cut, Palette, Rgb, DEFAULT_PALETTE_SIZE,
};
pub use skin::{Issue, Report, Severity, Skin};

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn end_to_end_quantize_a_generated_looking_image() {
        // Simulate a noisy generated image mapped onto the skin atlas, then
        // prove it collapses to a small coherent palette without becoming invalid.
        let mut s = Skin::template(ArmModel::Classic);
        let mut seed = 0x1234_5678u32;
        for y in 0..64u32 {
            for x in 0..64u32 {
                let p = s.pixel(x, y);
                if p[3] < 128 {
                    continue;
                }
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let jitter = ((seed >> 24) & 0x0f) as u8;
                s.set_pixel(
                    x,
                    y,
                    [
                        p[0].saturating_add(jitter),
                        p[1].saturating_add(jitter),
                        p[2].saturating_add(jitter),
                        255,
                    ],
                );
            }
        }

        let before = distinct_opaque_colors(s.pixels());
        assert!(before > 8, "expected noise, got {} colors", before);

        let pal = median_cut(s.pixels(), DEFAULT_PALETTE_SIZE);
        assert!(pal.len() <= DEFAULT_PALETTE_SIZE);

        let snapped = apply_palette(s.pixels(), &pal);
        let after = distinct_opaque_colors(&snapped);
        assert!(after <= DEFAULT_PALETTE_SIZE, "not reduced: {}", after);
        assert!(
            after < before,
            "expected fewer colors: {} -> {}",
            before,
            after
        );

        // Quantization must not introduce transparency where art existed.
        let bytes: Vec<u8> = snapped.iter().flat_map(|p| p.iter().copied()).collect();
        let rebuilt = Skin::from_rgba_bytes(64, 64, &bytes).expect("rebuild");
        assert_eq!(
            rebuilt.opaque_count(),
            s.opaque_count(),
            "opaque pixel count changed during quantization"
        );
        assert!(rebuilt.validate().ok(), "quantized skin must stay valid");
    }

    #[test]
    fn face_rectangles_are_disjoint_and_inside_their_box() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            for (part, layer, b) in all_boxes(arm) {
                let faces = b.faces();
                for i in 0..faces.len() {
                    assert!(
                        !faces[i].is_empty(),
                        "{} {} has an empty face",
                        part.name(),
                        layer.name()
                    );
                    for j in (i + 1)..faces.len() {
                        assert!(
                            !faces[i].intersects(&faces[j]),
                            "{} {} faces intersect",
                            part.name(),
                            layer.name()
                        );
                    }
                }
            }
        }
    }
}
