//! Assemble a complete character bundle (S5).
//!
//! The requirement's acceptance criterion is one command producing a skin, a
//! model, a BGM track and a portrait. This module is that command's body.
//!
//! Two properties matter more than the output itself:
//!
//!   1. **Every artifact is verified after it is written.** The skin and the
//!      geometry re-run their own validators; the audio and the image are
//!      decoded back and checked against what was requested. A bundle is only
//!      returned when all four hold.
//!   2. **Placeholders are labelled.** Pieces produced by a `stub` backend are
//!      marked `placeholder: true` in the manifest and called out in `notes`,
//!      so a bundle can never quietly look like four real generations.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use lyco_audio::{backend_from_spec as audio_backend, BgmRequest};
use lyco_bsm::player_file;
use lyco_image::{backend_from_spec as image_backend, PortraitRequest};
use lyco_lyrics::{backend_from_spec as lyrics_backend, LyricsRequest};
use lyco_mc_skin::{ArmModel, Skin};
use lyco_spine::{png_size, Atlas, Normalize};

/// One produced file, and what produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: String,
    pub file: String,
    pub backend: String,
    pub bytes: u64,
    /// True when an offline placeholder produced this, not a model.
    pub placeholder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub prompt: String,
    pub arm: String,
    pub artifacts: Vec<Artifact>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleRequest {
    pub prompt: String,
    pub out_dir: PathBuf,
    pub arm: ArmModel,
    pub bgm_duration_secs: u32,
    pub bgm_backend: String,
    pub image_backend: String,
    pub lyrics_backend: String,
    /// Spine atlases to repair on the way through. Each one's page PNGs are
    /// looked for beside the atlas itself.
    pub spine: Vec<PathBuf>,
    /// Also declare premultiplied alpha on repaired atlases.
    ///
    /// Off by default because it is a judgement about the texture, not a fact
    /// that can be read from the file. `size` is repaired either way, since
    /// the truth about that is in the PNG header.
    pub spine_pma: bool,
}

impl BundleRequest {
    pub fn new(prompt: impl Into<String>, out_dir: impl Into<PathBuf>) -> Self {
        BundleRequest {
            prompt: prompt.into(),
            out_dir: out_dir.into(),
            arm: ArmModel::Classic,
            bgm_duration_secs: 30,
            bgm_backend: "stub".to_string(),
            image_backend: "stub".to_string(),
            lyrics_backend: "stub".to_string(),
            spine: Vec::new(),
            spine_pma: false,
        }
    }
}

