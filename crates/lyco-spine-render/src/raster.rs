//! A small software rasteriser for posed Spine meshes.
//!
//! Deliberately knows nothing about Spine. It takes plain triangles with UVs
//! and a texture, and draws them. That split is what lets the hard part - the
//! rasteriser - be unit tested without a skeleton, a GPU, a window, or the Spine
//! runtime, while the spine-aware code above it stays thin enough to read.
//!
//! Why software rather than wgpu: the operator's machine has no real GPU and CI
//! has none at all. A CPU rasteriser is slower and completely portable, and it
//! renders the same bytes everywhere, which is what tests need.

use anyhow::{bail, Context, Result};

/// One texture page, straight (non-premultiplied) RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Texture {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!("a texture cannot be {width}x{height}");
        }
        let expected = (width as usize) * (height as usize) * 4;
        if pixels.len() != expected {
            bail!(
                "a {width}x{height} texture needs {expected} bytes, got {}",
                pixels.len()
            );
        }
        Ok(Texture {
            width,
            height,
            pixels,
        })
    }

    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        let at = (y * self.width as usize + x) * 4;
        [
            self.pixels[at] as f32 / 255.0,
            self.pixels[at + 1] as f32 / 255.0,
            self.pixels[at + 2] as f32 / 255.0,
            self.pixels[at + 3] as f32 / 255.0,
        ]
    }

    /// Bilinear sample in normalised texture space.
    ///
    /// Bilinear rather than nearest because the skeleton is usually scaled to
    /// fit, and nearest makes scaled character art look like gravel.
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let fx = u * self.width as f32 - 0.5;
        let fy = v * self.height as f32 - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (x0, y0) = (x0 as i64, y0 as i64);

        let c00 = self.texel(x0, y0);
        let c10 = self.texel(x0 + 1, y0);
        let c01 = self.texel(x0, y0 + 1);
        let c11 = self.texel(x0 + 1, y0 + 1);

        let mut out = [0.0f32; 4];
        for channel in 0..4 {
            let top = c00[channel] * (1.0 - tx) + c10[channel] * tx;
            let bottom = c01[channel] * (1.0 - tx) + c11[channel] * tx;
            out[channel] = top * (1.0 - ty) + bottom * ty;
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Additive,
    Multiply,
    Screen,
}

/// Triangle soup with everything the rasteriser needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Positions, in skeleton space.
    pub vertices: Vec<[f32; 2]>,
    /// Texture coordinates, parallel to `vertices`.
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u16>,
    pub color: [f32; 4],
    pub blend: Blend,
    /// Which texture to sample.
    pub texture: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32, background: [u8; 4]) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!("a frame cannot be {width}x{height}");
        }
        let mut pixels = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for _ in 0..(width as usize) * (height as usize) {
            pixels.extend_from_slice(&background);
        }
        Ok(Frame {
            width,
            height,
            pixels,
        })
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y as usize) * (self.width as usize) + x as usize) * 4;
        [
            self.pixels[at],
            self.pixels[at + 1],
            self.pixels[at + 2],
            self.pixels[at + 3],
        ]
    }

    fn set(&mut self, x: u32, y: u32, value: [u8; 4]) {
        let at = ((y as usize) * (self.width as usize) + x as usize) * 4;
        self.pixels[at..at + 4].copy_from_slice(&value);
    }

    /// The bounding box of everything that is not the background, if anything.
    pub fn opaque_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0) = (self.width, self.height);
        let (mut x1, mut y1) = (0u32, 0u32);
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.pixel(x, y)[3] > 0 {
                    found = true;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        found.then_some((x0, y0, x1, y1))
    }

    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        let buffer = image::RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
            .context("the frame buffer does not match its dimensions")?;
        buffer
            .save(path)
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// Fraction of the frame left empty, so nothing touches the edge.
    pub margin: f32,
    /// True when the textures already have their colour multiplied by alpha.
    pub premultiplied_textures: bool,
}

impl Default for Fit {
    fn default() -> Self {
        Fit {
            margin: 0.06,
            premultiplied_textures: false,
        }
    }
}

