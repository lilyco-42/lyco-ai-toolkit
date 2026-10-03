//! Lyrics generation backends (F8).
//!
//! Same shape as the audio and image backends, and the same rule: the produced
//! file is parsed and validated rather than trusted. For lyrics that check has
//! real teeth, because a language model asked for a structured song sheet will
//! cheerfully return prose.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::lyrics::{Lyrics, Section, SectionKind, Severity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsRequest {
    pub prompt: String,
    pub out_path: PathBuf,
}

impl LyricsRequest {
    pub fn new(prompt: impl Into<String>, out_path: impl Into<PathBuf>) -> Self {
        LyricsRequest {
            prompt: prompt.into(),
            out_path: out_path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsResult {
    pub path: PathBuf,
    pub backend: String,
    pub sections: usize,
    pub lines: usize,
}

pub trait LyricsBackend {
    fn name(&self) -> &str;

    fn generate(&self, req: &LyricsRequest) -> Result<LyricsResult>;
}

/// Read a produced sheet back and refuse anything structurally broken.
pub fn verify_lyrics(path: &Path) -> Result<Lyrics> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("{} was never written", path.display()))?;
    let lyrics = Lyrics::parse(&text)
        .with_context(|| format!("{} is not a usable lyric sheet", path.display()))?;
    let issues = lyrics.validate();
    if let Some(error) = issues.iter().find(|i| i.severity == Severity::Error) {
        bail!("{}: {} - {}", path.display(), error.code, error.message);
    }
    if lyrics.total_lines() == 0 {
        bail!("{} contains no lyric lines", path.display());
    }
    Ok(lyrics)
}

/// 64-bit FNV-1a, matching the copies in the audio and image crates.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const OPENERS: [&str; 8] = [
    "Holding on",
    "Falling through",
    "Counting days",
    "Somewhere quiet",
    "Every morning",
    "Half awake",
    "Letting go",
    "Still here",
];

const IMAGES: [&str; 8] = [
    "the window light",
    "an empty street",
    "the summer rain",
    "a paper moon",
    "the old piano",
    "your handwriting",
    "the last train",
    "a borrowed coat",
];

/// A deterministic placeholder sheet with a real verse/chorus/bridge shape.
///
/// The words are drawn from a fixed bank, so this is not songwriting. It exists
/// so the structure can be exercised offline, and every caller labels it.
pub fn placeholder_lyrics(prompt: &str) -> Lyrics {
    let seed = fnv1a(prompt.as_bytes());
    let bits = |shift: u32| ((seed >> shift) & 0x7) as usize;
    let line = |a: usize, b: usize| -> String {
        format!(
            "{}, {}",
            OPENERS[a % OPENERS.len()],
            IMAGES[b % IMAGES.len()]
        )
    };

    let chorus = vec![
        line(bits(0), bits(3)),
        line(bits(6), bits(9)),
        line(bits(12), bits(15)),
        line(bits(18), bits(21)),
    ];
    let verse_one = vec![
        line(bits(24), bits(27)),
        line(bits(30), bits(33)),
        line(bits(36), bits(39)),
        line(bits(42), bits(45)),
    ];
    let verse_two = vec![
        line(bits(48), bits(51)),
        line(bits(54), bits(57)),
        line(bits(3), bits(12)),
        line(bits(21), bits(30)),
    ];
    let bridge = vec![line(bits(9), bits(18)), line(bits(27), bits(36))];

    let numbered = |n: &str| Some(n.to_string());
    Lyrics {
        sections: vec![
            Section {
                kind: SectionKind::Verse,
                label: numbered("1"),
                lines: verse_one,
            },
            Section {
                kind: SectionKind::Chorus,
                label: None,
                lines: chorus.clone(),
            },
            Section {
                kind: SectionKind::Verse,
                label: numbered("2"),
                lines: verse_two,
            },
            Section {
                kind: SectionKind::Bridge,
                label: None,
                lines: bridge,
            },
            Section {
                kind: SectionKind::Chorus,
                label: None,
                lines: chorus,
            },
        ],
    }
}

/// Offline placeholder writer. Deterministic and clearly labelled.
#[derive(Debug, Clone, Copy, Default)]
pub struct StubBackend;

impl LyricsBackend for StubBackend {
    fn name(&self) -> &str {
        "stub"
    }

    fn generate(&self, req: &LyricsRequest) -> Result<LyricsResult> {
        let lyrics = placeholder_lyrics(&req.prompt);
        write_text(&req.out_path, &lyrics.render())?;
        let verified = verify_lyrics(&req.out_path)?;
        Ok(LyricsResult {
            path: req.out_path.clone(),
            backend: self.name().to_string(),
            sections: verified.sections.len(),
            lines: verified.total_lines(),
        })
    }
}

/// Run an external writer as a subprocess.
///
/// Invoked with `--prompt <text> --out <path>`. Point it at a local model or an
/// API wrapper; nothing here assumes which.
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

impl LyricsBackend for CommandBackend {
    fn name(&self) -> &str {
        &self.label
    }