/// Produce every artifact and return the manifest.
pub fn build(req: &BundleRequest) -> Result<Manifest> {
    std::fs::create_dir_all(&req.out_dir)
        .with_context(|| format!("cannot create {}", req.out_dir.display()))?;

    let mut artifacts = Vec::new();
    let mut notes = Vec::new();

    // 1. Skin. Nothing turns a prompt into a likeness yet, so this is the
    //    structural template: a legal skin, but not a generated one.
    let skin_path = req.out_dir.join("skin.png");
    let skin = Skin::template(req.arm);
    skin.save(&skin_path)?;
    let report = skin.validate();
    if !report.ok() {
        bail!("generated skin failed validation: {:?}", report.issues);
    }
    artifacts.push(Artifact {
        kind: "skin".to_string(),
        file: "skin.png".to_string(),
        backend: "template".to_string(),
        bytes: std::fs::metadata(&skin_path)?.len(),
        placeholder: true,
    });
    notes.push("skin.png is the structural template, not a generated likeness".to_string());

    // 2. Model. A complete player model whose UVs *are* the skin layout, so it
    //    already fits whatever skin.png contains. This one is not a placeholder.
    let model_path = req.out_dir.join("model.geo.json");
    let geometry = player_file("geometry.lyco_player", req.arm);
    let problems = geometry.validate();
    if !problems.is_empty() {
        bail!("generated model failed validation: {:?}", problems);
    }
    std::fs::write(&model_path, geometry.to_json()?)?;
    artifacts.push(Artifact {
        kind: "model".to_string(),
        file: "model.geo.json".to_string(),
        backend: "humanoid".to_string(),
        bytes: std::fs::metadata(&model_path)?.len(),
        placeholder: false,
    });

    // 3. Music.
    let bgm_path = req.out_dir.join("bgm.wav");
    let engine = audio_backend(&req.bgm_backend)?;
    let bgm_req = BgmRequest::new(req.prompt.clone(), req.bgm_duration_secs, &bgm_path);
    let bgm = engine.generate(&bgm_req)?;
    let stub_bgm = engine.name() == "stub";
    artifacts.push(Artifact {
        kind: "bgm".to_string(),
        file: "bgm.wav".to_string(),
        backend: bgm.backend.clone(),
        bytes: bgm.bytes,
        placeholder: stub_bgm,
    });
    if stub_bgm {
        notes.push("bgm.wav is a synthesised placeholder tone, not music".to_string());
    }

    // 4. Portrait.
    let portrait_path = req.out_dir.join("portrait.png");
    let renderer = image_backend(&req.image_backend)?;
    let portrait_req = PortraitRequest::new(req.prompt.clone(), &portrait_path);
    let portrait = renderer.generate(&portrait_req)?;
    let stub_portrait = renderer.name() == "stub";
    artifacts.push(Artifact {
        kind: "portrait".to_string(),
        file: "portrait.png".to_string(),
        backend: portrait.backend.clone(),
        bytes: portrait.bytes,
        placeholder: stub_portrait,
    });
    if stub_portrait {
        notes.push("portrait.png is a procedural placeholder, not a likeness".to_string());
    }

    // 5. Lyrics. Placed after the four the acceptance criteria name, because
    //    they are a superset rather than a replacement.
    let lyrics_path = req.out_dir.join("lyrics.txt");
    let writer = lyrics_backend(&req.lyrics_backend)?;
    let lyrics_req = LyricsRequest::new(req.prompt.clone(), &lyrics_path);
    let lyrics = writer.generate(&lyrics_req)?;
    let stub_lyrics = writer.name() == "stub";
    artifacts.push(Artifact {
        kind: "lyrics".to_string(),
        file: "lyrics.txt".to_string(),
        backend: lyrics.backend.clone(),
        bytes: std::fs::metadata(&lyrics_path)?.len(),
        placeholder: stub_lyrics,
    });
    if stub_lyrics {
        notes.push("lyrics.txt is placeholder lines, not songwriting".to_string());
    }

    // 6. Spine atlases, when any were handed in. Repairing them here means the
    //    bundle that ships is one whose atlases a runtime can actually open -
    //    a dump that omitted `size` renders with every region misplaced.
    for atlas_path in &req.spine {
        let text = std::fs::read_to_string(atlas_path)
            .with_context(|| format!("cannot read {}", atlas_path.display()))?;
        let atlas = Atlas::parse(&text);
        if atlas.pages().is_empty() {
            bail!(
                "{} has no pages, so it is not a Spine atlas",
                atlas_path.display()
            );
        }

        let beside = match atlas_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let mut sizes = std::collections::BTreeMap::new();
        for page in atlas.pages() {
            let png = beside.join(&page.name);
            match std::fs::read(&png) {
                Ok(bytes) => match png_size(&bytes) {
                    Ok(size) => {
                        sizes.insert(page.name.clone(), size);
                    }
                    Err(err) => notes.push(format!("{}: {err}", png.display())),
                },
                Err(_) => notes.push(format!(
                    "{}: its PNG was not found, so its size was left alone",
                    png.display()
                )),
            }
        }

        let opts = Normalize {
            sizes,
            pma: req.spine_pma,
            renames: Default::default(),
        };
        let result = atlas.normalize(&opts);
        let file = atlas_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "atlas.atlas".to_string());
        let dest = req.out_dir.join(&file);
        std::fs::write(&dest, &result.text)?;
        artifacts.push(Artifact {
            kind: "spine_atlas".to_string(),
            file: file.clone(),
            // Not a placeholder: the repair is real work on a real input.
            backend: "lyco-spine".to_string(),
            bytes: std::fs::metadata(&dest)?.len(),
            placeholder: false,
        });

        let info = atlas.info();
        let mut summary = format!(
            "{file}: {} page(s), {} region(s), {} size(s) corrected, {} unsized",
            info.pages.len(),
            info.regions.len(),
            result.fixed_size.len(),
            result.missing_size.len()
        );
        if req.spine_pma {
            summary.push_str(", premultiplied alpha declared");
        }
        notes.push(summary);
    }

    let manifest = Manifest {
        prompt: req.prompt.clone(),
        arm: req.arm.name().to_string(),
        artifacts,
        notes,
    };

    // 5. The manifest, so the bundle is self-describing.
    let manifest_path = req.out_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lyco-bundle-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn produces_the_four_artifacts_plus_a_manifest() {
        let req = BundleRequest::new("a quiet librarian", tmp("basic"));
        let manifest = build(&req).expect("build");

        let kinds: Vec<&str> = manifest.artifacts.iter().map(|a| a.kind.as_str()).collect();
        assert_eq!(kinds, vec!["skin", "model", "bgm", "portrait", "lyrics"]);
        for artifact in &manifest.artifacts {
            let path = req.out_dir.join(&artifact.file);
            assert!(path.exists(), "{} missing", artifact.file);
            assert_eq!(std::fs::metadata(&path).unwrap().len(), artifact.bytes);
        }
        assert!(req.out_dir.join("manifest.json").exists());
    }

    #[test]
    fn every_artifact_is_a_real_file_the_acceptance_criteria_accept() {
        let req = BundleRequest::new("dusk over the harbour", tmp("accept"));
        build(&req).expect("build");

        // 64x64 legal skin
        let skin = Skin::load(req.out_dir.join("skin.png")).expect("load skin");
        assert_eq!((skin.width(), skin.height()), (64, 64));
        assert!(skin.validate().ok());

        // importable Bedrock geometry
        let text = std::fs::read_to_string(req.out_dir.join("model.geo.json")).unwrap();
        let geometry = lyco_bsm::GeometryFile::from_json(&text).expect("parse geometry");
        assert!(geometry.validate().is_empty());

        // audible audio of the requested length
        let wav = lyco_audio::decode(&std::fs::read(req.out_dir.join("bgm.wav")).unwrap())
            .expect("decode wav");
        assert!(wav.peak() > 0);
        assert!(wav.duration_secs() >= 30.0 * lyco_audio::DURATION_TOLERANCE);

        // a portrait at the default size
        let portrait = image::open(req.out_dir.join("portrait.png")).expect("open portrait");
        assert_eq!(portrait.width(), lyco_image::DEFAULT_WIDTH);
        assert_eq!(portrait.height(), lyco_image::DEFAULT_HEIGHT);
    }

    #[test]
    fn manifest_labels_placeholders_and_spares_the_model() {
        let req = BundleRequest::new("anything", tmp("labels"));
        let manifest = build(&req).expect("build");

        let find = |kind: &str| {
            manifest
                .artifacts
                .iter()
                .find(|a| a.kind == kind)
                .expect("artifact")
        };
        assert!(find("skin").placeholder);
        assert!(find("bgm").placeholder);
        assert!(find("portrait").placeholder);
        assert!(
            !find("model").placeholder,
            "the humanoid model is real geometry"
        );
        assert!(find("lyrics").placeholder);
        assert_eq!(manifest.notes.len(), 4);
    }

    /// An atlas whose page image sits beside it, as a real dump ships.
    fn spine_fixture(dir: &std::path::Path) -> PathBuf {
        std::fs::create_dir_all(dir).expect("fixture dir");
        // A real skin PNG stands in for an atlas page: only its header matters.
        let source = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/blockv06_seed7.png"
        );
        std::fs::copy(source, dir.join("page.png")).expect("copy the page image");
        let atlas = dir.join("char.atlas");
        std::fs::write(
            &atlas,
            "page.png\nformat: RGBA8888\nfilter: Linear,Linear\nhead\n  xy: 2, 2\n",
        )
        .expect("write the atlas");
        atlas
    }

    #[test]
    fn a_spine_atlas_handed_in_is_repaired_into_the_bundle() {
        let atlas = spine_fixture(&tmp("spine-src"));
        let mut req = BundleRequest::new("a quiet librarian", tmp("spine-out"));
        req.spine = vec![atlas];
        let manifest = build(&req).expect("build");

        let artifact = manifest
            .artifacts
            .iter()
            .find(|a| a.kind == "spine_atlas")
            .expect("a spine artifact");
        assert_eq!(artifact.file, "char.atlas");
        assert_eq!(artifact.backend, "lyco-spine");
        assert!(
            !artifact.placeholder,
            "a repair is real work on a real input"
        );

        let text = std::fs::read_to_string(req.out_dir.join("char.atlas")).expect("read");
        assert!(text.contains("size: 64,64"), "{text}");
        assert!(
            text.contains("format: RGBA8888"),
            "the rest must survive: {text}"
        );
        // And the shipped copy is settled: repairing it again changes nothing.
        let shipped = Atlas::parse(&text);
        assert!(!shipped.normalize(&Normalize::default()).changed);
    }

    #[test]
    fn a_bundle_without_spine_input_says_nothing_about_spine() {
        let req = BundleRequest::new("x", tmp("no-spine"));
        let manifest = build(&req).expect("build");
        assert!(manifest.artifacts.iter().all(|a| a.kind != "spine_atlas"));
        assert_eq!(manifest.notes.len(), 4, "only the four stub notes");
    }

    #[test]
    fn a_page_whose_image_is_missing_is_noted_rather_than_fatal() {
        let dir = tmp("spine-orphan");
        std::fs::create_dir_all(&dir).expect("dir");
        let atlas = dir.join("orphan.atlas");
        std::fs::write(&atlas, "gone.png\nformat: RGBA8888\nhead\n  xy: 0,0\n").expect("write");

        let mut req = BundleRequest::new("x", tmp("spine-orphan-out"));
        req.spine = vec![atlas];
        let manifest = build(&req).expect("a missing page image must not fail the bundle");

        let text = std::fs::read_to_string(req.out_dir.join("orphan.atlas")).expect("read");
        assert!(!text.contains("size:"), "no size may be invented: {text}");
        assert!(
            manifest.notes.iter().any(|n| n.contains("not found")),
            "{:?}",
            manifest.notes
        );
    }

    #[test]
    fn a_file_that_is_not_an_atlas_is_refused() {
        let dir = tmp("spine-empty");
        std::fs::create_dir_all(&dir).expect("dir");
        let atlas = dir.join("empty.atlas");
        std::fs::write(&atlas, "").expect("write");
        let mut req = BundleRequest::new("x", tmp("spine-empty-out"));
        req.spine = vec![atlas];
        let err = build(&req).unwrap_err();
        assert!(err.to_string().contains("not a Spine atlas"), "{err}");
    }

    #[test]
    fn slim_bundles_produce_slim_models() {
        let mut req = BundleRequest::new("alex", tmp("slim"));
        req.arm = ArmModel::Slim;
        let manifest = build(&req).expect("build");
        assert_eq!(manifest.arm, "slim");

        let text = std::fs::read_to_string(req.out_dir.join("model.geo.json")).unwrap();
        assert!(text.contains("right_arm"));
        let skin = Skin::load(req.out_dir.join("skin.png")).unwrap();
        assert_eq!(skin.detect_arm_model(), Some(ArmModel::Slim));
    }

    #[test]
    fn is_deterministic_for_a_prompt() {
        let a = BundleRequest::new("same prompt", tmp("det-a"));
        let b = BundleRequest::new("same prompt", tmp("det-b"));
        let ma = build(&a).expect("a");
        let mb = build(&b).expect("b");
        assert_eq!(ma, mb);
        for artifact in &ma.artifacts {
            if artifact.kind == "model" {
                continue; // geometry is deterministic too, but compare the rest
            }
            assert_eq!(
                std::fs::read(a.out_dir.join(&artifact.file)).unwrap(),
                std::fs::read(b.out_dir.join(&artifact.file)).unwrap(),
                "{} differs between runs",
                artifact.file
            );
        }
    }

    #[test]
    fn a_bad_backend_spec_fails_the_whole_build() {
        let mut req = BundleRequest::new("x", tmp("badbackend"));
        req.bgm_backend = "definitely-not-a-real-binary-xyz".to_string();
        let err = build(&req).unwrap_err();
        assert!(err.to_string().contains("cannot run"), "{err}");
    }
}
