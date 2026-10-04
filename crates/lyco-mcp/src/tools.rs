//! The tools this server exposes to an agent.
//!
//! Every tool is offline: none of them needs a GPU or a network. The ones that
//! would (music, images, lyrics) take an optional backend argument, so an agent
//! gets a working placeholder by default and real output once an operator
//! points the backend at a generator.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use lyco_audio::{backend_from_spec as audio_backend, BgmRequest};
use lyco_bsm::{convert_glb, player_file, ConvertConfig, GeometryFile, Glb};
use lyco_bundle::BundleRequest;
use lyco_image::{backend_from_spec as image_backend, PortraitRequest};
use lyco_lyrics::{backend_from_spec as lyrics_backend, Lyrics, LyricsRequest};
use lyco_mc_skin::{apply_palette, distinct_opaque_colors, median_cut, ArmModel, Skin};

/// A single callable tool.
///
/// Function pointers implement `Debug`, so the registry can derive it too.
#[derive(Debug, Clone, Copy)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: fn() -> Value,
    pub handler: fn(&Value) -> Result<String>,
}

#[derive(Debug, Default)]
pub struct Registry {
    tools: Vec<Tool>,
}

impl Registry {
    /// Every tool the toolkit can offer an agent.
    pub fn standard() -> Self {
        Registry {
            tools: vec![
                Tool {
                    name: "skin_template",
                    description: "Write a structurally valid 64x64 Minecraft skin template.",
                    schema: schema_skin_template,
                    handler: skin_template,
                },
                Tool {
                    name: "skin_validate",
                    description: "Validate a Minecraft skin's structure and report its palette.",
                    schema: schema_path_only,
                    handler: skin_validate,
                },
                Tool {
                    name: "skin_quantize",
                    description: "Snap a skin onto a small, coherent palette.",
                    schema: schema_skin_quantize,
                    handler: skin_quantize,
                },
                Tool {
                    name: "model_humanoid",
                    description: "Write the standard player model as Bedrock geometry.",
                    schema: schema_model_humanoid,
                    handler: model_humanoid,
                },
                Tool {
                    name: "model_validate",
                    description: "Validate a Bedrock geometry (.geo.json) file.",
                    schema: schema_path_only,
                    handler: model_validate,
                },
                Tool {
                    name: "glb_info",
                    description: "Describe a GLB's nodes, meshes and triangle counts.",
                    schema: schema_path_only,
                    handler: glb_info,
                },
                Tool {
                    name: "model_from_glb",
                    description: "Convert a GLB mesh into Bedrock geometry plus a texture.",
                    schema: schema_model_from_glb,
                    handler: model_from_glb,
                },
                Tool {
                    name: "lyrics_generate",
                    description: "Write a sectioned lyric sheet for a prompt.",
                    schema: schema_lyrics_generate,
                    handler: lyrics_generate,
                },
                Tool {
                    name: "lyrics_validate",
                    description: "Validate a lyric sheet's structure.",
                    schema: schema_path_only,
                    handler: lyrics_validate,
                },
                Tool {
                    name: "bgm_generate",
                    description: "Write background music as a WAV file.",
                    schema: schema_bgm_generate,
                    handler: bgm_generate,
                },
                Tool {
                    name: "portrait_generate",
                    description: "Write a character portrait as a PNG.",
                    schema: schema_portrait_generate,
                    handler: portrait_generate,
                },
                Tool {
                    name: "bundle",
                    description:
                        "Produce a whole character bundle: skin, model, BGM, portrait, lyrics.",
                    schema: schema_bundle,
                    handler: bundle,
                },
            ],
        }
    }

    pub fn find(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.tools.iter().map(|t| t.name).collect()
    }

    /// The payload for a tools/list response.
    pub fn list_json(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": (t.schema)(),
                })
            })
            .collect();
        json!({ "tools": tools })
    }
}

