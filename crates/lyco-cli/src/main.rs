//! lyco-cli - unified AI asset studio (one entry, many crates).
//!
//! See _RESEARCH/00-requirement.md for the one-line requirement,
//! _RESEARCH/03-decision.md for the build-vs-buy matrix, and
//! _RESEARCH/04-validation-checklist.md for milestones S0-S5.
//!
//! Every subcommand is implemented and works offline with no GPU:
//!   * `bundle`   - skin + model + BGM + portrait + lyrics, with a manifest
//!   * `skin` (F1) - template / validate / info / quantize
//!   * `bsm`  (F2/F3) - humanoid / validate / info / glb-info
//!   * `bgm`  (F6) - background music
//!   * `lyrics` (F8) - sectioned lyric sheets
//!   * `portrait` (F7) - a single portrait
//!
//! What needs a model (diffusion, an audio engine, an LLM) is reached through a
//! subprocess backend the operator points at. The built-in `stub` backends are
//! labelled as placeholders on every surface, so a green run can never be
//! mistaken for a produced asset.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use lyco_audio::{backend_from_spec, BgmRequest};
use lyco_bsm::{
    convert_glb, glb::Primitive, player_file, ConvertConfig, GeometryFile, Glb, Problem,
};
use lyco_bundle::{Artifact, BundleRequest};
use lyco_image::{backend_from_spec as image_backend, PortraitRequest};
use lyco_lyrics::{backend_from_spec as lyrics_backend, LyricsRequest};
use lyco_mc_skin::{
    apply_palette, character_view, distinct_opaque_colors, fix_white_overlay, median_cut, ArmModel,
    OverlayFixConfig, Report, Severity, Side, Skin,
};
use lyco_mcp::Registry;
use lyco_spine::{png_size, read_header, sanitize_page_name, Atlas, Normalize};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod kaggle;

#[derive(Parser, Debug)]
#[command(name = "lyco-cli", about = "lyco AI asset studio", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Generate a full character bundle (skin + model + BGM + portrait)
    Bundle {
        /// Character description, e.g. 'a quiet librarian, grey hair, soft light'
        #[arg(long, short)]
        prompt: String,
        /// Output directory
        #[arg(long, short, default_value = "out")]
        out: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
        /// BGM duration in seconds
        #[arg(long, default_value_t = 30)]
        bgm_duration: u32,
        /// BGM backend: 'stub' or a generator command line
        #[arg(long, default_value = "stub")]
        bgm_backend: String,
        /// Portrait backend: 'stub' or a generator command line
        #[arg(long, default_value = "stub")]
        image_backend: String,
        /// A Spine atlas to repair into the bundle; repeat for more than one
        #[arg(long)]
        spine: Vec<String>,
        /// Also declare premultiplied alpha on the repaired atlases
        #[arg(long)]
        spine_pma: bool,
    },
    /// Minecraft skin tools (F1)
    Skin {
        #[command(subcommand)]
        cmd: SkinCmd,
    },
    /// Minecraft model tools: Bedrock geometry and GLB inspection (F2/F3)
    Bsm {
        #[command(subcommand)]
        cmd: BsmCmd,
    },
    /// Generate background music (F6)
    Bgm {
        /// Mood / style description
        #[arg(long, short)]
        prompt: String,
        /// Duration in seconds
        #[arg(long, default_value_t = 60)]
        duration: u32,
        /// Output WAV path
        #[arg(long, short, default_value = "out/bgm.wav")]
        out: String,
        /// 'stub' for the offline placeholder, or a generator command line
        #[arg(long, default_value = "stub")]
        backend: String,
    },
    /// Generate lyrics with verse/chorus/bridge structure (F8)
    Lyrics {
        /// Theme or title
        #[arg(long, short)]
        prompt: String,
        /// Output text path
        #[arg(long, short, default_value = "out/lyrics.txt")]
        out: String,
        /// 'stub' for the offline placeholder, or a writer command line
        #[arg(long, default_value = "stub")]
        backend: String,
    },
    /// Inspect and repair Spine texture atlases (F11)
    Spine {
        #[command(subcommand)]
        cmd: SpineCmd,
    },
    /// Run the MCP server on stdin/stdout, for an agent to drive (F9)
    Mcp,
    /// Run a generation job on Kaggle's free GPUs (F1)
    Kaggle {
        #[command(subcommand)]
        cmd: KaggleCmd,
    },
    /// Generate just a portrait (F7)
    Portrait {
        /// Character description
        #[arg(long, short)]
        prompt: String,
        /// Output PNG path
        #[arg(long, short, default_value = "out/portrait.png")]
        out: String,
        /// 'stub' for the offline placeholder, or a generator command line
        #[arg(long, default_value = "stub")]
        backend: String,
    },
}

