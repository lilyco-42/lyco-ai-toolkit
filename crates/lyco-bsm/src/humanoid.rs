//! Standard Minecraft player model as Bedrock geometry (F2).
//!
//! This is where F1 and F2 meet: the player model's UVs *are* the 64x64 skin
//! layout, so every cube's box UV is taken straight from
//! [`lyco_mc_skin::part_box`] instead of being duplicated here.
//!
//! Why box UV rather than per-face UV: a Bedrock box UV `[u, v]` on a cube of
//! size `[w, h, d]` unwraps exactly the way the Minecraft skin layout does.
//! Verified against Mojang's own `armor_stand.geo.json`, whose `hat` bone is
//! `size [8,8,8]` with `uv [32,0]` - and `(32, 0)` is precisely the head
//! overlay anchor of the skin atlas, whose unwrap is 32x16. Using box UV also
//! sidesteps any need to assert how Bedrock's north/south/east/west map onto
//! the skin's front/back/left/right.
//!
//! Model space: Y is up, feet at `y = 0`, one unit is 1/16 block. A player is
//! 32 units tall: legs 0..12, body 12..24, head 24..32.

use crate::bedrock::{Bone, Cube, Description, Geometry, GeometryFile};
use crate::glb::Glb;
use lyco_mc_skin::{part_box, ArmModel, Layer, Part};

/// Height of a player model in model units.
pub const PLAYER_HEIGHT: f32 = 32.0;
/// How far the hat layer sits above the head surface.
pub const HAT_INFLATE: f32 = 0.5;
/// How far the jacket / sleeve / trouser layer sits above the body surface.
pub const OVERLAY_INFLATE: f32 = 0.25;

/// One bone's geometry description, independent of the skin anchors.
struct BoneSpec {
    name: &'static str,
    part: Part,
    parent: Option<&'static str>,
    pivot: [f32; 3],
    origin: [f32; 3],
    overlay_inflate: f32,
}

/// The six bones of a player, in parent-before-child order.
fn specs(arm: ArmModel) -> Vec<BoneSpec> {
    let aw = arm.arm_width() as f32;
    // Classic arms are 4 wide and start at x = -8; slim arms are 3 wide and
    // start at x = -7, keeping the outer edge aligned at -8.
    let right_arm_x = -4.0 - aw;
    let left_arm_x = 4.0;
    vec![
        BoneSpec {
            name: "body",
            part: Part::Body,
            parent: None,
            pivot: [0.0, 12.0, 0.0],
            origin: [-4.0, 12.0, -2.0],
            overlay_inflate: OVERLAY_INFLATE,
        },
        BoneSpec {
            name: "head",
            part: Part::Head,
            parent: Some("body"),
            pivot: [0.0, 24.0, 0.0],
            origin: [-4.0, 24.0, -4.0],
            overlay_inflate: HAT_INFLATE,
        },
        BoneSpec {
            name: "right_arm",
            part: Part::RightArm,
            parent: Some("body"),
            pivot: [-5.0, 22.0, 0.0],
            origin: [right_arm_x, 12.0, -2.0],
            overlay_inflate: OVERLAY_INFLATE,
        },
        BoneSpec {
            name: "left_arm",
            part: Part::LeftArm,
            parent: Some("body"),
            pivot: [5.0, 22.0, 0.0],
            origin: [left_arm_x, 12.0, -2.0],
            overlay_inflate: OVERLAY_INFLATE,
        },
        BoneSpec {
            name: "right_leg",
            part: Part::RightLeg,
            parent: Some("body"),
            pivot: [-2.0, 12.0, 0.0],
            origin: [-4.0, 0.0, -2.0],
            overlay_inflate: OVERLAY_INFLATE,
        },
        BoneSpec {
            name: "left_leg",
            part: Part::LeftLeg,
            parent: Some("body"),
            pivot: [2.0, 12.0, 0.0],
            origin: [0.0, 0.0, -2.0],
            overlay_inflate: OVERLAY_INFLATE,
        },
    ]
}

fn size_of(part: Part) -> [f32; 3] {
    match part {
        Part::Head => [8.0, 8.0, 8.0],
        Part::Body => [8.0, 12.0, 4.0],
        Part::RightArm | Part::LeftArm => [4.0, 12.0, 4.0],
        Part::RightLeg | Part::LeftLeg => [4.0, 12.0, 4.0],
    }
}

/// UV anchor for a part/layer, read from the skin layout.
fn anchor(part: Part, layer: Layer, arm: ArmModel) -> [f32; 2] {
    let b = part_box(part, layer, arm);
    [b.u as f32, b.v as f32]
}

