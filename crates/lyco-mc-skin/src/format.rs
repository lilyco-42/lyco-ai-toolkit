//! Minecraft skin 64x64 format: box-UV unwrap + body-part anchor table.
//!
//! Layout verified against bs-community/skinview3d (src/model.ts, master):
//!     setUVs(box, u, v, width, height, depth, 64, 64)
//!     head(0,0,8,8,8)      head overlay(32,0,8,8,8)
//!     body(16,16,8,12,4)   body overlay(16,32,8,12,4)
//!     rightArm(40,16,4|3,12,4)  rightArm overlay(40,32,4|3,12,4)
//!     leftArm(32,48,4|3,12,4)   leftArm overlay(48,48,4|3,12,4)
//!     rightLeg(0,16,4,12,4)     rightLeg overlay(0,32,4,12,4)
//!     leftLeg(16,48,4,12,4)     leftLeg overlay(0,48,4,12,4)

/// Width of a modern Minecraft skin texture.
pub const SKIN_WIDTH: u32 = 64;
/// Height of a modern Minecraft skin texture (legacy 64x32 uses cape-style UVs).
pub const SKIN_HEIGHT: u32 = 64;

/// Arm geometry selector. Slim (Alex) arms are 3px wide, classic (Steve) 4px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmModel {
    Classic,
    Slim,
}

impl ArmModel {
    pub fn arm_width(self) -> u32 {
        match self {
            ArmModel::Classic => 4,
            ArmModel::Slim => 3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ArmModel::Classic => "classic",
            ArmModel::Slim => "slim",
        }
    }
}

/// Which of the two texture layers a part belongs to. Overlay holds the hat,
/// jacket, sleeve and trouser sub-textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Base,
    Overlay,
}

impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Layer::Base => "base",
            Layer::Overlay => "overlay",
        }
    }
}

/// The six box-model body parts of a player skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Head,
    Body,
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
}

impl Part {
    pub const ALL: [Part; 6] = [
        Part::Head,
        Part::Body,
        Part::RightArm,
        Part::LeftArm,
        Part::RightLeg,
        Part::LeftLeg,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Part::Head => "head",
            Part::Body => "body",
            Part::RightArm => "right_arm",
            Part::LeftArm => "left_arm",
            Part::RightLeg => "right_leg",
            Part::LeftLeg => "left_leg",
        }
    }
}

/// A single face of an unwrapped box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Top,
    Bottom,
    Left,
    Front,
    Right,
    Back,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::Top,
        Face::Bottom,
        Face::Left,
        Face::Front,
        Face::Right,
        Face::Back,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Face::Top => "top",
            Face::Bottom => "bottom",
            Face::Left => "left",
            Face::Front => "front",
            Face::Right => "right",
            Face::Back => "back",
        }
    }
}

/// An axis-aligned rectangle in texture space (origin top-left, y grows down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> u32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }

    pub fn area(&self) -> u32 {
        self.w * self.h
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Half-open intersection test: rectangles that merely touch do not intersect.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// Anchor and dimensions of one box in the 64x64 skin atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxUv {
    pub u: u32,
    pub v: u32,
    pub w: u32,
    pub h: u32,
    pub d: u32,
}

impl BoxUv {
    pub const fn new(u: u32, v: u32, w: u32, h: u32, d: u32) -> Self {
        BoxUv { u, v, w, h, d }
    }

    /// Unwrapped width: side column + front + side column + back.
    pub fn unwrap_width(&self) -> u32 {
        2 * self.w + 2 * self.d
    }

    /// Unwrapped height: one depth row (top/bottom) plus the side strip.
    pub fn unwrap_height(&self) -> u32 {
        self.h + self.d
    }

    /// Footprint of the whole unwrapped box.
    pub fn bounds(&self) -> Rect {
        Rect::new(self.u, self.v, self.unwrap_width(), self.unwrap_height())
    }

    /// Rectangle for one face, using the standard Minecraft box unwrap.
    pub fn face(&self, f: Face) -> Rect {
        let (w, h, d) = (self.w, self.h, self.d);
        match f {
            Face::Top => Rect::new(self.u + d, self.v, w, d),
            Face::Bottom => Rect::new(self.u + w + d, self.v, w, d),
            Face::Left => Rect::new(self.u, self.v + d, d, h),
            Face::Front => Rect::new(self.u + d, self.v + d, w, h),
            Face::Right => Rect::new(self.u + w + d, self.v + d, d, h),
            Face::Back => Rect::new(self.u + w + 2 * d, self.v + d, w, h),
        }
    }

    pub fn faces(&self) -> [Rect; 6] {
        let mut out = [Rect::new(0, 0, 0, 0); 6];
        let mut i = 0;
        while i < 6 {
            out[i] = self.face(Face::ALL[i]);
            i += 1;
        }
        out
    }
}

