//! Bedrock geometry (`.geo.json`) data model, writer and validator.
//!
//! Field names and shapes were taken from real files in Mojang's own
//! `bedrock-samples` resource pack (`resource_pack/models/entity/*.geo.json`),
//! not from memory. Conventions worth knowing:
//!
//!   * `origin` is the cube's minimum corner in its bone's space, `size` its extent.
//!   * A bone may carry a `pivot` with no cubes at all - that is how the vanilla
//!     armor stand builds an empty `waist` joint between `baseplate` and `body`.
//!   * `uv` is either a `[u, v]` pair (box unwrap) or a per-face object keyed by
//!     `north` / `south` / `east` / `west` / `up` / `down`.
//!   * `size` components may be `0.0`; the vanilla arrow uses flat planes.
//!   * Y is up and one unit is 1/16 of a block.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The geometry format version used by every vanilla sample inspected.
pub const FORMAT_VERSION: &str = "1.12.0";

/// The six per-face keys accepted by a per-face `uv` object.
pub const FACES: [&str; 6] = ["north", "south", "east", "west", "up", "down"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryFile {
    pub format_version: String,
    #[serde(rename = "minecraft:geometry")]
    pub geometry: Vec<Geometry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub description: Description,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bones: Vec<Bone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Description {
    pub identifier: String,
    pub texture_width: f32,
    pub texture_height: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_bounds_offset: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_bounds_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_bounds_height: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inflate: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cubes: Vec<Cube>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cube {
    pub origin: [f32; 3],
    pub size: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inflate: Option<f32>,
    pub uv: Uv,
}

/// A cube's UV mapping: either a box-unwrap pair or a per-face object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Uv {
    /// `"uv": [u, v]` - the box unwrap derived from `size`.
    Box([f32; 2]),
    /// `"uv": { "east": { "uv": [u, v] }, ... }` - explicit per face.
    PerFace(BTreeMap<String, FaceUv>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceUv {
    pub uv: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_size: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_instance: Option<String>,
}

/// A structural problem found by [`Geometry::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    pub message: String,
}

impl GeometryFile {
    /// Wrap one geometry in a complete file at the current format version.
    pub fn new(geometry: Geometry) -> Self {
        GeometryFile {
            format_version: FORMAT_VERSION.to_string(),
            geometry: vec![geometry],
        }
    }

    /// Serialize the way Blockbench and Mojang write it: pretty, tab-free JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Validate every geometry in the file.
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.geometry.is_empty() {
            problems.push(Problem {
                code: "no-geometry",
                message: "file contains no minecraft:geometry entries".to_string(),
            });
        }
        for g in &self.geometry {
            g.validate_into(&mut problems);
        }
        problems
    }
}

impl Geometry {
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        self.validate_into(&mut problems);
        problems
    }

    fn validate_into(&self, problems: &mut Vec<Problem>) {
        let id = &self.description.identifier;
        if !id.starts_with("geometry.") {
            problems.push(Problem {
                code: "bad-identifier",
                message: format!("identifier '{id}' must start with 'geometry.'"),
            });
        }
        if !(self.description.texture_width > 0.0) || !(self.description.texture_height > 0.0) {
            problems.push(Problem {
                code: "bad-texture-size",
                message: format!(
                    "texture size must be positive, got {}x{}",
                    self.description.texture_width, self.description.texture_height
                ),
            });
        }

        // Bone names must be unique, because parents reference them by name.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for bone in &self.bones {
            if bone.name.trim().is_empty() {
                problems.push(Problem {
                    code: "empty-bone-name",
                    message: "a bone has an empty name".to_string(),
                });
            } else if !seen.insert(bone.name.as_str()) {
                problems.push(Problem {
                    code: "duplicate-bone",
                    message: format!("bone '{}' is defined more than once", bone.name),
                });
            }
        }

        let by_name: BTreeMap<&str, &Bone> =
            self.bones.iter().map(|b| (b.name.as_str(), b)).collect();

        for bone in &self.bones {
            if let Some(parent) = &bone.parent {
                if !by_name.contains_key(parent.as_str()) {
                    problems.push(Problem {
                        code: "missing-parent",
                        message: format!("bone '{}' names unknown parent '{parent}'", bone.name),
                    });
                }
                if parent == &bone.name {
                    problems.push(Problem {
                        code: "self-parent",
                        message: format!("bone '{}' is its own parent", bone.name),
                    });
                }
            }

            // Walk the parent chain; a repeated name means a cycle.
            let mut walked: BTreeSet<&str> = BTreeSet::new();
            let mut cursor = Some(bone.name.as_str());
            while let Some(current) = cursor {
                if !walked.insert(current) {
                    problems.push(Problem {
                        code: "parent-cycle",
                        message: format!("bone '{}' is part of a parent cycle", bone.name),
                    });
                    break;
                }
                cursor = by_name
                    .get(current)
                    .and_then(|b| b.parent.as_deref());
            }

            for (i, cube) in bone.cubes.iter().enumerate() {
                if cube.size.iter().any(|s| *s < 0.0) {
                    problems.push(Problem {
                        code: "negative-size",
                        message: format!(
                            "bone '{}' cube {i} has a negative size component",
                            bone.name
                        ),
                    });
                }
                if cube.size.iter().any(|s| !s.is_finite())
                    || cube.origin.iter().any(|s| !s.is_finite())
                {
                    problems.push(Problem {
                        code: "non-finite",
                        message: format!("bone '{}' cube {i} has a non-finite value", bone.name),
                    });
                }
                if let Uv::PerFace(faces) = &cube.uv {
                    for key in faces.keys() {
                        if !FACES.contains(&key.as_str()) {
                            problems.push(Problem {
                                code: "unknown-face",
                                message: format!(
                                    "bone '{}' cube {i} uses unknown face '{key}'",
                                    bone.name
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Total cube count across all bones.
    pub fn cube_count(&self) -> usize {
        self.bones.iter().map(|b| b.cubes.len()).sum()
    }
}

impl Bone {
    pub fn new(name: impl Into<String>) -> Self {
        Bone {
            name: name.into(),
            parent: None,
            pivot: None,
            rotation: None,
            mirror: None,
            inflate: None,
            cubes: Vec::new(),
        }
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn with_pivot(mut self, pivot: [f32; 3]) -> Self {
        self.pivot = Some(pivot);
        self
    }

    pub fn with_cube(mut self, cube: Cube) -> Self {
        self.cubes.push(cube);
        self
    }
}

impl Cube {
    /// A cube with a box-unwrap UV.
    pub fn boxed(origin: [f32; 3], size: [f32; 3], uv: [f32; 2]) -> Self {
        Cube {
            origin,
            size,
            rotation: None,
            pivot: None,
            inflate: None,
            uv: Uv::Box(uv),
        }
    }

    /// A cube with explicit per-face UVs.
    pub fn per_face(origin: [f32; 3], size: [f32; 3], faces: BTreeMap<String, FaceUv>) -> Self {
        Cube {
            origin,
            size,
            rotation: None,
            pivot: None,
            inflate: None,
            uv: Uv::PerFace(faces),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed but faithful copy of `resource_pack/models/entity/armor_stand.geo.json`.
    const ARMOR_STAND: &str = r#"
{
  "format_version": "1.12.0",
  "minecraft:geometry": [
    {
      "description": {
        "identifier": "geometry.armor_stand",
        "texture_width": 64.0,
        "texture_height": 64.0,
        "visible_bounds_offset": [0.0, 0.5, 0.0],
        "visible_bounds_width": 1.5
      },
      "bones": [
        {
          "name": "baseplate",
          "cubes": [
            { "origin": [-6.0, 0.0, -6.0], "size": [12.0, 1.0, 12.0], "uv": [0.0, 32.0] }
          ]
        },
        { "name": "waist", "parent": "baseplate", "pivot": [0.0, 12.0, 0.0] },
        {
          "name": "body",
          "parent": "waist",
          "pivot": [0.0, 24.0, 0.0],
          "cubes": [
            { "origin": [-6.0, 21.0, -1.5], "size": [12.0, 3.0, 3.0], "uv": [0.0, 26.0] }
          ]
        },
        {
          "name": "leftarm",
          "parent": "body",
          "mirror": true,
          "pivot": [5.0, 22.0, 0.0],
          "cubes": [
            { "origin": [5.0, 12.0, -1.0], "size": [2.0, 10.0, 2.0],
              "uv": { "east": { "uv": [40.0, 16.0] } } }
          ]
        }
      ]
    }
  ]
}
"#;

    #[test]
    fn parses_a_real_vanilla_sample() {
        let file = GeometryFile::from_json(ARMOR_STAND).expect("parse vanilla sample");
        assert_eq!(file.format_version, "1.12.0");
        assert_eq!(file.geometry.len(), 1);
        let g = &file.geometry[0];
        assert_eq!(g.description.identifier, "geometry.armor_stand");
        assert_eq!(g.description.texture_width, 64.0);
        assert_eq!(g.description.visible_bounds_offset, Some([0.0, 0.5, 0.0]));
        assert_eq!(g.bones.len(), 4);
        assert!(g.validate().is_empty(), "{:?}", g.validate());
    }

    #[test]
    fn keeps_a_cube_less_joint_bone() {
        let file = GeometryFile::from_json(ARMOR_STAND).unwrap();
        let waist = file.geometry[0]
            .bones
            .iter()
            .find(|b| b.name == "waist")
            .expect("waist bone");
        assert!(waist.cubes.is_empty());
        assert_eq!(waist.pivot, Some([0.0, 12.0, 0.0]));
        assert_eq!(waist.parent.as_deref(), Some("baseplate"));
    }

    #[test]
    fn reads_both_uv_shapes() {
        let file = GeometryFile::from_json(ARMOR_STAND).unwrap();
        let bones = &file.geometry[0].bones;
        let plate = bones.iter().find(|b| b.name == "baseplate").unwrap();
        assert_eq!(plate.cubes[0].uv, Uv::Box([0.0, 32.0]));

        let arm = bones.iter().find(|b| b.name == "leftarm").unwrap();
        match &arm.cubes[0].uv {
            Uv::PerFace(faces) => {
                assert_eq!(faces.len(), 1);
                assert_eq!(faces["east"].uv, [40.0, 16.0]);
            }
            other => panic!("expected per-face uv, got {other:?}"),
        }
        assert_eq!(arm.mirror, Some(true));
    }

    #[test]
    fn round_trips_through_json() {
        let file = GeometryFile::from_json(ARMOR_STAND).unwrap();
        let text = file.to_json().expect("serialize");
        let again = GeometryFile::from_json(&text).expect("re-parse");
        assert_eq!(file, again, "JSON round trip must be lossless");
    }

    #[test]
    fn writer_emits_the_keys_the_game_expects() {
        let file = GeometryFile::from_json(ARMOR_STAND).unwrap();
        let text = file.to_json().unwrap();
        assert!(text.contains("\"format_version\""), "{text}");
        assert!(text.contains("\"minecraft:geometry\""));
        assert!(text.contains("\"texture_width\""));
        assert!(text.contains("\"identifier\""));
        // Absent optional fields must not be written as null.
        assert!(!text.contains("null"), "{text}");
        assert!(!text.contains("rotation"), "no cube declares rotation");
    }

    #[test]
    fn builds_a_model_programmatically() {
        let g = Geometry {
            description: Description {
                identifier: "geometry.lyco_demo".to_string(),
                texture_width: 64.0,
                texture_height: 64.0,
                visible_bounds_offset: None,
                visible_bounds_width: None,
                visible_bounds_height: None,
            },
            bones: vec![
                {
                    let torso = Cube::boxed([-4.0, 0.0, -2.0], [8.0, 12.0, 4.0], [16.0, 16.0]);
                    Bone::new("root").with_cube(torso)
                },
                {
                    let skull = Cube::boxed([-4.0, 12.0, -4.0], [8.0, 8.0, 8.0], [0.0, 0.0]);
                    Bone::new("head")
                        .with_parent("root")
                        .with_pivot([0.0, 12.0, 0.0])
                        .with_cube(skull)
                },
            ],
        };
        assert!(g.validate().is_empty(), "{:?}", g.validate());
        assert_eq!(g.cube_count(), 2);
        let text = GeometryFile::new(g).to_json().unwrap();
        assert!(text.contains("geometry.lyco_demo"));
    }

    #[test]
    fn rejects_a_non_geometry_identifier() {
        let mut g = Geometry {
            description: Description {
                identifier: "my_model".to_string(),
                texture_width: 64.0,
                texture_height: 64.0,
                visible_bounds_offset: None,
                visible_bounds_width: None,
                visible_bounds_height: None,
            },
            bones: Vec::new(),
        };
        let codes: Vec<_> = g.validate().iter().map(|p| p.code).collect();
        assert!(codes.contains(&"bad-identifier"), "{codes:?}");
        g.description.identifier = "geometry.ok".to_string();
        assert!(g.validate().is_empty());
    }

    #[test]
    fn rejects_a_missing_parent() {
        let g = Geometry {
            description: Description {
                identifier: "geometry.x".to_string(),
                texture_width: 64.0,
                texture_height: 64.0,
                visible_bounds_offset: None,
                visible_bounds_width: None,
                visible_bounds_height: None,
            },
            bones: vec![Bone::new("arm").with_parent("torso")],
        };
        assert!(g.validate().iter().any(|p| p.code == "missing-parent"));
    }

    #[test]
    fn detects_a_parent_cycle() {
        let g = Geometry {
            description: Description {
                identifier: "geometry.x".to_string(),
                texture_width: 64.0,
                texture_height: 64.0,
                visible_bounds_offset: None,
                visible_bounds_width: None,
                visible_bounds_height: None,
            },
            bones: vec![
                Bone::new("a").with_parent("b"),
                Bone::new("b").with_parent("a"),
            ],
        };
        assert!(g.validate().iter().any(|p| p.code == "parent-cycle"));
    }

    #[test]
    fn detects_duplicate_bone_and_bad_face_and_negative_size() {
        let mut faces = BTreeMap::new();
        faces.insert(
            "sideways".to_string(),
            FaceUv {
                uv: [0.0, 0.0],
                uv_size: None,
                rotation: None,
                material_instance: None,
            },
        );
        let g = Geometry {
            description: Description {
                identifier: "geometry.x".to_string(),
                texture_width: 64.0,
                texture_height: 64.0,
                visible_bounds_offset: None,
                visible_bounds_width: None,
                visible_bounds_height: None,
            },
            bones: vec![
                Bone::new("dup").with_cube(Cube::per_face([0.0; 3], [-1.0, 1.0, 1.0], faces)),
                Bone::new("dup"),
            ],
        };
        let codes: Vec<_> = g.validate().iter().map(|p| p.code).collect();
        assert!(codes.contains(&"duplicate-bone"), "{codes:?}");
        assert!(codes.contains(&"unknown-face"), "{codes:?}");
        assert!(codes.contains(&"negative-size"), "{codes:?}");
    }

    #[test]
    fn empty_file_is_reported() {
        let file = GeometryFile {
            format_version: FORMAT_VERSION.to_string(),
            geometry: Vec::new(),
        };
        assert_eq!(file.validate()[0].code, "no-geometry");
    }
}
