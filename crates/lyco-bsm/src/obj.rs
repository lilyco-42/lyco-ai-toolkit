//! A Wavefront OBJ reader, for meshes other tools hand us.
//!
//! ## Why this exists
//!
//! `convert` could only turn a GLB into Bedrock geometry. That is a gap, because
//! the tools that make 3D assets - and the exporters that pull them out of game
//! clients - very often emit OBJ, sometimes *only* OBJ.
//!
//! ## What the real files look like
//!
//! The quirks below are properties of real exports, not of the OBJ grammar, and
//! they were learned by reading how `sganggs/Stronghold-Protocol` (GPL-3.0)
//! consumes the OBJ its UnityPy extractor writes. No code was copied from it:
//! this project is MIT.
//!
//!   * `v` may carry a colour after the position - `v x y z r g b [a]`. That is a
//!     widespread extension, and UnityPy's exporter uses it, so a reader that
//!     only takes three floats loses every colour in the file.
//!   * Faces are not always triangles. They must be fanned, not rejected.
//!   * Indices may be negative, meaning "counted back from the end so far", which
//!     is why they cannot be resolved until the whole file is read.
//!   * A face corner is a `(v, vt, vn)` triplet, and two corners that agree on all
//!     three are the same vertex however many times they appear. Exports repeat
//!     them constantly.

use anyhow::{bail, Context, Result};

