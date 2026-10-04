//! lyco-kaggle - run generation on Kaggle's free GPUs (F1).
//!
//! The division of labour this crate exists to enforce:
//!
//!   * Kaggle does the one thing nothing else in this workspace can - GPU
//!     inference - and it is free, which is why the generation path finally
//!     closes without owning a graphics card.
//!   * Everything that is *knowledge* rather than compute stays in this
//!     repository, where it is covered by tests. In particular the notebook
//!     this crate generates contains no UV table at all: the overlay repair and
//!     the preview assembly live in `lyco-mc-skin`, so there is exactly one
//!     copy of that knowledge instead of two that can drift.
//!
//! A job is a notebook plus a kernel-metadata.json, both generated from Rust
//! rather than edited in a browser.

pub mod metadata;
pub mod notebook;
pub mod runner;
pub mod skin;

pub use metadata::{KernelMetadata, T4};
pub use notebook::{Cell, Notebook};
pub use runner::{parse_status, wait_until_terminal, Kaggle, Status};
pub use skin::{
    SkinKernelConfig, DEFAULT_GUIDANCE, DEFAULT_MODEL, DEFAULT_REFERENCE, DEFAULT_STEPS,
};