/// Build the standard player geometry for the given arm model.
///
/// Each bone carries a base cube plus an overlay cube (hat / jacket / sleeve /
/// trousers) matching the skin's second layer, so a 64x64 skin with an overlay
/// layer renders completely.
pub fn player_geometry(identifier: &str, arm: ArmModel) -> Geometry {
    let mut bones = Vec::new();

    for spec in specs(arm) {
        let size = size_of(spec.part);
        // Slim arms are 3 wide even though size_of reports the classic 4.
        let size = if matches!(spec.part, Part::RightArm | Part::LeftArm) {
            [arm.arm_width() as f32, size[1], size[2]]
        } else {
            size
        };

        let base = Cube::boxed(spec.origin, size, anchor(spec.part, Layer::Base, arm));
        let mut overlay =
            Cube::boxed(spec.origin, size, anchor(spec.part, Layer::Overlay, arm));
        overlay.inflate = Some(spec.overlay_inflate);

        let mut bone = Bone::new(spec.name).with_pivot(spec.pivot);
        if let Some(parent) = spec.parent {
            bone = bone.with_parent(parent);
        }
        bone = bone.with_cube(base).with_cube(overlay);
        bones.push(bone);
    }

    Geometry {
        description: Description {
            identifier: identifier.to_string(),
            texture_width: 64.0,
            texture_height: 64.0,
            visible_bounds_offset: Some([0.0, 1.0, 0.0]),
            visible_bounds_width: Some(2.0),
            visible_bounds_height: Some(2.5),
        },
        bones,
    }
}

/// Build a complete `.geo.json` file for the standard player model.
pub fn player_file(identifier: &str, arm: ArmModel) -> GeometryFile {
    GeometryFile::new(player_geometry(identifier, arm))
}

