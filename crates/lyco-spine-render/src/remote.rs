//! The hosted mode: ship assets to the render API on lain42.top.
//!
//! `lyco-spine-render render ... --remote` keeps every local flag and
//! simply draws elsewhere, against the caller's point balance (frame
//! 5 pts, animate 20 pts; rendering failures are not charged). Local
//! rendering stays free and needs nothing — `--remote` is for pipelines
//! that want the hosted result, or agents spending their own budget.

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

use lyco_spine::Atlas as AtlasText;
use lyco_spine_render::{RenderRequest, SpineSource};

const DEFAULT_BASE: &str = "https://lain42.top/studio/render";
const TIMEOUT: Duration = Duration::from_secs(150);

fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).cloned()
}

fn bearer(args: &[String]) -> Result<String> {
    if let Some(value) = flag(args, "--token") {
        return Ok(value);
    }
    match std::env::var("SPINE_RENDER_TOKEN") {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => bail!(
            "--remote needs a token: set SPINE_RENDER_TOKEN or pass --token <value> \
             (register at lain42.top/studio to get one; signup comes with 25 pts)"
        ),
    }
}

fn request_id(args: &[String]) -> String {
    flag(args, "--request-id").unwrap_or_else(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("cli-{:x}-{:x}", nanos, std::process::id())
    })
}

/// Base64-pack the three things the API expects. Page images are
/// discovered the same way local rendering discovers them: beside the
/// atlas, named by the atlas itself.
fn pack_assets(src: &SpineSource) -> Result<Value> {
    let atlas_text = std::fs::read_to_string(&src.atlas)
        .with_context(|| format!("cannot read {}", src.atlas.display()))?;
    let skel_bytes = std::fs::read(&src.skeleton)
        .with_context(|| format!("cannot read {}", src.skeleton.display()))?;

    let beside = match src.atlas.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let parsed = AtlasText::parse(&atlas_text);
    if parsed.pages().is_empty() {
        bail!("{} has no pages", src.atlas.display());
    }
    let mut pages = Vec::new();
    for page in parsed.pages() {
        let png = std::fs::read(beside.join(&page.name))
            .with_context(|| format!("cannot read the page image {}", page.name))?;
        pages.push(json!({ "name": page.name, "png": B64.encode(png) }));
    }

    let mut body = json!({
        "atlas": B64.encode(atlas_text.as_bytes()),
        "pages": pages,
    });
    if src.binary {
        body["skel"] = Value::String(B64.encode(skel_bytes));
    } else {
        body["skel_json"] = Value::String(String::from_utf8_lossy(&skel_bytes).into_owned());
    }
    Ok(body)
}

fn post(args: &[String], kind: &str, body: Value) -> Result<Value> {
    let base = flag(args, "--base").unwrap_or_else(|| DEFAULT_BASE.to_string());
    let url = format!("{base}/{kind}");
    let token = bearer(args)?;
    let response = ureq::post(&url)
        .timeout(TIMEOUT)
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(body)
        .map_err(|error| match error {
            ureq::Error::Status(code, response) => {
                let detail = response.into_string().unwrap_or_default();
                anyhow!("HTTP {code} from {url}: {detail}")
            }
            other => anyhow!("request to {url} failed: {other}"),
        })?;
    response
        .into_json::<Value>()
        .with_context(|| format!("{url} answered with something other than JSON"))
}

fn unpack_image(payload: &Value, out: &Path) -> Result<()> {
    let encoded = payload
        .get("image_base64")
        .and_then(Value::as_str)
        .context("the API answered without an image")?;
    let bytes = B64
        .decode(encoded)
        .context("the API answered with a broken image")?;
    std::fs::write(out, bytes).with_context(|| format!("cannot write {}", out.display()))
}

fn report(payload: &Value, out: &Path) {
    println!("wrote {} (remote)", out.display());
    if let Some(delta) = payload.pointer("/points/delta") {
        println!("charged {delta} pts");
    }
    if let Some(balance) = payload.get("balance") {
        println!("balance: {balance}");
    }
}

pub fn describe(src: &SpineSource, args: &[String]) -> Result<()> {
    let mut body = pack_assets(src)?;
    body["request_id"] = Value::String(request_id(args));
    let payload = post(args, "describe", body)?;
    match payload.get("describe").and_then(Value::as_str) {
        Some(text) => {
            print!("{text}");
            Ok(())
        }
        None => bail!("unexpected describe answer: {}", payload),
    }
}

pub fn render(
    src: &SpineSource,
    req: &RenderRequest,
    out: &Path,
    animate: bool,
    frames: u32,
    fps: u32,
    args: &[String],
) -> Result<()> {
    let mut body = pack_assets(src)?;
    body["request_id"] = Value::String(request_id(args));
    if let Some(name) = &req.animation {
        body["animation"] = Value::String(name.clone());
    }
    body["width"] = json!(req.width);
    body["height"] = json!(req.height);
    if req.background != [0, 0, 0, 0] {
        body["background"] = json!(req.background);
    }
    if animate {
        if req.animation.is_none() {
            bail!("remote animate needs --animation (the API requires a name)");
        }
        body["frames"] = json!(frames);
        body["fps"] = json!(fps);
    } else {
        body["time"] = json!(req.time);
    }
    let kind = if animate { "animate" } else { "frame" };
    let payload = post(args, kind, body)?;
    if payload.get("ok") != Some(&Value::Bool(true)) {
        bail!("the API refused: {}", payload);
    }
    unpack_image(&payload, out)?;
    report(&payload, out);
    Ok(())
}