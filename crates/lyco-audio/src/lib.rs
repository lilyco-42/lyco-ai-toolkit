//! lyco-audio - background music generation (F6).
//!
//! Two layers:
//!
//!   * [`wav`] - a hand-rolled 16-bit PCM reader/writer. Reading matters as much
//!     as writing, because it is how generated output gets verified.
//!   * [`backend`] - the generation trait, an offline `stub`, and a subprocess
//!     backend for a real engine (ACE-Step) on a GPU host.

pub mod backend;
pub mod wav;

pub use backend::{
    backend_from_spec, verify_wav, AudioBackend, BgmRequest, BgmResult, CommandBackend,
    StubBackend, DURATION_TOLERANCE,
};
pub use wav::{decode, encode, DecodedWav, WavError, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-audio-integration");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn stub_output_decodes_to_stereo_44k_audio() {
        let req = BgmRequest::new("a quiet library at dusk", 1, tmp("integration.wav"));
        let result = StubBackend.generate(&req).expect("generate");
        let bytes = std::fs::read(&result.path).expect("read back");
        let decoded = wav::decode(&bytes).expect("decode");
        assert_eq!(decoded.channels, DEFAULT_CHANNELS);
        assert_eq!(decoded.sample_rate, DEFAULT_SAMPLE_RATE);
        assert!(decoded.peak() > 0);
        assert!((decoded.duration_secs() - 1.0).abs() < 0.01);
    }
}
