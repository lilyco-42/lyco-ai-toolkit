//! lyco-cli - unified AI asset studio (one entry, many crates).
//!
//! See _RESEARCH/00-requirement.md for the one-line requirement,
//! _RESEARCH/03-decision.md for the build-vs-buy matrix, and
//! _RESEARCH/04-validation-checklist.md for milestones S0-S5.
//!
//! Implemented today, all offline with no GPU:
//!   * `skin` (F1) - template / validate / info / quantize
//!   * `bsm`  (F2/F3) - humanoid / validate / info / glb-info
//!
//! Everything that needs an image or audio model still fails fast, so a green
//! run can never be mistaken for a produced asset.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use lyco_audio::{backend_from_spec, BgmRequest};
use lyco_bsm::{glb::Primitive, player_file, GeometryFile, Glb, Problem};
use lyco_bundle::{Artifact, BundleRequest};
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
    /// Generate lyrics with verse/chorus/bridge structure (F8, not implemented)
    Lyrics {
        /// Theme or title
        prompt: String,
    },
    /// Generate just a portrait (F7, not implemented; `bundle` covers it)
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
        } => run_bundle(prompt, out, arm, bgm_duration, bgm_backend, image_backend),
        Cmd::Skin { cmd } => run_skin(cmd),
        Cmd::Bsm { cmd } => run_bsm(cmd),
        Cmd::Bgm {
            prompt,
            duration,
            out,
            backend,
        } => run_bgm(prompt, duration, out, backend),
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

#[allow(clippy::too_many_arguments)]
fn run_bundle(
    prompt: String,
    out: String,
    arm: String,
    bgm_duration: u32,
    bgm_backend: String,
    image_backend: String,
) -> Result<()> {
    let mut request = BundleRequest::new(prompt, out);
    request.arm = parse_arm(&arm)?;
    request.bgm_duration_secs = bgm_duration;
    request.bgm_backend = bgm_backend;
    request.image_backend = image_backend;

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

fn fail_fast(cmd: &Cmd) -> Result<()> {
    let (what, next) = match cmd {
        Cmd::Bundle { .. } => ("bundle", "S5"),
        Cmd::Bgm { .. } => ("bgm", "S3"),
        Cmd::Lyrics { .. } => ("lyrics", "S4"),
        Cmd::Portrait { .. } => ("portrait", "S4"),
        Cmd::Skin { .. } => ("skin", "S1"),
        Cmd::Bsm { .. } => ("bsm", "S2"),
    };
    bail!(
        "lyco-cli: subcommand '{what}' is scaffolded but not implemented yet.\n\
         Next milestone: {next} - see _RESEARCH/04-validation-checklist.md"
    );
}
