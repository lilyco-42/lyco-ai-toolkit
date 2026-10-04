//! Repair the second (overlay) layer of a model-generated skin.
//!
//! Background: a diffusion model asked for a skin atlas returns RGB, with no
//! alpha channel. The overlay layer - hat, jacket, sleeves, trousers - is
//! supposed to be transparent wherever there is no garment, and since the
//! training previews had white backgrounds the model fills those regions with
//! white. Minecraft renders the overlay *on top of* the base layer, so a white
//! overlay makes the whole character a featureless white blob in game.
//!
//! This module is a port of the `fix_white_overlay` step from the working Kaggle
//! notebook (`lilyco4242/blockv0-6-lyco-skin-generator-v7`), moved into Rust for
//! two reasons: it needs no GPU, so it belongs on the side that can be unit
//! tested; and the UV rectangles it operates on are already the table in
//! [`crate::format`], so neither side has to hand-maintain a second copy.

use anyhow::{bail, Result};

use crate::format::{part_box, ArmModel, Face, Layer, Part, Rect, SKIN_HEIGHT, SKIN_WIDTH};
use crate::skin::Skin;

/// A channel at or above this is treated as background white.
pub const DEFAULT_WHITE_THRESHOLD: u8 = 235;

/// If less than this percentage of an overlay rectangle survives, the whole
/// rectangle was background and is cleared outright.
pub const DEFAULT_KEEP_PERCENT: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayFixConfig {
    pub white_threshold: u8,
    pub keep_percent: u32,
}

impl Default for OverlayFixConfig {
    fn default() -> Self {
        OverlayFixConfig {
            white_threshold: DEFAULT_WHITE_THRESHOLD,
            keep_percent: DEFAULT_KEEP_PERCENT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartOutcome {
    pub part: Part,
    pub rect: Rect,
    /// Pixels this step made transparent.
    pub cleared: usize,
    /// Opaque pixels left behind.
    pub kept: usize,
    /// True when the rectangle was judged to be all background.
    pub cleared_entirely: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayFixReport {
    pub outcomes: Vec<PartOutcome>,
    /// False when the base layer lost any opacity, which would break the skin.
    pub first_layer_intact: bool,
}

impl OverlayFixReport {
    pub fn total_cleared(&self) -> usize {
        self.outcomes.iter().map(|o| o.cleared).sum()
    }

    pub fn cleared_rects(&self) -> Vec<Part> {
        self.outcomes
            .iter()
            .filter(|o| o.cleared_entirely)
            .map(|o| o.part)
            .collect()
    }
}

fn count_opaque(skin: &Skin, rect: Rect) -> usize {
    let mut n = 0;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            if skin.pixel(x, y)[3] > 0 {
                n += 1;
            }
        }
    }
    n
}

fn clear_rect(skin: &mut Skin, rect: Rect) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            skin.set_pixel(x, y, [0, 0, 0, 0]);
        }
    }
}

/// True when every rendered base-layer face is still fully opaque.
///
/// Deliberately checks the six faces rather than the unwrap's bounding
/// rectangle. A box unwrap leaves two d by d squares unused at the top
/// corners, which no face ever samples, so demanding opacity there would
/// flag perfectly good skins - including this crate's own template.
fn base_layer_intact(skin: &Skin, arm: ArmModel) -> bool {
    Part::ALL.iter().all(|part| {
        let b = part_box(*part, Layer::Base, arm);
        Face::ALL
            .iter()
            .all(|f| rect_fully_opaque(skin, b.face(*f)))
    })
}

fn rect_fully_opaque(skin: &Skin, rect: Rect) -> bool {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            if skin.pixel(x, y)[3] != 255 {
                return false;
            }
        }
    }
    true
}

