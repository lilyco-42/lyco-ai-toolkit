//! Minimal GLB (binary glTF 2.0) reader - the input side of F3.
//!
//! Deliberately not a full glTF implementation. It reads the container and
//! exactly what a mesh-to-Minecraft conversion needs: node transforms, mesh
//! primitives, and the POSITION / index accessors.
//!
//! Spec: Khronos glTF 2.0. The container is a 12-byte header followed by
//! 4-byte-aligned chunks; chunkData(JSON) is space-padded and chunkData(BIN)
//! is zero-padded, and that padding is counted inside chunkLength.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// ASCII 'glTF' as a little-endian u32.
const MAGIC: u32 = 0x4654_6C67;
/// ASCII 'JSON' - the required first chunk.
const CHUNK_JSON: u32 = 0x4E4F_534A;
/// ASCII 'BIN\0' - the optional binary payload.
const CHUNK_BIN: u32 = 0x004E_4942;

const COMPONENT_U8: u32 = 5121;
const COMPONENT_U16: u32 = 5123;
const COMPONENT_U32: u32 = 5125;
const COMPONENT_F32: u32 = 5126;

/// A parsed GLB: the glTF document plus its decoded binary buffer.
#[derive(Debug, Clone)]
pub struct Glb {
    pub gltf: Gltf,
    pub bin: Vec<u8>,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Gltf {
    pub asset: Asset,
    pub scene: Option<usize>,
    pub scenes: Vec<Scene>,
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub accessors: Vec<Accessor>,
    pub buffer_views: Vec<BufferView>,
    pub buffers: Vec<Buffer>,
    pub materials: Vec<Material>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Asset {
    pub version: String,
    pub generator: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Scene {
    pub name: Option<String>,
    pub nodes: Vec<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Node {
    pub name: Option<String>,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
    pub matrix: Option<[f32; 16]>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Mesh {
    pub name: Option<String>,
    pub primitives: Vec<Primitive>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Primitive {
    pub attributes: Attributes,
    pub indices: Option<usize>,
    pub material: Option<usize>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Attributes {
    #[serde(rename = "POSITION")]
    pub position: Option<usize>,
    #[serde(rename = "NORMAL")]
    pub normal: Option<usize>,
    #[serde(rename = "TEXCOORD_0")]
    pub texcoord_0: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Accessor {
    pub name: Option<String>,
    pub buffer_view: Option<usize>,
    pub byte_offset: Option<usize>,
    pub component_type: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub count: usize,
    pub min: Option<Vec<f32>>,
    pub max: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BufferView {
    pub buffer: usize,
    pub byte_offset: Option<usize>,
    pub byte_length: usize,
    pub byte_stride: Option<usize>,
    pub target: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Buffer {
    pub byte_length: usize,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Material {
    pub name: Option<String>,
    pub pbr_metallic_roughness: Option<Pbr>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Pbr {
    pub base_color_factor: Option<[f32; 4]>,
    pub metallic_factor: Option<f32>,
    pub roughness_factor: Option<f32>,
}

impl Glb {
    /// Parse a GLB container. Unknown chunk types are skipped, as the spec allows.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 12 {
            bail!("GLB too short: {} bytes, need at least 12", bytes.len());
        }
        let magic = u32::from_le_bytes(read4(bytes, 0)?);
        if magic != MAGIC {
            bail!("not a GLB: magic {magic:#010x}, expected {MAGIC:#010x}");
        }
        let version = u32::from_le_bytes(read4(bytes, 4)?);
        let declared = u32::from_le_bytes(read4(bytes, 8)?) as usize;
        if declared > bytes.len() {
            bail!(
                "GLB declares {declared} bytes but only {} were provided",
                bytes.len()
            );
        }

        let mut offset = 12usize;
        let mut gltf: Option<Gltf> = None;
        let mut bin: Vec<u8> = Vec::new();

        while offset + 8 <= declared {
            let chunk_len = u32::from_le_bytes(read4(bytes, offset)?) as usize;
            let chunk_type = u32::from_le_bytes(read4(bytes, offset + 4)?);
            let start = offset + 8;
            let end = start
                .checked_add(chunk_len)
                .context("chunk length overflows")?;
            if end > declared {
                bail!("chunk at {offset} runs past the declared length");
            }

            match chunk_type {
                CHUNK_JSON => {
                    let doc = serde_json::from_slice(&bytes[start..end])
                        .context("cannot parse the glTF JSON chunk")?;
                    gltf = Some(doc);
                }
                CHUNK_BIN => bin = bytes[start..end].to_vec(),
                // Per spec, readers must ignore chunks they do not understand.
                _ => {}
            }

            // Chunk data is padded to a 4-byte boundary; the padding is
            // included in chunkLength, so this normally lands already aligned.
            offset = end;
            while offset < declared && !offset.is_multiple_of(4) {
                offset += 1;
            }
        }

        let gltf = gltf.context("GLB has no JSON chunk")?;
        if gltf.asset.version.is_empty() {
            bail!("glTF asset.version is missing");
        }
        Ok(Glb {
            gltf,
            bin,
            version,
        })
    }

    /// Read a VEC3 f32 accessor, e.g. a primitive's POSITION.
    pub fn read_vec3(&self, accessor_index: usize) -> Result<Vec<[f32; 3]>> {
        let acc = self.accessor(accessor_index)?;
        if acc.kind != "VEC3" {
            bail!("accessor {accessor_index} is {} not VEC3", acc.kind);
        }
        if acc.component_type != COMPONENT_F32 {
            bail!("accessor {accessor_index} is not float");
        }
        let stride = self.stride_of(acc)?;
        let base = self.base_offset(acc)?;
        let mut out = Vec::with_capacity(acc.count);
        for i in 0..acc.count {
            let o = base + i * stride;
            let x = f32::from_le_bytes(read4(&self.bin, o)?);
            let y = f32::from_le_bytes(read4(&self.bin, o + 4)?);
            let z = f32::from_le_bytes(read4(&self.bin, o + 8)?);
            out.push([x, y, z]);
        }
        Ok(out)
    }

    /// Read a SCALAR index accessor, widening u8/u16/u32 to u32.
    pub fn read_indices(&self, accessor_index: usize) -> Result<Vec<u32>> {
        let acc = self.accessor(accessor_index)?;
        if acc.kind != "SCALAR" {
            bail!("accessor {accessor_index} is {} not SCALAR", acc.kind);
        }
        let stride = self.stride_of(acc)?;
        let base = self.base_offset(acc)?;
        let width = match acc.component_type {
            COMPONENT_U8 => 1usize,
            COMPONENT_U16 => 2usize,
            COMPONENT_U32 => 4usize,
            other => bail!("unsupported index componentType {other}"),
        };
        let mut out = Vec::with_capacity(acc.count);
        for i in 0..acc.count {
            let o = base + i * stride;
            let v = match width {
                1 => self.bin[o] as u32,
                2 => u16::from_le_bytes([self.byte(o)?, self.byte(o + 1)?]) as u32,
                _ => u32::from_le_bytes(read4(&self.bin, o)?),
            };
            out.push(v);
        }
        Ok(out)
    }

    fn byte(&self, at: usize) -> Result<u8> {
        self.bin
            .get(at)
            .copied()
            .with_context(|| format!("binary buffer ends before offset {at}"))
    }

    fn accessor(&self, i: usize) -> Result<&Accessor> {
        self.gltf
            .accessors
            .get(i)
            .with_context(|| format!("no accessor {i}"))
    }

    fn base_offset(&self, acc: &Accessor) -> Result<usize> {
        let view_index = acc
            .buffer_view
            .context("accessor has no bufferView (sparse accessors are unsupported)")?;
        let view = self
            .gltf
            .buffer_views
            .get(view_index)
            .with_context(|| format!("no bufferView {view_index}"))?;
        Ok(view.byte_offset.unwrap_or(0) + acc.byte_offset.unwrap_or(0))
    }

    /// Element stride, falling back to tightly-packed element size.
    fn stride_of(&self, acc: &Accessor) -> Result<usize> {
        let view_index = acc.buffer_view.context("accessor has no bufferView")?;
        let view = self
            .gltf
            .buffer_views
            .get(view_index)
            .with_context(|| format!("no bufferView {view_index}"))?;
        Ok(view
            .byte_stride
            .unwrap_or_else(|| element_size(&acc.kind, acc.component_type)))
    }
}

fn element_size(kind: &str, component_type: u32) -> usize {
    let components = match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        _ => 1,
    };
    let unit = match component_type {
        COMPONENT_U8 => 1,
        COMPONENT_U16 => 2,
        _ => 4,
    };
    components * unit
}

fn read4(bytes: &[u8], at: usize) -> Result<[u8; 4]> {
    bytes
        .get(at..at + 4)
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .with_context(|| format!("need 4 bytes at {at}, buffer is {}", bytes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a GLB from a JSON document and a binary payload.
    fn build(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json_bytes = json.as_bytes().to_vec();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let mut bin_bytes = bin.to_vec();
        while !bin_bytes.len().is_multiple_of(4) {
            bin_bytes.push(0);
        }

        let total = 12 + 8 + json_bytes.len() + 8 + bin_bytes.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        out.extend_from_slice(&json_bytes);
        out.extend_from_slice(&(bin_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&bin_bytes);
        out
    }

    /// A one-triangle mesh: 3 float positions then 3 u16 indices.
    fn triangle() -> (String, Vec<u8>) {
        let json = concat!(
            "{\"asset\":{\"version\":\"2.0\"},",
            "\"scene\":0,",
            "\"scenes\":[{\"nodes\":[0]}],",
            "\"nodes\":[{\"name\":\"Body\",\"mesh\":0}],",
            "\"meshes\":[{\"primitives\":[{\"attributes\":{\"POSITION\":0},",
            "\"indices\":1}]}],",
            "\"accessors\":[",
            "{\"bufferView\":0,\"componentType\":5126,\"count\":3,\"type\":\"VEC3\"},",
            "{\"bufferView\":1,\"componentType\":5123,\"count\":3,\"type\":\"SCALAR\"}],",
            "\"bufferViews\":[",
            "{\"buffer\":0,\"byteOffset\":0,\"byteLength\":36},",
            "{\"buffer\":0,\"byteOffset\":36,\"byteLength\":6}],",
            "\"buffers\":[{\"byteLength\":42}]}"
        );
        let mut bin = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        (json.to_string(), bin)
    }

    #[test]
    fn parses_a_minimal_triangle() {
        let (json, bin) = triangle();
        let glb = Glb::parse(&build(&json, &bin)).expect("parse");
        assert_eq!(glb.version, 2);
        assert_eq!(glb.gltf.asset.version, "2.0");
        assert_eq!(glb.gltf.meshes.len(), 1);
        assert_eq!(glb.gltf.nodes[0].name.as_deref(), Some("Body"));

        let prim = &glb.gltf.meshes[0].primitives[0];
        let pos = glb
            .read_vec3(prim.attributes.position.unwrap())
            .expect("pos");
        assert_eq!(pos, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);

        let idx = glb.read_indices(prim.indices.unwrap()).expect("idx");
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn rejects_a_non_glb() {
        let err = Glb::parse(b"this is not a glb file at all").unwrap_err();
        assert!(err.to_string().contains("not a GLB"), "{err}");
    }

    #[test]
    fn rejects_a_truncated_header() {
        let err = Glb::parse(b"glTF").unwrap_err();
        assert!(err.to_string().contains("too short"), "{err}");
    }

    #[test]
    fn rejects_a_length_larger_than_the_buffer() {
        let (json, bin) = triangle();
        let mut bytes = build(&json, &bin);
        // Inflate the declared total length past what we actually hold.
        let bogus = (bytes.len() as u32 + 4096).to_le_bytes();
        bytes[8..12].copy_from_slice(&bogus);
        let err = Glb::parse(&bytes).unwrap_err();
        assert!(err.to_string().contains("declares"), "{err}");
    }

    #[test]
    fn ignores_unknown_chunks() {
        let (json, bin) = triangle();
        let mut bytes = build(&json, &bin);
        // Append a well-formed chunk of an unknown type.
        let payload = [0u8; 8];
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        bytes.extend_from_slice(&payload);
        let total = bytes.len() as u32;
        bytes[8..12].copy_from_slice(&total.to_le_bytes());

        let glb = Glb::parse(&bytes).expect("unknown chunks must be skipped");
        assert_eq!(glb.gltf.meshes.len(), 1);
    }

    #[test]
    fn honours_byte_stride_on_interleaved_data() {
        // Two VEC3 positions interleaved with 4 bytes of padding each.
        let json = concat!(
            "{\"asset\":{\"version\":\"2.0\"},",
            "\"accessors\":[{\"bufferView\":0,\"componentType\":5126,",
            "\"count\":2,\"type\":\"VEC3\"}],",
            "\"bufferViews\":[{\"buffer\":0,\"byteOffset\":0,",
            "\"byteLength\":32,\"byteStride\":16}],",
            "\"buffers\":[{\"byteLength\":32}]}"
        );
        let mut bin = Vec::new();
        for p in [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
            bin.extend_from_slice(&[0, 0, 0, 0]);
        }
        let glb = Glb::parse(&build(json, &bin)).expect("parse");
        let pos = glb.read_vec3(0).expect("pos");
        assert_eq!(pos, vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    }
}
