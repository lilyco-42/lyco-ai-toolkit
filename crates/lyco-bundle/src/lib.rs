//! lyco-bundle - assemble a complete character bundle (S5).
//!
//! See [`bundle`] for the orchestration. This crate is the only one that
//! depends on all four capability crates, which keeps the dependency graph a
//! tree rather than a web.

pub mod bundle;

pub use bundle::{build, Artifact, BundleRequest, Manifest};
