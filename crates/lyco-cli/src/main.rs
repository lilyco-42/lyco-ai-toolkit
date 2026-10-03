//! lyco-cli - unified AI asset studio (one entry, many crates)
//!
//! See _RESEARCH/00-requirement.md (1-line requirement):
//!     'lyco-cli bundle --prompt ...' => skin.png + model.bsm + bgm.mp3 + portrait.png
//! See _RESEARCH/03-decision.md for the build-vs-buy matrix.
//! See _RESEARCH/04-validation-checklist.md for MVP milestones.
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name="lyco-cli", about="lyco AI asset studio")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand, Debug)]
enum Cmd {
    /// Generate a full character bundle (skin + 3d + bgm + portrait)
    Bundle {
        #[arg(long, short)]
        prompt: String,
    },
    /// Generate a Minecraft skin 64x64 (F1)
    Skin {
        #[arg]
        prompt: String,
    },
    /// Generate BSM/YSM entity model (F2/F3)
    Bsm {
        #[arg]
        prompt: String,
    },
    /// Generate background music (F6, default via ACE-Step)
    Bgm {
        #[arg]
        prompt: String,
        #[arg(long, default_value="60")]
        duration: u32,
    },
    /// Generate lyrics (F8, default via local LLM)
    Lyrics {
        #[arg]
        prompt: String,
    },
    /// Print version
    Version,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Version => println!("lyco-cli 0.1.0-dev"),
        _ => {
            // TODO (S0-S5 in _RESEARCH/04-validation-checklist.md)
            // wire each subcommand to its crate. Until then fail fast:
            eprintln!("not implemented yet - see _RESEARCH/04-validation-checklist.md");
            std::process::exit(1);
        }
    }
    Ok(())
}
