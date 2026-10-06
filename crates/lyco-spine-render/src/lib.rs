//! Render Spine skeletons headlessly, on the CPU (F11).
//!
//! ## Licence, up front
//!
//! This crate reaches the official Spine runtime, which is under the **Spine
//! Runtimes License Agreement** - a commercial licence. The source here is MIT
//! like the rest of the workspace, but a binary built from it is not MIT, and
//! shipping one needs a Spine licence. It is a separate crate for exactly that
//! reason, and `lyco-cli` deliberately does not depend on it.
//!
//! ## Why a software rasteriser
//!
//! The Spine runtime hands back posed triangles; something still has to draw
//! them. The obvious choice is wgpu, and it is the wrong one here: the
//! operator's machine has no real GPU and CI has none at all. [`raster`] draws
//! on the CPU, which is slower and completely portable, and produces identical
//! bytes everywhere, which is what a test needs.
//!
//! The split also keeps the interesting part testable: [`raster`] knows nothing
//! about Spine, so its geometry and blending are unit tested without a skeleton
//! or a licence file in sight.

pub mod raster;

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lyco_spine::Atlas as AtlasText;
use rusty_spine::{
    AnimationStateData, Atlas as SpineAtlas, BlendMode, SkeletonBinary, SkeletonData, SkeletonJson,
};
// Not re-exported at the crate root, so it is named through its module.
use rusty_spine::controller::SkeletonController;

/// Where a skeleton's two halves live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineSource {
    /// The `.atlas` text file. Its page images are expected beside it.
    pub atlas: PathBuf,
    /// The `.skel` (binary) or `.json` skeleton.
    pub skeleton: PathBuf,
    /// Read the skeleton as binary rather than JSON.
    pub binary: bool,
}

impl SpineSource {
    pub fn new(atlas: impl Into<PathBuf>, skeleton: impl Into<PathBuf>) -> Self {
        let skeleton = skeleton.into();
        // Arknights exports binary skeletons; anything else is taken as JSON.
        let binary = skeleton
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("skel"));
        SpineSource {
            atlas: atlas.into(),
            skeleton,
            binary,
        }
    }
}

/// What a skeleton says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub version: String,
    pub hash: String,
    /// How many visible attachments the setup pose has, i.e. what there is to draw.
    pub meshes: usize,
    pub vertices: usize,
    pub triangles: usize,
    /// Animation names, in the order the skeleton declares them.
    pub animations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderRequest {
    pub width: u32,
    pub height: u32,
    pub background: [u8; 4],
    /// Animation to play. Without one the setup pose is drawn, which is a
    /// legal pose but rarely the one anybody wants a picture of.
    pub animation: Option<String>,
    /// Seconds to advance the pose by.
    pub time: f32,
}

impl Default for RenderRequest {
    fn default() -> Self {
        RenderRequest {
            width: 512,
            height: 512,
            background: [0, 0, 0, 0],
            animation: None,
            time: 0.0,
        }
    }
}

/// Advance a controller to `time` in fixed steps.
///
/// Stepping rather than one jump, so the picture does not depend on how the
/// caller chose to slice the interval: an animation samples differently across
/// a single large delta than across the sixty small ones a second of playback
/// really is.
fn advance(controller: &mut SkeletonController, time: f32) {
    const STEP: f32 = 1.0 / 60.0;
    let mut elapsed = 0.0f32;
    while elapsed < time {
        let dt = STEP.min(time - elapsed);
        controller.update(dt);
        elapsed += dt;
    }
}

/// Start an animation, naming what exists when the name is wrong.
fn start_animation(
    controller: &mut SkeletonController,
    data: &SkeletonData,
    name: &str,
) -> Result<()> {
    controller
        .animation_state
        .set_animation_by_name(0, name, true)
        .map_err(|_| {
            // Owned, because `name()` borrows the animation the iterator just
            // handed us, which does not outlive this closure.
            let available: Vec<String> = data.animations().map(|a| a.name().to_string()).collect();
            anyhow::anyhow!("no animation called '{name}'; this skeleton has {available:?}")
        })?;
    Ok(())
}

