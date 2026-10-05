//! The Spine texture-atlas text format: parse, inspect, normalize.
//!
//! An atlas is a small text file describing how a PNG is cut into regions:
//!
//! ```text
//! page.png
//! size: 1024,512
//! format: RGBA8888
//! filter: Linear,Linear
//! repeat: none
//! head
//!   rotate: false
//!   xy: 2, 2
//!   size: 128, 128
//! ```
//!
//! A page starts at the first non-blank line after a blank line or at the start
//! of the file. Its name is followed by `key: value` page fields. Every other
//! non-blank line starts a region, whose own `key: value` lines follow it.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: String,
    /// Index into the atlas lines this field was read from.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub name: String,
    /// Index of the page-name line.
    pub line: usize,
    pub fields: Vec<Field>,
    pub regions: Vec<String>,
    /// Index of the last page field, which is where `pma` belongs.
    pub last_field_line: usize,
}

impl Page {
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The declared page size, if it parses.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.field("size").and_then(|f| parse_size(&f.value))
    }

    pub fn has_pma(&self) -> bool {
        self.field("pma").is_some_and(|f| f.value == "true")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasInfo {
    pub pages: Vec<String>,
    pub regions: BTreeSet<String>,
    /// True when every page declares a well-formed size.
    pub has_size: bool,
    /// True when every page declares premultiplied alpha.
    pub has_pma: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    lines: Vec<String>,
    pages: Vec<Page>,
}

impl Atlas {
    pub fn parse(text: &str) -> Atlas {
        let lines = normalized_lines(text);
        let mut pages: Vec<Page> = Vec::new();
        let mut open = false;
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim().is_empty() {
                // A blank line closes the current page.
                open = false;
                i += 1;
                continue;
            }
            if !open {
                let name = lines[i].trim().to_string();
                let page_line = i;
                i += 1;
                let mut fields = Vec::new();
                let mut last_field_line = page_line;
                while i < lines.len() {
                    match split_entry(&lines[i]) {
                        Some((key, value)) => {
                            fields.push(Field {
                                key,
                                value,
                                line: i,
                            });
                            last_field_line = i;
                            i += 1;
                        }
                        None => break,
                    }
                }
                pages.push(Page {
                    name,
                    line: page_line,
                    fields,
                    regions: Vec::new(),
                    last_field_line,
                });
                open = true;
                continue;
            }
            // Everything else on an open page is a region.
            let page = pages.last_mut().expect("a page is open");
            page.regions.push(lines[i].trim().to_string());
            i += 1;
            while i < lines.len() && split_entry(&lines[i]).is_some() {
                i += 1;
            }
        }
        Atlas { lines, pages }
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn info(&self) -> AtlasInfo {
        let mut regions = BTreeSet::new();
        for page in &self.pages {
            for region in &page.regions {
                regions.insert(region.clone());
            }
        }
        AtlasInfo {
            pages: self.pages.iter().map(|p| p.name.clone()).collect(),
            regions,
            has_size: !self.pages.is_empty() && self.pages.iter().all(|p| p.size().is_some()),
            has_pma: !self.pages.is_empty() && self.pages.iter().all(Page::has_pma),
        }
    }

    /// Repair a page's `size` field, add `pma`, and optionally rename pages.
    ///
    /// Idempotent: normalizing an already-normalized atlas changes nothing and
    /// reports `changed: false`.
    pub fn normalize(&self, opts: &Normalize) -> Normalized {
        let mut inserts: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut replace: BTreeMap<usize, String> = BTreeMap::new();
        let mut missing_size = Vec::new();
        let mut fixed_size = Vec::new();

        for page in &self.pages {
            if let Some(local) = opts.renames.get(&page.name) {
                if !local.is_empty() && local != &page.name {
                    replace.insert(page.line, local.clone());
                }
            }

            let real = opts.sizes.get(&page.name).copied();
            match (page.size(), real) {
                // The field is missing and we know the truth from the PNG.
                (None, Some((w, h))) => {
                    inserts
                        .entry(page.line)
                        .or_default()
                        .push(format!("size: {w},{h}"));
                }
                // Missing and unknown: say so rather than invent a number.
                (None, None) => missing_size.push(page.name.clone()),
                // Present but wrong.
                (Some(declared), Some((w, h))) if declared != (w, h) => {
                    if let Some(field) = page.field("size") {
                        replace.insert(field.line, format!("size: {w},{h}"));
                        fixed_size.push(page.name.clone());
                    }
                }
                _ => {}
            }

            if opts.pma && page.field("pma").is_none() {
                inserts
                    .entry(page.last_field_line)
                    .or_default()
                    .push("pma: true".to_string());
            }
        }

        let mut out: Vec<String> = Vec::with_capacity(self.lines.len());
        for (i, line) in self.lines.iter().enumerate() {
            out.push(replace.get(&i).cloned().unwrap_or_else(|| line.clone()));
            if let Some(extra) = inserts.get(&i) {
                // `size` has to sit immediately after the page name for the
                // readers that expect page fields in order.
                let mut ordered = extra.clone();
                ordered.sort_by_key(|s| u8::from(!s.starts_with("size:")));
                out.extend(ordered);
            }
        }

        let text = out.join("\n");
        let original = self.lines.join("\n");
        Normalized {
            changed: text != original,
            text,
            pages: self
                .pages
                .iter()
                .map(|p| {
                    replace
                        .get(&p.line)
                        .cloned()
                        .unwrap_or_else(|| p.name.clone())
                })
                .collect(),
            missing_size,
            fixed_size,
        }
    }
}

/// What to repair, and what is known about the real textures.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Normalize {
    /// Page name to the real size of its PNG.
    pub sizes: BTreeMap<String, (u32, u32)>,
    /// Declare premultiplied alpha on every page that does not say.
    pub pma: bool,
    /// Page name to the local file name it should be rewritten to.
    pub renames: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub text: String,
    pub changed: bool,
    /// Page names after renaming.
    pub pages: Vec<String>,
    /// Pages whose size is unknown, so it could not be repaired.
    pub missing_size: Vec<String>,
    /// Pages whose declared size disagreed with the PNG and was corrected.
    pub fixed_size: Vec<String>,
}