/// Placeholder for F3: turn a decoded GLB mesh into Bedrock boxes.
///
/// Not implemented. Converting a triangle soup into axis-aligned boxes is
/// voxelisation plus box fitting, and it is the open algorithmic problem in
/// this crate - so it fails loudly rather than returning something wrong.
pub fn boxes_from_glb(_glb: &Glb) -> anyhow::Result<Geometry> {
    anyhow::bail!(
        "GLB to Bedrock geometry is not implemented yet: it needs voxelisation \
         and box fitting, tracked as S2d in _RESEARCH/04-validation-checklist.md"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glb::Gltf;
    use lyco_mc_skin::Face;

    #[test]
    fn player_geometry_is_valid_for_both_arm_models() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            let g = player_geometry("geometry.lyco_player", arm);
            let problems = g.validate();
            assert!(problems.is_empty(), "{arm:?}: {problems:?}");
        }
    }

    #[test]
    fn has_six_bones_twelve_cubes() {
        let g = player_geometry("geometry.lyco_player", ArmModel::Classic);
        assert_eq!(g.bones.len(), 6);
        assert_eq!(g.cube_count(), 12, "each bone has a base and an overlay cube");
    }

    #[test]
    fn bone_hierarchy_hangs_off_body() {
        let g = player_geometry("geometry.lyco_player", ArmModel::Classic);
        let body = g.bones.iter().find(|b| b.name == "body").unwrap();
        assert_eq!(body.parent, None, "body is the root");
        for name in ["head", "right_arm", "left_arm", "right_leg", "left_leg"] {
            let b = g.bones.iter().find(|b| b.name == name).unwrap();
            assert_eq!(b.parent.as_deref(), Some("body"), "{name}");
        }
    }

    /// The whole point of this module: the model's UVs must be the skin layout.
    #[test]
    fn every_box_uv_matches_the_skin_anchor_table() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            let g = player_geometry("geometry.lyco_player", arm);
            for spec in specs(arm) {
                let bone = g.bones.iter().find(|b| b.name == spec.name).unwrap();
                let expect_base = anchor(spec.part, Layer::Base, arm);
                let expect_overlay = anchor(spec.part, Layer::Overlay, arm);
                assert_eq!(
                    bone.cubes[0].uv,
                    crate::bedrock::Uv::Box(expect_base),
                    "{} base",
                    spec.name
                );
                assert_eq!(
                    bone.cubes[1].uv,
                    crate::bedrock::Uv::Box(expect_overlay),
                    "{} overlay",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn base_uv_anchors_are_the_known_skin_layout() {
        // Independent restatement of the atlas so a change to the skin crate
        // cannot quietly move the player model's UVs.
        let g = player_geometry("geometry.lyco_player", ArmModel::Classic);
        let uv = |name: &str| match &g.bones.iter().find(|b| b.name == name).unwrap().cubes[0].uv {
            crate::bedrock::Uv::Box(pair) => *pair,
            other => panic!("expected box uv, got {other:?}"),
        };
        assert_eq!(uv("head"), [0.0, 0.0]);
        assert_eq!(uv("body"), [16.0, 16.0]);
        assert_eq!(uv("right_arm"), [40.0, 16.0]);
        assert_eq!(uv("left_arm"), [32.0, 48.0]);
        assert_eq!(uv("right_leg"), [0.0, 16.0]);
        assert_eq!(uv("left_leg"), [16.0, 48.0]);
    }

    #[test]
    fn overlay_uv_anchors_are_the_skin_second_layer() {
        let g = player_geometry("geometry.lyco_player", ArmModel::Classic);
        let uv = |name: &str| match &g.bones.iter().find(|b| b.name == name).unwrap().cubes[1].uv {
            crate::bedrock::Uv::Box(pair) => *pair,
            other => panic!("expected box uv, got {other:?}"),
        };
        assert_eq!(uv("head"), [32.0, 0.0]);
        assert_eq!(uv("body"), [16.0, 32.0]);
        assert_eq!(uv("right_arm"), [40.0, 32.0]);
        assert_eq!(uv("left_arm"), [48.0, 48.0]);
        assert_eq!(uv("right_leg"), [0.0, 32.0]);
        assert_eq!(uv("left_leg"), [0.0, 48.0]);
    }

    /// A Bedrock box UV unwraps like the skin atlas, so the per-face rects must
    /// land inside the region the anchor claims. This is the invariant that lets
    /// us skip per-face UVs entirely.
    #[test]
    fn box_uv_unwrap_stays_inside_the_skin_atlas() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            for (part, layer, b) in lyco_mc_skin::all_boxes(arm) {
                let bounds = b.bounds();
                assert!(
                    bounds.right() <= 64 && bounds.bottom() <= 64,
                    "{} {} escapes the atlas",
                    part.name(),
                    layer.name()
                );
                // And every face rect must sit inside that footprint.
                for face in Face::ALL {
                    let r = b.face(face);
                    assert!(
                        r.right() <= bounds.right() && r.bottom() <= bounds.bottom(),
                        "{} {} {} face escapes its own box",
                        part.name(),
                        layer.name(),
                        face.name()
                    );
                }
            }
        }
    }

    #[test]
    fn slim_arms_are_narrower_and_keep_the_outer_edge() {
        let classic = player_geometry("geometry.p", ArmModel::Classic);
        let slim = player_geometry("geometry.p", ArmModel::Slim);
        let arm = |g: &Geometry| {
            let b = g.bones.iter().find(|b| b.name == "right_arm").unwrap();
            (b.cubes[0].origin, b.cubes[0].size)
        };
        let (co, cs) = arm(&classic);
        let (so, ss) = arm(&slim);
        assert_eq!(cs[0], 4.0);
        assert_eq!(ss[0], 3.0);
        // Inner edges coincide; slim simply reaches one unit less far inward.
        assert_eq!(so[0] - co[0], 1.0, "slim origin shifts inward by one");
    }

    #[test]
    fn model_stands_feet_at_zero_and_head_at_thirty_two() {
        let g = player_geometry("geometry.p", ArmModel::Classic);
        let leg = g.bones.iter().find(|b| b.name == "right_leg").unwrap();
        assert_eq!(leg.cubes[0].origin[1], 0.0, "feet on the ground plane");
        let head = g.bones.iter().find(|b| b.name == "head").unwrap();
        let top = head.cubes[0].origin[1] + head.cubes[0].size[1];
        assert_eq!(top, PLAYER_HEIGHT);
    }

    #[test]
    fn writes_a_file_blockbench_can_read() {
        let file = player_file("geometry.lyco_player", ArmModel::Classic);
        assert!(file.validate().is_empty(), "{:?}", file.validate());
        let text = file.to_json().expect("serialize");
        let again = GeometryFile::from_json(&text).expect("re-parse");
        assert_eq!(file, again);
        assert!(text.contains("geometry.lyco_player"));
        assert!(text.contains("right_arm"));
    }

    #[test]
    fn glb_conversion_fails_loudly_instead_of_guessing() {
        // An empty but structurally valid Glb is enough: the point is that the
        // unimplemented conversion must refuse rather than invent boxes.
        let glb = Glb {
            gltf: Gltf::default(),
            bin: Vec::new(),
            version: 2,
        };
        let err = boxes_from_glb(&glb).unwrap_err();
        assert!(err.to_string().contains("not implemented"), "{err}");
    }
}
