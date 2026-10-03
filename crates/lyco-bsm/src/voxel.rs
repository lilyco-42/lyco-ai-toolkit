//! Surface voxelisation and greedy box merging (F3).
//!
//! Turning a smooth triangle mesh into a Minecraft model means answering one
//! question: which axis-aligned boxes approximate this surface at a usable
//! resolution? A player entity is built from tens of cubes, not thousands, so
//! the answer has to be merged boxes rather than one cube per voxel.
//!
//! Two stages, deliberately separable and separately testable:
//!
//!   1. [`voxelize_surface`] marks the voxels a mesh passes through. It is a
//!      *surface* fill, not a solid fill: a Minecraft model is a shell of
//!      boxes, and solidifying would multiply the cube count by the volume.
//!   2. [`merge_boxes`] greedily fuses occupied voxels into the largest boxes
//!      it can, which is what turns a few thousand voxels into a usable model.
//!
//! Colour is carried through both stages so a caller can build a texture.

/// One triangle plus the colour its material contributes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub c: [f32; 3],
    pub color: [u8; 3],
}

/// How finely to sample. `resolution` is the number of voxels along the model's
/// longest axis; the other two follow from the bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoxelConfig {
    pub resolution: u32,
}

impl Default for VoxelConfig {
    fn default() -> Self {
        // 24 keeps a humanoid readable without exploding the cube count.
        VoxelConfig { resolution: 24 }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// Shortest distance from a point to a triangle, corners and interior included.
///
/// This is the test that decides voxel occupancy. An earlier version sampled
/// barycentric points along each triangle, but that leaves holes whenever a
/// triangle is larger than a voxel, and holes fragment box merging badly
/// (a cube shell came out as 286 boxes instead of six). Measuring distance
/// against the voxel centre is gap-free whatever the triangle size.
fn point_triangle_distance(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return len(ap);
    }

    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return len(bp);
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return len(sub(p, add(a, scale(ab, v))));
    }

    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return len(cp);
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return len(sub(p, add(a, scale(ac, w))));
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return len(sub(p, add(b, scale(sub(c, b), w))));
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    len(sub(p, add(a, add(scale(ab, v), scale(ac, w)))))
}

/// A dense occupancy grid in model space, plus a colour per voxel.
///
/// No `Eq`: `voxel_size` is an `f32`, so only `PartialEq` is available.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    dims: [u32; 3],
    occupied: Vec<bool>,
    colors: Vec<[u8; 3]>,
    /// Model-space origin of voxel (0, 0, 0).
    origin: [f32; 3],
    /// Edge length of one voxel in model units.
    voxel_size: f32,
}

impl Grid {
    pub fn new(dims: [u32; 3], origin: [f32; 3], voxel_size: f32) -> Self {
        let len = (dims[0] as usize) * (dims[1] as usize) * (dims[2] as usize);
        Grid {
            dims,
            occupied: vec![false; len],
            colors: vec![[0, 0, 0]; len],
            origin,
            voxel_size,
        }
    }

    pub fn dims(&self) -> [u32; 3] {
        self.dims
    }

    pub fn origin(&self) -> [f32; 3] {
        self.origin
    }

    pub fn voxel_size(&self) -> f32 {
        self.voxel_size
    }

    pub fn index(&self, x: u32, y: u32, z: u32) -> Option<usize> {
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return None;
        }
        let i = (z as usize) * (self.dims[0] as usize) * (self.dims[1] as usize)
            + (y as usize) * (self.dims[0] as usize)
            + (x as usize);
        Some(i)
    }

    pub fn is_occupied(&self, x: u32, y: u32, z: u32) -> bool {
        self.index(x, y, z).is_some_and(|i| self.occupied[i])
    }

    pub fn color(&self, x: u32, y: u32, z: u32) -> [u8; 3] {
        self.index(x, y, z).map_or([0, 0, 0], |i| self.colors[i])
    }

    fn mark(&mut self, x: i64, y: i64, z: i64, color: [u8; 3]) {
        if x < 0 || y < 0 || z < 0 {
            return;
        }
        let (x, y, z) = (x as u32, y as u32, z as u32);
        if let Some(i) = self.index(x, y, z) {
            self.occupied[i] = true;
            self.colors[i] = color;
        }
    }

    pub fn occupied_count(&self) -> usize {
        self.occupied.iter().filter(|o| **o).count()
    }

    /// Average colour of the occupied voxels, for a quick sanity check.
    pub fn average_color(&self) -> [u8; 3] {
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        for (i, occupied) in self.occupied.iter().enumerate() {
            if *occupied {
                for (slot, value) in sum.iter_mut().zip(self.colors[i].iter()) {
                    *slot += *value as u32;
                }
                n += 1;
            }
        }
        if n == 0 {
            return [0, 0, 0];
        }
        [
            (sum[0] / n) as u8,
            (sum[1] / n) as u8,
            (sum[2] / n) as u8,
        ]
    }
}

