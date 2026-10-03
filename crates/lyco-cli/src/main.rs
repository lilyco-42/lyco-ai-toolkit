//! lyco-cli - unified AI asset studio (one entry, many crates).
//!
//! See _RESEARCH/00-requirement.md for the one-line requirement,
//! _RESEARCH/03-decision.md for the build-vs-buy matrix, and
//! _RESEARCH/04-validation-checklist.md for milestones S0-S5.
//!
//! Implemented today: the `skin` group (F1). It is real, not scaffolded -
//! template/validate/info/quantize all work offline with no GPU.
//! Everything that needs an image or audio model still fails fast, so a green
//! run can never be mistaken for a produced asset.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use lyco_mc_skin::{
    apply_palette, distinct_opaque_colors, median_cut, ArmModel, Report, Severity, Skin,
};

#[derive(Parser, Debug)]
#[command(name = "lyco-cli", about = "lyco AI asset studio", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Generate a full character bundle (skin + 3D + BGM + portrait)
    Bundle {
        /// Character description, e.g. 'a quiet librarian, grey hair, soft light'
        #[arg(long, short)]
        prompt: String,
        /// Output directory
        #[arg(long, short, default_value = "out")]
        out: String,
    },
    /// Minecraft skin tools (F1)
    Skin {
        #[command(subcommand)]
        cmd: SkinCmd,
    },
    /// Generate a BSM/YSM entity model (F2/F3)
    Bsm {
        /// Character description
        prompt: String,
        /// Output BSM path
        #[arg(long, short)]
        out: Option<String>,
    },
    /// Generate background music (F6, via ACE-Step)
    Bgm {
        /// Mood / style description
        prompt: String,
        /// Duration in seconds
        #[arg(long, default_value_t = 60)]
        duration: u32,
    },
    /// Generate lyrics with verse/chorus/bridge structure (F8)
    Lyrics {
        /// Theme or title
        prompt: String,
    },
    /// Generate a character portrait via ComfyUI (F7)
    Portrait {
        /// Character description
        prompt: String,
    },
}

#[derive(Subcommand, Debug)]
enum SkinCmd {
    /// Write a structurally valid template skin (no model needed)
    Template {
        /// Output PNG path
        #[arg(long, short, default_value = "out/skin.png")]
        out: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
    },
    /// Validate a skin's structure; exits non-zero on any error-severity issue
    Validate {
        /// Skin PNG to inspect
        path: String,
        /// Emit machine-readable JSON instead of a human summary
        #[arg(long)]
        json: bool,
    },
    /// Report dimensions, opaque pixels, distinct colors and detected arm model
    Info {
        /// Skin PNG to inspect
        path: String,
    },
    /// Snap a skin onto a small, coherent palette
    Quantize {
        /// Input skin PNG
        path: String,
        /// Output skin PNG
        #[arg(long, short, default_value = "out/skin-quantized.png")]
        out: String,
        /// Maximum palette entries
        #[arg(long, default_value_t = 8)]
        colors: usize,
    },
    /// Generate a skin from a character prompt (needs an image backend)
    Generate {
        /// Character description
        prompt: String,
        /// Output PNG path
        #[arg(long, short, default_value = "out/skin.png")]
        out: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Skin { cmd } => run_skin(cmd),
        other => fail_fast(&other),
    }
}

/// Parse an arm-model selector. Accepts the canonical names and the player
/// names people actually use.
fn parse_arm(s: &str) -> Result<ArmModel> {
    match s.to_ascii_lowercase().as_str() {
        "classic" | "steve" | "4" => Ok(ArmModel::Classic),
        "slim" | "alex" | "3" => Ok(ArmModel::Slim),
        other => bail!("unknown arm model '{other}': expected classic or slim"),
    }
}

