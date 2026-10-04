//! Generate the notebook that runs image-to-image skin inference on Kaggle.
//!
//! The notebook is a transcription of the working one at
//! `lilyco4242/blockv0-6-lyco-skin-generator-v7`, with the hard-won constants
//! kept and their reasons written down, and with two things changed:
//!
//!   1. It carries **no UV table**. The overlay repair and the preview used to
//!      be Python that duplicated the atlas layout by hand; both now live in
//!      `lyco-mc-skin`, so this file has one less thing that can drift.
//!   2. It writes a machine-readable manifest and **fails loudly** when every
//!      seed comes back blank. The v4 run reported success while producing
//!      three pure-white images, which is the failure mode worth engineering
//!      against.

use anyhow::{bail, Result};

use crate::metadata::{KernelMetadata, T4};
use crate::notebook::Notebook;

/// The model a working run used: a Flux2Klein derivative, about 15 GB.
pub const DEFAULT_MODEL: &str = "AliceKJ/BLOCKv0.6";

/// The classifier-free guidance scale the model card asks for.
///
/// This is the single most important number here. A working notebook recorded
/// that 1.0 - which reads like "no guidance" - collapses the output to a blank
/// white image and still exits successfully.
pub const DEFAULT_GUIDANCE: f32 = 4.0;

/// Inference steps the model card suggests.
pub const DEFAULT_STEPS: u32 = 30;

/// A reference image the model translates into a skin atlas.
pub const DEFAULT_REFERENCE: &str =
    "https://huggingface.co/AliceKJ/BLOCKv0.6/resolve/main/examples/ex1_preview.png";

#[derive(Debug, Clone, PartialEq)]
pub struct SkinKernelConfig {
    pub owner: String,
    pub slug: String,
    pub title: String,
    pub model_id: String,
    pub reference_url: String,
    pub seeds: Vec<u64>,
    pub steps: u32,
    pub guidance: f32,
}

impl SkinKernelConfig {
    pub fn new(owner: impl Into<String>, slug: impl Into<String>) -> Self {
        SkinKernelConfig {
            owner: owner.into(),
            slug: slug.into(),
            title: "Lyco Skin Generator".to_string(),
            model_id: DEFAULT_MODEL.to_string(),
            reference_url: DEFAULT_REFERENCE.to_string(),
            seeds: vec![7],
            steps: DEFAULT_STEPS,
            guidance: DEFAULT_GUIDANCE,
        }
    }

    pub fn kernel_id(&self) -> String {
        format!("{}/{}", self.owner, self.slug)
    }

    pub fn code_file(&self) -> String {
        format!("{}.ipynb", self.slug)
    }

    /// Reject a configuration that could only waste GPU quota.
    pub fn validate(&self) -> Result<()> {
        if self.owner.trim().is_empty() || self.slug.trim().is_empty() {
            bail!("a kernel needs a non-empty owner and slug");
        }
        if self.model_id.trim().is_empty() {
            bail!("a kernel needs a model to run");
        }
        if self.seeds.is_empty() {
            bail!("at least one seed is needed, or the run would do nothing");
        }
        if self.steps == 0 {
            bail!("step count must be at least 1");
        }
        // Spelled without a negated comparison: clippy rejects those on
        // partially ordered types, and the explicit form also catches NaN.
        if self.guidance <= 1.0 || !self.guidance.is_finite() {
            bail!(
                "guidance {} is at or near 1.0, which disables classifier-free \
                 guidance and collapses the output to a blank image",
                self.guidance
            );
        }
        Ok(())
    }

    pub fn metadata(&self) -> KernelMetadata {
        let mut meta = KernelMetadata::new(self.kernel_id(), self.title.clone(), self.code_file());
        meta.machine_shape = Some(T4.to_string());
        meta
    }

    fn seeds_literal(&self) -> String {
        let items: Vec<String> = self.seeds.iter().map(u64::to_string).collect();
        format!("[{}]", items.join(", "))
    }