/// Twice the signed area of the triangle `a b c`; also the barycentric weight.
fn edge(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[allow(clippy::too_many_arguments)]
fn draw_triangle(
    frame: &mut Frame,
    texture: &Texture,
    positions: [[f32; 2]; 3],
    uvs: [[f32; 2]; 3],
    color: [f32; 4],
    blend: Blend,
    fit: &Fit,
) {
    let signed_area = edge(positions[0], positions[1], positions[2]);
    if signed_area.abs() < 1e-9 {
        return;
    }
    // Normalise the winding so the inside test can be a plain `>= 0`.
    let sign = if signed_area < 0.0 { -1.0 } else { 1.0 };
    let area = signed_area * sign;

    let min_x = positions
        .iter()
        .map(|p| p[0])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0);
    let max_x = positions
        .iter()
        .map(|p| p[0])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(frame.width as f32 - 1.0);
    let min_y = positions
        .iter()
        .map(|p| p[1])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0);
    let max_y = positions
        .iter()
        .map(|p| p[1])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(frame.height as f32 - 1.0);
    if min_x > max_x || min_y > max_y {
        return;
    }

    for y in (min_y as u32)..=(max_y as u32) {
        for x in (min_x as u32)..=(max_x as u32) {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let w0 = edge(positions[1], positions[2], p) * sign / area;
            let w1 = edge(positions[2], positions[0], p) * sign / area;
            let w2 = edge(positions[0], positions[1], p) * sign / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let u = w0 * uvs[0][0] + w1 * uvs[1][0] + w2 * uvs[2][0];
            let v = w0 * uvs[0][1] + w1 * uvs[1][1] + w2 * uvs[2][1];
            let texel = texture.sample(u, v);

            // `a` is the coverage after the tint; `src` is premultiplied.
            let a = (texel[3] * color[3]).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let straight = if fit.premultiplied_textures {
                let inv = if texel[3] > 1e-6 { 1.0 / texel[3] } else { 0.0 };
                [texel[0] * inv, texel[1] * inv, texel[2] * inv]
            } else {
                [texel[0], texel[1], texel[2]]
            };
            let tinted = [
                straight[0] * color[0],
                straight[1] * color[1],
                straight[2] * color[2],
            ];
            let src = [tinted[0] * a, tinted[1] * a, tinted[2] * a];

            let dst = frame.pixel(x, y);
            let d = [
                dst[0] as f32 / 255.0,
                dst[1] as f32 / 255.0,
                dst[2] as f32 / 255.0,
                dst[3] as f32 / 255.0,
            ];
            let out = match blend {
                // Source over.
                Blend::Normal => [
                    src[0] + d[0] * (1.0 - a),
                    src[1] + d[1] * (1.0 - a),
                    src[2] + d[2] * (1.0 - a),
                    a + d[3] * (1.0 - a),
                ],
                // Everything the source has, plus whatever was there.
                Blend::Additive => [
                    src[0] + d[0],
                    src[1] + d[1],
                    src[2] + d[2],
                    (a + d[3]).min(1.0),
                ],
                Blend::Multiply => [
                    d[0] * (1.0 - a) + d[0] * tinted[0] * a,
                    d[1] * (1.0 - a) + d[1] * tinted[1] * a,
                    d[2] * (1.0 - a) + d[2] * tinted[2] * a,
                    a + d[3] * (1.0 - a),
                ],
                Blend::Screen => [
                    d[0] + src[0] - d[0] * tinted[0] * a,
                    d[1] + src[1] - d[1] * tinted[1] * a,
                    d[2] + src[2] - d[2] * tinted[2] * a,
                    a + d[3] * (1.0 - a),
                ],
            };
            frame.set(
                x,
                y,
                [
                    to_byte(out[0]),
                    to_byte(out[1]),
                    to_byte(out[2]),
                    to_byte(out[3]),
                ],
            );
        }
    }
}

