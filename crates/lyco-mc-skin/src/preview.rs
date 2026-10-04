//! Render a skin as a flat character view, so a human can judge it.
//!
//! A 64x64 skin is a UV atlas: unreadable on its own. The working Kaggle
//! notebook solved that with an `assemble()` helper that blits the front and
//! back faces onto a 16x32 character silhouette, and this is that helper moved
//! into Rust.
//!
//! Moving it here rather than generating a Python copy is what lets the
//! generated notebook contain no UV table at all - the knowledge lives in
//! [`crate::format`] once, and the Kaggle side only does inference.

use anyhow::{Context, Result};

use crate::format::{part_box, ArmModel, Face, Layer, Part};
use crate::skin::Skin;

/// Width of the composed character canvas.
pub const VIEW_WIDTH: u32 = 16;
/// Height of the composed character canvas.
pub const VIEW_HEIGHT: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl View {
    fn empty() -> Self {
        View {
            width: VIEW_WIDTH,
            height: VIEW_HEIGHT,
            pixels: vec![[0, 0, 0, 0]; (VIEW_WIDTH * VIEW_HEIGHT) as usize],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        self.pixels[(y * self.width + x) as usize]
    }

    fn set(&mut self, x: u32, y: u32, value: [u8; 4]) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = value;
        }
    }

    pub fn opaque_count(&self) -> usize {
        self.pixels.iter().filter(|p| p[3] > 0).count()
    }

    pub fn raw(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    /// The view as RGBA bytes, row major.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|p| p.iter().copied()).collect()
    }

    /// Nearest-neighbour magnification. A 16x32 character is hard to judge at
    /// its real size, and smoothing would hide exactly the pixel detail this
    /// preview exists to show.
    pub fn scaled(&self, factor: u32) -> View {
        let factor = factor.max(1);
        let mut out = View {
            width: self.width * factor,
            height: self.height * factor,
            pixels: vec![[0, 0, 0, 0]; (self.width * factor * self.height * factor) as usize],
        };
        for y in 0..self.height {
            for x in 0..self.width {
                let px = self.pixel(x, y);
                for dy in 0..factor {
                    for dx in 0..factor {
                        out.set(x * factor + dx, y * factor + dy, px);
                    }
                }
            }
        }
        out
    }

    /// Write the view to a PNG.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        let buffer = image::RgbaImage::from_raw(self.width, self.height, self.to_rgba_bytes())
            .ok_or_else(|| anyhow::anyhow!("view buffer does not match its dimensions"))?;
        buffer
            .save(path)
            .with_context(|| format!("cannot write {}", path.display()))
    }

    /// Flip left to right. Looking at someone from behind swaps their sides.
    pub fn mirrored(&self) -> View {
        let mut out = View::empty();
        for y in 0..self.height {
            for x in 0..self.width {
                out.set(self.width - 1 - x, y, self.pixel(x, y));
            }
        }
        out
    }
}

/// Where each part sits on the 16x32 silhouette.
///
/// The head tops the canvas, then one row of body flanked by arms, then legs.
/// Arm and leg x positions depend on the arm width so slim skins line up.
fn placement(arm: ArmModel) -> [(Part, u32, u32); 6] {
    let aw = arm.arm_width();
    let leg = 4u32;
    [
        (Part::Head, 4, 0),
        (Part::RightArm, 0, 8),
        (Part::Body, 4, 8),
        (Part::LeftArm, VIEW_WIDTH - aw, 8),
        (Part::RightLeg, 4, 20),
        (Part::LeftLeg, VIEW_WIDTH - leg, 20),
    ]
}