fn run_skin(cmd: SkinCmd) -> Result<()> {
    match cmd {
        SkinCmd::Template { out, arm } => {
            let arm = parse_arm(&arm)?;
            let skin = Skin::template(arm);
            skin.save(&out)?;
            let rep = skin.validate();
            println!(
                "wrote {out} ({}x{}, arm={})",
                skin.width(),
                skin.height(),
                arm.name()
            );
            println!("opaque pixels : {}", rep.opaque_pixels);
            println!("distinct color: {}", rep.distinct_colors);
            if !rep.ok() {
                print_report(&rep);
                bail!("the generated template failed its own validation");
            }
            Ok(())
        }

        SkinCmd::Validate { path, json } => {
            let skin = Skin::load(&path)?;
            let rep = skin.validate();
            if json {
                println!("{}", report_json(&rep));
            } else {
                print_report(&rep);
            }
            if !rep.ok() {
                bail!("{}: {} error(s)", path, rep.count(Severity::Error));
            }
            Ok(())
        }

        SkinCmd::Info { path } => {
            let skin = Skin::load(&path)?;
            let rep = skin.validate();
            println!("path           : {path}");
            println!("dimensions     : {}x{}", skin.width(), skin.height());
            println!("opaque pixels  : {}", rep.opaque_pixels);
            println!("distinct colors: {}", rep.distinct_colors);
            println!("arm model      : {}", arm_label(rep.detected_arm));
            let errors = rep.count(Severity::Error);
            let warns = rep.count(Severity::Warn);
            println!("errors/warns   : {errors} / {warns}");
            Ok(())
        }

        SkinCmd::Quantize { path, out, colors } => {
            if colors == 0 {
                bail!("--colors must be at least 1");
            }
            let skin = Skin::load(&path)?;
            let before = distinct_opaque_colors(skin.pixels());
            let palette = median_cut(skin.pixels(), colors);
            if palette.is_empty() {
                bail!("{path} has no opaque pixels to quantize");
            }
            let snapped = apply_palette(skin.pixels(), &palette);
            let bytes: Vec<u8> = snapped.iter().flat_map(|p| p.iter().copied()).collect();
            let rebuilt = Skin::from_rgba_bytes(skin.width(), skin.height(), &bytes)?;
            if !rebuilt.validate().ok() {
                bail!("quantization produced a structurally invalid skin");
            }
            rebuilt.save(&out)?;
            let after = distinct_opaque_colors(rebuilt.pixels());
            println!("wrote {out}");
            println!(
                "colors    : {before} -> {after} ({} in palette)",
                palette.len()
            );
            println!("palette   : {}", palette.as_hex().join(" "));
            Ok(())
        }

        SkinCmd::Generate { prompt, out } => {
            let _ = (prompt, out);
            bail!(
                "skin generation needs a diffusion backend, which is not wired yet.\n\
                 Working offline alternatives:\n\
                   lyco-cli skin template --out out/skin.png",
            )
        }
    }
}

fn arm_label(arm: Option<ArmModel>) -> &'static str {
    match arm {
        Some(ArmModel::Classic) => "classic",
        Some(ArmModel::Slim) => "slim",
        None => "unknown (not a full 64x64 skin)",
    }
}

fn print_report(rep: &Report) {
    let verdict = if rep.ok() { "valid" } else { "INVALID" };
    println!("{}: {}x{}", verdict, rep.width, rep.height);
    for issue in &rep.issues {
        println!(
            "  [{}] {}: {}",
            issue.severity.name(),
            issue.code,
            issue.message
        );
    }
}

fn report_json(rep: &Report) -> String {
    let issues: Vec<serde_json::Value> = rep
        .issues
        .iter()
        .map(|i| {
            serde_json::json!({
                "severity": i.severity.name(),
                "code": i.code,
                "message": i.message,
            })
        })
        .collect();
    let value = serde_json::json!({
        "valid": rep.ok(),
        "width": rep.width,
        "height": rep.height,
        "opaque_pixels": rep.opaque_pixels,
        "distinct_colors": rep.distinct_colors,
        "detected_arm": rep.detected_arm.map(|a| a.name()),
        "issues": issues,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

fn fail_fast(cmd: &Cmd) -> Result<()> {
    let (what, next) = match cmd {
        Cmd::Bundle { .. } => ("bundle", "S5"),
        Cmd::Bsm { .. } => ("bsm", "S2"),
        Cmd::Bgm { .. } => ("bgm", "S3"),
        Cmd::Lyrics { .. } => ("lyrics", "S4"),
        Cmd::Portrait { .. } => ("portrait", "S4"),
        Cmd::Skin { .. } => ("skin", "S1"),
    };
    bail!(
        "lyco-cli: subcommand '{what}' is scaffolded but not implemented yet.\n\
         Next milestone: {next} - see _RESEARCH/04-validation-checklist.md"
    );
}
