//! Lyrics as a structured document (F8).
//!
//! An LLM returns free text, so the engineering here is not generation but
//! **constraining**: parse the text into sections, then validate the structure.
//!
//! The bracket-marker convention is the one Suno and ACE-Step already accept,
//! which means a lyric sheet produced here can be handed straight to the BGM
//! backend as its prompt without translation:
//!
//!     [Verse 1]
//!     a line
//!     another line
//!
//!     [Chorus]
//!     ...

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Severity of a structural finding. Mirrors `lyco_mc_skin::Severity` so the
/// CLI can render both the same way.
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

/// The section roles a song is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionKind {
    Intro,
    Verse,
    PreChorus,
    Chorus,
    Bridge,
    Outro,
    /// A marker we do not recognise; kept so round-tripping is lossless.
    Other,
}

impl SectionKind {
    pub fn marker(self) -> &'static str {
        match self {
            SectionKind::Intro => "Intro",
            SectionKind::Verse => "Verse",
            SectionKind::PreChorus => "Pre-Chorus",
            SectionKind::Chorus => "Chorus",
            SectionKind::Bridge => "Bridge",
            SectionKind::Outro => "Outro",
            SectionKind::Other => "Other",
        }
    }

    pub fn is_chorus(self) -> bool {
        self == SectionKind::Chorus
    }
}

