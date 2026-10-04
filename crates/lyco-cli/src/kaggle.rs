//! The `lyco-cli kaggle` subcommands.
//!
//! Split out of `main.rs` because this is the one place the toolkit leaves the
//! machine: it uploads a generated notebook, waits for a free GPU to run it, and
//! pulls the result back. The interesting part is what happens *after* the
//! download - the repair and the preview are ordinary local code, and they are
//! why the notebook itself needs to know nothing about the atlas layout.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

use lyco_kaggle::{Kaggle, SkinKernelConfig};
use lyco_mc_skin::{character_view, fix_white_overlay, ArmModel, OverlayFixConfig, Side, Skin};

/// How much a preview is magnified, so a 16x32 character is readable.
const PREVIEW_SCALE: u32 = 8;

/// Parse `7,42,2024` into seeds, refusing anything that is not a number.
pub fn parse_seeds(text: &str) -> Result<Vec<u64>> {
    let mut seeds = Vec::new();
    for piece in text.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        seeds.push(
            piece
                .parse::<u64>()
                .with_context(|| format!("'{piece}' is not a seed"))?,
        );
    }
    if seeds.is_empty() {
        bail!("at least one seed is required");
    }
    Ok(seeds)
}

fn config(owner: &str, slug: &str, seeds: &str) -> Result<SkinKernelConfig> {
    let mut cfg = SkinKernelConfig::new(owner, slug);
    cfg.seeds = parse_seeds(seeds)?;
    cfg.validate()?;
    Ok(cfg)
}

/// Fail with something actionable when the CLI is missing.
fn require_kaggle() -> Result<Kaggle> {
    let kaggle = Kaggle::new();
    if !kaggle.available() {
        bail!("the kaggle CLI is not on PATH; install it and authenticate it first");
    }
    Ok(kaggle)
}

fn write_kernel(cfg: &SkinKernelConfig, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    std::fs::write(dir.join(cfg.code_file()), cfg.notebook().to_json()?)?;
    std::fs::write(dir.join("kernel-metadata.json"), cfg.metadata().to_json()?)?;
    Ok(())
}

fn list_files(dir: &Path) -> Result<()> {
    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    if names.is_empty() {
        println!("  (the run produced no files)");
        return Ok(());
    }
    for name in &names {
        println!("  {name}");
    }
    Ok(())
}

pub fn render(owner: &str, slug: &str, seeds: &str, out: &str) -> Result<()> {
    let cfg = config(owner, slug, seeds)?;
    let dir = PathBuf::from(out);
    write_kernel(&cfg, &dir)?;
    println!("kernel {} rendered into {}", cfg.kernel_id(), dir.display());
    println!("  {}", cfg.code_file());
    println!("  kernel-metadata.json");
    println!("seeds: {:?}", cfg.seeds);
    Ok(())
}

pub fn push(dir: &str, timeout: Option<u32>) -> Result<()> {
    let kaggle = require_kaggle()?;
    print!("{}", kaggle.push(Path::new(dir), timeout)?);
    Ok(())
}

pub fn status(kernel: &str) -> Result<()> {
    let kaggle = require_kaggle()?;
    let state = kaggle.status(kernel)?;
    println!("{kernel}: {}", state.name());
    if !state.is_success() {
        println!("not a successful finish");
    }
    Ok(())
}

pub fn fetch(kernel: &str, out: &str) -> Result<()> {
    let kaggle = require_kaggle()?;
    let dir = PathBuf::from(out);
    print!("{}", kaggle.download_output(kernel, &dir)?);
    list_files(&dir)
}