#[derive(Subcommand, Debug)]
enum SpineCmd {
    /// Report pages, regions, and whether size and pma are declared
    Info {
        /// Atlas text file
        path: String,
    },
    /// Report the Spine version a .skel file needs
    Version {
        /// .skel file
        path: String,
    },
    /// Repair size fields from the page PNGs, and optionally declare pma
    Normalize {
        /// Atlas text file
        path: String,
        /// Where the page PNGs live; defaults to the atlas's own directory
        #[arg(long)]
        dir: Option<String>,
        /// Write the repaired atlas here
        #[arg(long, short)]
        out: Option<String>,
        /// Overwrite the input file instead
        #[arg(long)]
        write: bool,
        /// Declare premultiplied alpha on every page that does not say
        #[arg(long)]
        pma: bool,
        /// Remove page fields a Spine 3.8 runtime cannot read, pma above all
        #[arg(long)]
        strip_3_8: bool,
        /// Also rewrite page names to safe local file names
        #[arg(long)]
        sanitize: bool,
    },
}

#[derive(Subcommand, Debug)]
enum KaggleCmd {
    /// Write the kernel folder locally; uploads nothing
    Render {
        /// Kaggle owner, for example lilyco4242
        #[arg(long)]
        owner: String,
        /// Kernel slug, for example lyco-skin-gen
        #[arg(long)]
        slug: String,
        /// Seeds to generate, comma separated
        #[arg(long, default_value = "7")]
        seeds: String,
        /// Folder to write the kernel into
        #[arg(long, short, default_value = "out/kernel")]
        out: String,
    },
    /// Upload the folder and start a run
    Push {
        /// Folder holding the notebook and kernel-metadata.json
        #[arg(long, short, default_value = "out/kernel")]
        dir: String,
        /// Kaggle-imposed run time limit in seconds
        #[arg(long)]
        timeout: Option<u32>,
    },
    /// Report what a run is doing
    Status {
        /// Kernel reference, owner/slug
        kernel: String,
    },
    /// Download a finished run's output
    Fetch {
        /// Kernel reference, owner/slug
        kernel: String,
        /// Where to put the downloaded files
        #[arg(long, short, default_value = "out/kaggle-output")]
        out: String,
    },
    /// The whole loop: render, push, wait, fetch, then repair and preview
    Run {
        /// Kaggle owner
        #[arg(long)]
        owner: String,
        /// Kernel slug
        #[arg(long)]
        slug: String,
        /// Seeds to generate, comma separated
        #[arg(long, default_value = "7")]
        seeds: String,
        /// Working folder
        #[arg(long, short, default_value = "out/kaggle")]
        out: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
        /// Seconds between status polls
        #[arg(long, default_value_t = 30)]
        poll_secs: u64,
        /// Give up after this many polls
        #[arg(long, default_value_t = 60)]
        max_polls: u32,
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
    /// Clear a model's white background out of the overlay layer
    FixOverlay {
        /// Input skin PNG, typically straight from a model
        path: String,
        /// Output skin PNG
        #[arg(long, short, default_value = "out/skin-fixed.png")]
        out: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
        /// A channel at or above this counts as background white
        #[arg(long, default_value_t = 235)]
        threshold: u8,
        /// Below this percentage surviving, the rectangle is cleared entirely
        #[arg(long, default_value_t = 12)]
        keep_percent: u32,
    },
    /// Compose a skin into a flat front view a human can judge
    Preview {
        /// Input skin PNG
        path: String,
        /// Output PNG
        #[arg(long, short, default_value = "out/skin-preview.png")]
        out: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
        /// Magnification, so a 16x32 character is readable
        #[arg(long, default_value_t = 8)]
        scale: u32,
    },
    /// Generate a skin, on Kaggle's free GPU or from a local command
    Generate {
        /// Character description. The Kaggle path drives off a reference image,
        /// so this is optional there, and a flag here as everywhere else.
        #[arg(long, short, default_value = "")]
        prompt: String,
        /// Output PNG path for a local backend
        #[arg(long, short, default_value = "out/skin.png")]
        out: String,
        /// 'kaggle' for free cloud GPU, or a generator command line
        #[arg(long, default_value = "kaggle")]
        backend: String,
        /// Kaggle owner, required for --backend kaggle
        #[arg(long, default_value = "")]
        owner: String,
        /// Kaggle kernel slug, required for --backend kaggle
        #[arg(long, default_value = "")]
        slug: String,
        /// Seeds to generate, comma separated
        #[arg(long, default_value = "7")]
        seeds: String,
        /// Working folder for the Kaggle path
        #[arg(long, default_value = "out/kaggle")]
        work_dir: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
        /// Seconds between status polls
        #[arg(long, default_value_t = 30)]
        poll_secs: u64,
        /// Give up after this many polls
        #[arg(long, default_value_t = 60)]
        max_polls: u32,
    },
}

#[derive(Subcommand, Debug)]
enum BsmCmd {
    /// Write the standard player model as Bedrock geometry (.geo.json)
    Humanoid {
        /// Output .geo.json path
        #[arg(long, short, default_value = "out/model.geo.json")]
        out: String,
        /// Arm geometry: classic (Steve) or slim (Alex)
        #[arg(long, default_value = "classic")]
        arm: String,
        /// Geometry identifier written into the file
        #[arg(long, default_value = "geometry.lyco_player")]
        identifier: String,
    },
    /// Validate a Bedrock geometry file; exits non-zero on any problem
    Validate {
        /// Geometry .geo.json to inspect
        path: String,
        /// Emit machine-readable JSON instead of a human summary
        #[arg(long)]
        json: bool,
    },
    /// Report a geometry file's bones, cubes and texture size
    Info {
        /// Geometry .geo.json to inspect
        path: String,
    },
    /// Inspect a GLB container without converting it (F3 input side)
    GlbInfo {
        /// GLB file to inspect
        path: String,
    },
    /// Convert a GLB mesh into Bedrock geometry, with a generated texture (F3)
    FromGlb {
        /// GLB file to convert
        path: String,
        /// Output .geo.json path
        #[arg(long, short, default_value = "out/model.geo.json")]
        out: String,
        /// Output texture PNG path; empty string skips writing it
        #[arg(long, default_value = "out/model.png")]
        texture: String,
        /// Voxels along the model's longest axis
        #[arg(long, default_value_t = 24)]
        resolution: u32,
        /// Geometry identifier written into the file
        #[arg(long, default_value = "geometry.lyco_converted")]
        identifier: String,
    },
    /// Inspect a Wavefront OBJ without converting it (F3 input side)
    ObjInfo {
        /// OBJ file to inspect
        path: String,
    },
    /// Convert a Wavefront OBJ into Bedrock geometry, with a generated texture (F3)
    FromObj {
        /// OBJ file to convert
        path: String,
        /// Output .geo.json path
        #[arg(long, short, default_value = "out/model.geo.json")]
        out: String,
        /// Output texture PNG path
        #[arg(long, default_value = "out/model.png")]
        texture: String,
        /// Voxels along the model's longest axis
        #[arg(long, default_value_t = 24)]
        resolution: u32,
        /// Geometry identifier written into the file
        #[arg(long, default_value = "geometry.lyco_converted")]
        identifier: String,
    },
}

/// Restore the default SIGPIPE handler on Unix.
///
/// Rust ignores SIGPIPE, so when stdout is closed early - piping into `head`
/// or `grep -q` - the next `println!` fails and panics with "Broken pipe".
/// Conventional Unix tools are simply terminated by the signal instead.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: both arguments are libc constants and `signal` is the portable
    // way to install a handler. No Rust state is read or written.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> Result<()> {
    restore_sigpipe();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Bundle {
            prompt,
            out,
            arm,
            bgm_duration,
            bgm_backend,
            image_backend,
            spine,
            spine_pma,
        } => run_bundle(
            prompt,
            out,
            arm,
            bgm_duration,
            bgm_backend,
            image_backend,
            spine,
            spine_pma,
        ),
        Cmd::Skin { cmd } => run_skin(cmd),
        Cmd::Bsm { cmd } => run_bsm(cmd),
        Cmd::Bgm {
            prompt,
            duration,
            out,
            backend,
        } => run_bgm(prompt, duration, out, backend),
        Cmd::Lyrics {
            prompt,
            out,
            backend,
        } => run_lyrics(prompt, out, backend),
        Cmd::Portrait {
            prompt,
            out,
            backend,
        } => run_portrait(prompt, out, backend),
        Cmd::Mcp => lyco_mcp::serve_stdio(&Registry::standard()),
        Cmd::Kaggle { cmd } => run_kaggle(cmd),
        Cmd::Spine { cmd } => run_spine(cmd),
    }
}

