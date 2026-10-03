//! Deterministic palette quantization for Minecraft skins.
//!
//! Minecraft skin generation has no established open-source solution, so the
//! coherent-palette step is ours: reduce a generated image to a small, fixed
//! palette (default 8 colors) so the resulting skin reads as hand-drawn rather
//! than as a downscaled photograph.
//!
//! The quantizer is median-cut, chosen because it is fully deterministic:
//! no RNG, no iteration-count sensitivity, stable output across platforms.
//! That matters because skin output feeds a version-controlled asset pipeline.

/// An 8-bit-per-channel opaque color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    pub fn as_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Squared euclidean distance in RGB space.
    fn dist2(&self, o: &Rgb) -> u32 {
        let dr = self.r as i32 - o.r as i32;
        let dg = self.g as i32 - o.g as i32;
        let db = self.b as i32 - o.b as i32;
        (dr * dr + dg * dg + db * db) as u32
    }
}

/// A fixed set of colors that generated art is snapped to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    colors: Vec<Rgb>,
}

impl Palette {
    pub fn new(colors: Vec<Rgb>) -> Self {
        Palette { colors }
    }

    pub fn colors(&self) -> &[Rgb] {
        &self.colors
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    pub fn as_hex(&self) -> Vec<String> {
        self.colors.iter().map(|c| c.as_hex()).collect()
    }

    /// Closest palette entry, or the input when the palette is empty.
    pub fn nearest(&self, c: Rgb) -> Rgb {
        let mut best = c;
        let mut best_d = u32::MAX;
        for p in &self.colors {
            let d = p.dist2(&c);
            if d < best_d {
                best_d = d;
                best = *p;
            }
        }
        best
    }
}

/// Default palette size. Eight colors keeps a 64x64 skin readable; fewer
/// flattens facial detail, more starts reintroducing photographic noise.
pub const DEFAULT_PALETTE_SIZE: usize = 8;

/// Channel with the widest spread, as (channel index, spread).
fn widest_channel(px: &[[u8; 3]]) -> (usize, u8) {
    let mut best = (0usize, 0u8);
    for ch in 0..3 {
        let mut lo = 255u8;
        let mut hi = 0u8;
        for p in px {
            if p[ch] < lo {
                lo = p[ch];
            }
            if p[ch] > hi {
                hi = p[ch];
            }
        }
        let spread = hi - lo;
        if spread > best.1 {
            best = (ch, spread);
        }
    }
    best
}

fn average(px: &[[u8; 3]]) -> Rgb {
    let n = px.len() as u64;
    let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
    for p in px {
        sr += p[0] as u64;
        sg += p[1] as u64;
        sb += p[2] as u64;
    }
    Rgb::new((sr / n) as u8, (sg / n) as u8, (sb / n) as u8)
}

/// Build a palette of at most `max_colors` entries from RGBA pixels.
///
/// Fully transparent pixels (alpha < 128) are ignored: a skin's unused
/// regions must not drag the palette toward black.
pub fn median_cut(pixels: &[[u8; 4]], max_colors: usize) -> Palette {
    if max_colors == 0 {
        return Palette::new(Vec::new());
    }

    let opaque: Vec<[u8; 3]> = pixels
        .iter()
        .filter(|p| p[3] >= 128)
        .map(|p| [p[0], p[1], p[2]])
        .collect();

    if opaque.is_empty() {
        return Palette::new(Vec::new());
    }

    let mut boxes: Vec<Vec<[u8; 3]>> = vec![opaque];

    while boxes.len() < max_colors {
        // Split the box with the widest channel spread; ties go to the lowest index
        // so the result never depends on map iteration order.
        let mut target: Option<usize> = None;
        let mut best_spread = 0u8;
        for (i, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            let (_, spread) = widest_channel(b);
            if spread > best_spread {
                best_spread = spread;
                target = Some(i);
            }
        }

        let Some(i) = target else { break };

        let mut b = boxes.swap_remove(i);
        let (ch, _) = widest_channel(&b);
        // Total order including all channels makes the split reproducible.
        b.sort_unstable_by_key(|p| (p[ch], p[0], p[1], p[2]));
        let mid = b.len() / 2;
        let right = b.split_off(mid);
        boxes.push(b);
        boxes.push(right);
    }

    let mut colors: Vec<Rgb> = boxes
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| average(b))
        .collect();

    colors.sort_unstable();
    colors.dedup();
    Palette::new(colors)
}

