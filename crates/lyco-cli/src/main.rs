//! lyco-cli - unified AI asset studio (one entry, many crates)
//!
//! See _RESEARCH/00-requirement.md (1-line requirement):
//!     lyco-cli bundle --prompt ...  =>  skin.png + model.bsm + bgm.mp3 + portrait.png
//! See _RESEARCH/03-decision.md for the build-vs-buy matrix.
//! See _RESEARCH/04-validation-checklist.md for MVP milestones (S0-S5).
//!
//! Scaffold state: argument surface only. Each subcommand currently fails fast
//! so that no pipeline step can silently pretend to have produced an asset.
use anyhow::Result;
use clap::{Parser, Subcommand};

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
        /// Output directory [default: ./out]
        #[arg(long, short, default_value = "out")]
        out: String,
    },
    /// Generate a Minecraft skin 64x64 (F1)
    Skin {
        /// Character description
        prompt: String,
        /// Output PNG path [default: ./out/skin.png]
        #[arg(long, short)]
        out: Option<String>,
    },
    /// Generate a BSM/YSM entity model (F2/F3)
    Bsm {
        /// Character description
        prompt: String,
        /// Output BSM path [default: ./out/model.bsm]
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    let (what, next) = match &cli.cmd {
        Cmd::Bundle { .. } => ("bundle", "S5 - _RESEARCH/04-validation-checklist.md#s5"),
        Cmd::Skin { .. } => ("skin", "S1 - _RESEARCH/04-validation-checklist.md#s1"),
        Cmd::Bsm { .. } => ("bsm", "S2 - _RESEARCH/04-validation-checklist.md#s2"),
        Cmd::Bgm { .. } => ("bgm", "S3 - _RESEARCH/04-validation-checklist.md#s3"),
        Cmd::Lyrics { .. } => ("lyrics", "S4 - _RESEARCH/04-validation-checklist.md#s4"),
        Cmd::Portrait { .. } => ("portrait", "S4 - _RESEARCH/04-validation-checklist.md#s4"),
    };

    // Fail fast: a scaffold must never look like a successful generation.
    anyhow::bail!(
        "lyco-cli: subcommand '{what}' is scaffolded but not implemented yet.\n\
         Next milestone: {next}"
    );
}
