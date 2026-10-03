//! Procedural placeholder portrait rendering (F7).
//!
//! Pure pixel maths, no IO: everything here is deterministic in the prompt, so
//! a bundle produced twice from the same prompt is byte-identical and a test
//! can assert on exact pixels.
//!
//! This is a placeholder, not art. It exists so the bundle pipeline can be
//! exercised on a machine with no GPU, and every surface that uses it labels
//! the result as a placeholder.

/// Palette choices are fixed so the output stays coherent and reproducible.
const BACKGROUNDS: [[u8; 3]; 6] = [
    [28, 32, 48],
    [44, 34, 52],
    [24, 44, 44],
    [52, 44, 32],
    [36, 40, 60],
    [48, 30, 38],
];

const SKINS: [[u8; 3]; 5] = [
    [232, 190, 160],
    [206, 160, 130],
    [176, 128, 98],
    [138, 98, 74],
    [96, 68, 52],
];

const HAIRS: [[u8; 3]; 6] = [
    [30, 26, 24],
    [92, 62, 34],
    [168, 132, 70],
    [190, 190, 190],
    [120, 40, 40],
    [40, 60, 96],
];

const CLOTHES: [[u8; 3]; 6] = [
    [58, 78, 116],
    [92, 60, 72],
    [64, 92, 78],
    [110, 96, 62],
    [72, 68, 108],
    [96, 84, 84],
];

/// 64-bit FNV-1a. Matches the copy in `lyco-audio`; both are tiny and keeping
/// them local avoids a shared utility crate for six lines.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.clamp(0.0, 255.0) as u8
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

/// Render a deterministic placeholder portrait as row-major RGBA.
///
/// Composition is deliberately simple: a vertical gradient, a shoulder
/// trapezoid, an elliptical head with a hair cap and two eyes.
pub fn placeholder_portrait(prompt: &str, width: u32, height: u32) -> Vec<[u8; 4]> {
    let seed = fnv1a(prompt.as_bytes());
    let pick = |i: u64, n: usize| ((seed >> (i * 5)) % n as u64) as usize;

    let bg_top = BACKGROUNDS[pick(0, BACKGROUNDS.len())];
    let bg_bottom = mix(bg_top, [8, 8, 12], 0.6);
    let skin = SKINS[pick(1, SKINS.len())];
    let hair = HAIRS[pick(2, HAIRS.len())];
    let cloth = CLOTHES[pick(3, CLOTHES.len())];

    let w = width as f32;
    let h = height as f32;
    // Head geometry scales with the canvas so any size looks intentional.
    let head_cx = w * 0.5;
    let head_cy = h * 0.36;
    let head_rx = w * 0.17;
    let head_ry = h * 0.19;

    let mut out = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        let fy = y as f32;
        let gradient = if h > 0.0 { fy / h } else { 0.0 };
        for x in 0..width {
            let fx = x as f32;
            let mut colour = mix(bg_top, bg_bottom, gradient);

            // Shoulders: a trapezoid widening towards the bottom edge.
            let shoulder_top = head_cy + head_ry * 0.85;
            if fy >= shoulder_top {
                let t = ((fy - shoulder_top) / (h - shoulder_top).max(1.0)).min(1.0);
                let half = w * (0.16 + 0.20 * t);
                if (fx - head_cx).abs() <= half {
                    colour = mix(cloth, [0, 0, 0], 0.25 * gradient);
                }
            }

            // Head: an ellipse.
            let dx = (fx - head_cx) / head_rx.max(1.0);
            let dy = (fy - head_cy) / head_ry.max(1.0);
            let inside_head = dx * dx + dy * dy <= 1.0;
            if inside_head {
                colour = skin;
                // Hair cap over the upper third of the head.
                if dy < -0.15 {
                    colour = hair;
                }
                // Two eyes at a fixed offset, plus a hint of shading.
                let eye_y = head_cy - head_ry * 0.05;
                for side in [-1.0f32, 1.0] {
                    let ex = head_cx + side * head_rx * 0.38;
                    let edx = (fx - ex) / (head_rx * 0.10).max(1.0);
                    let edy = (fy - eye_y) / (head_ry * 0.07).max(1.0);
                    if edx * edx + edy * edy <= 1.0 {
                        colour = [24, 22, 30];
                    }
                }
                colour = mix(colour, [0, 0, 0], 0.15 * (0.5 - dy).max(0.0));
            }

            out.push([colour[0], colour[1], colour[2], 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic_for_a_prompt() {
        let a = placeholder_portrait("a quiet librarian", 64, 64);
        let b = placeholder_portrait("a quiet librarian", 64, 64);
        assert_eq!(a, b);
    }

    #[test]
    fn different_prompts_differ() {
        let a = placeholder_portrait("alpha", 64, 64);
        let b = placeholder_portrait("bravo", 64, 64);
        assert_ne!(a, b);
    }

    #[test]
    fn fills_exactly_the_requested_buffer() {
        for (w, h) in [(1u32, 1u32), (64, 64), (513, 257)] {
            let px = placeholder_portrait("size", w, h);
            assert_eq!(px.len(), (w * h) as usize, "{w}x{h}");
            assert!(px.iter().all(|p| p[3] == 255), "{w}x{h} must be opaque");
        }
    }

    #[test]
    fn is_not_a_flat_colour_block() {
        let px = placeholder_portrait("varied", 128, 128);
        let mut seen = std::collections::BTreeSet::new();
        for p in &px {
            seen.insert((p[0], p[1], p[2]));
        }
        assert!(seen.len() > 32, "only {} distinct colours", seen.len());
    }

    #[test]
    fn the_head_region_is_not_background() {
        // Centre of the head must differ from the top-left corner.
        let px = placeholder_portrait("face", 200, 200);
        let corner = px[0];
        let centre = px[(100 * 200 + 100) as usize];
        assert_ne!(corner, centre, "the face should be drawn, not blank");
    }
}
