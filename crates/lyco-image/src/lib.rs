//! lyco-image - portrait generation (F7).
//!
//!   * [`render`] - pure, deterministic placeholder pixels
//!   * [`backend`] - the generation trait, an offline `stub`, and a subprocess
//!     backend for ComfyUI on a GPU host

pub mod backend;
pub mod render;

pub use backend::{
    backend_from_spec, verify_image, CommandBackend, ImageBackend, ImageResult, PortraitRequest,
    StubBackend, DEFAULT_HEIGHT, DEFAULT_WIDTH,
};
pub use render::placeholder_portrait;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-image-integration");
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn stub_output_is_a_decodable_png_with_variation() {
        let req = PortraitRequest::new("dusk", tmp("integration.png")).with_size(128, 128);
        let result = StubBackend.generate(&req).expect("generate");
        let decoded = verify_image(&result.path, &req).expect("verify");
        let colours: std::collections::BTreeSet<_> = decoded.pixels().map(|p| p.0).collect();
        assert!(colours.len() > 8, "only {} colours", colours.len());
    }
}
