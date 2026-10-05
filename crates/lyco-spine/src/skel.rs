//! Read the header of a Spine binary skeleton, to learn what it needs.
//!
//! ## Why this exists
//!
//! Spine's binary format is not stable across releases: a 3.8 exporter writes a
//! hash string and then a version string, and 4.x dropped the hash. Runtimes are
//! equally narrow - the pure-Rust `spine2d` accepts 4.3 and *rejects* anything
//! else. Reading two length-prefixed strings costs microseconds and answers the
//! question that otherwise costs an afternoon:
//!
//! > can the runtime I am about to wire up even open this file?
//!
//! It is also the cheapest way to tell whether a dump is the version you think.
//! Real Arknights exports answer `3.8.99`, which is why no 4.3 runtime can read
//! them, whatever their README promises.
//!
//! ## The encoding
//!
//! A string is a single length byte followed by `length - 1` bytes of text; the
//! count includes a terminator that is not part of the value. `0` means absent.
//! Verified against a real `char_002_amiya.skel` (179,880 bytes) whose first 35
//! bytes are `1c` + 27 bytes of hash + `07` + `3.8.99`.

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkelHeader {
    /// The exporter hash that 3.8-era files carry, if present.
    pub hash: Option<String>,
    /// The version the file was exported for, if it declares one.
    pub version: Option<String>,
}

impl SkelHeader {
    /// The major and minor parts of the declared version.
    pub fn major_minor(&self) -> Option<(u32, u32)> {
        let version = self.version.as_deref()?;
        let mut parts = version.split('.');
        let major = parts.next()?.parse::<u32>().ok()?;
        let minor = parts.next().unwrap_or("0").parse::<u32>().ok()?;
        Some((major, minor))
    }