    pub fn notebook(&self) -> Notebook {
        Notebook::new()
            .markdown(self.intro_markdown())
            .code("!pip install -q diffusers transformers accelerate pillow httpx")
            .code(self.setup_cell())
            .code(self.load_cell())
            .code(self.reference_cell())
            .code(self.generate_cell())
            .code(self.report_cell())
    }

    fn intro_markdown(&self) -> String {
        format!(
            "# {}\n\nGenerated from lyco-ai-toolkit. Do not edit by hand.\n\n\
             Model: `{}`\n\nSeeds: {}\n\n\
             The atlas layout deliberately does not appear here. Overlay repair \
             and preview rendering belong to `lyco-mc-skin`, which has tests.",
            self.title,
            self.model_id,
            self.seeds_literal()
        )
    }

    fn setup_cell(&self) -> String {
        format!(
            r#"import io, json, os, time
import numpy as np
import torch
from PIL import Image
import httpx

os.environ["PYTORCH_CUDA_ALLOC_CONF"] = "expandable_segments:True"

MODEL_ID = "{}"
SEEDS = {}
STEPS = {}
GUIDANCE = {}
WORK = "/kaggle/working"

assert torch.cuda.is_available(), "this needs a GPU"
name = torch.cuda.get_device_name(0)
gb = torch.cuda.get_device_properties(0).total_memory / 1e9
print(f"GPU: {{name}}  {{gb:.1f}} GB")

# T4 is Turing and has no bfloat16, so float16 it is.
DTYPE = torch.float16
"#,
            self.model_id,
            self.seeds_literal(),
            self.steps,
            self.guidance
        )
    }

    fn load_cell(&self) -> String {
        r#"from diffusers import Flux2KleinPipeline

t0 = time.time()
print(f"loading {MODEL_ID}")
pipe = Flux2KleinPipeline.from_pretrained(MODEL_ID, torch_dtype=DTYPE, use_safetensors=True)
# A 4B model does not fit the T4's 15.6 GB without offloading.
pipe.enable_model_cpu_offload()
print(f"loaded in {time.time() - t0:.0f}s")
"#
        .to_string()
    }

    fn reference_cell(&self) -> String {
        format!(
            r#"URL = "{}"
r = httpx.get(URL, timeout=120, follow_redirects=True)
r.raise_for_status()
reference = Image.open(io.BytesIO(r.content)).convert("RGB")
print(f"reference {{reference.size}} from {{URL}}")

PROMPT = (
    "Image-to-image translation using the reference image. "
    "The reference shows the same 3D Minecraft character with front and back "
    "views in a single image. Generate the corresponding Minecraft skin UV "
    "atlas in 64x64 pixel-art UV layout. High-quality anime-style. "
    "Flat shading, sharp pixel edges, no blur, no anti-aliasing. "
    "Keep consistent UV placement and mapping; match the same character "
    "design from the reference. Model type: classic."
)
"#,
            self.reference_url
        )
    }

    fn generate_cell(&self) -> String {
        r#"def generate(seed):
    gen = torch.Generator("cpu").manual_seed(seed)
    t = time.time()
    out = pipe(
        prompt=PROMPT,
        # Without this resize it infers at 1024, which is four times slower
        # and returns the wrong size.
        image=reference.resize((512, 512)),
        num_inference_steps=STEPS,
        # 1.0 here reads like "no guidance" and produces a blank white image.
        guidance_scale=GUIDANCE,
        generator=gen,
    ).images[0]
    return out.convert("RGB"), time.time() - t


results = []
blank_seeds = []
for seed in SEEDS:
    torch.cuda.empty_cache()
    raw, seconds = generate(seed)
    arr = np.asarray(raw)
    colors = len(np.unique(arr.reshape(-1, 3), axis=0))
    blank = arr.std() < 3.0 or colors < 8
    verdict = "BLANK" if blank else "ok"
    print(f"seed={seed} {seconds:.1f}s mean={arr.mean():.1f} "
          f"std={arr.std():.1f} colors={colors} {verdict}")
    if blank:
        blank_seeds.append(seed)
        continue
    path = f"{WORK}/skin_seed{seed}.png"
    raw.resize((64, 64), Image.NEAREST).save(path)
    results.append({"seed": seed, "file": os.path.basename(path),
                    "seconds": round(seconds, 1), "colors": colors})
"#
        .to_string()
    }