fn string_prop(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn integer_prop(description: &str) -> Value {
    json!({ "type": "integer", "description": description })
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn schema_path_only() -> Value {
    schema(json!({ "path": string_prop("File to inspect") }), &["path"])
}

fn schema_skin_template() -> Value {
    schema(
        json!({
            "out": string_prop("Output PNG path"),
            "arm": string_prop("classic (Steve) or slim (Alex)"),
        }),
        &["out"],
    )
}

fn schema_skin_quantize() -> Value {
    schema(
        json!({
            "path": string_prop("Input skin PNG"),
            "out": string_prop("Output skin PNG"),
            "colors": integer_prop("Maximum palette entries"),
        }),
        &["path", "out"],
    )
}

fn schema_model_humanoid() -> Value {
    schema(
        json!({
            "out": string_prop("Output .geo.json path"),
            "arm": string_prop("classic or slim"),
            "identifier": string_prop("Geometry identifier"),
        }),
        &["out"],
    )
}

fn schema_model_from_glb() -> Value {
    schema(
        json!({
            "path": string_prop("Input GLB"),
            "out": string_prop("Output .geo.json path"),
            "texture": string_prop("Output texture PNG path"),
            "resolution": integer_prop("Voxels along the longest axis, 1-64"),
        }),
        &["path", "out"],
    )
}

fn schema_lyrics_generate() -> Value {
    schema(
        json!({
            "prompt": string_prop("Theme or title"),
            "out": string_prop("Output text path"),
            "backend": string_prop("'stub' or a writer command line"),
        }),
        &["prompt", "out"],
    )
}

fn schema_bgm_generate() -> Value {
    schema(
        json!({
            "prompt": string_prop("Mood or style"),
            "out": string_prop("Output WAV path"),
            "duration": integer_prop("Seconds"),
            "backend": string_prop("'stub' or a generator command line"),
        }),
        &["prompt", "out"],
    )
}

fn schema_portrait_generate() -> Value {
    schema(
        json!({
            "prompt": string_prop("Character description"),
            "out": string_prop("Output PNG path"),
            "backend": string_prop("'stub' or a generator command line"),
        }),
        &["prompt", "out"],
    )
}

fn schema_bundle() -> Value {
    schema(
        json!({
            "prompt": string_prop("Character description"),
            "out": string_prop("Output directory"),
            "arm": string_prop("classic or slim"),
            "bgm_duration": integer_prop("BGM seconds"),
            "bgm_backend": string_prop("'stub' or a generator command line"),
            "image_backend": string_prop("'stub' or a generator command line"),
            "lyrics_backend": string_prop("'stub' or a writer command line"),
        }),
        &["prompt", "out"],
    )
}

fn required_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("missing required string argument '{key}'"))
}

fn optional_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn optional_u32(args: &Value, key: &str, default: u32) -> Result<u32> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n <= u32::MAX as u64 => Ok(n as u32),
            _ => bail!("argument '{key}' must be a non-negative integer"),
        },
    }
}

fn parse_arm(args: &Value) -> Result<ArmModel> {
    match optional_str(args, "arm").as_deref() {
        None | Some("classic") | Some("steve") => Ok(ArmModel::Classic),
        Some("slim") | Some("alex") => Ok(ArmModel::Slim),
        Some(other) => bail!("unknown arm '{other}': expected classic or slim"),
    }
}

fn write_text(path: &str, text: &str) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, text).with_context(|| format!("cannot write {path}"))
}

fn skin_template(args: &Value) -> Result<String> {
    let out = required_str(args, "out")?;
    let arm = parse_arm(args)?;
    let skin = Skin::template(arm);
    skin.save(&out)?;
    let rep = skin.validate();
    if !rep.ok() {
        bail!("the template failed its own validation");
    }
    Ok(format!(
        "wrote {out} (64x64, arm={}); opaque pixels {}, {} distinct colours",
        arm.name(),
        rep.opaque_pixels,
        rep.distinct_colors
    ))
}