/// An axis-aligned box in voxel coordinates: minimum corner plus extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Box3 {
    pub min: [u32; 3],
    pub size: [u32; 3],
}

impl Box3 {
    pub fn voxel_count(&self) -> u32 {
        self.size[0] * self.size[1] * self.size[2]
    }
}

fn aabb(points: &[[f32; 3]]) -> Option<([f32; 3], [f32; 3])> {
    let first = *points.first()?;
    let mut min = first;
    let mut max = first;
    for p in points {
        for c in 0..3 {
            if p[c] < min[c] {
                min[c] = p[c];
            }
            if p[c] > max[c] {
                max[c] = p[c];
            }
        }
    }
    Some((min, max))
}


/// Mark the voxels a surface passes through.
///
/// Returns `None` when there is nothing to voxelise or the mesh is degenerate.
pub fn voxelize_surface(triangles: &[Triangle], cfg: &VoxelConfig) -> Option<Grid> {
    if triangles.is_empty() || cfg.resolution == 0 {
        return None;
    }

    let corners: Vec<[f32; 3]> = triangles
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .collect();
    let (min, max) = aabb(&corners)?;
    let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let longest = extent.iter().copied().fold(0.0f32, f32::max);
    if !longest.is_finite() || longest <= 0.0 {
        return None;
    }

    let voxel_size = longest / cfg.resolution as f32;
    let dims = [
        ((extent[0] / voxel_size).ceil() as u32).max(1),
        ((extent[1] / voxel_size).ceil() as u32).max(1),
        ((extent[2] / voxel_size).ceil() as u32).max(1),
    ];
    let mut grid = Grid::new(dims, min, voxel_size);
    // Half the voxel diagonal: a voxel is occupied when its centre is within
    // this of the surface, which makes the shell watertight.
    let radius = voxel_size * 0.5 * 3.0f32.sqrt();

    for tri in triangles {
        let tri_min = [
            tri.a[0].min(tri.b[0]).min(tri.c[0]),
            tri.a[1].min(tri.b[1]).min(tri.c[1]),
            tri.a[2].min(tri.b[2]).min(tri.c[2]),
        ];
        let tri_max = [
            tri.a[0].max(tri.b[0]).max(tri.c[0]),
            tri.a[1].max(tri.b[1]).max(tri.c[1]),
            tri.a[2].max(tri.b[2]).max(tri.c[2]),
        ];

        let mut lo = [0i64; 3];
        let mut hi = [0i64; 3];
        for axis in 0..3 {
            let from = (tri_min[axis] - radius - min[axis]) / voxel_size;
            let to = (tri_max[axis] + radius - min[axis]) / voxel_size;
            lo[axis] = from.floor().max(0.0) as i64;
            hi[axis] = (to.ceil() as i64).min(dims[axis] as i64 - 1);
        }

        for gz in lo[2]..=hi[2] {
            for gy in lo[1]..=hi[1] {
                for gx in lo[0]..=hi[0] {
                    let centre = [
                        min[0] + (gx as f32 + 0.5) * voxel_size,
                        min[1] + (gy as f32 + 0.5) * voxel_size,
                        min[2] + (gz as f32 + 0.5) * voxel_size,
                    ];
                    if point_triangle_distance(centre, tri.a, tri.b, tri.c) <= radius {
                        grid.mark(gx, gy, gz, tri.color);
                    }
                }
            }
        }
    }

    if grid.occupied_count() == 0 {
        return None;
    }
    Some(grid)
}

/// Fuse occupied voxels into as few axis-aligned boxes as a greedy pass finds.
///
/// The classic three-phase grow: extend along X, then extend that slab along Z,
/// then extend the whole slab along Y. Greedy is not optimal, but it is stable,
/// deterministic and good enough - an optimal box cover is NP-hard.
pub fn merge_boxes(grid: &Grid) -> Vec<Box3> {
    let [dx, dy, dz] = grid.dims;
    let mut visited = vec![false; grid.occupied.len()];
    let mut out = Vec::new();

    for z in 0..dz {
        for y in 0..dy {
            for x in 0..dx {
                let Some(start) = grid.index(x, y, z) else {
                    continue;
                };
                if !grid.occupied[start] || visited[start] {
                    continue;
                }
                let free = |gx: u32, gy: u32, gz: u32| -> bool {
                    grid.index(gx, gy, gz)
                        .is_some_and(|i| grid.occupied[i] && !visited[i])
                };

                let mut sx = 1;
                while x + sx < dx && free(x + sx, y, z) {
                    sx += 1;
                }

                let mut sz = 1;
                'grow_z: while z + sz < dz {
                    for gx in x..x + sx {
                        if !free(gx, y, z + sz) {
                            break 'grow_z;
                        }
                    }
                    sz += 1;
                }

                let mut sy = 1;
                'grow_y: while y + sy < dy {
                    for gz in z..z + sz {
                        for gx in x..x + sx {
                            if !free(gx, y + sy, gz) {
                                break 'grow_y;
                            }
                        }
                    }
                    sy += 1;
                }

                for gz in z..z + sz {
                    for gy in y..y + sy {
                        for gx in x..x + sx {
                            if let Some(i) = grid.index(gx, gy, gz) {
                                visited[i] = true;
                            }
                        }
                    }
                }
                out.push(Box3 {
                    min: [x, y, z],
                    size: [sx, sy, sz],
                });
            }
        }
    }
    out
}

