//! Image generation backends (F7).
//!
//! Mirrors `lyco-audio::backend` deliberately: the same two implementations and
//! the same rule that a backend's exit status is never taken as proof of
//! output. ComfyUI is the chosen engine but needs a GPU, so the offline path
//! has to exist for the pipeline to be exercisable here.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::render::placeholder_portrait;

pub const DEFAULT_WIDTH: u32 = 512;
pub const DEFAULT_HEIGHT: u32 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortraitRequest {
    pub prompt: String,
    pub out_path: PathBuf,
    pub width: u32,
    pub height: u32,
}

impl PortraitRequest {
    pub fn new(prompt: impl Into<String>, out_path: impl Into<PathBuf>) -> Self {
        PortraitRequest {
            prompt: prompt.into(),
            out_path: out_path.into(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResult {
    pub path: PathBuf,
    pub backend: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

pub trait ImageBackend {
    fn name(&self) -> &str;

    fn generate(&self, req: &PortraitRequest) -> Result<ImageResult>;
}

/// Decode the produced file and assert it is a usable image of the right size.
pub fn verify_image(path: &Path, req: &PortraitRequest) -> Result<image::RgbaImage> {
    let img = image::open(path)
        .with_context(|| format!("{} is not a readable image", path.display()))?
        .to_rgba8();
    if img.width() != req.width || img.height() != req.height {
        bail!(
            "{} is {}x{}, expected {}x{}",
            path.display(),
            img.width(),
            img.height(),
            req.width,
            req.height
        );
    }
    // A flat canvas is not a portrait, so catch a backend that "succeeds" by
    // writing one.
    let mut distinct = std::collections::BTreeSet::new();
    for px in img.pixels() {
        distinct.insert((px[0], px[1], px[2], px[3]));
        if distinct.len() > 1 {
            break;
        }
    }
    if distinct.len() <= 1 {
        bail!("{} is a single flat colour", path.display());
    }
    Ok(img)
}

/// Offline placeholder renderer. Deterministic, and labelled as a placeholder.
#[derive(Debug, Clone, Copy, Default)]
pub struct StubBackend;

impl ImageBackend for StubBackend {
    fn name(&self) -> &str {
        "stub"
    }

    fn generate(&self, req: &PortraitRequest) -> Result<ImageResult> {
        let pixels = placeholder_portrait(&req.prompt, req.width, req.height);
        let raw: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
        let img = image::RgbaImage::from_raw(req.width, req.height, raw)
            .context("placeholder buffer does not match the requested size")?;
        save_image(&req.out_path, &img)?;
        let decoded = verify_image(&req.out_path, req)?;
        Ok(ImageResult {
            path: req.out_path.clone(),
            backend: self.name().to_string(),
            width: decoded.width(),
            height: decoded.height(),
            bytes: std::fs::metadata(&req.out_path)?.len(),
        })
    }
}

/// Run an external generator as a subprocess.
///
/// Invoked with `--prompt <text> --width <n> --height <n> --out <path>`, which
/// is a shape any wrapper script can accept. Nothing here assumes a particular
/// engine, so pointing it at a ComfyUI driver is a configuration step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandBackend {
    label: String,
    program: Vec<String>,
}

impl CommandBackend {
    pub fn new(label: impl Into<String>, program: Vec<String>) -> Result<Self> {
        if program.is_empty() {
            bail!("a command backend needs at least a program name");
        }
        Ok(CommandBackend {
            label: label.into(),
            program,
        })
    }

    pub fn program(&self) -> &[String] {
        &self.program
    }
}

impl ImageBackend for CommandBackend {
    fn name(&self) -> &str {
        &self.label
    }

    fn generate(&self, req: &PortraitRequest) -> Result<ImageResult> {
        let (exe, rest) = self
            .program
            .split_first()
            .context("program list is empty")?;
        let status = std::process::Command::new(exe)
            .args(rest)
            .arg("--prompt")
            .arg(&req.prompt)
            .arg("--width")
            .arg(req.width.to_string())
            .arg("--height")
            .arg(req.height.to_string())
            .arg("--out")
            .arg(&req.out_path)
            .status()
            .with_context(|| {
                format!("cannot run '{}': is it installed and on PATH?", self.label)
            })?;
        if !status.success() {
            bail!("{} exited with {status}", self.label);
        }

        // The exit status is not evidence. Decode the file and check it.
        let decoded = verify_image(&req.out_path, req)?;
        Ok(ImageResult {
            path: req.out_path.clone(),
            backend: self.label.clone(),
            width: decoded.width(),
            height: decoded.height(),
            bytes: std::fs::metadata(&req.out_path)?.len(),
        })
    }
}

fn save_image(path: &Path, img: &image::RgbaImage) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    img.save(path)
        .with_context(|| format!("cannot write {}", path.display()))
}

/// Resolve a spec into a backend. See `lyco_audio::backend_from_spec`.
pub fn backend_from_spec(spec: &str) -> Result<Box<dyn ImageBackend>> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        bail!("a backend spec must not be empty");
    }
    if trimmed.eq_ignore_ascii_case("stub") {
        return Ok(Box::new(StubBackend));
    }
    let program: Vec<String> = trimmed.split_whitespace().map(str::to_string).collect();
    let backend = CommandBackend::new(trimmed.to_string(), program)?;
    Ok(Box::new(backend))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-image-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn stub_writes_a_png_of_the_requested_size() {
        let req = PortraitRequest::new("a quiet librarian", tmp("portrait.png"));
        let result = StubBackend.generate(&req).expect("generate");
        assert_eq!(result.backend, "stub");
        assert_eq!(result.width, DEFAULT_WIDTH);
        assert_eq!(result.height, DEFAULT_HEIGHT);
        assert!(result.bytes > 1000, "a real PNG, not an empty file");
    }

    #[test]
    fn stub_is_deterministic() {
        let a = PortraitRequest::new("same", tmp("det-a.png"));
        let b = PortraitRequest::new("same", tmp("det-b.png"));
        StubBackend.generate(&a).expect("a");
        StubBackend.generate(&b).expect("b");
        assert_eq!(
            std::fs::read(&a.out_path).unwrap(),
            std::fs::read(&b.out_path).unwrap()
        );
    }

    #[test]
    fn stub_honours_a_custom_size() {
        let req = PortraitRequest::new("small", tmp("small.png")).with_size(96, 128);
        let result = StubBackend.generate(&req).expect("generate");
        assert_eq!(result.width, 96);
        assert_eq!(result.height, 128);
    }

    #[test]
    fn verify_rejects_the_wrong_size() {
        let path = tmp("wrong-size.png");
        let small = PortraitRequest::new("x", &path).with_size(32, 32);
        StubBackend.generate(&small).expect("generate");
        let expect_big = PortraitRequest::new("x", &path);
        let err = verify_image(&path, &expect_big).unwrap_err();
        assert!(err.to_string().contains("expected"), "{err}");
    }

    #[test]
    fn verify_rejects_a_flat_canvas() {
        let path = tmp("flat.png");
        let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([10, 20, 30, 255]));
        save_image(&path, &img).expect("save");
        let req = PortraitRequest::new("x", &path).with_size(16, 16);
        let err = verify_image(&path, &req).unwrap_err();
        assert!(err.to_string().contains("flat colour"), "{err}");
    }

