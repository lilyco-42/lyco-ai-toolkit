//! lyco-spine - Spine texture atlases: parse, inspect, normalize (F11).
//!
//! ## Where this knowledge comes from
//!
//! The Spine *atlas* format is public and documented by Esoteric Software; this
//! crate implements it from that format. What is not in the format spec is which
//! mistakes real-world dumps make, and that is worth writing down:
//!
//!   * Some dumps omit the `size` page field entirely. `spine-ts` then divides by
//!     zero and the renderer misplaces every region.
//!   * Some dumps write a `size` that disagrees with the actual PNG.
//!   * Enemy dumps from one particular exporter ship *premultiplied alpha*
//!     textures and say nothing about it, so the model renders with bright halos.
//!
//! Those three facts, and the shape of the repair, were learned by reading the
//! asset tooling of `sganggs/Stronghold-Protocol` (GPL-3.0) and its `docs/ASSETS.md`.
//! **No code was copied from it**: that project is GPL-3.0 and this one is MIT,
//! so only the ideas crossed over. They are not copyrightable; its source is.
//!
//! ## Why repair rather than re-download
//!
//! The atlas is text, the truth about a page's size is in its PNG header, and the
//! fix is idempotent. Repairing a local file costs microseconds and needs no
//! network, which matters because these dumps are hundreds of megabytes.

pub mod atlas;
pub mod png;

pub use atlas::{parse_size, sanitize_page_name, Atlas, AtlasInfo, Normalize, Normalized, Page};
pub use png::png_size;