    fn report_cell(&self) -> String {
        r#"manifest = {
    "model": MODEL_ID,
    "seeds_requested": SEEDS,
    "seeds_produced": [r["seed"] for r in results],
    "blank_seeds": blank_seeds,
    "results": results,
}
with open(f"{WORK}/kaggle_manifest.json", "w") as f:
    json.dump(manifest, f, indent=2)
print(json.dumps(manifest, indent=2))

# Fail loudly. A run that produced nothing must not look like a success,
# which is exactly how an earlier version shipped three white images.
if not results:
    raise RuntimeError(f"every seed produced a blank image: {blank_seeds}")
"#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SkinKernelConfig {
        SkinKernelConfig::new("me", "lyco-skin-gen")
    }

    fn all_source(cfg: &SkinKernelConfig) -> String {
        cfg.notebook().to_json().expect("serialize")
    }

    #[test]
    fn defaults_come_from_a_run_that_worked() {
        let cfg = config();
        assert_eq!(cfg.model_id, DEFAULT_MODEL);
        assert_eq!(cfg.steps, 30);
        assert_eq!(cfg.guidance, 4.0);
        assert_eq!(cfg.seeds, vec![7]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn metadata_points_at_the_generated_notebook() {
        let cfg = config();
        let meta = cfg.metadata();
        assert_eq!(meta.id, "me/lyco-skin-gen");
        assert_eq!(meta.code_file, "lyco-skin-gen.ipynb");
        assert!(meta.enable_gpu);
        assert!(meta.enable_internet, "weights come from Hugging Face");
    }

    #[test]
    fn the_notebook_carries_the_constants_that_matter() {
        let text = all_source(&config());
        assert!(text.contains("GUIDANCE = 4"), "{text}");
        assert!(text.contains("STEPS = 30"));
        assert!(text.contains("torch.float16"), "Turing has no bfloat16");
        assert!(
            text.contains("enable_model_cpu_offload"),
            "15.6 GB will not hold 4B"
        );
        assert!(
            text.contains("resize((512, 512))"),
            "inferring at 1024 is four times slower"
        );
    }

    /// The point of this crate's design: the atlas layout lives in one place.
    #[test]
    fn the_notebook_contains_no_uv_table() {
        let text = all_source(&config());
        for forbidden in ["OVERLAY_RECTS", "FIRST_LAYER_RECTS", "fix_white_overlay"] {
            assert!(
                !text.contains(forbidden),
                "{forbidden} belongs in lyco-mc-skin, not in the notebook"
            );
        }
    }

    #[test]
    fn a_blank_run_raises_rather_than_reporting_success() {
        let text = all_source(&config());
        assert!(text.contains("raise RuntimeError"), "{text}");
        assert!(
            text.contains("blank"),
            "the notebook must sense a blank output"
        );
    }

    #[test]
    fn guidance_of_one_is_refused_before_any_quota_is_spent() {
        let mut cfg = config();
        cfg.guidance = 1.0;
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("blank image"), "{err}");
    }

    #[test]
    fn an_empty_seed_list_is_refused() {
        let mut cfg = config();
        cfg.seeds.clear();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn steps_of_zero_is_refused() {
        let mut cfg = config();
        cfg.steps = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn a_missing_owner_is_refused() {
        let mut cfg = config();
        cfg.owner = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn seeds_are_written_as_a_python_list() {
        let mut cfg = config();
        cfg.seeds = vec![7, 42, 2024];
        assert!(all_source(&cfg).contains("SEEDS = [7, 42, 2024]"));
    }
}