/// The Spine version this build can actually read.
///
/// One version, not a range, because the binary layout changed between every
/// minor release and each change is incompatible with the last. A single
/// runtime cannot span them, and no Rust runtime exists for 2.x, 3.5-3.7 or
/// 4.0 at all - only 3.8, 4.1, 4.2 and 4.3 have one.
const SUPPORTED: (u32, u32) = (3, 8);

/// Refuse a skeleton this build cannot read, and say why.
///
/// Without this the 3.8 runtime is simply handed a 4.2 file and fails with
/// whatever the C layer says about a truncated read, which tells the caller
/// nothing. The version is in the file's first bytes, so it costs nothing to
/// read it first and name the real problem.
fn check_version(src: &SpineSource) -> Result<()> {
    let found = if src.binary {
        let bytes = std::fs::read(&src.skeleton)
            .with_context(|| format!("cannot read {}", src.skeleton.display()))?;
        // A header that cannot be read is not this guard's business; the
        // runtime will produce the real error a moment later.
        lyco_spine::read_header(&bytes)
            .ok()
            .and_then(|header| header.major_minor())
    } else {
        None
    };

    match found {
        Some(version) if version == SUPPORTED => Ok(()),
        Some((major, minor)) => bail!(
            "{} is Spine {major}.{minor}, and this build reads Spine {}.{} only.\n\
             Spine changes its binary layout between releases, so one runtime cannot \
             read them all; a build per version is the only way.",
            src.skeleton.display(),
            SUPPORTED.0,
            SUPPORTED.1
        ),
        None => Ok(()),
    }
}

/// Load the skeleton data, binary or JSON.
fn load(src: &SpineSource) -> Result<Arc<SkeletonData>> {
    check_version(src)?;
    let atlas = Arc::new(
        SpineAtlas::new_from_file(&src.atlas)
            .with_context(|| format!("cannot read {}", src.atlas.display()))?,
    );
    let data = if src.binary {
        SkeletonBinary::new(atlas).read_skeleton_data_file(&src.skeleton)
    } else {
        SkeletonJson::new(atlas).read_skeleton_data_file(&src.skeleton)
    }
    .with_context(|| format!("cannot read the skeleton {}", src.skeleton.display()))?;
    Ok(Arc::new(data))
}

/// Report what the skeleton declares and how much it poses to.
pub fn describe(src: &SpineSource) -> Result<Description> {
    let data = load(src)?;
    let state = Arc::new(AnimationStateData::new(data.clone()));
    let mut controller = SkeletonController::new(data.clone(), state);
    controller.update(0.0);
    let renderables = controller.renderables();

    let mut vertices = 0usize;
    let mut triangles = 0usize;
    for renderable in &renderables {
        vertices += renderable.vertices.len();
        triangles += renderable.indices.len() / 3;
    }

    Ok(Description {
        // The two accessors differ, and the compiler is the only reliable
        // source: `version()` is optional and `hash()` is a plain &str.
        version: data.version().unwrap_or("(none)").to_string(),
        hash: data.hash().to_string(),
        meshes: renderables.len(),
        vertices,
        triangles,
        animations: data.animations().map(|a| a.name().to_string()).collect(),
    })
}

/// Load the page images an atlas refers to.
///
/// The atlas is read with `lyco-spine`'s own parser rather than the runtime's,
/// because all that is wanted here is the list of page file names, and the
/// runtime expects textures to be handed to it through a C callback.
fn load_textures(atlas_path: &Path) -> Result<Vec<raster::Texture>> {
    let text = std::fs::read_to_string(atlas_path)
        .with_context(|| format!("cannot read {}", atlas_path.display()))?;
    let parsed = AtlasText::parse(&text);
    if parsed.pages().is_empty() {
        bail!("{} has no pages", atlas_path.display());
    }
    let beside = match atlas_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };

    let mut textures = Vec::new();
    for page in parsed.pages() {
        let file = beside.join(&page.name);
        let image = image::open(&file)
            .with_context(|| format!("cannot open the page image {}", file.display()))?;
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        textures.push(raster::Texture::new(width, height, rgba.into_raw())?);
    }
    Ok(textures)
}

