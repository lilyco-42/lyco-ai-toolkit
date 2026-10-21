//! Lyco Spine Viewer — a desktop window that plays Spine 3.8 skeletons.
//!
//! `tao` owns the window, `wry` renders the page, and the page asks Rust for
//! work over IPC: every message names the atlas/skel pair, the animation and
//! the pose time, and Rust answers with a PNG data URL (still) or an export
//! path (PNG / GIF).
//!
//! Licence: this binary links the official Spine runtime through
//! `lyco-spine-render`, so running it is fine for evaluation, but shipping it
//! needs a Spine licence of your own.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

const BUILD_STAMP: &str = concat!("lyco-spine-viewer-", env!("CARGO_PKG_VERSION"));

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lyco_spine::Atlas as AtlasText;
use lyco_spine_render::{describe, render, render_animation, render_to_file, rig, Anchor,
    RenderRequest, SpineSource};
use serde_json::json;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::{DragDropEvent, WebContext, WebViewBuilder};

/// Replies the page is still waiting for, so a slow render never overlaps
/// into the frame before it.
#[derive(Default)]
struct Ui {
    /// `(request id, json reply)` once the work for that id is done.
    replies: Vec<(u32, String)>,
    /// Files dropped onto the webview, to forward to the page.
    drops: Vec<PathBuf>,
}

fn main() {
    declare_dpi_awareness();
    trace(BUILD_STAMP);
    if let Err(err) = run() {
        report(&format!("Spine Viewer failed to start: {err}"));
    }
}

fn declare_dpi_awareness() {
    const PER_MONITOR_V2: isize = -4;
    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    }
    unsafe {
        SetProcessDpiAwarenessContext(PER_MONITOR_V2);
    }
}

fn run() -> wry::Result<()> {
    let event_loop: EventLoop<()> = EventLoopBuilder::<()>::with_user_event().build();
    let proxy: EventLoopProxy<()> = event_loop.create_proxy();
    let ui = Arc::new(Mutex::new(Ui::default()));
    let shared = Arc::clone(&ui);

    let window = WindowBuilder::new()
        .with_title(format!("Lyco Spine Viewer {BUILD_STAMP}"))
        .with_inner_size(LogicalSize::new(1180.0, 760.0))
        .with_min_inner_size(LogicalSize::new(920.0, 600.0))
        .build(&event_loop)
        .expect("failed to create the native window");

    let mut context = WebContext::new(Some(webview_data_dir()));
    // The page embeds the official spine-ts 3.8 WebGL runtime (the same
    // architecture PRTS wiki uses): the preview is rendered frame-by-frame in
    // the browser, so no per-frame PNG encode / IPC / decode can stutter it.
    // Rust keeps the deterministic headless renders for exports.
    let html = {
        let html = include_str!("../assets/ui.html");
        let runtime = include_str!("../assets/vendor/spine-webgl-3.8.js");
        html.replace("/*__SPINE_RUNTIME__*/", runtime)
    };
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_html(html)
        .with_initialization_script(&format!(
            "window.__BUILD_STAMP = '{}';",
            BUILD_STAMP.replace('\'', "\\'")
        ))
        .with_devtools(true)
        .with_ipc_handler({
            let shared = Arc::clone(&ui);
            let proxy = proxy.clone();
            move |req| {
                let msg = req.body().clone();
                trace(&format!("ipc: {msg}"));
                if let Some(reply) = handle_ipc(&msg) {
                    let mut guard = shared.lock().expect("ui lock");
                    guard.replies.push(reply);
                    let _ = proxy.send_event(());
                }
            }
        })
        // The webview covers the whole window, so the OLE drop target is
        // WebView2's: tao's `DroppedFile` never fires on Windows. Claim every
        // drag event here instead, or the webview swallows it (and a drop can
        // even navigate the page away to a file:// URL).
        .with_drag_drop_handler({
            let shared = Arc::clone(&ui);
            let proxy = proxy.clone();
            move |event| match event {
                DragDropEvent::Drop { paths, .. } => {
                    let mut guard = shared.lock().expect("ui lock");
                    guard.drops.extend(paths);
                    let _ = proxy.send_event(());
                    true
                }
                _ => true,
            }
        })
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(()) => flush(&webview, &ui),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => on_dropped(&webview, &path),
            _ => {}
        }
    });
}