/// The whole loop, which is what the objective actually asks for.
pub fn run(
    owner: &str,
    slug: &str,
    seeds: &str,
    out: &str,
    arm: ArmModel,
    poll_secs: u64,
    max_polls: u32,
) -> Result<()> {
    let cfg = config(owner, slug, seeds)?;
    let root = PathBuf::from(out);
    let kernel_dir = root.join("kernel");
    write_kernel(&cfg, &kernel_dir)?;
    println!(
        "kernel {} rendered into {}",
        cfg.kernel_id(),
        kernel_dir.display()
    );

    let kaggle = require_kaggle()?;
    print!("{}", kaggle.push(&kernel_dir, None)?);
    println!("polling every {poll_secs}s, at most {max_polls} times");

    let state = kaggle.wait(&cfg.kernel_id(), Duration::from_secs(poll_secs), max_polls)?;
    if !state.is_success() {
        bail!(
            "the run ended as {}; see https://www.kaggle.com/code/{}",
            state.name(),
            cfg.kernel_id()
        );
    }
    println!("run finished: {}", state.name());

    let output_dir = root.join("output");
    kaggle.download_output(&cfg.kernel_id(), &output_dir)?;
    println!("downloaded into {}", output_dir.display());
    list_files(&output_dir)?;

    let repaired = repair_skins(&output_dir, arm)?;
    if repaired == 0 {
        bail!("the run produced no skin files to repair");
    }
    println!("{repaired} skin(s) repaired and previewed");
    Ok(())
}

/// Repair every downloaded skin and render a view a human can judge.
///
/// This is the half of the pipeline that needs no GPU, which is exactly why it
/// is local code with tests rather than notebook cells.
fn repair_skins(dir: &Path, arm: ArmModel) -> Result<usize> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_png = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if is_png {
            paths.push(path);
        }
    }
    paths.sort();

    let mut repaired = 0usize;
    for path in paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // Skip anything this function itself wrote, so a second run is stable.
        if name.ends_with("_fixed.png") || name.ends_with("_front.png") {
            continue;
        }
        if name.ends_with("_back.png") {
            continue;
        }

        let mut skin = Skin::load(&path)?;
        let report = fix_white_overlay(&mut skin, arm, &OverlayFixConfig::default())?;
        if !report.first_layer_intact {
            eprintln!("{name}: base layer looks damaged, skipping");
            continue;
        }

        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        skin.save(dir.join(format!("{stem}_fixed.png")))?;
        character_view(&skin, arm, Side::Front)
            .scaled(PREVIEW_SCALE)
            .save(dir.join(format!("{stem}_front.png")))?;
        character_view(&skin, arm, Side::Back)
            .scaled(PREVIEW_SCALE)
            .save(dir.join(format!("{stem}_back.png")))?;

        let validation = skin.validate();
        println!(
            "  {name}: cleared {} px, {} distinct colours, valid={}",
            report.total_cleared(),
            validation.distinct_colors,
            validation.ok()
        );
        repaired += 1;
    }
    Ok(repaired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_parse_from_a_comma_separated_list() {
        assert_eq!(parse_seeds("7").expect("one"), vec![7]);
        assert_eq!(parse_seeds("7, 42,2024").expect("many"), vec![7, 42, 2024]);
    }

    #[test]
    fn an_empty_seed_list_is_refused() {
        assert!(parse_seeds("").is_err());
        assert!(parse_seeds("  ,  ").is_err());
    }

    #[test]
    fn a_non_numeric_seed_names_the_offender() {
        let err = parse_seeds("7,banana").unwrap_err();
        assert!(err.to_string().contains("banana"), "{err}");
    }

    #[test]
    fn a_kernel_written_to_disk_is_a_folder_kaggle_can_push() {
        let dir = std::env::temp_dir().join("lyco-kaggle-render-test");
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = config("me", "lyco-skin-gen", "7,42").expect("config");
        write_kernel(&cfg, &dir).expect("write");
        assert!(dir.join("lyco-skin-gen.ipynb").is_file());
        assert!(dir.join("kernel-metadata.json").is_file());

        // The metadata must name the notebook that is actually next to it.
        let meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("kernel-metadata.json")).expect("read"),
        )
        .expect("parse");
        assert_eq!(meta["code_file"], "lyco-skin-gen.ipynb");
        assert_eq!(meta["enable_gpu"], true);

        // And the notebook must be real nbformat, not just a file.
        let nb: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("lyco-skin-gen.ipynb")).expect("read"),
        )
        .expect("parse");
        assert_eq!(nb["nbformat"], 4);
        assert!(nb["cells"].as_array().is_some_and(|c| c.len() >= 5));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