/// Compose one side of the character onto a flat canvas.
///
/// A preview, not a renderer: it blits faces flat with no perspective, which is
/// exactly enough to tell a good skin from a white blob.
pub fn character_view(skin: &Skin, arm: ArmModel, side: Side) -> View {
    let face = match side {
        Side::Front => Face::Front,
        Side::Back => Face::Back,
    };

    let mut view = View::empty();
    for (part, dx, dy) in placement(arm) {
        let src = part_box(part, Layer::Base, arm).face(face);
        for y in 0..src.h {
            for x in 0..src.w {
                let px = skin.pixel(src.x + x, src.y + y);
                if px[3] == 0 {
                    continue;
                }
                view.set(dx + x, dy + y, px);
            }
        }
    }

    match side {
        Side::Front => view,
        Side::Back => view.mirrored(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canvas_is_a_character_silhouette() {
        let skin = Skin::template(ArmModel::Classic);
        let view = character_view(&skin, ArmModel::Classic, Side::Front);
        assert_eq!((view.width, view.height), (16, 32));
        // Head 8x8, body 8x12, four limbs 4x12 = 64 + 96 + 192 = 352.
        assert_eq!(view.opaque_count(), 352);
    }

    #[test]
    fn a_blank_skin_composes_to_nothing() {
        let view = character_view(&Skin::blank(), ArmModel::Classic, Side::Front);
        assert_eq!(view.opaque_count(), 0);
    }

    #[test]
    fn the_head_goes_to_the_top_of_the_canvas() {
        let mut skin = Skin::blank();
        let head = part_box(Part::Head, Layer::Base, ArmModel::Classic);
        let front = head.face(Face::Front);
        for y in front.y..front.bottom() {
            for x in front.x..front.right() {
                skin.set_pixel(x, y, [255, 0, 0, 255]);
            }
        }
        let view = character_view(&skin, ArmModel::Classic, Side::Front);
        // The head occupies x 4..12, y 0..8 and nothing else was painted.
        assert_eq!(view.opaque_count(), 64);
        assert_eq!(view.pixel(4, 0), [255, 0, 0, 255]);
        assert_eq!(view.pixel(11, 7), [255, 0, 0, 255]);
        assert_eq!(view.pixel(4, 8), [0, 0, 0, 0], "nothing below the head");
    }

    #[test]
    fn arms_flank_the_body_and_slim_arms_shift_inward() {
        let mut skin = Skin::blank();
        let arm = part_box(Part::LeftArm, Layer::Base, ArmModel::Classic);
        let front = arm.face(Face::Front);
        for y in front.y..front.bottom() {
            for x in front.x..front.right() {
                skin.set_pixel(x, y, [0, 255, 0, 255]);
            }
        }
        let classic = character_view(&skin, ArmModel::Classic, Side::Front);
        assert_eq!(
            classic.pixel(12, 8),
            [0, 255, 0, 255],
            "classic arm at x=12"
        );

        // A slim arm is one pixel narrower, so it starts one pixel further right.
        let slim_arm = part_box(Part::LeftArm, Layer::Base, ArmModel::Slim);
        let slim_front = slim_arm.face(Face::Front);
        let mut slim_skin = Skin::blank();
        for y in slim_front.y..slim_front.bottom() {
            for x in slim_front.x..slim_front.right() {
                slim_skin.set_pixel(x, y, [0, 255, 0, 255]);
            }
        }
        let slim = character_view(&slim_skin, ArmModel::Slim, Side::Front);
        assert_eq!(slim.pixel(13, 8), [0, 255, 0, 255], "slim arm at x=13");
        assert_eq!(slim.pixel(12, 8), [0, 0, 0, 0]);
    }

    /// The back view reads the Back faces and mirrors them, because looking at
    /// someone from behind swaps their sides. It is *not* the front view
    /// flipped: those are different pixels of the atlas.
    #[test]
    fn the_back_view_reads_back_faces_and_mirrors_them() {
        let mut skin = Skin::blank();
        let back = part_box(Part::Head, Layer::Base, ArmModel::Classic).face(Face::Back);
        for y in back.y..back.bottom() {
            for x in back.x..back.right() {
                skin.set_pixel(x, y, [10, 20, 30, 255]);
            }
        }
        // A mark on the left column of the source face, so mirroring is visible.
        for y in back.y..back.bottom() {
            skin.set_pixel(back.x, y, [200, 0, 0, 255]);
        }

        let view = character_view(&skin, ArmModel::Classic, Side::Back);
        // The head sits at x 4..12; source column 0 lands on x 11.
        assert_eq!(
            view.pixel(11, 0),
            [200, 0, 0, 255],
            "the mark mirrors right"
        );
        assert_eq!(view.pixel(4, 0), [10, 20, 30, 255]);

        // Only the back faces were painted, so the front view is empty.
        let front = character_view(&skin, ArmModel::Classic, Side::Front);
        assert_eq!(front.opaque_count(), 0);
    }

    #[test]
    fn a_transparent_face_leaves_a_hole_rather_than_a_blank_fill() {
        let mut skin = Skin::template(ArmModel::Classic);
        // Punch a hole in the middle of the body's front face.
        let front = part_box(Part::Body, Layer::Base, ArmModel::Classic).face(Face::Front);
        skin.set_pixel(front.x + 1, front.y + 1, [0, 0, 0, 0]);
        let view = character_view(&skin, ArmModel::Classic, Side::Front);
        assert_eq!(view.pixel(4 + 1, 8 + 1), [0, 0, 0, 0]);
        assert_eq!(view.opaque_count(), 351);
    }
}