fn skin_validate(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let skin = Skin::load(&path)?;
    let rep = skin.validate();
    let mut out = format!(
        "{}x{}, opaque {}, {} distinct colours, valid={}",
        rep.width,
        rep.height,
        rep.opaque_pixels,
        rep.distinct_colors,
        rep.ok()
    );
    for issue in &rep.issues {
        out.push_str(&format!(
            "\n[{}] {}: {}",
            issue.severity.name(),
            issue.code,
            issue.message
        ));
    }
    Ok(out)
}

fn skin_quantize(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let out = required_str(args, "out")?;
    let colors = optional_u32(args, "colors", 8)?.max(1) as usize;
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
        bail!("quantization produced an invalid skin");
    }
    rebuilt.save(&out)?;
    let after = distinct_opaque_colors(rebuilt.pixels());
    Ok(format!(
        "wrote {out}; colours {before} -> {after}, palette {}",
        palette.as_hex().join(" ")
    ))
}

fn model_humanoid(args: &Value) -> Result<String> {
    let out = required_str(args, "out")?;
    let arm = parse_arm(args)?;
    let identifier =
        optional_str(args, "identifier").unwrap_or_else(|| "geometry.lyco_player".to_string());
    let file = player_file(&identifier, arm);
    let problems = file.validate();
    if !problems.is_empty() {
        bail!("the generated model failed validation: {problems:?}");
    }
    write_text(&out, &file.to_json()?)?;
    let g = &file.geometry[0];
    Ok(format!(
        "wrote {out}; {identifier}, arm {}, {} bones, {} cubes",
        arm.name(),
        g.bones.len(),
        g.cube_count()
    ))
}

fn model_validate(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let text = std::fs::read_to_string(&path)?;
    let file = GeometryFile::from_json(&text)
        .with_context(|| format!("{path} is not valid Bedrock geometry JSON"))?;
    let problems = file.validate();
    let mut out = format!(
        "{} geometry entries, valid={}",
        file.geometry.len(),
        problems.is_empty()
    );
    for p in &problems {
        out.push_str(&format!("\n[error] {}: {}", p.code, p.message));
    }
    for g in &file.geometry {
        out.push_str(&format!(
            "\n{}: {} bones, {} cubes",
            g.description.identifier,
            g.bones.len(),
            g.cube_count()
        ));
    }
    if !problems.is_empty() {
        bail!(out);
    }
    Ok(out)
}

fn glb_info(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let bytes = std::fs::read(&path)?;
    let glb = Glb::parse(&bytes).with_context(|| format!("cannot read {path}"))?;
    let g = &glb.gltf;
    let mut triangles = 0usize;
    for mesh in &g.meshes {
        for prim in &mesh.primitives {
            if prim.mode.is_some_and(|m| m != 4) {
                continue;
            }
            match prim.indices {
                Some(i) => triangles += glb.read_indices(i).map(|v| v.len() / 3).unwrap_or(0),
                None => {
                    if let Some(pos) = prim.attributes.position {
                        triangles += glb.read_vec3(pos).map(|v| v.len() / 3).unwrap_or(0);
                    }
                }
            }
        }
    }
    Ok(format!(
        "glb v{}, {} nodes, {} meshes, {} materials, {triangles} triangles",
        glb.version,
        g.nodes.len(),
        g.meshes.len(),
        g.materials.len()
    ))
}

fn model_from_glb(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let out = required_str(args, "out")?;
    let resolution = optional_u32(args, "resolution", 24)?.clamp(1, 64);
    let bytes = std::fs::read(&path)?;
    let glb = Glb::parse(&bytes).with_context(|| format!("cannot read {path}"))?;
    let cfg = ConvertConfig {
        resolution,
        ..Default::default()
    };
    let conversion = convert_glb(&glb, &cfg)?;
    write_text(&out, &conversion.geometry.to_json()?)?;
    let mut summary = format!(
        "wrote {out}; {} triangles -> {} voxels -> {} boxes",
        conversion.triangles, conversion.voxels, conversion.boxes
    );
    if let Some(texture) = optional_str(args, "texture") {
        conversion.save_texture(&texture)?;
        summary.push_str(&format!("\nwrote {texture}"));
    }
    Ok(summary)
}