/// Snap every pixel to the palette. Transparent pixels stay transparent.
pub fn apply(pixels: &[[u8; 4]], palette: &Palette) -> Vec<[u8; 4]> {
    pixels
        .iter()
        .map(|p| {
            if p[3] < 128 {
                [0, 0, 0, 0]
            } else {
                let n = palette.nearest(Rgb::new(p[0], p[1], p[2]));
                [n.r, n.g, n.b, 255]
            }
        })
        .collect()
}

/// Count distinct opaque colors, the measure of whether quantization helped.
pub fn distinct_opaque_colors(pixels: &[[u8; 4]]) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for p in pixels {
        if p[3] >= 128 {
            seen.insert((p[0], p[1], p[2]));
        }
    }
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(c: [u8; 3]) -> [u8; 4] {
        [c[0], c[1], c[2], 255]
    }

    #[test]
    fn transparent_pixels_do_not_pollute_the_palette() {
        let mut pixels = vec![[0, 0, 0, 0]; 32];
        pixels.push(px([255, 0, 0]));
        let p = median_cut(&pixels, 8);
        assert_eq!(p.len(), 1);
        assert_eq!(p.colors()[0], Rgb::new(255, 0, 0));
    }

    #[test]
    fn fully_transparent_input_yields_empty_palette() {
        let pixels = vec![[0, 0, 0, 0]; 16];
        assert!(median_cut(&pixels, 8).is_empty());
    }

    #[test]
    fn never_exceeds_requested_size() {
        let mut pixels = Vec::new();
        for i in 0..=255u8 {
            pixels.push(px([i, 255 - i, i / 2]));
        }
        for n in [1usize, 2, 6, 8, 16] {
            assert!(median_cut(&pixels, n).len() <= n, "n={}", n);
        }
    }

    #[test]
    fn is_deterministic_across_runs() {
        let mut pixels = Vec::new();
        for i in 0..1000u32 {
            pixels.push(px([(i % 251) as u8, ((i * 7) % 241) as u8, ((i * 13) % 233) as u8]));
        }
        let a = median_cut(&pixels, 8);
        let b = median_cut(&pixels, 8);
        assert_eq!(a, b);
    }

    #[test]
    fn is_order_independent() {
        let mut a = Vec::new();
        for i in 0..200u32 {
            a.push(px([(i % 97) as u8, ((i * 3) % 89) as u8, ((i * 5) % 83) as u8]));
        }
        let mut b = a.clone();
        b.reverse();
        assert_eq!(median_cut(&a, 8), median_cut(&b, 8));
    }

    #[test]
    fn separates_two_clear_clusters() {
        let mut pixels = Vec::new();
        for _ in 0..50 {
            pixels.push(px([250, 10, 10]));
            pixels.push(px([10, 10, 250]));
        }
        let p = median_cut(&pixels, 2);
        assert_eq!(p.len(), 2);
        let hexes = p.as_hex();
        assert!(hexes.contains(&"#fa0a0a".to_string()), "{:?}", hexes);
        assert!(hexes.contains(&"#0a0afa".to_string()), "{:?}", hexes);
    }

    #[test]
    fn apply_snaps_every_opaque_pixel_into_the_palette() {
        let pixels = vec![px([250, 10, 10]), px([12, 9, 240]), [1, 2, 3, 0]];
        let p = median_cut(&pixels, 2);
        let out = apply(&pixels, &p);
        assert_eq!(out[2], [0, 0, 0, 0], "transparent stays transparent");
        for o in &out[..2] {
            assert_eq!(o[3], 255);
            let c = Rgb::new(o[0], o[1], o[2]);
            assert!(p.colors().contains(&c), "{:?} not in palette", c);
        }
    }

    #[test]
    fn apply_collapses_a_gradient_to_the_palette() {
        let mut pixels = Vec::new();
        for i in 0..=255u8 {
            pixels.push(px([i, i, i]));
        }
        assert_eq!(distinct_opaque_colors(&pixels), 256);
        let p = median_cut(&pixels, 8);
        let out = apply(&pixels, &p);
        assert!(distinct_opaque_colors(&out) <= 8);
    }

    #[test]
    fn nearest_falls_back_to_input_on_empty_palette() {
        let p = Palette::new(Vec::new());
        let c = Rgb::new(9, 8, 7);
        assert_eq!(p.nearest(c), c);
    }
}