/// Draw every mesh into a frame, scaled to fit.
///
/// The meshes are drawn in order, so the first is behind all the others, which
/// is the order a Spine drawer emits them in.
pub fn render(
    meshes: &[Mesh],
    textures: &[Texture],
    width: u32,
    height: u32,
    background: [u8; 4],
    fit: &Fit,
) -> Result<Frame> {
    let mut frame = Frame::new(width, height, background)?;

    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for mesh in meshes {
        for vertex in &mesh.vertices {
            min_x = min_x.min(vertex[0]);
            min_y = min_y.min(vertex[1]);
            max_x = max_x.max(vertex[0]);
            max_y = max_y.max(vertex[1]);
        }
    }
    if min_x > max_x || min_y > max_y {
        // Nothing was submitted, so the background is the correct answer.
        return Ok(frame);
    }

    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    let margin = fit.margin.clamp(0.0, 0.9);
    let scale =
        ((width as f32 * (1.0 - margin)) / span_x).min((height as f32 * (1.0 - margin)) / span_y);
    let centre_x = (min_x + max_x) * 0.5;
    let centre_y = (min_y + max_y) * 0.5;
    // Skeleton space is Y-up; a frame is Y-down.
    let to_screen = |p: [f32; 2]| {
        [
            width as f32 * 0.5 + (p[0] - centre_x) * scale,
            height as f32 * 0.5 - (p[1] - centre_y) * scale,
        ]
    };

    for mesh in meshes {
        let texture = textures.get(mesh.texture).with_context(|| {
            format!(
                "a mesh asks for texture {} but only {} were given",
                mesh.texture,
                textures.len()
            )
        })?;
        for triangle in mesh.indices.chunks_exact(3) {
            let (i0, i1, i2) = (
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            );
            let count = mesh.vertices.len();
            if i0 >= count || i1 >= count || i2 >= count {
                bail!("a triangle indexes a vertex that does not exist");
            }
            draw_triangle(
                &mut frame,
                texture,
                [
                    to_screen(mesh.vertices[i0]),
                    to_screen(mesh.vertices[i1]),
                    to_screen(mesh.vertices[i2]),
                ],
                [mesh.uvs[i0], mesh.uvs[i1], mesh.uvs[i2]],
                mesh.color,
                mesh.blend,
                fit,
            );
        }
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Texture {
        let mut pixels = Vec::new();
        for _ in 0..(width as usize * height as usize) {
            pixels.extend_from_slice(&rgba);
        }
        Texture::new(width, height, pixels).expect("texture")
    }

    /// A quad spanning -1..1, with UVs oriented so that skeleton +Y is the top
    /// of the texture, which is how Spine exports them.
    fn quad(uvs: [[f32; 2]; 4]) -> Mesh {
        Mesh {
            vertices: vec![[-1.0, 1.0], [1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]],
            uvs: uvs.to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            color: [1.0, 1.0, 1.0, 1.0],
            blend: Blend::Normal,
            texture: 0,
        }
    }

    const UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    #[test]
    fn a_quad_paints_the_texture_across_the_fit_box() {
        let frame = render(
            &[quad(UVS)],
            &[solid(4, 4, [255, 0, 0, 255])],
            32,
            32,
            [0, 0, 0, 0],
            &Fit {
                margin: 0.0,
                ..Fit::default()
            },
        )
        .expect("render");

        assert_eq!(frame.width, 32);
        assert_eq!(frame.pixel(16, 16), [255, 0, 0, 255]);
        // With no margin the quad reaches the corners.
        assert_eq!(frame.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(frame.pixel(31, 31), [255, 0, 0, 255]);
    }

    #[test]
    fn nothing_drawn_leaves_the_background_exactly_as_given() {
        let frame = render(&[], &[], 8, 4, [10, 20, 30, 255], &Fit::default()).expect("render");
        assert_eq!(frame.pixel(0, 0), [10, 20, 30, 255]);
        assert_eq!(frame.opaque_bounds(), Some((0, 0, 7, 3)));
    }

    /// The frame is Y-down and skeleton space is Y-up, so the top of the texture
    /// has to land at the top of the frame.
    #[test]
    fn the_vertical_axis_is_flipped() {
        // Red on the top row, blue on the bottom row of the texture.
        let mut pixels = Vec::new();
        for y in 0..4 {
            for _ in 0..4 {
                pixels.extend_from_slice(if y < 2 {
                    &[255u8, 0, 0, 255]
                } else {
                    &[0u8, 0, 255, 255]
                });
            }
        }
        let texture = Texture::new(4, 4, pixels).expect("texture");

        let frame = render(
            &[quad(UVS)],
            &[texture],
            32,
            32,
            [0, 0, 0, 0],
            &Fit {
                margin: 0.0,
                ..Fit::default()
            },
        )
        .expect("render");

        assert_eq!(frame.pixel(16, 2)[0], 255, "the top is red");
        assert_eq!(frame.pixel(16, 29)[2], 255, "the bottom is blue");
    }

    #[test]
    fn the_margin_leaves_the_edges_empty() {
        let frame = render(
            &[quad(UVS)],
            &[solid(2, 2, [255, 255, 255, 255])],
            40,
            40,
            [0, 0, 0, 0],
            &Fit {
                margin: 0.25,
                ..Fit::default()
            },
        )
        .expect("render");

        assert_eq!(frame.pixel(0, 20), [0, 0, 0, 0], "the left edge is clear");
        assert_eq!(
            frame.pixel(20, 20),
            [255, 255, 255, 255],
            "but the middle is painted"
        );
        let (x0, y0, x1, y1) = frame.opaque_bounds().expect("something drawn");
        assert!(
            x0 >= 4 && y0 >= 4 && x1 <= 35 && y1 <= 35,
            "{x0},{y0},{x1},{y1}"
        );
    }

    #[test]
    fn the_order_of_the_meshes_decides_what_is_in_front() {
        let mut behind = quad(UVS);
        behind.color = [1.0, 0.0, 0.0, 1.0];
        let mut front = quad(UVS);
        front.color = [0.0, 1.0, 0.0, 1.0];

        let white = solid(2, 2, [255, 255, 255, 255]);
        let frame = render(
            &[behind, front],
            &[white],
            16,
            16,
            [0, 0, 0, 0],
            &Fit {
                margin: 0.0,
                ..Fit::default()
            },
        )
        .expect("render");
        // The later mesh wins where they overlap.
        assert_eq!(frame.pixel(8, 8), [0, 255, 0, 255]);
    }

    #[test]
    fn a_transparent_tint_draws_nothing() {
        let mut mesh = quad(UVS);
        mesh.color = [1.0, 1.0, 1.0, 0.0];
        let frame = render(
            &[mesh],
            &[solid(2, 2, [255, 0, 0, 255])],
            8,
            8,
            [0, 0, 0, 0],
            &Fit::default(),
        )
        .expect("render");
        assert_eq!(frame.opaque_bounds(), None);
    }

    #[test]
    fn additive_lightens_what_is_already_there() {
        let mut mesh = quad(UVS);
        mesh.blend = Blend::Additive;
        mesh.color = [0.5, 0.0, 0.0, 1.0];
        let frame = render(
            &[mesh],
            &[solid(2, 2, [255, 255, 255, 255])],
            8,
            8,
            [0, 0, 0, 0],
            &Fit {
                margin: 0.0,
                ..Fit::default()
            },
        )
        .expect("render");
        // White texture tinted to half red, added to nothing.
        let p = frame.pixel(4, 4);
        assert!(p[0] > 120 && p[1] < 10 && p[2] < 10, "{p:?}");
    }

    #[test]
    fn a_premultiplied_texture_gives_back_its_colour() {
        // Half-alpha red, stored premultiplied, so the stored value is (128,0,0,128).
        let texture = solid(2, 2, [128, 0, 0, 128]);
        let frame = render(
            &[quad(UVS)],
            &[texture],
            8,
            8,
            [0, 0, 0, 0],
            &Fit {
                margin: 0.0,
                premultiplied_textures: true,
            },
        )
        .expect("render");
        let p = frame.pixel(4, 4);
        // Un-premultiplied red is back near full, at about half coverage.
        assert!(p[0] > 200, "the colour is recovered: {p:?}");
        assert!((120..=136).contains(&p[3]), "the coverage is kept: {p:?}");
    }

    #[test]
    fn a_mesh_that_indexes_a_missing_vertex_is_refused() {
        let mut mesh = quad(UVS);
        mesh.indices = vec![0, 1, 99];
        let err = render(
            &[mesh],
            &[solid(2, 2, [255, 255, 255, 255])],
            8,
            8,
            [0, 0, 0, 0],
            &Fit::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
    }

    #[test]
    fn a_missing_texture_is_refused_rather_than_drawn_black() {
        let err = render(&[quad(UVS)], &[], 8, 8, [0, 0, 0, 0], &Fit::default()).unwrap_err();
        assert!(err.to_string().contains("texture"), "{err}");
    }

    #[test]
    fn a_texture_with_the_wrong_number_of_bytes_is_refused() {
        let err = Texture::new(4, 4, vec![0; 10]).unwrap_err();
        assert!(err.to_string().contains("needs 64 bytes"), "{err}");
        assert!(Texture::new(0, 4, vec![]).is_err());
    }

    #[test]
    fn a_degenerate_triangle_draws_nothing_instead_of_dividing_by_zero() {
        let mesh = Mesh {
            vertices: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]],
            uvs: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]],
            indices: vec![0, 1, 2],
            color: [1.0, 1.0, 1.0, 1.0],
            blend: Blend::Normal,
            texture: 0,
        };
        let frame = render(
            &[mesh],
            &[solid(2, 2, [255, 0, 0, 255])],
            8,
            8,
            [0, 0, 0, 0],
            &Fit::default(),
        )
        .expect("render");
        assert_eq!(frame.opaque_bounds(), None);
    }
}