fn run_spine(cmd: SpineCmd) -> Result<()> {
    match cmd {
        SpineCmd::Info { path } => {
            let text =
                std::fs::read_to_string(&path).with_context(|| format!("cannot read {path}"))?;
            let atlas = Atlas::parse(&text);
            let info = atlas.info();
            println!("pages:   {}", info.pages.len());
            for page in &info.pages {
                println!("  {page}");
            }
            println!("regions: {}", info.regions.len());
            println!("declares size: {}", info.has_size);
            println!("declares pma : {}", info.has_pma);
            Ok(())
        }
        SpineCmd::Version { path } => {
            let bytes = std::fs::read(&path).with_context(|| format!("cannot read {path}"))?;
            let header = read_header(&bytes)?;
            println!("file:    {path} ({} bytes)", bytes.len());
            println!(
                "version: {}",
                header.version.as_deref().unwrap_or("(not declared)")
            );
            println!("hash:    {}", header.hash.as_deref().unwrap_or("(none)"));
            println!("family:  {}", header.family().name());
            Ok(())
        }
        SpineCmd::Normalize {
            path,
            dir,
            out,
            write,
            pma,
            strip_3_8,
            sanitize,
        } => {
            let text =
                std::fs::read_to_string(&path).with_context(|| format!("cannot read {path}"))?;
            let atlas = Atlas::parse(&text);
            if atlas.pages().is_empty() {
                bail!("{path} has no pages, so there is nothing to repair");
            }

            // The truth about a page's size lives in its PNG header, so look
            // for the page images next to the atlas unless told otherwise.
            let atlas_path = PathBuf::from(&path);
            let base = match &dir {
                Some(dir) => PathBuf::from(dir),
                None => match atlas_path.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
                    _ => PathBuf::from("."),
                },
            };

            let mut sizes = BTreeMap::new();
            let mut renames = BTreeMap::new();
            for page in atlas.pages() {
                let png = base.join(&page.name);
                match std::fs::read(&png) {
                    Ok(bytes) => match png_size(&bytes) {
                        Ok(size) => {
                            sizes.insert(page.name.clone(), size);
                        }
                        Err(err) => eprintln!("{}: {err}", png.display()),
                    },
                    Err(_) => eprintln!(
                        "{}: not found, so this page's size cannot be repaired",
                        png.display()
                    ),
                }
                if sanitize {
                    renames.insert(page.name.clone(), sanitize_page_name(&page.name));
                }
            }

            let opts = Normalize {
                sizes,
                pma,
                renames,
            };
            let result = atlas.normalize(&opts);
            // Stripping runs last, so it wins over `--pma`: asking for both at
            // once is asking for a file no reader can open.
            let stripped = Atlas::parse(&result.text).strip_after_3_8();
            let text = if stripped.changed {
                &stripped.text
            } else {
                &result.text
            };
            let changed = result.changed || stripped.changed;

            if pma && strip_3_8 {
                eprintln!(
                    "--pma and --strip-3-8 want opposite things; the pma lines were removed anyway"
                );
            }
            for page in &result.fixed_size {
                eprintln!("{page}: size was wrong and was corrected from the PNG");
            }
            for page in &result.missing_size {
                eprintln!("{page}: size is unknown, so it was left alone");
            }
            for field in &stripped.removed {
                eprintln!("removed {field}, which a Spine 3.8 runtime cannot read");
            }

            if write {
                std::fs::write(&atlas_path, text)?;
                println!("rewrote {}", atlas_path.display());
            } else if let Some(out) = &out {
                std::fs::write(out, text)?;
                println!("wrote {out}");
            } else {
                // No destination means print, so the result can be piped.
                print!("{text}");
                eprintln!("changed: {changed}");
                return Ok(());
            }
            println!("changed: {changed}");
            Ok(())
        }
    }
}