/// Pose the skeleton and draw it.
pub fn render(src: &SpineSource, req: &RenderRequest) -> Result<raster::Frame> {
    let data = load(src)?;
    let state = Arc::new(AnimationStateData::new(data.clone()));
    let mut controller = SkeletonController::new(data.clone(), state);
    if let Some(name) = &req.animation {
        start_animation(&mut controller, &data, name)?;
    }
    advance(&mut controller, req.time.max(0.0));
    let renderables = controller.renderables();
    if renderables.is_empty() {
        bail!(
            "the skeleton posed to nothing; does {} match {}?",
            src.atlas.display(),
            src.skeleton.display()
        );
    }

    let textures = load_textures(&src.atlas)?;
    if textures.len() != 1 {
        // Multi-page would need the runtime's texture callback to say which page
        // each attachment landed on. Arknights atlases are single-page.
        bail!(
            "this build draws single-page atlases, and {} has {} pages",
            src.atlas.display(),
            textures.len()
        );
    }

    // The runtime reports this per attachment, so it is read rather than assumed.
    let premultiplied = renderables.iter().any(|r| r.premultiplied_alpha);
    let meshes: Vec<raster::Mesh> = renderables
        .iter()
        .map(|r| raster::Mesh {
            vertices: r.vertices.clone(),
            uvs: r.uvs.clone(),
            indices: r.indices.clone(),
            color: [r.color.r, r.color.g, r.color.b, r.color.a],
            blend: match r.blend_mode {
                BlendMode::Normal => raster::Blend::Normal,
                BlendMode::Additive => raster::Blend::Additive,
                BlendMode::Multiply => raster::Blend::Multiply,
                BlendMode::Screen => raster::Blend::Screen,
            },
            texture: 0,
        })
        .collect();

    let fit = raster::Fit {
        margin: 0.06,
        premultiplied_textures: premultiplied,
    };
    raster::render(
        &meshes,
        &textures,
        req.width,
        req.height,
        req.background,
        &fit,
    )
}

/// Pose, draw, and write the result out.
pub fn render_to_file(src: &SpineSource, req: &RenderRequest, out: &Path) -> Result<raster::Frame> {
    let frame = render(src, req)?;
    frame.save(out)?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_string(out: &mut Vec<u8>, text: &str) {
        // The length byte counts a terminator that is not written.
        out.push((text.len() + 1) as u8);
        out.extend_from_slice(text.as_bytes());
    }

    fn source(name: &str, bytes: &[u8]) -> SpineSource {
        let dir = std::env::temp_dir().join("lyco-spine-render-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let skeleton = dir.join(name);
        std::fs::write(&skeleton, bytes).expect("write");
        SpineSource {
            atlas: dir.join("unused.atlas"),
            skeleton,
            binary: true,
        }
    }

    #[test]
    fn a_3_8_skeleton_passes_the_guard() {
        let mut bytes = Vec::new();
        push_string(&mut bytes, "somehashvalue");
        push_string(&mut bytes, "3.8.99");
        assert!(check_version(&source("v38.skel", &bytes)).is_ok());
    }

    /// The failure this guard exists for: without it, a 4.2 file reaches a 3.8
    /// runtime and comes back as a truncated read that names nothing.
    #[test]
    fn a_4_2_skeleton_is_refused_by_name() {
        let mut bytes = Vec::new();
        push_string(&mut bytes, "4.2.00");
        let err = check_version(&source("v42.skel", &bytes))
            .unwrap_err()
            .to_string();
        assert!(err.contains("Spine 4.2"), "{err}");
        assert!(err.contains("3.8"), "{err}");
    }

    #[test]
    fn a_3_7_skeleton_is_refused_even_though_it_is_the_same_major() {
        // 3.7 and 3.8 are not binary compatible, so "3.x" is too coarse to
        // decide with; the minor version is what matters.
        let mut bytes = Vec::new();
        push_string(&mut bytes, "3.7.00");
        assert!(check_version(&source("v37.skel", &bytes)).is_err());
    }

    #[test]
    fn a_header_that_cannot_be_read_is_left_to_the_runtime() {
        assert!(check_version(&source("empty.skel", &[])).is_ok());
    }
}