/// A file was dropped onto the window: fill whichever field it matches and
/// let the page load when the pair is complete. Users reach for the `.png`
/// (the most visible of the triplet), a single asset folder, or a whole
/// collection folder holding many assets — all resolve to a pair here.
/// Dotfiles are refused: they are never assets, and the renderer once left
/// `.lyco-3.8-*` cleaned-atlas copies behind that a scan would pick first.
fn on_dropped(webview: &wry::WebView, path: &std::path::Path) {
    if is_dotfile(path) {
        forward_unknown(webview, path);
        return;
    }
    if path.is_dir() {
        match scan_for_atlas(path) {
            Some(atlas) => forward_drop(webview, "atlas", &atlas),
            None => forward_unknown(webview, path),
        }
        return;
    }

    let ext = ext_of(path);
    match ext.as_str() {
        "atlas" => forward_drop(webview, "atlas", path),
        "skel" | "json" => forward_drop(webview, "skel", path),
        // The texture shares the skeleton's stem, so it names the pair.
        "png" | "webp" => match sibling(path, "atlas") {
            Some(atlas) => forward_drop(webview, "atlas", &atlas),
            None => forward_unknown(webview, path),
        },
        _ => forward_unknown(webview, path),
    }
}

fn is_dotfile(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

/// Breadth-first, a few levels deep: an asset folder has the pair at its
/// root, a collection folder has one folder per asset (e.g. Arknights
/// `spine/op/<char>/<variant>/pair`). Nearest layer wins, entries sorted so
/// the pick is deterministic. Capped so a huge tree cannot stall a drop.
fn scan_for_atlas(root: &std::path::Path) -> Option<std::path::PathBuf> {
    const MAX_DEPTH: u32 = 4;
    const MAX_ENTRIES: usize = 2_000;

    let mut queue: std::collections::VecDeque<(std::path::PathBuf, u32)> =
        std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    let mut visited = 0usize;
    // A first skeleton seen with no atlas yet; its sibling may be one entry later.
    let mut fallback: Option<std::path::PathBuf> = None;

    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if visited >= MAX_ENTRIES {
                return None;
            }
            visited += 1;
            if is_dotfile(&path) {
                continue;
            }
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            match ext_of(&path).as_str() {
                "atlas" => return Some(path),
                "skel" | "json" if fallback.is_none() => {
                    fallback = sibling(&path, "atlas");
                }
                _ => {}
            }
        }
    }
    fallback
}

