//! A small command line over `lyco-spine-render`.
//!
//! Two verbs, because those are the two things worth doing from a shell: ask
//! what a skeleton is, and draw it.
//!
//! Note the licence: this binary links the official Spine runtime, so running it
//! is fine for evaluation but shipping it needs a Spine licence.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use lyco_spine_render::{
    describe, parts, parts_sequence, render_animation, render_to_file, rig, Anchor, RenderRequest,
    SpineSource, PARTS,
};

mod remote;

fn usage() -> String {
    [
        "usage:",
        "  lyco-spine-render describe <atlas> <skeleton>",
        "  lyco-spine-render rig      <atlas> <skeleton>",
        "  lyco-spine-render parts    <atlas> <skeleton> <out-prefix>",
        "  lyco-spine-render parts_seq <atlas> <skeleton> <out-dir> [options]",
        "  lyco-spine-render render   <atlas> <skeleton> <out.png> [options]",
        "  lyco-spine-render animate  <atlas> <skeleton> <out.gif> [options]",
        "",
        "options:",
        "  --frames <n>       animate: how many frames (default 24)",
        "  --fps <n>          animate: frames per second (default 12)",
        "  --animation <name>  play an animation; without it, the setup pose",
        "  --time <seconds>   advance the pose (default 0)",
        "  --anchor <where>   centre (default) or footline, as in 'footline:0.8',",
        "                     which is how PRTS wiki frames a battle sprite",
        "  --size <WxH>       frame size (default 512x512)",
        "  --background <R,G,B,A>  default 0,0,0,0",
        "",
        "hosted mode (draw on lain42.top against a point balance):",
        "  --remote           render/describe via the hosted API instead",
        "                     (frame 5 pts, animate 20 pts, describe free;",
        "                     failed renders are not charged)",
        "  --token <value>    bearer token, or env SPINE_RENDER_TOKEN",
        "  --request-id <id>  idempotency key: retries with the same id",
        "                     are never charged twice",
        "  --base <url>       API base (default https://lain42.top/studio/render)",
    ]
    .join("\n")
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).cloned()
}

fn parse_size(text: &str) -> Result<(u32, u32)> {
    let (w, h) = text
        .split_once(['x', 'X'])
        .with_context(|| format!("'{text}' is not WxH"))?;
    Ok((
        w.trim().parse().context("bad width")?,
        h.trim().parse().context("bad height")?,
    ))
}

