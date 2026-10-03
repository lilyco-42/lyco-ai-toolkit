//! lyco-bsm - Minecraft model conversion (F2/F3).
//!
//! Scope note, and an important correction to the original research:
//!
//!   * There is **no** "BSM" model format. A GitHub search finds nothing by
//!     that name, and Blockbench's `js/formats/` has no such module.
//!   * **YSM is not a Blockbench format either.** It is the private format of
//!     the Yes Steve Model mod, and it is an *encrypted binary*, which is why
//!     the community maintains a dedicated decryption parser.
//!   * The open, verifiable interchange target is **Bedrock geometry**
//!     (`.geo.json`): plain JSON, importable by Blockbench, consumed by
//!     ModelEngine / BetterModel. F2 targets that format.
//!
//! Built bottom-up from the parts that are certain:
//!
//!   * [`bedrock`] - the Bedrock geometry data model, writer and validator
//!   * [`glb`] - the GLB (binary glTF 2.0) reader that feeds conversion

pub mod bedrock;
pub mod glb;

pub use bedrock::{
    Bone, Cube, Description, FaceUv, Geometry, GeometryFile, Problem, Uv, FACES, FORMAT_VERSION,
};
pub use glb::{Glb, Gltf};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glb_module_is_reachable_from_the_crate_root() {
        let err = Glb::parse(b"not a glb at all, definitely").unwrap_err();
        assert!(err.to_string().contains("not a GLB"), "{err}");
    }

    #[test]
    fn bedrock_module_is_reachable_from_the_crate_root() {
        let file = GeometryFile {
            format_version: FORMAT_VERSION.to_string(),
            geometry: Vec::new(),
        };
        assert_eq!(file.validate()[0].code, "no-geometry");
    }
}
