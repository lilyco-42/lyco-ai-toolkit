//! Minimal RIFF/WAVE PCM 16-bit writer and reader.
//!
//! Hand-rolled rather than pulled from a crate: the whole surface needed is
//! a canonical 44-byte header plus interleaved samples, and being able to
//! *decode* is what lets the pipeline assert that a generated file really is
//! audio of the requested length instead of trusting a backend's exit code.

use std::fmt;

pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;
pub const DEFAULT_CHANNELS: u16 = 2;

/// RIFF/WAVE bytes with a canonical PCM header.
const CANONICAL_HEADER: usize = 44;

const FMT_ID: &[u8] = b"fmt ";
const DATA_ID: &[u8] = b"data";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavError {
    /// The buffer is too small to contain even a bare header.
    TooShort,
    /// The RIFF/WAVE magic is absent.
    BadMagic,
    /// Structurally valid RIFF but not 16-bit PCM, which is all we handle.
    Unsupported,
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WavError::TooShort => write!(f, "buffer is too short to be a WAV file"),
            WavError::BadMagic => write!(f, "missing RIFF/WAVE magic"),
            WavError::Unsupported => write!(f, "only 16-bit PCM WAV is supported"),
        }
    }
}

impl std::error::Error for WavError {}

/// Encode interleaved samples as a canonical 44-byte-header PCM WAV.
pub fn encode(samples: &[i16], channels: u16, sample_rate: u32) -> Result<Vec<u8>, WavError> {
    if channels == 0 || sample_rate == 0 {
        return Err(WavError::Unsupported);
    }
    let bits_per_sample = 16u16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;
    let data_len = (samples.len() * 2) as u32;

    let mut out = Vec::with_capacity(CANONICAL_HEADER + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(FMT_ID);
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(DATA_ID);
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    Ok(out)
}

/// A decoded WAV plus the format facts the pipeline needs to assert on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedWav {
    pub samples: Vec<i16>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl DecodedWav {
    /// Frames, i.e. sample sets across all channels.
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frames() as f64 / self.sample_rate as f64
    }

    /// Peak absolute amplitude, for "is this file actually silent?" checks.
    pub fn peak(&self) -> i16 {
        self.samples.iter().map(|s| s.saturating_abs()).max().unwrap_or(0)
    }
}

/// Decode a PCM 16-bit WAV, walking chunks so extra ones are tolerated.
pub fn decode(bytes: &[u8]) -> Result<DecodedWav, WavError> {
    if bytes.len() < CANONICAL_HEADER {
        return Err(WavError::TooShort);
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::BadMagic);
    }

    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut audio_format = 0u16;
    let mut data: Option<&[u8]> = None;

    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body_start = pos + 8;
        let body_end = match body_start.checked_add(size) {
            Some(end) if end <= bytes.len() => end,
            // A truncated trailing chunk is not worth failing the whole file.
            _ => break,
        };

        if id == FMT_ID {
            if size < 16 {
                return Err(WavError::Unsupported);
            }
            let fmt = &bytes[body_start..body_end];
            audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
            channels = u16::from_le_bytes([fmt[2], fmt[3]]);
            sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
            bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
        } else if id == DATA_ID {
            data = Some(&bytes[body_start..body_end]);
        }

        // Chunks are word-aligned: an odd size carries one pad byte.
        pos = body_end + (size % 2);
    }

    if audio_format != 1 || bits_per_sample != 16 || channels == 0 {
        return Err(WavError::Unsupported);
    }
    let raw = data.ok_or(WavError::Unsupported)?;
    let samples = raw
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    Ok(DecodedWav {
        samples,
        channels,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<i16> {
        (0..n).map(|i| (i as i16).wrapping_mul(37)).collect()
    }

    #[test]
    fn round_trips_through_encode_and_decode() {
        let samples = ramp(1000);
        let bytes = encode(&samples, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE).expect("encode");
        let decoded = decode(&bytes).expect("decode");
        assert_eq!(decoded.samples, samples);
        assert_eq!(decoded.channels, DEFAULT_CHANNELS);
        assert_eq!(decoded.sample_rate, DEFAULT_SAMPLE_RATE);
    }

    #[test]
    fn writes_a_canonical_header() {
        let samples = vec![0i16; 50];
        let bytes = encode(&samples, 1, 8000).expect("encode");
        assert_eq!(bytes.len(), 44 + 100, "44-byte header plus the payload");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        // RIFF size excludes the leading 8 bytes.
        let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(riff_size as usize, bytes.len() - 8);
        // data size is exactly the payload.
        let data_size = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(data_size, 100);
    }

    #[test]
    fn duration_matches_the_requested_length() {
        let sample_rate = 44_100u32;
        let seconds = 2u32;
        let frames = (sample_rate * seconds) as usize;
        let samples = vec![0i16; frames * 2];
        let bytes = encode(&samples, 2, sample_rate).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.frames(), frames);
        assert!((decoded.duration_secs() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(decode(b"RIFF"), Err(WavError::TooShort));
    }

    #[test]
    fn rejects_missing_magic() {
        let mut bytes = encode(&[0i16; 8], 1, 8000).unwrap();
        bytes[0] = b'X';
        assert_eq!(decode(&bytes), Err(WavError::BadMagic));
    }

    #[test]
    fn rejects_non_pcm_audio_format() {
        let mut bytes = encode(&[0i16; 8], 1, 8000).unwrap();
        // audioFormat lives at offset 20; set it to 3 (IEEE float).
        bytes[20] = 3;
        assert_eq!(decode(&bytes), Err(WavError::Unsupported));
    }

    #[test]
    fn tolerates_extra_chunks_before_data() {
        let samples = ramp(64);
        let canonical = encode(&samples, 1, 8000).unwrap();
        // Splice a LIST chunk between the fmt chunk and the data chunk.
        let split = 36usize;
        let mut spliced = canonical[..split].to_vec();
        spliced.extend_from_slice(b"LIST");
        spliced.extend_from_slice(&4u32.to_le_bytes());
        spliced.extend_from_slice(b"INFO");
        spliced.extend_from_slice(&canonical[split..]);
        let riff_size = (spliced.len() - 8) as u32;
        spliced[4..8].copy_from_slice(&riff_size.to_le_bytes());

        let decoded = decode(&spliced).expect("extra chunks must be tolerated");
        assert_eq!(decoded.samples, samples);
    }

    #[test]
    fn zero_channels_is_refused() {
        assert_eq!(encode(&[0i16; 4], 0, 8000), Err(WavError::Unsupported));
        assert_eq!(encode(&[0i16; 4], 1, 0), Err(WavError::Unsupported));
    }

    #[test]
    fn peak_reports_silence_and_signal() {
        let silent = decode(&encode(&[0i16; 32], 1, 8000).unwrap()).unwrap();
        assert_eq!(silent.peak(), 0);
        let loud = decode(&encode(&[0i16, -3000, 120], 1, 8000).unwrap()).unwrap();
        assert_eq!(loud.peak(), 3000);
    }
}
