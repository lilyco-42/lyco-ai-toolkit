//! Background-music generation backends (F6).
//!
//! ACE-Step is the chosen engine (_RESEARCH/02-niche-knowledge.md section 3):
//! 8 GB VRAM, 34x real-time on a 4090, best-in-class Chinese support. But it
//! needs a GPU, and the machine this repo is developed on has none.
//!
//! So generation sits behind a trait with two implementations:
//!
//!   * [`StubBackend`] synthesises a short deterministic tone offline. It exists
//!     so the pipeline can be exercised end to end, and it is named `stub`
//!     everywhere it surfaces so it can never be mistaken for real music.
//!   * [`CommandBackend`] shells out to a generator the operator configures. It
//!     makes no assumption about any specific engine's command line.
//!
//! Both paths end in [`verify_wav`]: the produced file is decoded and checked
//! against the requested duration. A backend's exit status is never taken as
//! proof that usable audio exists.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::wav::{self, DecodedWav, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};

/// How much shorter than requested a generated file may be before it fails.
pub const DURATION_TOLERANCE: f64 = 0.9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BgmRequest {
    pub prompt: String,
    pub duration_secs: u32,
    pub out_path: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
}

impl BgmRequest {
    /// A request with the defaults, writing to `out_path`.
    pub fn new(
        prompt: impl Into<String>,
        duration_secs: u32,
        out_path: impl Into<PathBuf>,
    ) -> Self {
        BgmRequest {
            prompt: prompt.into(),
            duration_secs,
            out_path: out_path.into(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: DEFAULT_CHANNELS,
        }
    }

    /// The shortest acceptable duration for this request.
    pub fn min_duration_secs(&self) -> f64 {
        self.duration_secs as f64 * DURATION_TOLERANCE
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BgmResult {
    pub path: PathBuf,
    pub backend: String,
    pub duration_secs: f64,
    pub bytes: u64,
}

pub trait AudioBackend {
    fn name(&self) -> &str;

    fn generate(&self, req: &BgmRequest) -> Result<BgmResult>;
}

/// Decode a WAV and assert it is long enough to satisfy the request.
pub fn verify_wav(path: &Path, request: &BgmRequest) -> Result<DecodedWav> {
    let bytes =
        std::fs::read(path).with_context(|| format!("{} was never written", path.display()))?;
    let decoded = wav::decode(&bytes)
        .with_context(|| format!("{} is not a readable 16-bit PCM WAV", path.display()))?;
    let min = request.min_duration_secs();
    if decoded.duration_secs() + 1e-6 < min {
        bail!(
            "{} is only {:.2}s, expected at least {:.2}s",
            path.display(),
            decoded.duration_secs(),
            min
        );
    }
    if decoded.peak() == 0 {
        bail!("{} is entirely silent", path.display());
    }
    Ok(decoded)
}

/// Offline placeholder generator. Deterministic, and honest about what it is.
#[derive(Debug, Clone, Copy, Default)]
pub struct StubBackend;

impl AudioBackend for StubBackend {
    fn name(&self) -> &str {
        "stub"
    }

    fn generate(&self, req: &BgmRequest) -> Result<BgmResult> {
        let samples = synthesize(
            &req.prompt,
            req.duration_secs,
            req.sample_rate,
            req.channels,
        );
        let bytes = wav::encode(&samples, req.channels, req.sample_rate)?;
        write_bytes(&req.out_path, &bytes)?;
        let decoded = verify_wav(&req.out_path, req)?;
        Ok(BgmResult {
            path: req.out_path.clone(),
            backend: self.name().to_string(),
            duration_secs: decoded.duration_secs(),
            bytes: bytes.len() as u64,
        })
    }
}

/// A backend that runs an external generator as a subprocess.
///
/// The configured program is invoked with three trailing arguments,
/// `--prompt <text> --duration <secs> --out <path>`, which is a shape any
/// wrapper script can accept. Nothing here assumes a particular engine.
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

impl AudioBackend for CommandBackend {
    fn name(&self) -> &str {
        &self.label
    }

    fn generate(&self, req: &BgmRequest) -> Result<BgmResult> {
        let (exe, rest) = self
            .program
            .split_first()
            .context("program list is empty")?;
        let status = std::process::Command::new(exe)
            .args(rest)
            .arg("--prompt")
            .arg(&req.prompt)
            .arg("--duration")
            .arg(req.duration_secs.to_string())
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
        let decoded = verify_wav(&req.out_path, req)?;
        let bytes = std::fs::metadata(&req.out_path)?.len();
        Ok(BgmResult {
            path: req.out_path.clone(),
            backend: self.label.clone(),
            duration_secs: decoded.duration_secs(),
            bytes,
        })
    }
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    std::fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))
}

/// 64-bit FNV-1a, so the stub is deterministic across runs and platforms.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A quiet two-partial tone whose pitch is derived from the prompt.
///
/// This is a placeholder, not music. It is audible and non-silent so that a
/// produced file can be verified, and distinct prompts give distinct tones.
fn synthesize(prompt: &str, duration_secs: u32, sample_rate: u32, channels: u16) -> Vec<i16> {
    const SCALE: [f32; 5] = [220.0, 246.94, 293.66, 329.63, 392.0];
    let seed = fnv1a(prompt.as_bytes());
    let root = SCALE[(seed % 5) as usize];
    let fifth = root * 1.5;

    let frames = (sample_rate as u64).saturating_mul(duration_secs as u64) as usize;
    let mut out = Vec::with_capacity(frames * channels as usize);
    let two_pi = std::f32::consts::TAU;

    for i in 0..frames {
        let t = i as f32 / sample_rate as f32;
        let tone = (two_pi * root * t).sin() * 0.35 + (two_pi * fifth * t).sin() * 0.15;
        let tremolo = 0.75 + 0.25 * (two_pi * 0.5 * t).sin();
        let sample = (tone * tremolo * f32::from(i16::MAX) * 0.5) as i16;
        for _ in 0..channels {
            out.push(sample);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-audio-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn stub_produces_audio_of_the_requested_length() {
        let req = BgmRequest::new("a calm evening", 2, tmp("stub.wav"));
        let result = StubBackend.generate(&req).expect("generate");
        assert_eq!(result.backend, "stub");
        assert!((result.duration_secs - 2.0).abs() < 0.01, "{}", result.duration_secs);
        assert!(result.bytes > 44, "must contain samples, not just a header");
    }

    #[test]
    fn stub_is_deterministic() {
        let a = BgmRequest::new("same prompt", 1, tmp("a.wav"));
        let b = BgmRequest::new("same prompt", 1, tmp("b.wav"));
        StubBackend.generate(&a).expect("a");
        StubBackend.generate(&b).expect("b");
        let ba = std::fs::read(&a.out_path).unwrap();
        let bb = std::fs::read(&b.out_path).unwrap();
        assert_eq!(ba, bb, "identical prompts must give identical bytes");
    }

    #[test]
    fn different_prompts_give_different_audio() {
        let a = BgmRequest::new("alpha", 1, tmp("p1.wav"));
        let b = BgmRequest::new("bravo", 1, tmp("p2.wav"));
        StubBackend.generate(&a).expect("a");
        StubBackend.generate(&b).expect("b");
        let ba = std::fs::read(&a.out_path).unwrap();
        let bb = std::fs::read(&b.out_path).unwrap();
        assert_ne!(ba, bb);
    }

    #[test]
    fn verify_rejects_a_missing_file() {
        let req = BgmRequest::new("x", 1, tmp("definitely-absent.wav"));
        let err = verify_wav(&req.out_path, &req).unwrap_err();
        assert!(err.to_string().contains("never written"), "{err}");
    }

    #[test]
    fn verify_rejects_a_file_that_is_too_short() {
        let path = tmp("short.wav");
        let bytes = wav::encode(&vec![1000i16; 200], DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE);
        write_bytes(&path, &bytes.unwrap()).unwrap();
        let req = BgmRequest::new("x", 10, &path);
        let err = verify_wav(&path, &req).unwrap_err();
        assert!(err.to_string().contains("expected at least"), "{err}");
    }

    #[test]
    fn verify_rejects_silence() {
        let path = tmp("silent.wav");
        let frames = DEFAULT_SAMPLE_RATE as usize;
        let silent = vec![0i16; frames * DEFAULT_CHANNELS as usize];
        let bytes = wav::encode(&silent, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE).unwrap();
        write_bytes(&path, &bytes).unwrap();
        let req = BgmRequest::new("x", 1, &path);
        let err = verify_wav(&path, &req).unwrap_err();
        assert!(err.to_string().contains("silent"), "{err}");
    }

    #[test]
    fn verify_rejects_a_non_wav() {
        let path = tmp("notaudio.wav");
        write_bytes(&path, b"this is not audio at all, not even close").unwrap();
        let req = BgmRequest::new("x", 1, &path);
        let err = verify_wav(&path, &req).unwrap_err();
        assert!(err.to_string().contains("readable"), "{err}");
    }

    #[test]
    fn command_backend_needs_a_program() {
        assert!(CommandBackend::new("empty", Vec::new()).is_err());
    }

    #[test]
    fn command_backend_reports_a_missing_binary_clearly() {
        let backend = CommandBackend::new(
            "ace-step",
            vec!["definitely-not-a-real-binary-xyz".to_string()],
        )
        .expect("construct");
        assert_eq!(backend.name(), "ace-step");
        let req = BgmRequest::new("x", 1, tmp("whatever.wav"));
        let err = backend.generate(&req).unwrap_err();
        assert!(err.to_string().contains("cannot run"), "{err}");
    }

    /// End-to-end through the subprocess path using a shell script as the
    /// generator, which is exactly how a real engine would be wired in.
    #[cfg(unix)]
    #[test]
    fn command_backend_accepts_a_real_generator() {
        use std::io::Write;
        let script = tmp("fake-generator.sh");
        let body = [
            "#!/bin/sh",
            "while [ $# -gt 0 ]; do",
            "  case $1 in --out) out=$2; shift 2;; *) shift;; esac",
            "done",
            "printf 'RIFF' > \"$out\"",
            "",
        ]
        .join("\n");
        let mut f = std::fs::File::create(&script).expect("script");
        f.write_all(body.as_bytes()).expect("write");
        drop(f);
        // A generator that writes garbage must fail verification, not pass.
        // Go through `sh` so the test does not depend on the executable bit.
        let backend = CommandBackend::new(
            "fake",
            vec!["sh".to_string(), script.display().to_string()],
        )
        .expect("construct");
        let req = BgmRequest::new("x", 1, tmp("fake-out.wav"));
        let err = backend.generate(&req).unwrap_err();
        assert!(
            err.to_string().contains("readable"),
            "garbage output must be caught, got: {err}"
        );
    }
}