/// One named run of triangles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    /// Index of the first triangle in this group.
    pub start: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjMesh {
    /// De-duplicated positions, three floats each.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals, present only when every corner had one.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Per-vertex texture coordinates, present only when every corner had them.
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Per-vertex colours as RGBA in 0..1, present only when the file has them.
    pub colors: Option<Vec<[f32; 4]>>,
    /// Triangle indices into the arrays above.
    pub indices: Vec<u32>,
    pub groups: Vec<Group>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl ObjMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Split a vertex reference like `v/vt/vn`, `v//vn` or `v`.
///
/// A negative index counts back from however many have been seen so far, so the
/// caller passes the counts in.
fn corner_index(
    token: &str,
    seen: (usize, usize, usize),
) -> (Option<usize>, Option<usize>, Option<usize>) {
    let mut parts = token.split('/');
    let resolve = |raw: Option<&str>, count: usize| -> Option<usize> {
        let text = raw?.trim();
        if text.is_empty() {
            return None;
        }
        let value = text.parse::<i64>().ok()?;
        if value > 0 {
            Some(value as usize - 1)
        } else if value < 0 {
            let back = count as i64 + value;
            (back >= 0).then_some(back as usize)
        } else {
            None
        }
    };
    (
        resolve(parts.next(), seen.0),
        resolve(parts.next(), seen.1),
        resolve(parts.next(), seen.2),
    )
}

/// Read an OBJ file.
pub fn parse_obj(text: &str) -> Result<ObjMesh> {
    let mut raw_positions: Vec<[f32; 3]> = Vec::new();
    let mut raw_colors: Vec<[f32; 4]> = Vec::new();
    let mut raw_uvs: Vec<[f32; 2]> = Vec::new();
    let mut raw_normals: Vec<[f32; 3]> = Vec::new();

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // A corner is the same vertex only when all three of its parts agree.
    let mut seen: std::collections::HashMap<(usize, usize, usize), u32> =
        std::collections::HashMap::new();

    let mut groups: Vec<Group> = Vec::new();
    let mut group = Group {
        name: "default".to_string(),
        start: 0,
        count: 0,
    };
    // Whether every corner so far carried a uv and a normal. A file that mixes
    // them is left without the attribute rather than given a wrong one.
    let mut every_corner_has_uv = true;
    let mut every_corner_has_normal = true;
    let mut any_face = false;

    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let keyword = parts.next().unwrap_or("");
        let at = number + 1;

        match keyword {
            "v" => {
                let values: Vec<f32> = parts
                    .map(|p| {
                        p.parse::<f32>()
                            .context("a vertex coordinate is not a number")
                    })
                    .collect::<Result<_>>()?;
                if values.len() < 3 {
                    bail!(
                        "line {at}: a vertex needs three coordinates, got {}",
                        values.len()
                    );
                }
                raw_positions.push([values[0], values[1], values[2]]);
                // `v x y z r g b [a]`, the extension UnityPy writes.
                raw_colors.push(if values.len() >= 6 {
                    [
                        values[3],
                        values[4],
                        values[5],
                        *values.get(6).unwrap_or(&1.0),
                    ]
                } else {
                    [1.0, 1.0, 1.0, 1.0]
                });
            }
            "vt" => {
                let values: Vec<f32> = parts.filter_map(|p| p.parse::<f32>().ok()).collect();
                if values.len() < 2 {
                    bail!("line {at}: a texture coordinate needs two numbers");
                }
                raw_uvs.push([values[0], values[1]]);
            }
            "vn" => {
                let values: Vec<f32> = parts.filter_map(|p| p.parse::<f32>().ok()).collect();
                if values.len() < 3 {
                    bail!("line {at}: a normal needs three numbers");
                }
                raw_normals.push([values[0], values[1], values[2]]);
            }
            "g" | "o" => {
                // Close the previous group and open a new one. `o` is an object
                // name, but for our purposes it groups the same way.
                if group.count > 0 {
                    groups.push(group);
                }
                let name = parts.collect::<Vec<_>>().join(" ");
                group = Group {
                    name: if name.is_empty() {
                        "default".to_string()
                    } else {
                        name
                    },
                    start: indices.len() / 3,
                    count: 0,
                };
            }
            "f" => {
                let seen_counts = (raw_positions.len(), raw_uvs.len(), raw_normals.len());
                let corners: Vec<(usize, Option<usize>, Option<usize>)> = parts
                    .map(|token| corner_index(token, seen_counts))
                    .filter_map(|(v, t, n)| v.map(|v| (v, t, n)))
                    .collect();
                if corners.len() < 3 {
                    bail!("line {at}: a face needs at least three vertices");
                }
                any_face = true;

                // Fan a polygon: (0,1,2), (0,2,3), ... which is correct for the
                // convex quads and n-gons these exporters produce.
                let mut emit = |corner: (usize, Option<usize>, Option<usize>)| -> Result<u32> {
                    if corner.0 >= raw_positions.len() {
                        bail!("line {at}: vertex {} does not exist", corner.0 + 1);
                    }
                    let key = (
                        corner.0,
                        corner.1.map(|t| t + 1).unwrap_or(0),
                        corner.2.map(|n| n + 1).unwrap_or(0),
                    );
                    // A corner that names no uv, or names one the file never
                    // defined, means the attribute cannot be filled honestly.
                    // Real exports do the second constantly: UnityPy's OBJ
                    // writer emits `f v/vt/vn` with no `vn` lines at all.
                    if corner.1.is_none_or(|t| t >= raw_uvs.len()) {
                        every_corner_has_uv = false;
                    }
                    if corner.2.is_none_or(|n| n >= raw_normals.len()) {
                        every_corner_has_normal = false;
                    }
                    if let Some(existing) = seen.get(&key) {
                        return Ok(*existing);
                    }
                    let id = positions.len() as u32;
                    positions.push(raw_positions[corner.0]);
                    uvs.push(
                        corner
                            .1
                            .and_then(|t| raw_uvs.get(t))
                            .copied()
                            .unwrap_or([0.0, 0.0]),
                    );
                    normals.push(
                        corner
                            .2
                            .and_then(|n| raw_normals.get(n))
                            .copied()
                            .unwrap_or([0.0, 1.0, 0.0]),
                    );
                    colors.push(raw_colors.get(corner.0).copied().unwrap_or([1.0; 4]));
                    seen.insert(key, id);
                    Ok(id)
                };

                for i in 1..corners.len() - 1 {
                    for corner in [corners[0], corners[i], corners[i + 1]] {
                        indices.push(emit(corner)?);
                    }
                    group.count += 1;
                }
            }
            // `usemtl`, `s`, `mtllib` and anything else do not affect geometry.
            _ => {}
        }
    }

    if !any_face {
        bail!("no faces found, so this is not a mesh");
    }
    if group.count > 0 {
        groups.push(group);
    }

    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for position in &positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }

    let has_colors = raw_colors.iter().any(|c| *c != [1.0; 4]);
    Ok(ObjMesh {
        positions,
        normals: every_corner_has_normal.then_some(normals),
        uvs: every_corner_has_uv.then_some(uvs),
        colors: has_colors.then_some(colors),
        indices,
        groups,
        min,
        max,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRIANGLE: &str =
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1\n";

    #[test]
    fn a_plain_triangle_parses() {
        let mesh = parse_obj(TRIANGLE).expect("parse");
        assert_eq!(mesh.positions.len(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.indices, vec![0, 1, 2]);
        assert_eq!(mesh.min, [0.0, 0.0, 0.0]);
        assert_eq!(mesh.max, [1.0, 1.0, 0.0]);
        assert!(mesh.uvs.is_some(), "every corner had a uv");
        assert!(mesh.normals.is_some(), "every corner had a normal");
        assert!(mesh.colors.is_none(), "the file carries no colour");
    }

    /// Faces are not always triangles, and rejecting them loses real meshes.
    #[test]
    fn a_quad_is_fanned_into_two_triangles() {
        let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        let mesh = parse_obj(text).expect("parse");
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn a_pentagon_becomes_three_triangles() {
        let text = "v 0 0 0\nv 1 0 0\nv 2 1 0\nv 1 2 0\nv 0 1 0\nf 1 2 3 4 5\n";
        assert_eq!(parse_obj(text).expect("parse").triangle_count(), 3);
    }

    /// Negative indices count back from what has been read so far, which is why
    /// they cannot be resolved until the whole file is in hand.
    #[test]
    fn negative_indices_count_backwards() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n";
        assert_eq!(parse_obj(text).expect("parse").indices, vec![0, 1, 2]);
    }

    /// `v x y z r g b [a]` is a widespread extension and UnityPy's exporter
    /// writes it; a reader that stops at three floats loses every colour.
    #[test]
    fn vertex_colours_are_read_when_present() {
        let text = "v 0 0 0 1 0 0\nv 1 0 0 0 1 0\nv 0 1 0 0 0 1\nf 1 2 3\n";
        let mesh = parse_obj(text).expect("parse");
        let colors = mesh.colors.as_ref().expect("colours");
        assert_eq!(colors[0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(colors[2], [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_face_without_uvs_leaves_the_attribute_out_rather_than_zeroing_it() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1//1 2//1 3//1\n";
        let mesh = parse_obj(text).expect("parse");
        assert!(mesh.uvs.is_none(), "there were no texture coordinates");
        assert!(mesh.normals.is_some());
    }

    #[test]
    fn groups_are_recorded_with_their_triangle_runs() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\ng first\nf 1 2 3\ng second\nf 2 4 3\n";
        let mesh = parse_obj(text).expect("parse");
        assert_eq!(mesh.groups.len(), 2);
        assert_eq!(mesh.groups[0].name, "first");
        assert_eq!(mesh.groups[0].start, 0);
        assert_eq!(mesh.groups[0].count, 1);
        assert_eq!(mesh.groups[1].name, "second");
        assert_eq!(mesh.groups[1].start, 1);
    }

    /// Exports repeat corners constantly; two corners that agree on position,
    /// uv and normal are one vertex however many times they appear.
    #[test]
    fn repeated_corners_become_one_vertex() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nf 1 2 3\n";
        let mesh = parse_obj(text).expect("parse");
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.positions.len(), 3, "the second face reuses all three");
        assert_eq!(mesh.indices, vec![0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn comments_and_unknown_statements_are_ignored() {
        let text = "# c\nmtllib thing.mtl\nusemtl mat\ns 1\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        assert_eq!(parse_obj(text).expect("parse").triangle_count(), 1);
    }

    /// The shape a real UnityPy export has: `f v/vt/vn` corners with no `vn`
    /// lines anywhere. An earlier version trusted the index and reported
    /// normals that were all the fallback, which is worse than reporting none.
    #[test]
    fn a_dangling_normal_index_drops_the_attribute_instead_of_faking_it() {
        let text = "g m\nv 0 0 0\nv 1 0 0\nv 0 1 0\n\
vt 0 0\nvt 1 0\nvt 0 1\nf 3/3/3 2/2/2 1/1/1\n";
        let mesh = parse_obj(text).expect("parse");
        assert!(mesh.normals.is_none(), "the file defines no normals");
        assert!(mesh.uvs.is_some(), "but it does define uvs");
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.groups.len(), 1);
        assert_eq!(mesh.groups[0].name, "m");
    }

    #[test]
    fn a_dangling_uv_index_drops_that_attribute_too() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1/9/1 2/9/1 3/9/1\n";
        let mesh = parse_obj(text).expect("parse");
        assert!(mesh.uvs.is_none(), "uv 9 was never defined");
        assert!(mesh.normals.is_some());
    }

    #[test]
    fn a_file_with_no_faces_is_refused() {
        let err = parse_obj("v 0 0 0\nv 1 0 0\n").unwrap_err();
        assert!(err.to_string().contains("no faces"), "{err}");
    }

    #[test]
    fn a_face_naming_a_vertex_that_does_not_exist_is_refused() {
        let err = parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n").unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    #[test]
    fn a_short_vertex_is_refused_with_the_line_number() {
        let err = parse_obj("v 0 0\n").unwrap_err().to_string();
        assert!(err.contains("line 1"), "{err}");
    }
}
