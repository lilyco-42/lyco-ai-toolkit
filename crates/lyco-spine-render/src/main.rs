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
    describe, render_animation, render_to_file, rig, Anchor, RenderRequest, SpineSource,
};

fn usage() -> String {
    [
        "usage:",
        "  lyco-spine-render describe <atlas> <skeleton>",
        "  lyco-spine-render rig      <atlas> <skeleton>",
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
            for bone in &info.bones {
                println!("{bone}");
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