    #[test]
    fn verify_rejects_a_non_image() {
        let path = tmp("notimage.png");
        std::fs::write(&path, b"definitely not a PNG payload").unwrap();
        let req = PortraitRequest::new("x", &path).with_size(16, 16);
        let err = verify_image(&path, &req).unwrap_err();
        assert!(err.to_string().contains("readable image"), "{err}");
    }

    #[test]
    fn spec_resolves_to_each_backend() {
        assert_eq!(backend_from_spec("stub").unwrap().name(), "stub");
        let cmd = backend_from_spec("python3 gen.py").unwrap();
        assert_eq!(cmd.name(), "python3 gen.py");
        assert!(backend_from_spec("   ").is_err());
    }

    #[test]
    fn command_backend_needs_a_program() {
        assert!(CommandBackend::new("empty", Vec::new()).is_err());
    }

    #[test]
    fn command_backend_reports_a_missing_binary_clearly() {
        let backend = CommandBackend::new(
            "comfyui",
            vec!["definitely-not-a-real-binary-xyz".to_string()],
        )
        .expect("construct");
        assert_eq!(backend.name(), "comfyui");
        let req = PortraitRequest::new("x", tmp("nope.png"));
        let err = backend.generate(&req).unwrap_err();
        assert!(err.to_string().contains("cannot run"), "{err}");
    }

    /// A generator that exits 0 without writing anything must still fail.
    #[cfg(unix)]
    #[test]
    fn a_successful_exit_without_output_is_still_a_failure() {
        let backend = CommandBackend::new(
            "liar",
            vec!["sh".to_string(), "-c".to_string(), "exit 0".to_string()],
        )
        .expect("construct");
        let req = PortraitRequest::new("x", tmp("never-written.png"));
        let _ = std::fs::remove_file(&req.out_path);
        let err = backend.generate(&req).unwrap_err();
        assert!(
            err.to_string().contains("readable image"),
            "exit 0 with no file must not pass: {err}"
        );
    }
}