fn parse_background(text: &str) -> Result<[u8; 4]> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        bail!("'{text}' is not R,G,B,A");
    }
    let mut out = [0u8; 4];
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = part.parse().context("a channel is not 0-255")?;
    }
    Ok(out)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("describe") => {
            let (atlas, skeleton) = match (args.get(1), args.get(2)) {
                (Some(a), Some(s)) => (a, s),
                _ => bail!("{}", usage()),
            };
            let src = SpineSource::new(atlas, skeleton);
            if args.iter().any(|a| a == "--remote") {
                return remote::describe(&src, &args);
            }
            let info = describe(&src)?;
            println!("atlas:     {atlas}");
            println!("skeleton:  {skeleton}");
            println!("version:   {}", info.version);
            println!("hash:      {}", info.hash);
            println!("meshes:    {}", info.meshes);
            println!("vertices:  {}", info.vertices);
            println!("triangles: {}", info.triangles);
            println!("animations: {}", info.animations.len());
            for name in info.animations.iter().take(15) {
                println!("  {name}");
            }
            if info.animations.len() > 15 {
                println!("  ... and {} more", info.animations.len() - 15);
            }
            Ok(())
        }
        Some("rig") => {
            let (atlas, skeleton) = match (args.get(1), args.get(2)) {
                (Some(a), Some(s)) => (a, s),
                _ => bail!("{}", usage()),
            };
            let src = SpineSource::new(atlas, skeleton);
            let info = rig(&src)?;
            // Tab-separated, so a caller comparing rigs does not have to parse
            // prose: one bone per line, then one "name <tab> seconds" per clip.
            println!("bones:\t{}", info.bones.len());
            println!("slots:\t{}", info.slots.len());
            println!("animations:\t{}", info.animations.len());
            println!("--bones--");
            for (bone, parent) in &info.bones {
                println!("{bone}\t{parent}");
            }
            println!("--slots--");
            for (slot, bone) in &info.slots {
                println!("{slot}\t{bone}");
            }
            println!("--animations--");
            for (name, seconds) in &info.animations {
                println!("{name}\t{seconds}");
            }
            Ok(())
        }
        Some("parts") => {
            let (atlas, skeleton, out) = match (args.get(1), args.get(2), args.get(3)) {
                (Some(a), Some(s), Some(o)) => (a, s, o),
                _ => bail!("{}", usage()),
            };
            let (width, height) = match flag(&args, "--size") {
                Some(text) => parse_size(&text)?,
                None => (512, 512),
            };
            let req = RenderRequest {
                width,
                height,
                animation: flag(&args, "--animation"),
                time: match flag(&args, "--time") {
                    Some(text) => text.parse().context("bad --time")?,
                    None => 0.0,
                },
                ..RenderRequest::default()
            };
            let src = SpineSource::new(atlas, skeleton);
            let labelled = parts(&src, &req)?;
            let prefix = PathBuf::from(out);
            let picture = PathBuf::from(format!("{}_rgb.png", prefix.display()));
            let mask = PathBuf::from(format!("{}_labels.png", prefix.display()));
            labelled.frame.save(&picture)?;
            labelled.labels.save(&mask)?;
            println!("wrote {}", picture.display());
            println!("wrote {}", mask.display());
            // Named rather than numbered: a caller checking whether a skeleton
            // has a left forearm should not need the constant beside it.
            for (name, count) in PARTS.iter().zip(&labelled.counts) {
                println!("  {name}\t{count}");
            }
            Ok(())
        }
        Some("parts_seq") => {
            let (atlas, skeleton, out) = match (args.get(1), args.get(2), args.get(3)) {
                (Some(a), Some(s), Some(o)) => (a, s, o),
                _ => bail!("{}", usage()),
            };
            let (width, height) = match flag(&args, "--size") {
                Some(text) => parse_size(&text)?,
                None => (256, 256),
            };
            let frames = match flag(&args, "--frames") {
                Some(text) => text.parse().context("bad --frames")?,
                None => 24,
            };
            let fps = match flag(&args, "--fps") {
                Some(text) => text.parse().context("bad --fps")?,
                None => 12,
            };
            let body_only = args.iter().any(|a| a == "--body-bounds");
            let req = RenderRequest {
                width,
                height,
                animation: flag(&args, "--animation"),
                time: match flag(&args, "--time") {
                    Some(text) => text.parse().context("bad --time")?,
                    None => 0.0,
                },
                ..RenderRequest::default()
            };

            let src = SpineSource::new(atlas, skeleton);
            let seq = parts_sequence(&src, &req, frames, fps, body_only)?;

            let dir = PathBuf::from(out);
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
            let keep = match flag(&args, "--keep") {
                Some(text) => text.parse::<f32>().context("bad --keep")?,
                None => 0.0,
            };

            let mut manifest = String::from("frame\tcoverage");
            for name in PARTS.iter() {
                manifest.push('\t');
                manifest.push_str(name);
            }
            manifest.push('\n');

            let mut kept = 0;
            for (i, frame) in seq.iter().enumerate() {
                let tag = format!("frame_{:05}", i);
                let rgb_path = dir.join(format!("{tag}_rgb.png",));
                let lab_path = dir.join(format!("{tag}_labels.png"));
                frame.rgb.save(&rgb_path)?;
                frame.labels.save(&lab_path)?;
                let drop = keep > 0.0 && frame.coverage < keep;
                if drop {
                    std::fs::remove_file(&rgb_path).ok();
                    std::fs::remove_file(&lab_path).ok();
                } else {
                    kept += 1;
                }
                manifest.push_str(&format!(
                    "{}\t{:.4}",
                    if drop { "DROP" } else { &tag },
                    frame.coverage
                ));
                for count in &frame.counts {
                    manifest.push('\t');
                    manifest.push_str(&count.to_string());
                }
                manifest.push('\n');
            }
            let manifest_path = dir.join("meta.tsv");
            std::fs::write(&manifest_path, manifest)?;

            let mean: f32 = seq.iter().map(|f| f.coverage).sum::<f32>() / seq.len() as f32;
            println!(
                "wrote {} ({} frames, kept {kept}, mean coverage {:.1}%)",
                dir.display(),
                seq.len(),
                100.0 * mean
            );
            println!("  manifest: {}", manifest_path.display());
            Ok(())
        }
        Some("render") => {
            let (atlas, skeleton, out) = match (args.get(1), args.get(2), args.get(3)) {
                (Some(a), Some(s), Some(o)) => (a, s, o),
                _ => bail!("{}", usage()),
            };
            let (width, height) = match flag(&args, "--size") {
                Some(text) => parse_size(&text)?,
                None => (512, 512),
            };
            let req = RenderRequest {
                width,
                height,
                background: match flag(&args, "--background") {
                    Some(text) => parse_background(&text)?,
                    None => [0, 0, 0, 0],
                },
                animation: flag(&args, "--animation"),
                anchor: match flag(&args, "--anchor") {
                    None => Anchor::Centre,
                    Some(text) => match text.split_once(':') {
                        Some(("footline", value)) => {
                            Anchor::Footline(value.parse().context("bad --anchor footline value")?)
                        }
                        _ => bail!("--anchor wants 'centre' or 'footline:<0..1>'"),
                    },
                },
                time: match flag(&args, "--time") {
                    Some(text) => text.parse().context("bad --time")?,
                    None => 0.0,
                },
            };

            let src = SpineSource::new(atlas, skeleton);
            let into = PathBuf::from(out);
            if args.iter().any(|a| a == "--remote") {
                return remote::render(&src, &req, &into, false, 0, 0, &args);
            }
            let frame = render_to_file(&src, &req, &into)?;
            println!("wrote {} ({width}x{height})", into.display());
            match frame.opaque_bounds() {
                Some((x0, y0, x1, y1)) => {
                    println!("drawn area: {x0},{y0} to {x1},{y1}");
                }
                None => println!("warning: nothing was drawn"),
            }
            Ok(())
        }
        Some("animate") => {
            let (atlas, skeleton, out) = match (args.get(1), args.get(2), args.get(3)) {
                (Some(a), Some(s), Some(o)) => (a, s, o),
                _ => bail!("{}", usage()),
            };
            let (width, height) = match flag(&args, "--size") {
                Some(text) => parse_size(&text)?,
                None => (512, 512),
            };
            let req = RenderRequest {
                width,
                height,
                background: match flag(&args, "--background") {
                    Some(text) => parse_background(&text)?,
                    None => [0, 0, 0, 0],
                },
                animation: flag(&args, "--animation"),
                anchor: match flag(&args, "--anchor") {
                    None => Anchor::Centre,
                    Some(text) => match text.split_once(':') {
                        Some(("footline", value)) => {
                            Anchor::Footline(value.parse().context("bad --anchor footline value")?)
                        }
                        _ => bail!("--anchor wants 'centre' or 'footline:<0..1>'"),
                    },
                },
                time: match flag(&args, "--time") {
                    Some(text) => text.parse().context("bad --time")?,
                    None => 0.0,
                },
            };
            let frames = match flag(&args, "--frames") {
                Some(text) => text.parse().context("bad --frames")?,
                None => 24,
            };
            let fps = match flag(&args, "--fps") {
                Some(text) => text.parse().context("bad --fps")?,
                None => 12,
            };

            let src = SpineSource::new(atlas, skeleton);
            let into = PathBuf::from(out);
            if args.iter().any(|a| a == "--remote") {
                return remote::render(&src, &req, &into, true, frames, fps, &args);
            }
            let written = render_animation(&src, &req, &into, frames, fps)?;
            println!(
                "wrote {} ({written} frames at {fps} fps, {width}x{height})",
                into.display()
            );
            Ok(())
        }
        _ => bail!("{}", usage()),
    }
}