    fn generate(&self, req: &LyricsRequest) -> Result<LyricsResult> {
        let (exe, rest) = self
            .program
            .split_first()
            .context("program list is empty")?;
        let status = std::process::Command::new(exe)
            .args(rest)
            .arg("--prompt")
            .arg(&req.prompt)
            .arg("--out")
            .arg(&req.out_path)
            .status()
            .with_context(|| {
                format!("cannot run '{}': is it installed and on PATH?", self.label)
            })?;
        if !status.success() {
            bail!("{} exited with {status}", self.label);
        }

        let verified = verify_lyrics(&req.out_path)?;
        Ok(LyricsResult {
            path: req.out_path.clone(),
            backend: self.label.clone(),
            sections: verified.sections.len(),
            lines: verified.total_lines(),
        })
    }
}

/// Resolve a spec into a backend. See the sibling crates for the same contract.
pub fn backend_from_spec(spec: &str) -> Result<Box<dyn LyricsBackend>> {
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

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-lyrics-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn stub_writes_a_structurally_valid_sheet() {
        let req = LyricsRequest::new("a quiet librarian", tmp("stub.txt"));
        let result = StubBackend.generate(&req).expect("generate");
        assert_eq!(result.backend, "stub");
        assert_eq!(result.sections, 5);
        assert!(result.lines >= 12);

        let lyrics = verify_lyrics(&req.out_path).expect("verify");
        assert_eq!(lyrics.count(SectionKind::Chorus), 2);
        assert!(lyrics.validate().is_empty(), "{:?}", lyrics.validate());
    }

    #[test]
    fn stub_is_deterministic_but_prompt_sensitive() {
        let a = LyricsRequest::new("same", tmp("a.txt"));
        let b = LyricsRequest::new("same", tmp("b.txt"));
        StubBackend.generate(&a).expect("a");
        StubBackend.generate(&b).expect("b");
        assert_eq!(
            std::fs::read_to_string(&a.out_path).unwrap(),
            std::fs::read_to_string(&b.out_path).unwrap()
        );

        let c = LyricsRequest::new("different", tmp("c.txt"));
        StubBackend.generate(&c).expect("c");
        assert_ne!(
            std::fs::read_to_string(&a.out_path).unwrap(),
            std::fs::read_to_string(&c.out_path).unwrap()
        );
    }

    #[test]
    fn a_chorus_repeats_verbatim_across_chorus_sections() {
        let lyrics = placeholder_lyrics("anything");
        let choruses: Vec<&Section> = lyrics
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::Chorus)
            .collect();
        assert_eq!(choruses.len(), 2);
        assert_eq!(choruses[0].lines, choruses[1].lines, "a chorus repeats");
    }

    #[test]
    fn verify_rejects_a_missing_file() {
        let path = tmp("absent.txt");
        let _ = std::fs::remove_file(&path);
        let err = verify_lyrics(&path).unwrap_err();
        assert!(err.to_string().contains("never written"), "{err}");
    }

    #[test]
    fn verify_rejects_prose_with_no_markers() {
        let path = tmp("prose.txt");
        std::fs::write(&path, "Sure! Here is a song about a librarian.\n").unwrap();
        let err = verify_lyrics(&path).unwrap_err();
        assert!(err.to_string().contains("usable lyric sheet"), "{err}");
    }

    #[test]
    fn verify_rejects_an_empty_section() {
        let path = tmp("empty-section.txt");
        std::fs::write(&path, "[Verse]\n[Chorus]\nhook\n").unwrap();
        let err = verify_lyrics(&path).unwrap_err();
        assert!(err.to_string().contains("empty-section"), "{err}");
    }

    #[test]
    fn spec_resolves_to_each_backend() {
        assert_eq!(backend_from_spec("stub").unwrap().name(), "stub");
        let cmd = backend_from_spec("python3 gen.py").unwrap();
        assert_eq!(cmd.name(), "python3 gen.py");
        assert!(backend_from_spec("   ").is_err());
    }

    #[test]
    fn command_backend_reports_a_missing_binary_clearly() {
        let backend = CommandBackend::new(
            "local-llm",
            vec!["definitely-not-a-real-binary-xyz".to_string()],
        )
        .expect("construct");
        let req = LyricsRequest::new("x", tmp("nope.txt"));
        let err = backend.generate(&req).unwrap_err();
        assert!(err.to_string().contains("cannot run"), "{err}");
    }

    /// A writer that exits 0 but produces prose must still be rejected.
    #[cfg(unix)]
    #[test]
    fn a_successful_exit_with_unusable_output_is_still_a_failure() {
        let backend = CommandBackend::new(
            "liar",
            vec![
                "sh".to_string(),
                "-c".to_string(),
                [
                    "while [ $# -gt 0 ]; do",
                    "  case $1 in --out) o=$2; shift 2;; *) shift;; esac",
                    "done",
                    "printf 'just prose' > \"$o\"",
                ]
                .join("\n")
                .to_string(),
            ],
        )
        .expect("construct");
        let req = LyricsRequest::new("x", tmp("liar.txt"));
        let err = backend.generate(&req).unwrap_err();
        assert!(
            err.to_string().contains("usable lyric sheet"),
            "exit 0 with prose must not pass: {err}"
        );
    }
}
