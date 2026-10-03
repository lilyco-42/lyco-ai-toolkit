//! lyco-lyrics - lyrics generation and structural validation (F8).
//!
//!   * [`lyrics`] - the document model, its parser and its structural validator
//!   * [`backend`] - the generation trait, an offline `stub`, and a subprocess
//!     backend for a local model or an API wrapper

pub mod backend;
pub mod lyrics;

pub use backend::{
    backend_from_spec, placeholder_lyrics, verify_lyrics, CommandBackend, LyricsBackend,
    LyricsRequest, LyricsResult, StubBackend,
};
pub use lyrics::{split_marker, Issue, Lyrics, Section, SectionKind, Severity};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-lyrics-integration");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn a_sheet_survives_write_then_parse_then_render() {
        let req = LyricsRequest::new("harbour at dusk", tmp("cycle.txt"));
        StubBackend.generate(&req).expect("generate");

        let text = std::fs::read_to_string(&req.out_path).expect("read");
        let first = Lyrics::parse(&text).expect("parse");
        let second = Lyrics::parse(&first.render()).expect("re-parse");
        assert_eq!(first, second);
        assert!(first.ok());
    }
}