/// Make near-white overlay pixels transparent, in place.
///
/// Returns a per-part report so a caller can tell "the model drew a hat" from
/// "the model painted the whole overlay white", rather than silently shipping a
/// skin that renders as a white blob.
pub fn fix_white_overlay(
    skin: &mut Skin,
    arm: ArmModel,
    cfg: &OverlayFixConfig,
) -> Result<OverlayFixReport> {
    if skin.width() != SKIN_WIDTH || skin.height() != SKIN_HEIGHT {
        bail!(
            "overlay repair expects a 64x64 skin, got {}x{}",
            skin.width(),
            skin.height()
        );
    }
    let keep_percent = cfg.keep_percent.min(100);
    let mut outcomes = Vec::new();

    for part in Part::ALL {
        let rect = part_box(part, Layer::Overlay, arm).bounds();
        let area = rect.area() as usize;

        let mut cleared = 0usize;
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let px = skin.pixel(x, y);
                if px[3] == 0 {
                    continue;
                }
                let white = px[0] >= cfg.white_threshold
                    && px[1] >= cfg.white_threshold
                    && px[2] >= cfg.white_threshold;
                if white {
                    skin.set_pixel(x, y, [0, 0, 0, 0]);
                    cleared += 1;
                }
            }
        }

        // Little or nothing survived, so the rectangle was background.
        let mut kept = count_opaque(skin, rect);
        let mut cleared_entirely = false;
        if kept * 100 < area * keep_percent as usize {
            clear_rect(skin, rect);
            cleared += kept;
            kept = 0;
            cleared_entirely = true;
        }

        outcomes.push(PartOutcome {
            part,
            rect,
            cleared,
            kept,
            cleared_entirely,
        });
    }

    let first_layer_intact = base_layer_intact(skin, arm);

    Ok(OverlayFixReport {
        outcomes,
        first_layer_intact,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::ArmModel;

    /// A skin whose base layer is solid and whose overlay is a given colour.
    fn skin_with_overlay(color: [u8; 4]) -> Skin {
        let mut skin = Skin::template(ArmModel::Classic);
        for part in Part::ALL {
            let rect = part_box(part, Layer::Overlay, ArmModel::Classic).bounds();
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right() {
                    skin.set_pixel(x, y, color);
                }
            }
        }
        skin
    }

    /// These rectangles are the ones a working Kaggle notebook derived
    /// independently. Pinning them here means a change to the UV table cannot
    /// silently disagree with the model pipeline that consumes it.
    #[test]
    fn the_table_matches_the_rectangles_the_kaggle_pipeline_uses() {
        let expect_overlay = [
            (Part::Head, [32, 0, 64, 16]),
            (Part::Body, [16, 32, 40, 48]),
            (Part::RightArm, [40, 32, 56, 48]),
            (Part::LeftArm, [48, 48, 64, 64]),
            (Part::RightLeg, [0, 32, 16, 48]),
            (Part::LeftLeg, [0, 48, 16, 64]),
        ];
        for (part, want) in expect_overlay {
            let r = part_box(part, Layer::Overlay, ArmModel::Classic).bounds();
            assert_eq!(
                [r.x, r.y, r.right(), r.bottom()],
                want,
                "{} overlay",
                part.name()
            );
        }

        let expect_base = [
            (Part::Head, [0, 0, 32, 16]),
            (Part::Body, [16, 16, 40, 32]),
            (Part::RightArm, [40, 16, 56, 32]),
            (Part::RightLeg, [0, 16, 16, 32]),
            (Part::LeftLeg, [16, 48, 32, 64]),
            (Part::LeftArm, [32, 48, 48, 64]),
        ];
        for (part, want) in expect_base {
            let r = part_box(part, Layer::Base, ArmModel::Classic).bounds();
            assert_eq!(
                [r.x, r.y, r.right(), r.bottom()],
                want,
                "{} base",
                part.name()
            );
        }
    }

    #[test]
    fn an_all_white_overlay_is_cleared_completely() {
        let mut skin = skin_with_overlay([255, 255, 255, 255]);
        let report = fix_white_overlay(&mut skin, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");

        assert_eq!(
            report.cleared_rects().len(),
            6,
            "every overlay was background"
        );
        assert!(report.total_cleared() > 0);
        assert!(report.first_layer_intact, "the base layer must survive");

        // And the skin must still be structurally valid.
        assert!(skin.validate().ok());
        for part in Part::ALL {
            let rect = part_box(part, Layer::Overlay, ArmModel::Classic).bounds();
            assert_eq!(count_opaque(&skin, rect), 0, "{} not cleared", part.name());
        }
    }

    #[test]
    fn a_genuine_overlay_is_preserved() {
        // A dark garment covering most of every rectangle survives.
        let mut skin = skin_with_overlay([40, 60, 120, 255]);
        let report = fix_white_overlay(&mut skin, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");

        assert!(
            report.cleared_rects().is_empty(),
            "nothing should be judged background"
        );
        assert_eq!(report.total_cleared(), 0);
        for outcome in &report.outcomes {
            assert_eq!(
                outcome.kept,
                outcome.rect.area() as usize,
                "{}",
                outcome.part.name()
            );
        }
    }

    #[test]
    fn a_white_background_with_a_real_garment_keeps_only_the_garment() {
        let mut skin = skin_with_overlay([255, 255, 255, 255]);
        // Paint a genuine hat over the top-left quarter of the head overlay.
        let head = part_box(Part::Head, Layer::Overlay, ArmModel::Classic).bounds();
        for y in head.y..head.y + head.h / 2 {
            for x in head.x..head.x + head.w / 2 {
                skin.set_pixel(x, y, [200, 30, 30, 255]);
            }
        }

        let report = fix_white_overlay(&mut skin, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");

        let head_outcome = report
            .outcomes
            .iter()
            .find(|o| o.part == Part::Head)
            .expect("head outcome");
        // A quarter survives, which is above the 12% keep threshold.
        assert!(
            !head_outcome.cleared_entirely,
            "a quarter is not background"
        );
        assert!(head_outcome.kept > 0);
        assert!(
            head_outcome.cleared > 0,
            "the white three quarters should go"
        );
        assert_eq!(skin.pixel(head.x, head.y), [200, 30, 30, 255]);
    }

    #[test]
    fn a_mostly_white_speck_does_not_survive() {
        let mut skin = skin_with_overlay([255, 255, 255, 255]);
        let body = part_box(Part::Body, Layer::Overlay, ArmModel::Classic).bounds();
        // One dark pixel is under the keep threshold, so the rect goes entirely.
        skin.set_pixel(body.x, body.y, [10, 10, 10, 255]);

        let report = fix_white_overlay(&mut skin, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");
        let body_outcome = report
            .outcomes
            .iter()
            .find(|o| o.part == Part::Body)
            .expect("body outcome");
        assert!(body_outcome.cleared_entirely);
        assert_eq!(body_outcome.kept, 0);
    }

    #[test]
    fn threshold_decides_what_counts_as_white() {
        // 240 is above the default 235, so it is background.
        let mut above = skin_with_overlay([240, 240, 240, 255]);
        let report = fix_white_overlay(&mut above, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");
        assert_eq!(report.cleared_rects().len(), 6);

        // 230 is below it, so it is treated as art and kept.
        let mut below = skin_with_overlay([230, 230, 230, 255]);
        let report = fix_white_overlay(&mut below, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");
        assert_eq!(report.total_cleared(), 0);
    }

    /// Regression guard for the bug this check originally had: the unwrap's
    /// unused corner squares are transparent in a fresh template, and treating
    /// that as base-layer damage made three tests fail.
    #[test]
    fn unused_unwrap_corners_do_not_count_as_damage() {
        let b = part_box(Part::Body, Layer::Base, ArmModel::Classic);
        let bounds = b.bounds();
        let face_area: u32 = Face::ALL.iter().map(|f| b.face(*f).area()).sum();
        assert!(
            face_area < bounds.area(),
            "the unwrap must have unused corners for this test to mean anything"
        );

        let mut skin = Skin::template(ArmModel::Classic);
        let corner = Rect::new(bounds.x, bounds.y, b.d, b.d);
        assert!(
            skin.rect_is_transparent(corner),
            "the template leaves the unwrap corner clear"
        );

        let report = fix_white_overlay(&mut skin, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");
        assert!(
            report.first_layer_intact,
            "a clear corner is not base-layer damage"
        );
    }

    #[test]
    fn already_transparent_pixels_are_left_alone() {
        // The template leaves the overlay transparent, so nothing should change.
        let mut skin = Skin::template(ArmModel::Classic);
        let before = skin.clone();
        let report = fix_white_overlay(&mut skin, ArmModel::Classic, &OverlayFixConfig::default())
            .expect("fix");
        assert_eq!(skin, before, "an untouched skin must round trip unchanged");
        assert_eq!(report.total_cleared(), 0);
        assert!(report.first_layer_intact);
    }

    #[test]
    fn a_wrongly_sized_skin_is_refused() {
        let mut small = Skin::from_rgba_bytes(32, 32, &[0u8; 32 * 32 * 4]).expect("build");
        let err = fix_white_overlay(&mut small, ArmModel::Classic, &OverlayFixConfig::default())
            .unwrap_err();
        assert!(err.to_string().contains("64x64"), "{err}");
    }

    #[test]
    fn slim_skins_use_the_slim_rectangles() {
        let mut skin = Skin::template(ArmModel::Slim);
        for part in Part::ALL {
            let rect = part_box(part, Layer::Overlay, ArmModel::Slim).bounds();
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right() {
                    skin.set_pixel(x, y, [255, 255, 255, 255]);
                }
            }
        }
        let report = fix_white_overlay(&mut skin, ArmModel::Slim, &OverlayFixConfig::default())
            .expect("fix");
        assert_eq!(report.cleared_rects().len(), 6);
        assert!(report.first_layer_intact);
    }
}
