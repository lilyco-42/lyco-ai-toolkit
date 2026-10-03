//! A Minecraft skin texture plus structural validation.
//!
//! Validation is deliberately geometry-first: it proves the part/face table in
//! `format` maps onto the atlas correctly and that a file is a legal skin,
//! independent of whether any artwork in it is good.

use crate::format::{self, ArmModel, Face, Layer, Part, Rect, SKIN_HEIGHT, SKIN_WIDTH};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

/// Result of validating a skin. `ok()` is true when no Error-severity issue exists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub issues: Vec<Issue>,
    pub width: u32,
    pub height: u32,
    pub opaque_pixels: usize,
    pub distinct_colors: usize,
    pub detected_arm: Option<ArmModel>,
}

impl Report {
    pub fn ok(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn count(&self, s: Severity) -> usize {
        self.issues.iter().filter(|i| i.severity == s).count()
    }

    fn push(&mut self, severity: Severity, code: &'static str, message: String) {
        self.issues.push(Issue { severity, code, message });
    }
}

/// An RGBA skin texture held as row-major pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skin {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}

impl Skin {
    /// Build from RGBA bytes; the buffer length must match width*height*4.
    pub fn from_rgba_bytes(width: u32, height: u32, bytes: &[u8]) -> Result<Self> {
        let expected = (width as usize) * (height as usize) * 4;
        if bytes.len() != expected {
            bail!(
                "pixel buffer is {} bytes but {}x{} RGBA needs {}",
                bytes.len(),
                width,
                height,
                expected
            );
        }
        let pixels = bytes
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        Ok(Skin { width, height, pixels })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y as usize) * (self.width as usize) + (x as usize)]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = (y as usize) * (self.width as usize) + (x as usize);
        self.pixels[i] = c;
    }

    /// A 64x64 skin with every pixel fully transparent.
    pub fn blank() -> Self {
        Skin {
            width: SKIN_WIDTH,
            height: SKIN_HEIGHT,
            pixels: vec![[0, 0, 0, 0]; (SKIN_WIDTH * SKIN_HEIGHT) as usize],
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let img = image::open(path)
            .with_context(|| format!("cannot open image {}", path.display()))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        Skin::from_rgba_bytes(w, h, &img.into_raw())
            .with_context(|| format!("cannot decode {} as RGBA", path.display()))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let raw: Vec<u8> = self.pixels.iter().flat_map(|p| p.iter().copied()).collect();
        let img = image::RgbaImage::from_raw(self.width, self.height, raw)
            .context("pixel buffer does not match image dimensions")?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
        }
        img.save(path)
            .with_context(|| format!("cannot write {}", path.display()))
    }

    /// Fill one face of one part/layer with a solid color.
    pub fn fill_face(&mut self, part: Part, layer: Layer, face: Face, arm: ArmModel, c: [u8; 4]) {
        let r = format::part_box(part, layer, arm).face(face);
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                if x < self.width && y < self.height {
                    self.set_pixel(x, y, c);
                }
            }
        }
    }

    /// Copy the pixels of one face out as a row-major block.
    pub fn read_face(&self, part: Part, layer: Layer, face: Face, arm: ArmModel) -> Vec<[u8; 4]> {
        let r = format::part_box(part, layer, arm).face(face);
        let mut out = Vec::with_capacity((r.w * r.h) as usize);
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                out.push(self.pixel(x, y));
            }
        }
        out
    }

    /// A structurally valid skin: base layer filled with distinct solid colors,
    /// overlay layer and the unused atlas region left transparent.
    pub fn template(arm: ArmModel) -> Self {
        let mut s = Skin::blank();
        let tints: [[u8; 4]; 6] = [
            [201, 142, 103, 255], // head  - skin tone
            [86, 130, 190, 255],  // body  - shirt
            [72, 110, 165, 255],  // right arm
            [72, 110, 165, 255],  // left arm
            [58, 62, 82, 255],    // right leg
            [58, 62, 82, 255],    // left leg
        ];
        for (i, part) in Part::ALL.iter().enumerate() {
            for face in Face::ALL {
                // Shade faces slightly so the template reads as a 3D model.
                let mut c = tints[i];
                let k = match face {
                    Face::Top => 20i16,
                    Face::Front => 0,
                    Face::Left | Face::Right => -12,
                    Face::Back => -6,
                    Face::Bottom => -24,
                };
                for ch in 0..3 {
                    c[ch] = (c[ch] as i16 + k).clamp(0, 255) as u8;
                }
                s.fill_face(*part, Layer::Base, face, arm, c);
            }
        }
        s
    }

    /// Total number of opaque pixels.
    pub fn opaque_count(&self) -> usize {
        self.pixels.iter().filter(|p| p[3] >= 128).count()
    }

    /// True when every pixel in the rectangle is fully transparent.
    pub fn rect_is_transparent(&self, r: Rect) -> bool {
        for y in r.y..r.bottom().min(self.height) {
            for x in r.x..r.right().min(self.width) {
                if self.pixel(x, y)[3] >= 128 {
                    return false;
                }
            }
        }
        true
    }

    /// Heuristic model detection.
    ///
    /// Classic arms are 4px wide and slim arms 3px, which shifts the arm's
    /// unwrapped columns. The front face is *not* a usable discriminator: the
    /// column classic gains on the front is re-covered by the slim model's
    /// right-side face, so probing it always reports classic. The arm's back
    /// face is the reliable tell, because classic ends two columns further
    /// right than slim and nothing in the slim layout reaches that tail.
    pub fn detect_arm_model(&self) -> Option<ArmModel> {
        if self.width < SKIN_WIDTH || self.height < SKIN_HEIGHT {
            return None;
        }
        let classic_back =
            format::part_box(Part::RightArm, Layer::Base, ArmModel::Classic).face(Face::Back);
        let slim_back =
            format::part_box(Part::RightArm, Layer::Base, ArmModel::Slim).face(Face::Back);
        let tail = Rect::new(
            slim_back.right(),
            classic_back.y,
            classic_back.right() - slim_back.right(),
            classic_back.h,
        );
        if self.rect_is_transparent(tail) {
            Some(ArmModel::Slim)
        } else {
            Some(ArmModel::Classic)
        }
    }

    /// Structural validation, independent of artwork quality.
    pub fn validate(&self) -> Report {
        let mut rep = Report {
            width: self.width,
            height: self.height,
            opaque_pixels: self.opaque_count(),
            distinct_colors: crate::palette::distinct_opaque_colors(&self.pixels),
            detected_arm: self.detect_arm_model(),
            ..Default::default()
        };

        // 1. Dimensions.
        if (self.width, self.height) == (SKIN_WIDTH, SKIN_HEIGHT) {
            rep.push(Severity::Info, "dims", "64x64 modern skin".to_string());
        } else if (self.width, self.height) == (SKIN_WIDTH, 32) {
            rep.push(
                Severity::Warn,
                "dims-legacy",
                "64x32 legacy skin: the left arm/leg are mirrored from the right".to_string(),
            );
        } else {
            rep.push(
                Severity::Error,
                "dims-invalid",
                format!(
                    "expected 64x64 (or legacy 64x32), got {}x{}",
                    self.width, self.height
                ),
            );
            return rep;
        }

        // 2. Every box this format actually uses must fit inside the texture.
        //    A 64x32 legacy skin has no overlay layer and no left limbs, so
        //    testing the modern 12-box set against it would raise false alarms.
        let legacy = self.height == 32;
        let checked: Vec<(Part, Layer, format::BoxUv)> = if legacy {
            [Part::Head, Part::Body, Part::RightArm, Part::RightLeg]
                .iter()
                .map(|p| (*p, Layer::Base, format::part_box(*p, Layer::Base, ArmModel::Classic)))
                .collect()
        } else {
            let mut v = Vec::new();
            for arm in [ArmModel::Classic, ArmModel::Slim] {
                v.extend(format::all_boxes(arm));
            }
            v
        };

        let mut fits = true;
        for (part, layer, b) in &checked {
            let r = b.bounds();
            if r.right() > self.width || r.bottom() > self.height {
                fits = false;
                rep.push(
                    Severity::Error,
                    "uv-out-of-bounds",
                    format!(
                        "{} {} escapes the atlas at {:?}",
                        part.name(),
                        layer.name(),
                        r
                    ),
                );
            }
        }
        if fits {
            rep.push(
                Severity::Info,
                "uv-bounds",
                format!(
                    "all {} part boxes used by this format fit the atlas",
                    checked.len()
                ),
            );
        }

        // 3. Parts within one layer must not overlap.
        for layer in [Layer::Base, Layer::Overlay] {
            let boxes: Vec<(Part, Rect)> = Part::ALL
                .iter()
                .map(|p| (*p, format::part_box(*p, layer, ArmModel::Classic).bounds()))
                .collect();
            for i in 0..boxes.len() {
                for j in (i + 1)..boxes.len() {
                    if boxes[i].1.intersects(&boxes[j].1) {
                        rep.push(
                            Severity::Error,
                            "uv-overlap",
                            format!(
                                "{} and {} overlap in the {} layer",
                                boxes[i].0.name(),
                                boxes[j].0.name(),
                                layer.name()
                            ),
                        );
                    }
                }
            }
        }

        // 4. Some modern atlas space is officially unused; art there is ignored.
        if !legacy {
            let unused = Rect::new(56, 16, 8, 32);
            if !self.rect_is_transparent(unused) {
                rep.push(
                    Severity::Warn,
                    "unused-region-dirty",
                    format!("unused atlas region {:?} is not transparent", unused),
                );
            }
        }

        rep.push(
            Severity::Info,
            "palette",
            format!(
                "{} opaque pixels, {} distinct colors",
                rep.opaque_pixels, rep.distinct_colors
            ),
        );

        rep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_structurally_valid() {
        for arm in [ArmModel::Classic, ArmModel::Slim] {
            let s = Skin::template(arm);
            let rep = s.validate();
            assert!(rep.ok(), "template invalid: {:?}", rep.issues);
            assert_eq!(rep.count(Severity::Error), 0);
            assert_eq!(rep.width, 64);
            assert_eq!(rep.height, 64);
        }
    }

    #[test]
    fn template_leaves_overlay_and_unused_regions_empty() {
        let s = Skin::template(ArmModel::Classic);
        assert!(s.rect_is_transparent(Rect::new(56, 16, 8, 32)), "unused region");
        // Overlay head lives at (32,0)-(64,16).
        assert!(s.rect_is_transparent(Rect::new(32, 0, 32, 16)), "head overlay");
        // Overlay body lives at (16,32)-(40,48).
        assert!(s.rect_is_transparent(Rect::new(16, 32, 24, 16)), "body overlay");
    }

    #[test]
    fn template_fills_every_base_face() {
        let s = Skin::template(ArmModel::Classic);
        for part in Part::ALL {
            for face in Face::ALL {
                let px = s.read_face(part, Layer::Base, face, ArmModel::Classic);
                assert!(
                    px.iter().all(|p| p[3] == 255),
                    "{} {} has holes",
                    part.name(),
                    face.name()
                );
            }
        }
    }

    #[test]
    fn template_palette_is_small_and_coherent() {
        let s = Skin::template(ArmModel::Classic);
        let rep = s.validate();
        // 6 parts x 6 faces, each shaded from one of 4 base tints.
        assert!(rep.distinct_colors <= 36, "got {}", rep.distinct_colors);
        assert!(rep.distinct_colors >= 6, "got {}", rep.distinct_colors);
    }

    #[test]
    fn blank_skin_is_valid_but_empty() {
        let s = Skin::blank();
        let rep = s.validate();
        assert!(rep.ok());
        assert_eq!(rep.opaque_pixels, 0);
    }

    #[test]
    fn wrong_dimensions_are_an_error() {
        let s = Skin::from_rgba_bytes(32, 32, &vec![0u8; 32 * 32 * 4]).unwrap();
        let rep = s.validate();
        assert!(!rep.ok());
        assert_eq!(rep.count(Severity::Error), 1);
        assert_eq!(rep.issues[0].code, "dims-invalid");
    }

    #[test]
    fn legacy_64x32_warns_but_passes() {
        let s = Skin::from_rgba_bytes(64, 32, &vec![0u8; 64 * 32 * 4]).unwrap();
        let rep = s.validate();
        assert!(rep.ok(), "legacy should not be an error: {:?}", rep.issues);
        assert!(rep.issues.iter().any(|i| i.code == "dims-legacy"));
    }

    #[test]
    fn dirty_unused_region_warns() {
        let mut s = Skin::template(ArmModel::Classic);
        s.set_pixel(60, 20, [255, 0, 255, 255]);
        let rep = s.validate();
        assert!(rep.ok(), "should be a warning, not an error");
        assert!(rep.issues.iter().any(|i| i.code == "unused-region-dirty"));
    }

    #[test]
    fn detects_classic_when_arm_column_is_painted() {
        let s = Skin::template(ArmModel::Classic);
        assert_eq!(s.detect_arm_model(), Some(ArmModel::Classic));
    }

    #[test]
    fn detects_slim_when_arm_column_is_empty() {
        let s = Skin::template(ArmModel::Slim);
        assert_eq!(s.detect_arm_model(), Some(ArmModel::Slim));
    }

    #[test]
    fn classic_only_tail_is_the_two_columns_slim_never_touches() {
        let classic_back =
            format::part_box(Part::RightArm, Layer::Base, ArmModel::Classic).face(Face::Back);
        let slim_back =
            format::part_box(Part::RightArm, Layer::Base, ArmModel::Slim).face(Face::Back);
        let tail = Rect::new(
            slim_back.right(),
            classic_back.y,
            classic_back.right() - slim_back.right(),
            classic_back.h,
        );
        assert_eq!(tail, Rect::new(54, 20, 2, 12));
    }

    #[test]
    fn front_face_column_is_not_a_valid_model_discriminator() {
        // Regression guard: probing the column classic adds to the front face
        // always reports classic, because the slim model's right-side face
        // re-covers it. Detection must use the back-face tail instead.
        let classic_front =
            format::part_box(Part::RightArm, Layer::Base, ArmModel::Classic).face(Face::Front);
        let slim_right =
            format::part_box(Part::RightArm, Layer::Base, ArmModel::Slim).face(Face::Right);
        let last_classic_column = classic_front.right() - 1;
        assert_eq!(last_classic_column, slim_right.x, "faces are expected to abut");

        let slim = Skin::template(ArmModel::Slim);
        let probe = Rect::new(last_classic_column, classic_front.y, 1, classic_front.h);
        assert!(
            !slim.rect_is_transparent(probe),
            "a slim skin still paints that column, so it cannot discriminate"
        );
    }

    #[test]
    fn legacy_skin_is_checked_against_legacy_regions_only() {
        let bytes = vec![0u8; 64 * 32 * 4];
        let s = Skin::from_rgba_bytes(64, 32, &bytes).unwrap();
        let rep = s.validate();
        assert!(rep.ok(), "{:?}", rep.issues);
        let bounds = rep
            .issues
            .iter()
            .find(|i| i.code == "uv-bounds")
            .expect("expected a uv-bounds finding");
        assert!(
            bounds.message.contains("4 "),
            "legacy must check only its 4 regions, got: {}",
            bounds.message
        );
    }

    #[test]
    fn roundtrips_through_png() {
        let dir = std::env::temp_dir().join("lyco-mc-skin-test");
        let path = dir.join("template.png");
        let original = Skin::template(ArmModel::Classic);
        original.save(&path).expect("save");
        let loaded = Skin::load(&path).expect("load");
        assert_eq!(loaded.width(), 64);
        assert_eq!(loaded.height(), 64);
        assert_eq!(loaded, original, "PNG roundtrip must be lossless");
    }

    #[test]
    fn rejects_mismatched_buffer_length() {
        assert!(Skin::from_rgba_bytes(64, 64, &[0u8; 10]).is_err());
    }
}
