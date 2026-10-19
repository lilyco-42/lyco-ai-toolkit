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

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    let html = include_str!("../assets/ui.html");
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
/// (the most visible of the triplet) or the whole folder as often as for
/// the atlas/skel themselves, so those are resolved to a sibling pair here.
fn on_dropped(webview: &wry::WebView, path: &std::path::Path) {
    // A folder: take the first atlas inside (or a skeleton, and find its atlas).
    if path.is_dir() {
        let mut atlas: Option<std::path::PathBuf> = None;
        let mut skel: Option<std::path::PathBuf> = None;
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for entry in &entries {
            match ext_of(entry).as_str() {
                "atlas" if atlas.is_none() => atlas = Some(entry.clone()),
                "skel" | "json" if skel.is_none() => skel = Some(entry.clone()),
                _ => {}
            }
        }
        if let Some(atlas) = atlas.or_else(|| skel.as_deref().and_then(|s| sibling(s, "atlas"))) {
            forward_drop(webview, "atlas", &atlas);
        } else {
            forward_unknown(webview, path);
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
        "still" => do_still(&mut parts)?,
        "export-png" => do_export_png(&mut parts)?,
        "export-gif" => do_export_gif(&mut parts)?,
        other => json!({ "ok": false, "error": format!("unknown verb {other}") }).to_string(),
    };
    Some((id, reply))
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