fn ext_of(path: &std::path::Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// `char.png` next to `char.atlas` / `char.skel` — probe on disk.
fn sibling(path: &std::path::Path, ext: &str) -> Option<std::path::PathBuf> {
    let stem = path.file_stem()?;
    let candidate = path.with_file_name(format!("{}.{}", stem.to_string_lossy(), ext));
    candidate.is_file().then_some(candidate)
}

fn forward_drop(webview: &wry::WebView, field: &str, path: &std::path::Path) {
    let value = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let js = format!("__drop(['{field}', '{value}']);");
    let _ = webview.evaluate_script(&js);
    trace(&format!("dropped {field}: {value}"));
}

fn forward_unknown(webview: &wry::WebView, path: &std::path::Path) {
    let value = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let js = format!("__drop(['unknown', '{value}']);");
    let _ = webview.evaluate_script(&js);
    trace(&format!("dropped unknown: {value}"));
}

/// Deliver finished replies and dropped files to the page: one
/// `__reply(id, json)` call per reply, one `__drop` per file.
fn flush(webview: &wry::WebView, ui: &Arc<Mutex<Ui>>) {
    let (replies, drops) = {
        let mut guard = ui.lock().expect("ui lock");
        (
            std::mem::take(&mut guard.replies),
            std::mem::take(&mut guard.drops),
        )
    };
    for path in drops {
        on_dropped(webview, &path);
    }
    for (id, json) in replies {
        let js = format!("__reply({id}, {json});");
        let _ = webview.evaluate_script(&js);
    }
}

/// Parse one page message and do the work synchronously. Returns `(id, json)`
/// so the caller can queue the reply; `None` when the message is malformed.
fn handle_ipc(msg: &str) -> Option<(u32, String)> {
    let mut parts = msg.split('|');
    let id: u32 = parts.next()?.trim().parse().ok()?;
    let verb = parts.next()?.trim();

    let reply = match verb {
        "load" => do_load(parts.next()?.trim(), parts.next()?.trim()),
        "read" => do_read(parts.next()?.trim()),
        "still" => do_still(&mut parts)?,
        "export-png" => do_export_png(&mut parts)?,
        "export-gif" => do_export_gif(&mut parts)?,
        "cloud-frame" => do_cloud_render(&mut parts, false)?,
        "cloud-gif" => do_cloud_render(&mut parts, true)?,
        other => json!({ "ok": false, "error": format!("unknown verb {other}") }).to_string(),
    };
    Some((id, reply))
}

/// `read|<path>` — raw file bytes as base64, so the page can feed the
/// in-browser spine runtime (atlas text, skeleton binary, texture) without
/// a file:// fetch, which WebView2 would refuse.
fn do_read(path: &str) -> String {
    match std::fs::read(path) {
        Ok(bytes) => json!({ "ok": true, "bytes": base64_encode(&bytes) }).to_string(),
        Err(err) => json!({ "ok": false, "error": format!("{err:#}") }).to_string(),
    }
}

/// `load|<atlas>|<skel>` — what the skeleton declares itself to be.
fn do_load(atlas: &str, skel: &str) -> String {
    let src = SpineSource::new(atlas, skel);
    let out = match describe(&src).and_then(|d| rig(&src).map(|r| (d, r))) {
        Ok((d, r)) => json!({
            "ok": true,
            "version": d.version,
            "hash": d.hash,
            "meshes": d.meshes,
            "vertices": d.vertices,
            "triangles": d.triangles,
            "bones": r.bones.len(),
            "slots": r.slots.len(),
            "animations": r.animations.iter().map(|(n, s)| json!({ "name": n, "seconds": s })).collect::<Vec<_>>(),
        })
        .to_string(),
        Err(err) => json!({ "ok": false, "error": format!("{err:#}") }).to_string(),
    };
    out
}

/// `still|<atlas>|<skel>|<anim>|<time>|<w>|<h>|<r,g,b,a>` — one pose as a PNG
/// data URL. `<anim>` is `-` for the setup pose.
fn do_still(args: &mut std::str::Split<'_, char>) -> Option<String> {
    let atlas = args.next()?.trim();
    let skel = args.next()?.trim();
    let anim = args.next()?.trim();
    let time: f32 = args.next()?.trim().parse().ok()?;
    let width: u32 = args.next()?.trim().parse().ok()?;
    let height: u32 = args.next()?.trim().parse().ok()?;
    let bg = parse_bg(args.next()?.trim())?;

    let src = SpineSource::new(atlas, skel);
    let req = RenderRequest {
        width,
        height,
        background: bg,
        animation: if anim == "-" { None } else { Some(anim.to_string()) },
        time,
        anchor: Anchor::Centre,
    };
    Some(match render_frame_data_url(&src, &req) {
        Ok((url, bounds)) => json!({ "ok": true, "dataUrl": url, "bounds": bounds }).to_string(),
        Err(err) => json!({ "ok": false, "error": format!("{err:#}") }).to_string(),
    })
}

/// `export-png|<atlas>|<skel>|<anim>|<time>|<w>|<h>|<r,g,b,a>|<out>` — save one
/// pose to a file, and name the output the page can show.
fn do_export_png(args: &mut std::str::Split<'_, char>) -> Option<String> {
    let atlas = args.next()?.trim();
    let skel = args.next()?.trim();
    let anim = args.next()?.trim();
    let time: f32 = args.next()?.trim().parse().ok()?;
    let width: u32 = args.next()?.trim().parse().ok()?;
    let height: u32 = args.next()?.trim().parse().ok()?;
    let bg = parse_bg(args.next()?.trim())?;
    let out = args.next()?.trim();

    let src = SpineSource::new(atlas, skel);
    let req = RenderRequest {
        width,
        height,
        background: bg,
        animation: if anim == "-" { None } else { Some(anim.to_string()) },
        time,
        anchor: Anchor::Centre,
    };
    Some(match render_to_file(&src, &req, &PathBuf::from(out)) {
        Ok(frame) => json!({
            "ok": true,
            "path": out,
            "bounds": frame.opaque_bounds(),
        })
        .to_string(),
        Err(err) => json!({ "ok": false, "error": format!("{err:#}") }).to_string(),
    })
}

/// `export-gif|<atlas>|<skel>|<anim>|<frames>|<fps>|<w>|<h>|<r,g,b,a>|<out>` —
/// save a clip as an animated GIF and name the file.
fn do_export_gif(args: &mut std::str::Split<'_, char>) -> Option<String> {
    let atlas = args.next()?.trim();
    let skel = args.next()?.trim();
    let anim = args.next()?.trim();
    let frames: u32 = args.next()?.trim().parse().ok()?;
    let fps: u32 = args.next()?.trim().parse().ok()?;
    let width: u32 = args.next()?.trim().parse().ok()?;
    let height: u32 = args.next()?.trim().parse().ok()?;
    let bg = parse_bg(args.next()?.trim())?;
    let out = args.next()?.trim();

    let src = SpineSource::new(atlas, skel);
    let req = RenderRequest {
        width,
        height,
        background: bg,
        animation: if anim == "-" { None } else { Some(anim.to_string()) },
        time: 0.0,
        anchor: Anchor::Centre,
    };
    Some(match render_animation(&src, &req, &PathBuf::from(out), frames, fps) {
        Ok(written) => json!({ "ok": true, "path": out, "frames": written }).to_string(),
        Err(err) => json!({ "ok": false, "error": format!("{err:#}") }).to_string(),
    })
}

/// `cloud-frame|atlas|skel|anim|time|w|h|bg|token|out` /
/// `cloud-gif|atlas|skel|anim|frames|fps|w|h|bg|token|out`
/// — render on lain42.top (billed per call) and write the result to `out`.
fn do_cloud_render(args: &mut std::str::Split<'_, char>, animate: bool) -> Option<String> {
    let atlas = args.next()?.trim();
    let skel = args.next()?.trim();
    let anim = args.next()?.trim();
    // Animate names a clip length (frames/fps) instead of a single pose time.
    let (time, frames, fps): (f32, u32, u32) = if animate {
        let frames: u32 = args.next()?.trim().parse().ok()?;
        let fps: u32 = args.next()?.trim().parse().ok()?;
        (0.0, frames, fps)
    } else {
        (args.next()?.trim().parse().ok()?, 0, 0)
    };
    let width: u32 = args.next()?.trim().parse().ok()?;
    let height: u32 = args.next()?.trim().parse().ok()?;
    let bg = parse_bg(args.next()?.trim())?;
    let token = args.next()?.trim();
    let out = args.next()?.trim();

    let pages = match pack_pages(atlas) {
        Ok(pages) => pages,
        Err(err) => return Some(json!({ "ok": false, "error": err }).to_string()),
    };
    let atlas_bytes = match std::fs::read(atlas) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Some(
                json!({ "ok": false, "error": format!("cannot read {atlas}: {err}") }).to_string(),
            )
        }
    };
    let skel_bytes = match std::fs::read(skel) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Some(
                json!({ "ok": false, "error": format!("cannot read {skel}: {err}") }).to_string(),
            )
        }
    };

    // Unique per call so the server can dedupe retries; no extra crate needed.
    let request_id = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("viewer-{secs}-{}", std::process::id())
    };

    let mut body = json!({
        "atlas": base64_encode(&atlas_bytes),
        "skel": base64_encode(&skel_bytes),
        "pages": pages,
        "width": width,
        "height": height,
        "background": bg,
        "request_id": request_id,
    });
    // "-" is the setup pose: the API infers it, but animate still needs a name.
    if anim != "-" {
        body["animation"] = json!(anim);
    }
    if animate {
        body["frames"] = json!(frames);
        body["fps"] = json!(fps);
    } else {
        body["time"] = json!(time);
    }

    let kind = if animate { "animate" } else { "frame" };
    let url = format!("https://lain42.top/studio/render/{kind}");
    let response = ureq::post(&url)
        .timeout(Duration::from_secs(180))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_json(body);

    let payload = match response {
        Ok(resp) => match resp.into_json::<serde_json::Value>() {
            Ok(value) => value,
            Err(err) => {
                return Some(
                    json!({ "ok": false, "error": format!("{url} answered with something other than JSON: {err}") })
                        .to_string(),
                )
            }
        },
        Err(ureq::Error::Status(code, resp)) => {
            let raw = resp.into_string().unwrap_or_default();
            let value: serde_json::Value =
                serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw));
            if code == 402 {
                // 402 is the top-up case: `detail` is an object carrying the
                // recharge packs; anything else is a plain message.
                let detail = value
                    .get("detail")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let (message, packs) = match &detail {
                    serde_json::Value::Object(_) => (
                        detail
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| detail.to_string()),
                        detail
                            .pointer("/recharge/packs_cny")
                            .cloned()
                            .unwrap_or_else(|| json!([])),
                    ),
                    serde_json::Value::String(text) => (text.clone(), json!([])),
                    _ => (value.to_string(), json!([])),
                };
                return Some(
                    json!({ "ok": false, "needTopup": true, "error": message, "packs": packs })
                        .to_string(),
                );
            }
            let detail = match value.get("detail") {
                Some(serde_json::Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => value.to_string(),
            };
            return Some(json!({ "ok": false, "error": detail }).to_string());
        }
        Err(other) => {
            return Some(
                json!({ "ok": false, "error": format!("request to {url} failed: {other}") })
                    .to_string(),
            )
        }
    };

    let Some(encoded) = payload.get("image_base64").and_then(|v| v.as_str()) else {
        return Some(json!({ "ok": false, "error": "the API answered without an image" }).to_string());
    };
    let Some(bytes) = base64_decode(encoded) else {
        return Some(json!({ "ok": false, "error": "the API answered with a broken image" }).to_string());
    };
    if let Err(err) = std::fs::write(out, &bytes) {
        return Some(json!({ "ok": false, "error": format!("cannot write {out}: {err}") }).to_string());
    }
    Some(
        json!({
            "ok": true,
            "charged": payload.get("charged").cloned().unwrap_or_else(|| json!(false)),
            "balance": payload.get("balance").cloned().unwrap_or(serde_json::Value::Null),
            "path": out,
            "format": payload.get("format").cloned().unwrap_or_else(|| json!(if animate { "gif" } else { "png" })),
        })
        .to_string(),
    )
}