    /// Which family of runtime this file needs.
    ///
    /// The split is at 4.0 because that is where the binary layout changed and
    /// the hash field disappeared; a runtime written for one family cannot read
    /// the other.
    pub fn family(&self) -> RuntimeFamily {
        match self.major_minor() {
            Some((3, _)) => RuntimeFamily::Spine3,
            Some((major, _)) if major >= 4 => RuntimeFamily::Spine4,
            _ => RuntimeFamily::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFamily {
    Spine3,
    Spine4,
    Unknown,
}

impl RuntimeFamily {
    pub fn name(&self) -> &str {
        match self {
            RuntimeFamily::Spine3 => "spine 3.x",
            RuntimeFamily::Spine4 => "spine 4.x",
            RuntimeFamily::Unknown => "unknown",
        }
    }
}

/// Read one length-prefixed string, advancing the cursor.
///
/// Returns `Ok(None)` for the `0` length, which means "absent" rather than
/// "empty". A length of 1 is an empty string.
fn read_string(bytes: &[u8], at: &mut usize) -> Result<Option<String>> {
    let length = match bytes.get(*at) {
        Some(byte) => *byte as usize,
        None => bail!("the file ended where a string length was expected"),
    };
    *at += 1;
    if length == 0 {
        return Ok(None);
    }
    let count = length - 1;
    let end = *at + count;
    if end > bytes.len() {
        bail!(
            "a string claims {count} bytes but only {} remain",
            bytes.len() - *at
        );
    }
    let text = String::from_utf8_lossy(&bytes[*at..end]).to_string();
    *at = end;
    Ok(Some(text))
}

/// Does this look like a version number rather than a hash?
fn looks_like_version(text: &str) -> bool {
    let mut parts = text.split('.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(_)) => {
            major.chars().all(|c| c.is_ascii_digit())
                && !major.is_empty()
                && text.chars().all(|c| c.is_ascii_digit() || c == '.')
        }
        _ => false,
    }
}

/// Read the header of a `.skel` file.
///
/// The two strings are returned whichever order they appear in, because 3.8
/// writes the hash first and later formats write only the version.
pub fn read_header(bytes: &[u8]) -> Result<SkelHeader> {
    if bytes.is_empty() {
        bail!("the file is empty");
    }
    let mut at = 0usize;
    let mut strings: Vec<String> = Vec::new();
    for slot in 0..2 {
        match read_string(bytes, &mut at) {
            Ok(Some(text)) => strings.push(text),
            Ok(None) => {}
            // A 3.8 export carries a hash and then a version, but a 4.x export
            // carries the version alone, so running out of file after the first
            // string is normal rather than an error. Running out before it is
            // not: that is a truncated file.
            Err(err) => {
                if slot == 0 {
                    return Err(err);
                }
                break;
            }
        }
    }

    let mut header = SkelHeader {
        hash: None,
        version: None,
    };
    for candidate in strings {
        if looks_like_version(&candidate) {
            if header.version.is_none() {
                header.version = Some(candidate);
            }
        } else if header.hash.is_none() {
            header.hash = Some(candidate);
        }
    }

    if header.version.is_none() && header.hash.is_none() {
        bail!("no header strings found; this does not look like a .skel file");
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A length byte followed by its text, where the count includes a terminator.
    fn push_string(out: &mut Vec<u8>, text: &str) {
        out.push((text.len() + 1) as u8);
        out.extend_from_slice(text.as_bytes());
    }

    /// The first 35 bytes of a real Arknights export, byte for byte.
    fn real_arknights_header() -> Vec<u8> {
        let mut out = Vec::new();
        push_string(&mut out, "3zUmVpPXAB+DFpqv9VEzMu6Co8M");
        push_string(&mut out, "3.8.99");
        out.extend_from_slice(&[0xc2, 0xfa, 0xcd, 0xbc, 0xc0, 0x3d]);
        out
    }

    #[test]
    fn the_real_arknights_header_decodes_exactly() {
        let bytes = real_arknights_header();
        assert_eq!(bytes[0], 0x1c, "the hash length byte");
        assert_eq!(bytes[28], 0x07, "the version length byte");
        assert_eq!(&bytes[29..35], b"3.8.99");

        let header = read_header(&bytes).expect("header");
        assert_eq!(header.hash.as_deref(), Some("3zUmVpPXAB+DFpqv9VEzMu6Co8M"));
        assert_eq!(header.version.as_deref(), Some("3.8.99"));
        assert_eq!(header.major_minor(), Some((3, 8)));
    }

    /// The finding that decided a whole approach: the file is 3.8, and every
    /// pure-Rust Spine runtime on offer is 4.3-only.
    #[test]
    fn a_real_arknights_file_is_spine_3() {
        let header = read_header(&real_arknights_header()).expect("header");
        assert_eq!(header.family(), RuntimeFamily::Spine3);
        assert_eq!(header.family().name(), "spine 3.x");
    }

    /// Regression guard: 4.x exports have no hash, and an earlier version of
    /// `read_header` insisted on reading two strings, so it rejected every 4.x
    /// file - the exact family it was supposed to recognise.
    #[test]
    fn a_4_x_file_is_recognised_as_the_other_family() {
        let mut bytes = Vec::new();
        push_string(&mut bytes, "4.3.00");
        let header = read_header(&bytes).expect("header");
        assert_eq!(header.version.as_deref(), Some("4.3.00"));
        assert_eq!(header.family(), RuntimeFamily::Spine4);
        assert!(header.hash.is_none(), "4.x dropped the hash");
    }

    #[test]
    fn the_two_strings_are_identified_whichever_order_they_come_in() {
        let mut reversed = Vec::new();
        push_string(&mut reversed, "3.8.99");
        push_string(&mut reversed, "somehashvalue");
        let header = read_header(&reversed).expect("header");
        assert_eq!(header.version.as_deref(), Some("3.8.99"));
        assert_eq!(header.hash.as_deref(), Some("somehashvalue"));
    }

    #[test]
    fn a_zero_length_means_absent_not_empty() {
        let mut bytes = vec![0u8];
        push_string(&mut bytes, "3.8.99");
        let header = read_header(&bytes).expect("header");
        assert!(header.hash.is_none());
        assert_eq!(header.version.as_deref(), Some("3.8.99"));
    }

    #[test]
    fn an_empty_file_is_refused() {
        assert!(read_header(&[]).unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn a_truncated_string_is_refused_rather_than_guessed() {
        // Claims 40 bytes of hash but the file ends long before that.
        let bytes = vec![28u8, b'a', b'b'];
        let err = read_header(&bytes).unwrap_err();
        assert!(err.to_string().contains("remain"), "{err}");
    }

    #[test]
    fn a_file_with_no_recognisable_header_is_refused() {
        let bytes = vec![0u8, 0u8];
        assert!(read_header(&bytes).is_err());
    }

    #[test]
    fn version_shapes_are_judged_carefully() {
        assert!(looks_like_version("3.8.99"));
        assert!(looks_like_version("4.3"));
        assert!(looks_like_version("4.3.00"));
        assert!(!looks_like_version("3zUmVpPXAB+DFpqv9VEzMu6Co8M"));
        assert!(!looks_like_version(""));
        assert!(!looks_like_version("v3.8"));
        assert!(
            !looks_like_version("3"),
            "needs at least a major and a minor"
        );
        assert!(!looks_like_version("3.8.99-beta"));
    }
}