fn normalize(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut last_space = true;
    for ch in inner.chars() {
        let c = if ch == '-' || ch == '_' { ' ' } else { ch };
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.extend(c.to_lowercase());
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// Split a marker like `Verse 1` into its role and optional label.
pub fn split_marker(inner: &str) -> (SectionKind, Option<String>) {
    // Two-word kinds are tested first so that a pre-chorus splits correctly.
    const KINDS: [(&str, SectionKind); 6] = [
        ("pre chorus", SectionKind::PreChorus),
        ("intro", SectionKind::Intro),
        ("verse", SectionKind::Verse),
        ("chorus", SectionKind::Chorus),
        ("bridge", SectionKind::Bridge),
        ("outro", SectionKind::Outro),
    ];
    let normalized = normalize(inner);
    for (name, kind) in KINDS {
        if normalized == name {
            return (kind, None);
        }
        if let Some(rest) = normalized.strip_prefix(name) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return (kind, Some(rest.to_string()));
            }
        }
    }
    let raw = inner.trim();
    if raw.is_empty() {
        (SectionKind::Other, None)
    } else {
        (SectionKind::Other, Some(raw.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub kind: SectionKind,
    /// Qualifier from the marker, e.g. the 1 in [Verse 1].
    pub label: Option<String>,
    pub lines: Vec<String>,
}

impl Section {
    pub fn new(kind: SectionKind, lines: Vec<String>) -> Self {
        Section {
            kind,
            label: None,
            lines,
        }
    }

    /// The marker as it is written back out, e.g. Verse 1.
    ///
    /// An unrecognised marker is written back verbatim, so a sheet we did not
    /// fully understand still survives a parse/render round trip unchanged.
    pub fn marker(&self) -> String {
        match (&self.kind, &self.label) {
            (SectionKind::Other, Some(raw)) => raw.clone(),
            (_, Some(label)) => format!("{} {label}", self.kind.marker()),
            (_, None) => self.kind.marker().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Lyrics {
    pub sections: Vec<Section>,
}

impl Lyrics {
    /// Parse bracket-marker lyric text.
    ///
    /// Text before the first marker is an error rather than being dropped:
    /// silently discarding a model's output is how a bad sheet ships.
    pub fn parse(text: &str) -> Result<Self> {
        let mut sections: Vec<Section> = Vec::new();
        let mut current: Option<Section> = None;

        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            let number = index + 1;
            if line.is_empty() {
                continue;
            }
            if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if let Some(done) = current.take() {
                    sections.push(done);
                }
                let (kind, label) = split_marker(inner);
                current = Some(Section {
                    kind,
                    label,
                    lines: Vec::new(),
                });
                continue;
            }
            match current.as_mut() {
                Some(section) => section.lines.push(line.to_string()),
                None => bail!("line {number} has lyrics before any [Section] marker: {line}"),
            }
        }
        if let Some(done) = current.take() {
            sections.push(done);
        }
        Ok(Lyrics { sections })
    }

    /// Render back to bracket-marker text, newline-terminated.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push('[');
            out.push_str(&section.marker());
            out.push_str("]\n");
            for line in &section.lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }

    pub fn total_lines(&self) -> usize {
        self.sections.iter().map(|s| s.lines.len()).sum()
    }

    pub fn first(&self, kind: SectionKind) -> Option<&Section> {
        self.sections.iter().find(|s| s.kind == kind)
    }

    pub fn count(&self, kind: SectionKind) -> usize {
        self.sections.iter().filter(|s| s.kind == kind).count()
    }

    pub fn ok(&self) -> bool {
        !self
            .validate()
            .iter()
            .any(|i| i.severity == Severity::Error)
    }

    /// Structural checks. These are about shape, not about whether the words
    /// are any good - that is a judgement no validator should pretend to make.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();

        if self.sections.is_empty() {
            issues.push(Issue {
                severity: Severity::Error,
                code: "no-sections",
                message: "there are no [Section] markers at all".to_string(),
            });
            return issues;
        }

        for (i, section) in self.sections.iter().enumerate() {
            if section.lines.is_empty() {
                issues.push(Issue {
                    severity: Severity::Error,
                    code: "empty-section",
                    message: format!("section {} [{}] has no lines", i + 1, section.marker()),
                });
            }
            if section.kind == SectionKind::Other {
                issues.push(Issue {
                    severity: Severity::Warn,
                    code: "unknown-marker",
                    message: format!("section {} has an unrecognised marker", i + 1),
                });
            }
            for (j, line) in section.lines.iter().enumerate() {
                if line.trim() != line {
                    issues.push(Issue {
                        severity: Severity::Warn,
                        code: "untrimmed-line",
                        message: format!("section {} line {} has whitespace", i + 1, j + 1),
                    });
                }
                if line.chars().count() > 120 {
                    issues.push(Issue {
                        severity: Severity::Warn,
                        code: "long-line",
                        message: format!("section {} line {} is over 120 chars", i + 1, j + 1),
                    });
                }
            }
        }

        if self.count(SectionKind::Chorus) == 0 {
            issues.push(Issue {
                severity: Severity::Warn,
                code: "no-chorus",
                message: "a song without a [Chorus] is unusual".to_string(),
            });
        }

        // Two identical sections back to back usually means a broken template.
        for pair in self.sections.windows(2) {
            let same_kind = pair[0].kind == pair[1].kind;
            let same_label = pair[0].label == pair[1].label;
            if same_kind && same_label && pair[0].kind != SectionKind::Chorus {
                issues.push(Issue {
                    severity: Severity::Warn,
                    code: "repeated-section",
                    message: format!("two consecutive [{}] sections", pair[0].marker()),
                });
            }
        }

        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        "[Verse 1]\n",
        "first line\n",
        "second line\n",
        "\n",
        "[Chorus]\n",
        "hook one\n",
        "hook two\n",
        "\n",
        "[Verse 2]\n",
        "third line\n",
        "\n",
        "[Chorus]\n",
        "hook one\n",
        "hook two\n",
    );

    #[test]
    fn parses_the_bracket_marker_convention() {
        let lyrics = Lyrics::parse(SAMPLE).expect("parse");
        assert_eq!(lyrics.sections.len(), 4);
        assert_eq!(lyrics.sections[0].kind, SectionKind::Verse);
        assert_eq!(lyrics.sections[0].label.as_deref(), Some("1"));
        assert_eq!(lyrics.sections[1].kind, SectionKind::Chorus);
        assert_eq!(lyrics.sections[1].label, None);
        assert_eq!(lyrics.total_lines(), 7);
        assert_eq!(lyrics.count(SectionKind::Chorus), 2);
    }

    #[test]
    fn round_trips_through_render() {
        let lyrics = Lyrics::parse(SAMPLE).expect("parse");
        let again = Lyrics::parse(&lyrics.render()).expect("re-parse");
        assert_eq!(lyrics, again);
        assert!(lyrics.render().ends_with('\n'));
    }

    #[test]
    fn accepts_marker_spelling_variants() {
        let cases = [
            ("[pre-chorus]", SectionKind::PreChorus),
            ("[Pre Chorus 2]", SectionKind::PreChorus),
            ("[CHORUS]", SectionKind::Chorus),
            ("[intro]", SectionKind::Intro),
            ("[Bridge]", SectionKind::Bridge),
            ("[outro]", SectionKind::Outro),
        ];
        for (marker, kind) in cases {
            let text = format!("{marker}\na line\n");
            let lyrics = Lyrics::parse(&text).expect("parse");
            assert_eq!(lyrics.sections[0].kind, kind, "{marker}");
        }
    }

    #[test]
    fn keeps_an_unrecognised_marker_instead_of_dropping_it() {
        let lyrics = Lyrics::parse("[Screaming]\nhey\n").expect("parse");
        assert_eq!(lyrics.sections[0].kind, SectionKind::Other);
        assert_eq!(lyrics.sections[0].label.as_deref(), Some("Screaming"));
        assert!(lyrics.ok(), "only a warning: {:?}", lyrics.validate());

        // The original spelling must survive, not become [Other screaming].
        let rendered = lyrics.render();
        assert!(rendered.contains("[Screaming]"), "{rendered}");
        assert_eq!(Lyrics::parse(&rendered).expect("re-parse"), lyrics);
    }

    #[test]
    fn rejects_lyrics_before_the_first_marker() {
        let err = Lyrics::parse("orphan line\n[Verse]\nok\n").unwrap_err();
        assert!(err.to_string().contains("before any"), "{err}");
    }

    #[test]
    fn empty_section_is_an_error() {
        let lyrics = Lyrics::parse("[Verse]\n\n[Chorus]\nhook\n").expect("parse");
        assert!(!lyrics.ok());
        let codes: Vec<_> = lyrics.validate().iter().map(|i| i.code).collect();
        assert!(codes.contains(&"empty-section"), "{codes:?}");
    }

    #[test]
    fn no_sections_is_an_error() {
        let lyrics = Lyrics::parse("\n\n").expect("parse");
        assert!(!lyrics.ok());
        assert_eq!(lyrics.validate()[0].code, "no-sections");
    }

    #[test]
    fn a_song_without_a_chorus_warns_but_passes() {
        let lyrics = Lyrics::parse("[Verse]\nonly a verse\n").expect("parse");
        assert!(lyrics.ok(), "warning only: {:?}", lyrics.validate());
        let codes: Vec<_> = lyrics.validate().iter().map(|i| i.code).collect();
        assert!(codes.contains(&"no-chorus"), "{codes:?}");
    }

    #[test]
    fn flags_repeated_consecutive_sections_but_not_choruses() {
        let repeated = Lyrics::parse("[Verse]\na\n[Verse]\nb\n").expect("parse");
        let codes: Vec<_> = repeated.validate().iter().map(|i| i.code).collect();
        assert!(codes.contains(&"repeated-section"), "{codes:?}");

        // A chorus repeating is the point of a chorus.
        let chorus_twice = Lyrics::parse("[Chorus]\na\n[Chorus]\na\n").expect("parse");
        let codes: Vec<_> = chorus_twice.validate().iter().map(|i| i.code).collect();
        assert!(!codes.contains(&"repeated-section"), "{codes:?}");
    }

    #[test]
    fn a_clean_sheet_has_no_issues() {
        let lyrics = Lyrics::parse(SAMPLE).expect("parse");
        assert!(lyrics.validate().is_empty(), "{:?}", lyrics.validate());
    }
}