/// Base64-decode (standard alphabet, `=` padding). Returns None on malformed input.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some((byte - b'A') as u32),
            b'a'..=b'z' => Some((byte - b'a' + 26) as u32),
            b'0'..=b'9' => Some((byte - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let cleaned: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if cleaned.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        // Padding may only sit in the last two slots of a quartet.
        let pad = chunk.iter().filter(|&&b| b == b'=').count();
        if pad > 2 || (pad > 0 && chunk[..2].iter().any(|&b| b == b'=')) {
            return None;
        }
        let mut n = 0u32;
        for (i, &byte) in chunk.iter().enumerate() {
            n |= value(byte).unwrap_or(0) << (18 - 6 * i as u32);
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Base64-pack the atlas's page images, mirroring how local rendering finds them.
fn pack_pages(atlas_path: &str) -> Result<serde_json::Value, String> {
    let atlas_text = std::fs::read_to_string(atlas_path)
        .map_err(|err| format!("cannot read {atlas_path}: {err}"))?;
    let path = Path::new(atlas_path);
    let beside = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let parsed = AtlasText::parse(&atlas_text);
    if parsed.pages().is_empty() {
        return Err(format!("{atlas_path} has no pages"));
    }
    let mut pages = Vec::new();
    for page in parsed.pages() {
        let png = std::fs::read(beside.join(&page.name))
            .map_err(|err| format!("cannot read the page image {}: {err}", page.name))?;
        pages.push(json!({ "name": page.name, "png": base64_encode(&png) }));
    }
    Ok(serde_json::Value::Array(pages))
}

/// Render one pose to a PNG data URL, without touching the disk.
fn render_frame_data_url(
    src: &SpineSource,
    req: &RenderRequest,
) -> anyhow::Result<(String, Option<(u32, u32, u32, u32)>)> {
    let frame = render(src, req)?;
    let bounds = frame.opaque_bounds();
    let mut png: Vec<u8> = Vec::new();
    {
        use image::ImageEncoder;
        let encoder = image::codecs::png::PngEncoder::new(&mut png);
        encoder
            .write_image(
                &frame.pixels,
                frame.width,
                frame.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| anyhow::anyhow!("PNG encode failed: {e}"))?;
    }
    let encoded = base64_encode(&png);
    Ok((format!("data:image/png;base64,{encoded}"), bounds))
}

/// Parse `r,g,b,a` into the four-byte background the renderer expects.
fn parse_bg(text: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0u8; 4];
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = part.parse().ok()?;
    }
    Some(out)
}

/// Smallest possible base64 for a PNG buffer; avoids a dependency for one use.
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn webview_data_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push("lyco-spine-viewer");
    dir
}

fn trace(text: &str) {
    println!("{text}");
}

fn report(text: &str) {
    eprintln!("{text}");
}