fn run_kaggle(cmd: KaggleCmd) -> Result<()> {
    match cmd {
        KaggleCmd::Render {
            owner,
            slug,
            seeds,
            out,
        } => kaggle::render(&owner, &slug, &seeds, &out),
        KaggleCmd::Push { dir, timeout } => kaggle::push(&dir, timeout),
        KaggleCmd::Status { kernel } => kaggle::status(&kernel),
        KaggleCmd::Fetch { kernel, out } => kaggle::fetch(&kernel, &out),
        KaggleCmd::Run {
            owner,
            slug,
            seeds,
            out,
            arm,
            poll_secs,
            max_polls,
        } => {
            let arm = parse_arm(&arm)?;
            kaggle::run(&owner, &slug, &seeds, &out, arm, poll_secs, max_polls)
        }
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

#[allow(clippy::too_many_arguments)]
fn run_bundle(
    prompt: String,
    out: String,
    arm: String,
    bgm_duration: u32,
    bgm_backend: String,
    image_backend: String,
    spine: Vec<String>,
    spine_pma: bool,
) -> Result<()> {
    let mut request = BundleRequest::new(prompt, out);
    request.arm = parse_arm(&arm)?;
    request.bgm_duration_secs = bgm_duration;
    request.bgm_backend = bgm_backend;
    request.image_backend = image_backend;
    request.spine = spine.into_iter().map(PathBuf::from).collect();
    request.spine_pma = spine_pma;

    let manifest = lyco_bundle::build(&request)?;

    println!("bundle written to {}", request.out_dir.display());
    for artifact in &manifest.artifacts {
        println!("  {}", describe_artifact(artifact));
    }
    println!("  manifest.json");
    for note in &manifest.notes {
        println!("  note: {note}");
    }
    Ok(())
}

fn describe_artifact(artifact: &Artifact) -> String {
    let flag = if artifact.placeholder {
        "  <-- placeholder"
    } else {
        ""
    };
    let head = format!("{:<9} {:<16}", artifact.kind, artifact.file);
    let tail = format!("{:<22} {:>9} B", artifact.backend, artifact.bytes);
    format!("{head} {tail}{flag}")
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

        SkinCmd::FixOverlay {
            path,
            out,
            arm,
            threshold,
            keep_percent,
        } => {
            let arm = parse_arm(&arm)?;
            let mut skin = Skin::load(&path)?;
            let cfg = OverlayFixConfig {
                white_threshold: threshold,
                keep_percent,
            };
            let report = fix_white_overlay(&mut skin, arm, &cfg)?;
            if !report.first_layer_intact {
                bail!("the base layer lost opacity; refusing to save a broken skin");
            }
            skin.save(&out)?;
            println!("wrote {out}");
            for o in &report.outcomes {
                let verdict = if o.cleared_entirely {
                    "cleared"
                } else {
                    "kept"
                };
                println!(
                    "  {:<10} {:>4} px cleared, {:>4} kept  [{verdict}]",
                    o.part.name(),
                    o.cleared,
                    o.kept
                );
            }
            println!("total cleared: {} px", report.total_cleared());
            Ok(())
        }

        SkinCmd::Preview {
            path,
            out,
            arm,
            scale,
        } => {
            let arm = parse_arm(&arm)?;
            let skin = Skin::load(&path)?;
            let validation = skin.validate();
            if !validation.ok() {
                eprintln!("warning: {path} does not validate; the preview may be misleading");
            }
            let view = character_view(&skin, arm, Side::Front).scaled(scale);
            view.save(&out)?;
            println!(
                "wrote {out} ({}x{}) from {}x{} skin",
                view.width,
                view.height,
                skin.width(),
                skin.height()
            );
            Ok(())
        }

        SkinCmd::Generate {
            prompt,
            out,
            backend,
            owner,
            slug,
            seeds,
            work_dir,
            arm,
            poll_secs,
            max_polls,
        } => {
            let arm = parse_arm(&arm)?;
            match backend.as_str() {
                // The one path that actually generates: a notebook pushed to
                // Kaggle, run on a free T4, and the result repaired locally.
                "kaggle" => {
                    if owner.trim().is_empty() || slug.trim().is_empty() {
                        bail!("--backend kaggle needs both --owner and --slug");
                    }
                    kaggle::run(&owner, &slug, &seeds, &work_dir, arm, poll_secs, max_polls)
                }
                other => {
                    // A local diffusion command line is the other option, but
                    // nothing here can guess its interface, so it is refused
                    // rather than faked.
                    let _ = (&prompt, &out);
                    bail!(
                        "unknown skin backend '{other}'.\n\
                         Use --backend kaggle --owner <you> --slug <kernel> for free \
                         cloud GPU generation,\n\
                         or 'skin template' for an offline placeholder."
                    )
                }
            }
        }
    }
}

fn run_bsm(cmd: BsmCmd) -> Result<()> {
    match cmd {
        BsmCmd::Humanoid {
            out,
            arm,
            identifier,
        } => {
            let arm = parse_arm(&arm)?;
            let file = player_file(&identifier, arm);
            let problems = file.validate();
            if !problems.is_empty() {
                print_problems(&problems);
                bail!("the generated player model failed its own validation");
            }
            write_text(&out, &file.to_json()?)?;
            let g = &file.geometry[0];
            let (tw, th) = (g.description.texture_width, g.description.texture_height);
            println!("wrote {out}");
            println!("identifier: {}", g.description.identifier);
            println!("arm model : {}", arm.name());
            println!("bones     : {}", g.bones.len());
            println!("cubes     : {}", g.cube_count());
            println!("texture   : {tw}x{th}");
            Ok(())
        }

        BsmCmd::Validate { path, json } => {
            let file = load_geometry(&path)?;
            let problems = file.validate();
            if json {
                println!("{}", geometry_report_json(&file, &problems));
            } else {
                let verdict = if problems.is_empty() {
                    "valid"
                } else {
                    "INVALID"
                };
                println!("{verdict}: {} geometry entries", file.geometry.len());
                print_problems(&problems);
            }
            if !problems.is_empty() {
                bail!("{path}: {} problem(s)", problems.len());
            }
            Ok(())
        }

        BsmCmd::Info { path } => {
            let file = load_geometry(&path)?;
            println!("path  : {path}");
            println!("format: {}", file.format_version);
            for g in &file.geometry {
                println!("geometry : {}", g.description.identifier);
                println!("bones    : {}", g.bones.len());
                println!("cubes    : {}", g.cube_count());
                for b in &g.bones {
                    let parent = b.parent.as_deref().unwrap_or("-");
                    println!(
                        "  {:<12} parent={:<12} cubes={}",
                        b.name,
                        parent,
                        b.cubes.len()
                    );
                }
            }
            Ok(())
        }

        BsmCmd::FromGlb {
            path,
            out,
            texture,
            resolution,
            identifier,
        } => {
            let bytes = std::fs::read(&path)?;
            let glb = Glb::parse(&bytes).with_context(|| format!("cannot read {path}"))?;
            let cfg = ConvertConfig {
                resolution,
                identifier,
                ..Default::default()
            };
            let conversion = convert_glb(&glb, &cfg)?;
            write_text(&out, &conversion.geometry.to_json()?)?;
            if !texture.is_empty() {
                conversion.save_texture(&texture)?;
            }
            println!("wrote {out}");
            if !texture.is_empty() {
                println!("wrote {texture}");
            }
            println!("triangles: {}", conversion.triangles);
            println!("voxels   : {}", conversion.voxels);
            println!("boxes    : {}", conversion.boxes);
            Ok(())
        }

        BsmCmd::ObjInfo { path } => {
            let text =
                std::fs::read_to_string(&path).with_context(|| format!("cannot read {path}"))?;
            let mesh = lyco_bsm::parse_obj(&text)?;
            let size = [
                mesh.max[0] - mesh.min[0],
                mesh.max[1] - mesh.min[1],
                mesh.max[2] - mesh.min[2],
            ];
            println!("vertices:  {}", mesh.positions.len());
            println!("triangles: {}", mesh.triangle_count());
            println!("groups:    {}", mesh.groups.len());
            for group in &mesh.groups {
                println!("  {} ({} triangles)", group.name, group.count);
            }
            println!("bounds:    {:?} to {:?}", mesh.min, mesh.max);
            println!("size:      {size:?}");
            println!("uvs:       {}", mesh.uvs.is_some());
            println!("normals:   {}", mesh.normals.is_some());
            println!("colours:   {}", mesh.colors.is_some());
            Ok(())
        }
        BsmCmd::FromObj {
            path,
            out,
            texture,
            resolution,
            identifier,
        } => {
            let text =
                std::fs::read_to_string(&path).with_context(|| format!("cannot read {path}"))?;
            let mesh = lyco_bsm::parse_obj(&text)?;
            if mesh.is_empty() {
                bail!("{path} has no triangles");
            }
            let cfg = ConvertConfig {
                resolution: resolution.clamp(1, 64),
                identifier,
                ..Default::default()
            };
            let conversion = lyco_bsm::convert_obj(&mesh, &cfg)?;
            std::fs::write(&out, conversion.geometry.to_json()?)?;
            println!(
                "wrote {out}; {} triangles -> {} voxels -> {} boxes",
                conversion.triangles, conversion.voxels, conversion.boxes
            );
            if !texture.is_empty() {
                conversion.save_texture(&texture)?;
                println!("wrote {texture}");
            }
            Ok(())
        }
        BsmCmd::GlbInfo { path } => {
            let bytes = std::fs::read(&path)?;
            let glb = Glb::parse(&bytes).with_context(|| format!("cannot read {path}"))?;
            let g = &glb.gltf;
            let generator = g.asset.generator.as_deref().unwrap_or("-");
            println!("path       : {path}");
            println!("glb version: {}", glb.version);
            println!("generator  : {generator}");
            println!("nodes      : {}", g.nodes.len());
            println!("meshes     : {}", g.meshes.len());
            println!("materials  : {}", g.materials.len());
            println!("bin bytes  : {}", glb.bin.len());
            for (i, mesh) in g.meshes.iter().enumerate() {
                let name = mesh.name.as_deref().unwrap_or("(unnamed)");
                println!("  mesh {i}: {name}, {} primitive(s)", mesh.primitives.len());
                for (j, prim) in mesh.primitives.iter().enumerate() {
                    println!("    primitive {j}: {}", primitive_summary(&glb, prim));
                }
            }
            Ok(())
        }
    }
}

/// Read and parse a Bedrock geometry file with a useful error message.
fn load_geometry(path: &str) -> Result<GeometryFile> {
    let text = std::fs::read_to_string(path)?;
    GeometryFile::from_json(&text)
        .with_context(|| format!("{path} is not valid Bedrock geometry JSON"))
}

/// Describe one glTF primitive by vertex and triangle count.
fn primitive_summary(glb: &Glb, prim: &Primitive) -> String {
    let Some(pos) = prim.attributes.position else {
        return "no POSITION attribute".to_string();
    };
    let vertices = glb.read_vec3(pos).map(|v| v.len()).unwrap_or(0);
    let triangles = prim
        .indices
        .and_then(|i| glb.read_indices(i).ok())
        .map(|v| v.len() / 3)
        .unwrap_or(0);
    format!("{vertices} vertices, {triangles} triangles")
}

fn write_text(path: &str, text: &str) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, text)?;
    Ok(())
}

