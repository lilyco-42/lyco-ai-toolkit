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
//!     ModelEngine / BetterModel.
//!
//! This crate therefore builds bottom-up from the parts that are certain:
//! the GLB input side lives in [`glb`]. Bedrock geometry output follows once
//! the F2 target format is confirmed (see _RESEARCH/00-requirement.md Q1).

pub mod glb;

pub use glb::{Gltf, Glb};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glb_module_is_reachable_from_the_crate_root() {
        let err = Glb::parse(b"not a glb at all, definitely").unwrap_err();
        assert!(err.to_string().contains("not a GLB"), "{err}");
    }
}
