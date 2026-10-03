//! Convert a GLB mesh into Bedrock geometry, with a texture generated for it.
//!
//! The pipeline is: walk the scene graph to world-space triangles, voxelise the
//! surface, merge voxels into boxes, then emit one Bedrock cube per box.
//!
//! Why a generated texture: Bedrock geometry carries no per-cube colour, so a
//! converted model with no texture renders as one flat shade - technically
//! valid, visually useless. Each merged box therefore gets its own cell in a
//! small atlas, and the cell is filled with that box's mean source colour.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

use crate::bedrock::{Bone, Cube, Description, FaceUv, Geometry, GeometryFile, Uv, FACES};
use crate::glb::Glb;
use crate::voxel::{
    box_color, merge_boxes, voxelize_surface, Box3, Grid, Triangle, VoxelConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertConfig {
    pub resolution: u32,
    pub identifier: String,
    /// Texels per box cell in the generated atlas.
    pub cell: u32,
}

impl Default for ConvertConfig {
    fn default() -> Self {
        ConvertConfig {
            resolution: 24,
            identifier: "geometry.lyco_converted".to_string(),
            cell: 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Conversion {
    pub geometry: GeometryFile,
    pub texture: image::RgbaImage,
    pub triangles: usize,
    pub voxels: usize,
    pub boxes: usize,
}

// A row-major 4x4 matrix; points are treated as column vectors.
type Mat4 = [[f32; 4]; 4];

fn identity() -> Mat4 {
    let mut m = [[0.0f32; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0f32; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[i][k] * b[k][j];
            }
            *cell = sum;
        }
    }
    out
}

fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    let v = [p[0], p[1], p[2], 1.0];
    let mut out = [0.0f32; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3] * v[3];
    }
    // A perspective divide would only matter for a projection, which glTF
    // scene graphs do not contain; guard the degenerate case anyway.
    if out[3] != 0.0 && out[3] != 1.0 {
        return [out[0] / out[3], out[1] / out[3], out[2] / out[3]];
    }
    [out[0], out[1], out[2]]
}

fn from_trs(t: Option<[f32; 3]>, r: Option<[f32; 4]>, s: Option<[f32; 3]>) -> Mat4 {
    let (tx, ty, tz) = t.map_or((0.0, 0.0, 0.0), |v| (v[0], v[1], v[2]));
    let (sx, sy, sz) = s.map_or((1.0, 1.0, 1.0), |v| (v[0], v[1], v[2]));

    let rot = match r {
        // glTF stores quaternions as (x, y, z, w).
        Some(q) => {
            let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
            let (xx, yy, zz) = (x * x, y * y, z * z);
            let (xy, xz, yz) = (x * y, x * z, y * z);
            let (wx, wy, wz) = (w * x, w * y, w * z);
            [
                [1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz), 2.0 * (xz + wy), 0.0],
                [2.0 * (xy + wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx), 0.0],
                [2.0 * (xz - wy), 2.0 * (yz + wx), 1.0 - 2.0 * (xx + yy), 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        }
        None => identity(),
    };

    [
        [rot[0][0] * sx, rot[0][1] * sy, rot[0][2] * sz, tx],
        [rot[1][0] * sx, rot[1][1] * sy, rot[1][2] * sz, ty],
        [rot[2][0] * sx, rot[2][1] * sy, rot[2][2] * sz, tz],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// glTF colour factors are linear; textures and screens are sRGB.
fn linear_to_srgb_u8(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Every material's base colour, indexed the way primitives reference them.
fn material_colors(glb: &Glb) -> Vec<[u8; 3]> {
    glb.gltf
        .materials
        .iter()
        .map(|mat| match mat.pbr_metallic_roughness.as_ref().and_then(|p| p.base_color_factor) {
            Some(f) => [
                linear_to_srgb_u8(f[0]),
                linear_to_srgb_u8(f[1]),
                linear_to_srgb_u8(f[2]),
            ],
            None => [200, 200, 200],
        })
        .collect()
}

/// Flatten the scene graph into world-space triangles with per-triangle colour.
pub fn collect_world_triangles(glb: &Glb) -> Result<Vec<Triangle>> {
    let colors = material_colors(glb);
    let roots: Vec<usize> = match glb.gltf.scene.and_then(|s| glb.gltf.scenes.get(s)) {
        Some(scene) => scene.nodes.clone(),
        // No scene declared: treat every node as a root rather than reading
        // nothing, which is what a bare converted mesh usually looks like.
        None => (0..glb.gltf.nodes.len()).collect(),
    };

    let mut out = Vec::new();
    for root in roots {
        walk(glb, root, &identity(), &colors, &mut out, 0)?;
    }
    Ok(out)
}

/// Depth guard: a malformed file could otherwise recurse until the stack dies.
const MAX_DEPTH: usize = 64;

fn walk(
    glb: &Glb,
    node_index: usize,
    parent: &Mat4,
    colors: &[[u8; 3]],
    out: &mut Vec<Triangle>,
    depth: usize,
) -> Result<()> {
    if depth > MAX_DEPTH {
        bail!("scene graph is deeper than {MAX_DEPTH} levels");
    }
    let Some(node) = glb.gltf.nodes.get(node_index) else {
        return Ok(());
    };
    let local = match &node.matrix {
        Some(m) => {
            let mut mat = [[0.0f32; 4]; 4];
            for (i, row) in mat.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    *cell = m[i * 4 + j];
                }
            }
            mat
        }
        None => from_trs(node.translation, node.rotation, node.scale),
    };
    let world = multiply(parent, &local);

    if let Some(mesh_index) = node.mesh {
        if let Some(mesh) = glb.gltf.meshes.get(mesh_index) {
            for prim in &mesh.primitives {
                let Some(position_accessor) = prim.attributes.position else {
                    continue;
                };
                let color = prim
                    .material
                    .and_then(|m| colors.get(m))
                    .copied()
                    .unwrap_or([200, 200, 200]);
                let points = glb
                    .read_vec3(position_accessor)
                    .with_context(|| format!("mesh {mesh_index} positions"))?;
                let world_points: Vec<[f32; 3]> =
                    points.iter().map(|p| transform_point(&world, *p)).collect();

                // Mode is triangles by default; other modes are skipped rather
                // than misinterpreted.
                if prim.mode.is_some_and(|m| m != 4) {
                    continue;
                }
                match prim.indices {
                    Some(accessor) => {
                        let indices = glb.read_indices(accessor)?;
                        let (tris, _) = indices.as_chunks::<3>();
                        for chunk in tris {
                            let a = chunk[0] as usize;
                            let b = chunk[1] as usize;
                            let c = chunk[2] as usize;
                            if let (Some(pa), Some(pb), Some(pc)) =
                                (world_points.get(a), world_points.get(b), world_points.get(c))
                            {
                                out.push(Triangle {
                                    a: *pa,
                                    b: *pb,
                                    c: *pc,
                                    color,
                                });
                            }
                        }
                    }
                    None => {
                        let (tris, _) = world_points.as_chunks::<3>();
                        for chunk in tris {
                            out.push(Triangle {
                                a: chunk[0],
                                b: chunk[1],
                                c: chunk[2],
                                color,
                            });
                        }
                    }
                }
            }
        }
    }

    for child in &node.children {
        walk(glb, *child, &world, colors, out, depth + 1)?;
    }
    Ok(())
}

/// Build the atlas: one cell per box, filled with the box's mean colour.
fn build_atlas(colors: &[[u8; 3]], cell: u32) -> image::RgbaImage {
    let per_row = (colors.len() as f32).sqrt().ceil().max(1.0) as u32;
    let raw = per_row * cell;
    // Power-of-two sides: some renderers still prefer them, and it costs little.
    let side = raw.next_power_of_two().max(16);
    let mut img = image::RgbaImage::from_pixel(side, side, image::Rgba([0, 0, 0, 0]));
    for (i, color) in colors.iter().enumerate() {
        let cx = (i as u32 % per_row) * cell;
        let cy = (i as u32 / per_row) * cell;
        for y in cy..(cy + cell).min(side) {
            for x in cx..(cx + cell).min(side) {
                img.put_pixel(x, y, image::Rgba([color[0], color[1], color[2], 255]));
            }
        }
    }
    img
}

fn face_uv_map(u: f32, v: f32, cell: f32) -> Uv {
    let mut faces = BTreeMap::new();
    for name in FACES {
        faces.insert(
            name.to_string(),
            FaceUv {
                uv: [u, v],
                uv_size: Some([cell, cell]),
                rotation: None,
                material_instance: None,
            },
        );
    }
    Uv::PerFace(faces)
}

fn cube_from_box(b: &Box3, grid: &Grid) -> Cube {
    let v = grid.voxel_size();
    let dims = grid.dims();
    let half_x = dims[0] as f32 * v / 2.0;
    let half_z = dims[2] as f32 * v / 2.0;

    // Centre on X and Z, stand on Y = 0: the same convention as the humanoid,
    // so a converted mesh drops into the same rig.
    let origin = [
        b.min[0] as f32 * v - half_x,
        b.min[1] as f32 * v,
        b.min[2] as f32 * v - half_z,
    ];
    let size = [b.size[0] as f32 * v, b.size[1] as f32 * v, b.size[2] as f32 * v];
    // The caller replaces the uv with a per-face cell once it has picked one.
    Cube {
        origin,
        size,
        rotation: None,
        pivot: None,
        inflate: None,
        uv: Uv::Box([0.0, 0.0]),
    }
}

/// Convert a decoded GLB into Bedrock geometry plus its texture.
pub fn convert_glb(glb: &Glb, cfg: &ConvertConfig) -> Result<Conversion> {
    let triangles = collect_world_triangles(glb)?;
    if triangles.is_empty() {
        bail!("the GLB contains no triangles to convert");
    }
    let voxel_cfg = VoxelConfig {
        resolution: cfg.resolution.max(1),
    };
    let grid = voxelize_surface(&triangles, &voxel_cfg)
        .context("the mesh is degenerate, so it cannot be voxelised")?;
    let boxes = merge_boxes(&grid);
    if boxes.is_empty() {
        bail!("voxelisation produced no occupied voxels");
    }

    let cell = cfg.cell.max(1);
    let per_row = (boxes.len() as f32).sqrt().ceil().max(1.0) as u32;
    let side = (per_row * cell).next_power_of_two().max(16) as f32;

    let mut colors = Vec::with_capacity(boxes.len());
    let mut cubes = Vec::with_capacity(boxes.len());
    for (i, b) in boxes.iter().enumerate() {
        let color = box_color(&grid, b);
        let cx = (i as u32 % per_row) as f32 * cell as f32;
        let cy = (i as u32 / per_row) as f32 * cell as f32;
        let mut cube = cube_from_box(b, &grid);
        cube.uv = face_uv_map(cx, cy, cell as f32);
        colors.push(color);
        cubes.push(cube);
    }

    let texture = build_atlas(&colors, cell);
    let bone = Bone {
        name: "root".to_string(),
        parent: None,
        pivot: Some([0.0, 0.0, 0.0]),
        rotation: None,
        mirror: None,
        inflate: None,
        cubes,
    };
    let geometry = Geometry {
        description: Description {
            identifier: cfg.identifier.clone(),
            texture_width: side,
            texture_height: side,
            visible_bounds_offset: None,
            visible_bounds_width: None,
            visible_bounds_height: None,
        },
        bones: vec![bone],
    };

    let problems = geometry.validate();
    if !problems.is_empty() {
        bail!("conversion produced invalid geometry: {:?}", problems);
    }

    Ok(Conversion {
        geometry: GeometryFile::new(geometry),
        texture,
        triangles: triangles.len(),
        voxels: grid.occupied_count(),
        boxes: boxes.len(),
    })
}
impl Conversion {
    /// Write the generated texture. Kept here so callers need no `image`
    /// dependency of their own just to save it.
    pub fn save_texture(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        self.texture
            .save(path)
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glb::Gltf;

    const VERTICES: [[f32; 3]; 8] = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, -1.0, 1.0],
        [-1.0, -1.0, 1.0],
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    const INDICES: [u16; 36] = [
        0, 1, 2, 0, 2, 3,
        4, 6, 5, 4, 7, 6,
        0, 4, 5, 0, 5, 1,
        1, 5, 6, 1, 6, 2,
        2, 6, 7, 2, 7, 3,
        3, 7, 4, 3, 4, 0,
    ];

    fn cube_glb(translate: [f32; 3]) -> Glb {
        let mut bin = Vec::new();
        for v in VERTICES {
            for c in v {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in INDICES {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let gltf: Gltf = serde_json::from_value(serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "cube", "mesh": 0, "translation": translate}],
            "meshes": [{"primitives": [
                {"attributes": {"POSITION": 0}, "indices": 1, "material": 0}
            ]}],
            "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.0, 0.0, 1.0]}}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 36, "type": "SCALAR"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 96},
                {"buffer": 0, "byteOffset": 96, "byteLength": 72}
            ],
            "buffers": [{"byteLength": 168}]
        }))
        .expect("build gltf json");
        Glb {
            gltf,
            bin,
            version: 2,
        }
    }

    fn default_cfg(resolution: u32) -> ConvertConfig {
        ConvertConfig {
            resolution,
            ..Default::default()
        }
    }

    #[test]
    fn reads_every_triangle_of_the_mesh() {
        let tris = collect_world_triangles(&cube_glb([0.0, 0.0, 0.0])).expect("collect");
        assert_eq!(tris.len(), 12, "a cube is twelve triangles");
        assert_eq!(tris[0].color, [255, 0, 0]);
    }

    #[test]
    fn applies_node_translation() {
        let tris = collect_world_triangles(&cube_glb([10.0, 5.0, -3.0])).expect("collect");
        let min_x = tris.iter().map(|t| t.a[0]).fold(f32::MAX, f32::min);
        assert!(min_x > 8.0, "translation was not applied: min_x = {min_x}");
    }

    #[test]
    fn converts_a_cube_into_a_usable_model() {
        let c = convert_glb(&cube_glb([0.0, 0.0, 0.0]), &ConvertConfig::default())
            .expect("convert");
        assert_eq!(c.triangles, 12);
        assert!(c.voxels > 0);
        assert!(c.boxes < c.voxels, "{} boxes for {} voxels", c.boxes, c.voxels);
        assert!(c.boxes <= 24, "a cube shell should merge to few boxes, got {}", c.boxes);
    }

    #[test]
    fn the_emitted_geometry_validates_and_matches_its_texture() {
        let c = convert_glb(&cube_glb([0.0, 0.0, 0.0]), &ConvertConfig::default())
            .expect("convert");
        let problems = c.geometry.validate();
        assert!(problems.is_empty(), "{problems:?}");
        let g = &c.geometry.geometry[0];
        assert_eq!(g.description.texture_width, c.texture.width() as f32);
        assert_eq!(g.description.texture_height, c.texture.height() as f32);
    }

    #[test]
    fn every_cube_cites_a_cell_inside_the_texture() {
        let c = convert_glb(&cube_glb([0.0, 0.0, 0.0]), &ConvertConfig::default())
            .expect("convert");
        let side = c.texture.width() as f32;
        let cubes = &c.geometry.geometry[0].bones[0].cubes;
        assert!(!cubes.is_empty());
        for cube in cubes {
            match &cube.uv {
                Uv::PerFace(faces) => {
                    assert_eq!(faces.len(), 6, "all six faces need a uv");
                    for (name, face) in faces {
                        assert!(FACES.contains(&name.as_str()), "{name}");
                        assert!(face.uv[0] >= 0.0 && face.uv[0] < side, "{face:?}");
                        assert!(face.uv[1] >= 0.0 && face.uv[1] < side, "{face:?}");
                    }
                }
                other => panic!("expected per-face uv, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_model_stands_on_zero_and_is_centred() {
        let c = convert_glb(&cube_glb([0.0, 0.0, 0.0]), &ConvertConfig::default())
            .expect("convert");
        let cubes = &c.geometry.geometry[0].bones[0].cubes;
        let min_y = cubes.iter().map(|c| c.origin[1]).fold(f32::MAX, f32::min);
        assert_eq!(min_y, 0.0, "the model should stand on Y = 0");
        let min_x = cubes.iter().map(|c| c.origin[0]).fold(f32::MAX, f32::min);
        let max_x = cubes
            .iter()
            .map(|c| c.origin[0] + c.size[0])
            .fold(f32::MIN, f32::max);
        assert!(min_x < 0.0 && max_x > 0.0, "x spans {min_x}..{max_x}");
    }

    #[test]
    fn resolution_changes_the_voxel_count() {
        let coarse = convert_glb(&cube_glb([0.0, 0.0, 0.0]), &default_cfg(8)).expect("coarse");
        let fine = convert_glb(&cube_glb([0.0, 0.0, 0.0]), &default_cfg(32)).expect("fine");
        assert!(
            fine.voxels > coarse.voxels,
            "fine {} vs coarse {}",
            fine.voxels,
            coarse.voxels
        );
    }

    #[test]
    fn a_mesh_with_no_triangles_is_refused() {
        let gltf: Gltf = serde_json::from_value(serde_json::json!({
            "asset": {"version": "2.0"},
            "meshes": [{"primitives": []}]
        }))
        .expect("empty gltf");
        let glb = Glb {
            gltf,
            bin: Vec::new(),
            version: 2,
        };
        let err = convert_glb(&glb, &ConvertConfig::default()).unwrap_err();
        assert!(err.to_string().contains("no triangles"), "{err}");
    }
}
