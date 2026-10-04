//! The kernel-metadata.json that sits next to a pushed notebook.
//!
//! Field names and defaults were taken from a kernel that actually runs on
//! Kaggle (pulled with `kaggle kernels pull -m`), not from documentation, so
//! the GPU shape and internet flag match a known-good run.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The GPU a known-good run used. Kaggle assigns by availability, but naming
/// it keeps the request explicit.
pub const T4: &str = "NvidiaTeslaT4";

/// Lowercase a title the way Kaggle turns one into a URL slug.
///
/// This exists because of a bug that cost a real GPU run: Kaggle derives the
/// kernel's slug from its **title**, not from the `id` field, and silently
/// ignores the id when the two disagree. A kernel pushed as
/// `owner/lyco-skin-gen` with the title "Lyco Skin Generator" came back as
/// `owner/lyco-skin-generator`, so the follow-up status call could not find it.
/// Callers now check the title against the slug before spending any quota.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// Title-case a slug, so a generated title always slugs back to it.
pub fn title_from_slug(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|piece| !piece.is_empty())
        .map(|piece| {
            let mut chars = piece.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelMetadata {
    pub id: String,
    pub title: String,
    pub code_file: String,
    pub language: String,
    pub kernel_type: String,
    pub is_private: bool,
    pub enable_gpu: bool,
    /// Required for the kernel to reach Hugging Face for model weights.
    pub enable_internet: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dataset_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kernel_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competition_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_shape: Option<String>,
}

impl KernelMetadata {
    /// Defaults aimed at a private GPU notebook that can download weights.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        code_file: impl Into<String>,
    ) -> Self {
        KernelMetadata {
            id: id.into(),
            title: title.into(),
            code_file: code_file.into(),
            language: "python".to_string(),
            kernel_type: "notebook".to_string(),
            is_private: true,
            enable_gpu: true,
            enable_internet: true,
            dataset_sources: Vec::new(),
            kernel_sources: Vec::new(),
            competition_sources: Vec::new(),
            model_sources: Vec::new(),
            machine_shape: Some(T4.to_string()),
        }
    }

    /// The `owner` half of `owner/slug`.
    pub fn owner(&self) -> Result<&str> {
        match self.id.split_once('/') {
            Some((owner, _)) if !owner.is_empty() => Ok(owner),
            _ => bail!("kernel id '{}' must look like owner/slug", self.id),
        }
    }

    pub fn slug(&self) -> Result<&str> {
        match self.id.split_once('/') {
            Some((_, slug)) if !slug.is_empty() => Ok(slug),
            _ => bail!("kernel id '{}' must look like owner/slug", self.id),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_a_working_gpu_kernel() {
        let meta = KernelMetadata::new("me/thing", "Thing", "thing.ipynb");
        assert_eq!(meta.language, "python");
        assert_eq!(meta.kernel_type, "notebook");
        assert!(meta.is_private, "a run with a private model stays private");
        assert!(meta.enable_gpu);
        assert!(meta.enable_internet, "weights come from Hugging Face");
        assert_eq!(meta.machine_shape.as_deref(), Some(T4));
    }

    #[test]
    fn splits_owner_and_slug() {
        let meta = KernelMetadata::new("lilyco4242/some-kernel", "T", "k.ipynb");
        assert_eq!(meta.owner().expect("owner"), "lilyco4242");
        assert_eq!(meta.slug().expect("slug"), "some-kernel");
    }

    #[test]
    fn a_malformed_id_is_refused_rather_than_guessed() {
        let meta = KernelMetadata::new("no-slash", "T", "k.ipynb");
        assert!(meta.owner().is_err());
        assert!(meta.slug().is_err());
        let empty = KernelMetadata::new("/slug", "T", "k.ipynb");
        assert!(empty.owner().is_err());
    }

    #[test]
    fn serialises_with_the_snake_case_keys_kaggle_expects() {
        let meta = KernelMetadata::new("me/thing", "Thing", "thing.ipynb");
        let text = meta.to_json().expect("serialize");
        for key in [
            "\"id\"",
            "\"title\"",
            "\"code_file\"",
            "\"kernel_type\"",
            "\"is_private\"",
            "\"enable_gpu\"",
            "\"enable_internet\"",
            "\"machine_shape\"",
        ] {
            assert!(text.contains(key), "missing {key}");
        }
    }

    #[test]
    fn empty_source_lists_are_omitted() {
        let meta = KernelMetadata::new("me/thing", "Thing", "thing.ipynb");
        let text = meta.to_json().expect("serialize");
        assert!(!text.contains("dataset_sources"), "empty lists add noise");
        assert!(!text.contains("null"), "{text}");
    }

    #[test]
    fn round_trips_and_ignores_fields_kaggle_adds_on_pull() {
        // A real pull adds id_no and docker_image; deserialising must tolerate
        // them rather than fail, because that is what the CLI hands back.
        let pulled = r#"{
            "id": "me/thing",
            "id_no": 136668596,
            "title": "Thing",
            "code_file": "thing.ipynb",
            "language": "python",
            "kernel_type": "notebook",
            "is_private": true,
            "enable_gpu": true,
            "enable_tpu": false,
            "enable_internet": true,
            "dataset_sources": [],
            "kernel_sources": [],
            "competition_sources": [],
            "model_sources": [],
            "docker_image": "gcr.io/example"
        }"#;
        let meta: KernelMetadata = serde_json::from_str(pulled).expect("parse a pulled file");
        assert_eq!(meta.id, "me/thing");
        assert!(meta.enable_gpu);
    }
}