/// Fit a page name to something safe to use as a local file name.
pub fn sanitize_page_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "page.png".to_string();
    }
    out
}

/// Read a `size` value of the form `1024,512`.
pub fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (left, right) = text.split_once(',')?;
    let width = left.trim().parse::<u32>().ok()?;
    let height = right.trim().parse::<u32>().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

/// Split a `key: value` line. Returns `None` for a line that is a region name.
fn split_entry(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let colon = trimmed.find(':')?;
    Some((
        trimmed[..colon].trim().to_string(),
        trimmed[colon + 1..].trim().to_string(),
    ))
}

/// Drop a byte-order mark and collapse line endings.
fn normalized_lines(text: &str) -> Vec<String> {
    let without_bom = text.strip_prefix('\u{feff}').unwrap_or(text);
    let unified = without_bom.replace("\r\n", "\n").replace('\r', "\n");
    unified.split('\n').map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
page.png
size: 1024,512
format: RGBA8888
filter: Linear,Linear
repeat: none
head
  rotate: false
  xy: 2, 2
  size: 128, 128
body
  rotate: false
  xy: 132, 2
  size: 64, 64
";

    fn sizes(pairs: &[(&str, u32, u32)]) -> BTreeMap<String, (u32, u32)> {
        pairs
            .iter()
            .map(|(name, w, h)| (name.to_string(), (*w, *h)))
            .collect()
    }

    #[test]
    fn parses_a_page_its_fields_and_its_regions() {
        let atlas = Atlas::parse(SAMPLE);
        assert_eq!(atlas.pages().len(), 1);
        let page = &atlas.pages()[0];
        assert_eq!(page.name, "page.png");
        assert_eq!(page.size(), Some((1024, 512)));
        assert_eq!(
            page.field("format").map(|f| f.value.as_str()),
            Some("RGBA8888")
        );
        assert_eq!(page.regions, vec!["head".to_string(), "body".to_string()]);
    }

    /// A region's own `key: value` lines must not be read as page fields.
    #[test]
    fn region_entries_stay_out_of_the_page_fields() {
        let atlas = Atlas::parse(SAMPLE);
        let page = &atlas.pages()[0];
        assert!(page.field("rotate").is_none(), "rotate is a region entry");
        assert!(page.field("xy").is_none());
        assert_eq!(page.fields.len(), 4, "only the four page fields");
    }

    #[test]
    fn a_blank_line_starts_a_new_page() {
        let text = "a.png\nsize: 8,8\nr1\n  xy: 0,0\n\nb.png\nsize: 16,16\nr2\n  xy: 0,0\n";
        let atlas = Atlas::parse(text);
        assert_eq!(atlas.pages().len(), 2);
        assert_eq!(atlas.pages()[0].regions, vec!["r1".to_string()]);
        assert_eq!(atlas.pages()[1].name, "b.png");
        assert_eq!(atlas.pages()[1].size(), Some((16, 16)));
    }

    #[test]
    fn a_byte_order_mark_and_crlf_are_absorbed() {
        let text = "\u{feff}page.png\r\nsize: 4,4\r\nr\r\n  xy: 0,0\r\n";
        let atlas = Atlas::parse(text);
        assert_eq!(atlas.pages()[0].name, "page.png");
        assert_eq!(atlas.pages()[0].size(), Some((4, 4)));
        // And normalizing must not rewrite line endings that are already fine.
        assert!(!atlas.normalize(&Normalize::default()).changed);
    }

    #[test]
    fn info_collects_regions_and_flags() {
        let atlas = Atlas::parse(SAMPLE);
        let info = atlas.info();
        assert_eq!(info.pages, vec!["page.png".to_string()]);
        assert!(info.regions.contains("head"));
        assert!(info.regions.contains("body"));
        assert!(info.has_size);
        assert!(
            !info.has_pma,
            "the sample says nothing about premultiplied alpha"
        );

        let missing = Atlas::parse("p.png\nr\n  xy: 0,0\n");
        assert!(!missing.info().has_size);
        assert_eq!(missing.info().pages, vec!["p.png".to_string()]);
    }

    /// The repair that matters most: a dump that omitted `size` entirely.
    #[test]
    fn a_missing_size_is_inserted_from_the_real_texture() {
        let atlas = Atlas::parse("page.png\nformat: RGBA8888\nhead\n  xy: 0,0\n");
        let opts = Normalize {
            sizes: sizes(&[("page.png", 512, 256)]),
            ..Normalize::default()
        };
        let result = atlas.normalize(&opts);
        assert!(result.changed);
        assert!(result.missing_size.is_empty());
        let lines: Vec<&str> = result.text.lines().collect();
        assert_eq!(lines[0], "page.png");
        assert_eq!(
            lines[1], "size: 512,256",
            "size goes right after the page name"
        );
        assert_eq!(lines[2], "format: RGBA8888");
        assert_eq!(lines[3], "head");
        // And the repaired atlas now reads back correctly.
        assert_eq!(
            Atlas::parse(&result.text).pages()[0].size(),
            Some((512, 256))
        );
    }

    #[test]
    fn a_size_that_disagrees_with_the_texture_is_corrected() {
        let atlas = Atlas::parse(SAMPLE);
        let opts = Normalize {
            sizes: sizes(&[("page.png", 2048, 1024)]),
            ..Normalize::default()
        };
        let result = atlas.normalize(&opts);
        assert_eq!(result.fixed_size, vec!["page.png".to_string()]);
        assert!(result.text.contains("size: 2048,1024"));
        assert!(!result.text.contains("1024,512"));
    }

    #[test]
    fn an_unknown_size_is_reported_not_invented() {
        let atlas = Atlas::parse("page.png\nformat: RGBA8888\nhead\n  xy: 0,0\n");
        let result = atlas.normalize(&Normalize::default());
        assert_eq!(result.missing_size, vec!["page.png".to_string()]);
        assert!(
            !result.changed,
            "nothing could be repaired, so nothing changed"
        );
        assert!(!result.text.contains("size:"));
    }

    #[test]
    fn premultiplied_alpha_is_declared_after_the_other_fields() {
        let atlas = Atlas::parse("page.png\nsize: 8,8\nformat: RGBA8888\nhead\n  xy: 0,0\n");
        let opts = Normalize {
            pma: true,
            ..Normalize::default()
        };
        let result = atlas.normalize(&opts);
        let lines: Vec<&str> = result.text.lines().collect();
        assert_eq!(
            lines[..4],
            ["page.png", "size: 8,8", "format: RGBA8888", "pma: true"]
        );
        assert_eq!(lines[4], "head");
        assert!(Atlas::parse(&result.text).pages()[0].has_pma());
    }

    /// A repaired atlas must be settled: running the repair again does nothing.
    #[test]
    fn normalization_is_idempotent() {
        let atlas = Atlas::parse("page.png\nformat: RGBA8888\nhead\n  xy: 0,0\n");
        let opts = Normalize {
            sizes: sizes(&[("page.png", 512, 256)]),
            pma: true,
            ..Normalize::default()
        };
        let once = atlas.normalize(&opts);
        assert!(once.changed);
        let twice = Atlas::parse(&once.text).normalize(&opts);
        assert!(!twice.changed, "second pass rewrote:\n{}", twice.text);
        assert_eq!(once.text, twice.text);
    }

    #[test]
    fn size_is_inserted_before_pma() {
        let atlas = Atlas::parse("page.png\nformat: RGBA8888\nhead\n  xy: 0,0\n");
        let opts = Normalize {
            sizes: sizes(&[("page.png", 8, 4)]),
            pma: true,
            ..Normalize::default()
        };
        let result = atlas.normalize(&opts);
        let lines: Vec<&str> = result.text.lines().collect();
        assert_eq!(lines[1], "size: 8,4");
        assert_eq!(lines[3], "pma: true");
    }

    #[test]
    fn a_page_can_be_renamed_to_its_local_file_name() {
        let atlas = Atlas::parse(SAMPLE);
        let mut renames = BTreeMap::new();
        renames.insert("page.png".to_string(), "page_0.png".to_string());
        let opts = Normalize {
            renames,
            ..Normalize::default()
        };
        let result = atlas.normalize(&opts);
        assert_eq!(result.pages, vec!["page_0.png".to_string()]);
        assert!(result.text.starts_with("page_0.png\n"));
        assert_eq!(Atlas::parse(&result.text).pages()[0].name, "page_0.png");
    }

    #[test]
    fn an_empty_atlas_is_not_an_error() {
        let atlas = Atlas::parse("");
        assert!(atlas.pages().is_empty());
        let info = atlas.info();
        assert!(info.pages.is_empty());
        // An empty atlas has not declared anything, so neither flag holds.
        assert!(!info.has_size);
        assert!(!info.has_pma);
        assert!(!atlas.normalize(&Normalize::default()).changed);
    }

    #[test]
    fn page_names_are_sanitized_for_the_file_system() {
        assert_eq!(sanitize_page_name("page.png"), "page.png");
        assert_eq!(sanitize_page_name("a/b:c.png"), "a_b_c.png");
        assert_eq!(sanitize_page_name("char_107_liskam"), "char_107_liskam");
        assert_eq!(sanitize_page_name(""), "page.png");
        assert_eq!(sanitize_page_name("///"), "___");
    }

    #[test]
    fn size_parsing_rejects_anything_it_cannot_trust() {
        assert_eq!(parse_size("1024,512"), Some((1024, 512)));
        assert_eq!(parse_size(" 8 , 4 "), Some((8, 4)));
        assert_eq!(parse_size("8"), None);
        assert_eq!(parse_size("8,4,2"), None);
        assert_eq!(parse_size("8,x"), None);
        assert_eq!(parse_size("0,4"), None, "a zero-width page is not usable");
        assert_eq!(parse_size("8,0"), None);
    }
}