fn print_problems(problems: &[Problem]) {
    for p in problems {
        println!("  [error] {}: {}", p.code, p.message);
    }
}

fn geometry_report_json(file: &GeometryFile, problems: &[Problem]) -> String {
    let issues: Vec<serde_json::Value> = problems
        .iter()
        .map(|p| {
            serde_json::json!({
                "code": p.code,
                "message": p.message,
            })
        })
        .collect();
    let geometry: Vec<serde_json::Value> = file
        .geometry
        .iter()
        .map(|g| {
            serde_json::json!({
                "identifier": g.description.identifier,
                "bones": g.bones.len(),
                "cubes": g.cube_count(),
            })
        })
        .collect();
    let value = serde_json::json!({
        "valid": problems.is_empty(),
        "format_version": file.format_version,
        "geometry": geometry,
        "problems": issues,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

fn run_portrait(prompt: String, out: String, backend_spec: String) -> Result<()> {
    let backend = image_backend(&backend_spec)?;
    if backend.name() == "stub" {
        eprintln!("warning: --backend stub draws a procedural placeholder, not a likeness.");
    }
    let request = PortraitRequest::new(prompt, out);
    let result = backend.generate(&request)?;
    println!("wrote {}", result.path.display());
    println!("backend  : {}", result.backend);
    println!("size     : {}x{}", result.width, result.height);
    println!("bytes    : {}", result.bytes);
    Ok(())
}

fn run_lyrics(prompt: String, out: String, backend_spec: String) -> Result<()> {
    let backend = lyrics_backend(&backend_spec)?;
    if backend.name() == "stub" {
        eprintln!("warning: --backend stub writes placeholder lines, not songwriting.");
    }
    let request = LyricsRequest::new(prompt, out);
    let result = backend.generate(&request)?;
    println!("wrote {}", result.path.display());
    println!("backend  : {}", result.backend);
    println!("sections : {}", result.sections);
    println!("lines    : {}", result.lines);
    Ok(())
}

fn run_bgm(prompt: String, duration: u32, out: String, backend_spec: String) -> Result<()> {
    if duration == 0 {
        bail!("--duration must be at least 1 second");
    }
    let backend = backend_from_spec(&backend_spec)?;
    if backend.name() == "stub" {
        eprintln!("warning: --backend stub synthesises a placeholder tone, not music.");
        eprintln!("         Point --backend at a real generator to produce actual BGM.");
    }
    let request = BgmRequest::new(prompt, duration, out);
    let result = backend.generate(&request)?;
    println!("wrote {}", result.path.display());
    println!("backend  : {}", result.backend);
    println!("duration : {:.2}s", result.duration_secs);
    println!("bytes    : {}", result.bytes);
    Ok(())
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