fn lyrics_generate(args: &Value) -> Result<String> {
    let prompt = required_str(args, "prompt")?;
    let out = required_str(args, "out")?;
    let spec = optional_str(args, "backend").unwrap_or_else(|| "stub".to_string());
    let backend = lyrics_backend(&spec)?;
    let result = backend.generate(&LyricsRequest::new(prompt, out))?;
    Ok(format!(
        "wrote {}; backend {}, {} sections, {} lines",
        result.path.display(),
        result.backend,
        result.sections,
        result.lines
    ))
}

fn lyrics_validate(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let text = std::fs::read_to_string(&path)?;
    let lyrics = Lyrics::parse(&text).with_context(|| format!("{path} is not a lyric sheet"))?;
    let issues = lyrics.validate();
    let mut out = format!(
        "{} sections, {} lines, valid={}",
        lyrics.sections.len(),
        lyrics.total_lines(),
        lyrics.ok()
    );
    for issue in &issues {
        out.push_str(&format!(
            "\n[{}] {}: {}",
            issue.severity.name(),
            issue.code,
            issue.message
        ));
    }
    if !lyrics.ok() {
        bail!(out);
    }
    Ok(out)
}

fn bgm_generate(args: &Value) -> Result<String> {
    let prompt = required_str(args, "prompt")?;
    let out = required_str(args, "out")?;
    let duration = optional_u32(args, "duration", 30)?;
    if duration == 0 {
        bail!("duration must be at least 1 second");
    }
    let spec = optional_str(args, "backend").unwrap_or_else(|| "stub".to_string());
    let backend = audio_backend(&spec)?;
    let result = backend.generate(&BgmRequest::new(prompt, duration, out))?;
    Ok(format!(
        "wrote {}; backend {}, {:.2}s, {} bytes",
        result.path.display(),
        result.backend,
        result.duration_secs,
        result.bytes
    ))
}

fn portrait_generate(args: &Value) -> Result<String> {
    let prompt = required_str(args, "prompt")?;
    let out = required_str(args, "out")?;
    let spec = optional_str(args, "backend").unwrap_or_else(|| "stub".to_string());
    let backend = image_backend(&spec)?;
    let result = backend.generate(&PortraitRequest::new(prompt, out))?;
    Ok(format!(
        "wrote {}; backend {}, {}x{}, {} bytes",
        result.path.display(),
        result.backend,
        result.width,
        result.height,
        result.bytes
    ))
}

fn bundle(args: &Value) -> Result<String> {
    let prompt = required_str(args, "prompt")?;
    let out = required_str(args, "out")?;
    let mut request = BundleRequest::new(prompt, out);
    request.arm = parse_arm(args)?;
    request.bgm_duration_secs = optional_u32(args, "bgm_duration", 30)?;
    if let Some(spec) = optional_str(args, "bgm_backend") {
        request.bgm_backend = spec;
    }
    if let Some(spec) = optional_str(args, "image_backend") {
        request.image_backend = spec;
    }
    if let Some(spec) = optional_str(args, "lyrics_backend") {
        request.lyrics_backend = spec;
    }

    let manifest = lyco_bundle::build(&request)?;
    let mut summary = format!("bundle written to {}", request.out_dir.display());
    for artifact in &manifest.artifacts {
        let flag = if artifact.placeholder {
            " (placeholder)"
        } else {
            ""
        };
        summary.push_str(&format!(
            "\n  {} {} via {}{flag}",
            artifact.kind, artifact.file, artifact.backend
        ));
    }
    for note in &manifest.notes {
        summary.push_str(&format!("\n  note: {note}"));
    }
    Ok(summary)
}