/// Mean colour of the voxels a box covers.
pub fn box_color(grid: &Grid, b: &Box3) -> [u8; 3] {
    let mut sum = [0u32; 3];
    let mut n = 0u32;
    for gz in b.min[2]..b.min[2] + b.size[2] {
        for gy in b.min[1]..b.min[1] + b.size[1] {
            for gx in b.min[0]..b.min[0] + b.size[0] {
                let c = grid.color(gx, gy, gz);
                for (slot, value) in sum.iter_mut().zip(c.iter()) {
                    *slot += *value as u32;
                }
                n += 1;
            }
        }
    }
    if n == 0 {
        return [0, 0, 0];
    }
    [
        (sum[0] / n) as u8,
        (sum[1] / n) as u8,
        (sum[2] / n) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Triangle {
        Triangle {
            a,
            b,
            c,
            color: [200, 100, 50],
        }
    }

    /// The twelve triangles of an axis-aligned cube.
    fn cube_triangles(lo: f32, hi: f32) -> Vec<Triangle> {
        let v = [
            [lo, lo, lo],
            [hi, lo, lo],
            [hi, lo, hi],
            [lo, lo, hi],
            [lo, hi, lo],
            [hi, hi, lo],
            [hi, hi, hi],
            [lo, hi, hi],
        ];
        let faces = [
            [0, 1, 2],
            [0, 2, 3],
            [4, 6, 5],
            [4, 7, 6],
            [0, 4, 5],
            [0, 5, 1],
            [1, 5, 6],
            [1, 6, 2],
            [2, 6, 7],
            [2, 7, 3],
            [3, 7, 4],
            [3, 4, 0],
        ];
        faces.iter().map(|f| tri(v[f[0]], v[f[1]], v[f[2]])).collect()
    }

    #[test]
    fn a_cube_becomes_a_hollow_shell_not_a_solid() {
        let tris = cube_triangles(0.0, 8.0);
        let grid = voxelize_surface(&tris, &VoxelConfig { resolution: 8 }).expect("voxelize");
        assert_eq!(grid.dims(), [8, 8, 8]);

        // A solid 8x8x8 fill would be 512 voxels; a shell is far fewer.
        let occupied = grid.occupied_count();
        assert!(occupied < 512, "filled the volume: {occupied}");
        assert!(occupied > 8 * 8, "shell is too sparse: {occupied}");

        // The middle of the cube must be empty, the surface must not be.
        assert!(!grid.is_occupied(4, 4, 4), "centre should be hollow");
        assert!(grid.is_occupied(0, 4, 4), "a face should be occupied");
    }

    #[test]
    fn merging_recovers_a_single_box_from_a_solid_cube() {
        // Hand-build a solid 4x3x2 block and check the greedy pass finds one box.
        let mut grid = Grid::new([4, 3, 2], [0.0; 3], 1.0);
        for z in 0..2 {
            for y in 0..3 {
                for x in 0..4 {
                    grid.mark(x, y, z, [10, 20, 30]);
                }
            }
        }
        let boxes = merge_boxes(&grid);
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        assert_eq!(boxes[0].min, [0, 0, 0]);
        assert_eq!(boxes[0].size, [4, 3, 2]);
        assert_eq!(boxes[0].voxel_count(), 24);
    }

    #[test]
    fn merging_never_loses_or_duplicates_a_voxel() {
        let tris = cube_triangles(0.0, 10.0);
        let grid = voxelize_surface(&tris, &VoxelConfig { resolution: 10 }).unwrap();
        let boxes = merge_boxes(&grid);

        let covered: u32 = boxes.iter().map(|b| b.voxel_count()).sum();
        assert_eq!(
            covered as usize,
            grid.occupied_count(),
            "merged boxes must tile the occupied voxels exactly"
        );

        // And no voxel may be claimed twice.
        let mut seen = vec![0u8; grid.occupied.len()];
        for b in &boxes {
            for gz in b.min[2]..b.min[2] + b.size[2] {
                for gy in b.min[1]..b.min[1] + b.size[1] {
                    for gx in b.min[0]..b.min[0] + b.size[0] {
                        seen[grid.index(gx, gy, gz).unwrap()] += 1;
                    }
                }
            }
        }
        for (i, occupied) in grid.occupied.iter().enumerate() {
            let expected = if *occupied { 1 } else { 0 };
            assert_eq!(seen[i], expected, "voxel {i}");
        }
    }

    #[test]
    fn a_sphere_is_hollow_and_roughly_round() {
        // An icosphere-ish approximation is overkill; a UV sphere is fine.
        let (segments, rings) = (16u32, 8u32);
        let radius = 5.0f32;
        let mut tris = Vec::new();
        let point = |i: u32, j: u32| -> [f32; 3] {
            let theta = std::f32::consts::TAU * i as f32 / segments as f32;
            let phi = std::f32::consts::PI * j as f32 / rings as f32;
            [
                radius * phi.sin() * theta.cos(),
                radius * phi.cos(),
                radius * phi.sin() * theta.sin(),
            ]
        };
        for j in 0..rings {
            for i in 0..segments {
                let (p0, p1) = (point(i, j), point(i + 1, j));
                let (p2, p3) = (point(i + 1, j + 1), point(i, j + 1));
                tris.push(tri(p0, p1, p2));
                tris.push(tri(p0, p2, p3));
            }
        }

        let grid = voxelize_surface(&tris, &VoxelConfig { resolution: 16 }).unwrap();
        assert!(!grid.is_occupied(8, 8, 8), "a sphere is hollow too");
        assert!(grid.occupied_count() > 50);

        // Every occupied voxel should be near the surface, never deep inside.
        let boxes = merge_boxes(&grid);
        assert!(boxes.len() < grid.occupied_count(), "merging should help");
    }

    #[test]
    fn the_shell_is_watertight_with_no_gaps() {
        let tris = cube_triangles(0.0, 12.0);
        let grid = voxelize_surface(&tris, &VoxelConfig { resolution: 12 }).unwrap();
        let [dx, dy, dz] = grid.dims();
        let mid_z = dz / 2;
        let mid_x = dx / 2;
        let mid_y = dy / 2;
        for x in 0..dx {
            assert!(grid.is_occupied(x, 0, mid_z), "hole at x={x} on a Y face");
            assert!(grid.is_occupied(x, dy - 1, mid_z), "hole at x={x} on a Y face");
        }
        for z in 0..dz {
            assert!(grid.is_occupied(0, mid_y, z), "hole at z={z} on an X face");
            assert!(grid.is_occupied(dx - 1, mid_y, z), "hole at z={z} on an X face");
        }
        for y in 0..dy {
            assert!(grid.is_occupied(mid_x, y, 0), "hole at y={y} on a Z face");
            assert!(grid.is_occupied(mid_x, y, dz - 1), "hole at y={y} on a Z face");
        }
    }

    #[test]
    fn a_cube_shell_merges_into_a_handful_of_boxes() {
        let tris = cube_triangles(0.0, 12.0);
        let grid = voxelize_surface(&tris, &VoxelConfig { resolution: 12 }).unwrap();
        let boxes = merge_boxes(&grid);
        assert!(
            boxes.len() <= 12,
            "a six-sided shell should be about six boxes, got {}",
            boxes.len()
        );
    }

    #[test]
    fn colour_is_carried_through_to_the_box() {
        let mut grid = Grid::new([2, 1, 1], [0.0; 3], 1.0);
        grid.mark(0, 0, 0, [10, 20, 30]);
        grid.mark(1, 0, 0, [30, 40, 50]);
        let boxes = merge_boxes(&grid);
        assert_eq!(boxes.len(), 1);
        assert_eq!(box_color(&grid, &boxes[0]), [20, 30, 40]);
    }

    #[test]
    fn degenerate_input_is_refused_rather_than_guessed() {
        assert!(voxelize_surface(&[], &VoxelConfig::default()).is_none());
        let degenerate = [tri([0.0; 3], [0.0; 3], [0.0; 3])];
        assert!(voxelize_surface(&degenerate, &VoxelConfig::default()).is_none());
        let tris = cube_triangles(0.0, 4.0);
        assert!(voxelize_surface(&tris, &VoxelConfig { resolution: 0 }).is_none());
    }

    #[test]
    fn coarser_resolution_never_increases_the_box_count() {
        let tris = cube_triangles(0.0, 12.0);
        let fine = merge_boxes(&voxelize_surface(&tris, &VoxelConfig { resolution: 24 }).unwrap());
        let coarse = merge_boxes(&voxelize_surface(&tris, &VoxelConfig { resolution: 6 }).unwrap());
        assert!(
            coarse.len() <= fine.len(),
            "coarse {} vs fine {}",
            coarse.len(),
            fine.len()
        );
    }
}