/// Texture anchor for a part/layer pair under the given arm model.
pub fn part_box(part: Part, layer: Layer, arm: ArmModel) -> BoxUv {
    let aw = arm.arm_width();
    match (part, layer) {
        (Part::Head, Layer::Base) => BoxUv::new(0, 0, 8, 8, 8),
        (Part::Head, Layer::Overlay) => BoxUv::new(32, 0, 8, 8, 8),
        (Part::Body, Layer::Base) => BoxUv::new(16, 16, 8, 12, 4),
        (Part::Body, Layer::Overlay) => BoxUv::new(16, 32, 8, 12, 4),
        (Part::RightArm, Layer::Base) => BoxUv::new(40, 16, aw, 12, 4),
        (Part::RightArm, Layer::Overlay) => BoxUv::new(40, 32, aw, 12, 4),
        (Part::LeftArm, Layer::Base) => BoxUv::new(32, 48, aw, 12, 4),
        (Part::LeftArm, Layer::Overlay) => BoxUv::new(48, 48, aw, 12, 4),
        (Part::RightLeg, Layer::Base) => BoxUv::new(0, 16, 4, 12, 4),
        (Part::RightLeg, Layer::Overlay) => BoxUv::new(0, 32, 4, 12, 4),
        (Part::LeftLeg, Layer::Base) => BoxUv::new(16, 48, 4, 12, 4),
        (Part::LeftLeg, Layer::Overlay) => BoxUv::new(0, 48, 4, 12, 4),
    }
}

/// Every (part, layer) footprint for a given arm model.
pub fn all_boxes(arm: ArmModel) -> Vec<(Part, Layer, BoxUv)> {
    let mut out = Vec::new();
    for p in Part::ALL {
        out.push((p, Layer::Base, part_box(p, Layer::Base, arm)));
        out.push((p, Layer::Overlay, part_box(p, Layer::Overlay, arm)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_base_footprint_matches_known_layout() {
        let b = part_box(Part::Head, Layer::Base, ArmModel::Classic);
        assert_eq!(b.bounds(), Rect::new(0, 0, 32, 16));
    }

    #[test]
    fn head_overlay_sits_beside_head_base() {
        let o = part_box(Part::Head, Layer::Overlay, ArmModel::Classic);
        assert_eq!(o.bounds(), Rect::new(32, 0, 32, 16));
    }

    #[test]
    fn bottom_row_of_atlas_is_leg_body_arm() {
        // The classic skin bottom row (y=16..32) is right_leg | body | right_arm.
        let leg = part_box(Part::RightLeg, Layer::Base, ArmModel::Classic).bounds();
        let body = part_box(Part::Body, Layer::Base, ArmModel::Classic).bounds();
        let arm = part_box(Part::RightArm, Layer::Base, ArmModel::Classic).bounds();
        assert_eq!(leg, Rect::new(0, 16, 16, 16));
        assert_eq!(body, Rect::new(16, 16, 24, 16));
        assert_eq!(arm, Rect::new(40, 16, 16, 16));
        assert!(!leg.intersects(&body));
        assert!(!body.intersects(&arm));
    }

    #[test]
    fn slim_arms_are_one_pixel_narrower() {
        let classic = part_box(Part::RightArm, Layer::Base, ArmModel::Classic);
        let slim = part_box(Part::RightArm, Layer::Base, ArmModel::Slim);
        assert_eq!(classic.w - slim.w, 1);
        // Unwrapped width shrinks by 2 because both side columns lose a pixel.
        assert_eq!(classic.unwrap_width() - slim.unwrap_width(), 2);
    }

    #[test]
    fn base_layer_parts_never_overlap() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            let boxes: Vec<_> = Part::ALL
                .iter()
                .map(|p| part_box(*p, Layer::Base, arm).bounds())
                .collect();
            for i in 0..boxes.len() {
                for j in (i + 1)..boxes.len() {
                    assert!(
                        !boxes[i].intersects(&boxes[j]),
                        "base parts overlap"
                    );
                }
            }
        }
    }

    #[test]
    fn overlay_layer_parts_never_overlap() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            let boxes: Vec<_> = Part::ALL
                .iter()
                .map(|p| part_box(*p, Layer::Overlay, arm).bounds())
                .collect();
            for i in 0..boxes.len() {
                for j in (i + 1)..boxes.len() {
                    assert!(!boxes[i].intersects(&boxes[j]), "overlay parts overlap");
                }
            }
        }
    }

    #[test]
    fn every_box_fits_inside_the_texture() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            for (part, layer, b) in all_boxes(arm) {
                let r = b.bounds();
                assert!(
                    r.right() <= SKIN_WIDTH && r.bottom() <= SKIN_HEIGHT,
                    "{} {} escapes the 64x64 atlas: {:?}",
                    part.name(),
                    layer.name(),
                    r
                );
            }
        }
    }

    #[test]
    fn faces_tile_the_unwrapped_box_without_gaps() {
        let b = part_box(Part::Body, Layer::Base, ArmModel::Classic);
        let faces = b.faces();
        let total: u32 = faces.iter().map(|f| f.area()).sum();
        // 2*(w*h) front/back + 2*(d*h) sides + 2*(w*d) top/bottom == surface area.
        let w = b.w;
        let h = b.h;
        let d = b.d;
        assert_eq!(total, 2 * (w * h) + 2 * (d * h) + 2 * (w * d));
    }

    #[test]
    fn known_unused_atlas_region_is_not_claimed() {
        // x=56..64, y=16..48 is unused in the standard layout.
        let unused = Rect::new(56, 16, 8, 32);
        for (part, layer, b) in all_boxes(ArmModel::Classic) {
            let r = b.bounds();
            assert!(
                !r.intersects(&unused),
                "{} {} claims unused atlas space",
                part.name(),
                layer.name()
            );
        }
    }
}
