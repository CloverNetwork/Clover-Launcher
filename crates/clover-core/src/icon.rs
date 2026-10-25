//! Each instance's 8×8 block face, generated from its ID, as a PNG for OS shortcuts. The
//! same algorithm as the interface's BlockIcon.svelte, so an instance looks the same in the
//! launcher and in the Dock or app launcher.

use crate::error::{Result, msg};

struct Material {
    top: Option<&'static [u32]>,
    body: &'static [u32],
}

const DIRT: &[u32] = &[0x6b4a2f, 0x7d5838, 0x8f6a46];

const MATERIALS: &[Material] = &[
    Material { top: Some(&[0x4f8a2b, 0x5f9e32, 0x6fb33a]), body: DIRT }, // grass
    Material { top: None, body: &[0x6f6f6f, 0x7f7f7f, 0x8f8f8f, 0x9a9a9a] }, // stone
    Material { top: None, body: &[0xcfc08a, 0xd8cb98, 0xe0d4a6, 0xe8dcb0] }, // sand
    Material { top: None, body: &[0x36363b, 0x3f3f45, 0x4a4a50, 0x55555b] }, // deepslate
    Material { top: None, body: &[0xb8673f, 0xc77447, 0xd98a5a, 0x5aa38a] }, // copper
    Material { top: None, body: &[0x7a52b0, 0x8a63c4, 0x9c78d4, 0xc4a6ec] }, // amethyst
    Material { top: None, body: &[0xd48aa9, 0xe6a5c0, 0xf0b8cf, 0xf7cde0] }, // cherry
    Material { top: Some(&[0xe8eef2, 0xf4f8fa, 0xffffff]), body: DIRT }, // snowy
];

/// FNV-1a over UTF-16 code units, as JavaScript's charCodeAt sees the string.
fn hash(text: &str) -> u32 {
    text.encode_utf16().fold(0x811c9dc5u32, |h, unit| (h ^ unit as u32).wrapping_mul(0x01000193))
}

/// The 64 pixel colours, row by row.
pub fn pixels(seed: &str) -> Vec<u32> {
    let mut state = hash(seed);
    let material = &MATERIALS[state as usize % MATERIALS.len()];
    let mut next = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state as f64 / 4294967296.0
    };
    let mut out = Vec::with_capacity(64);
    for y in 0..8 {
        for _x in 0..8 {
            // Grass and snow overhang the top two rows and drip unevenly into the third.
            let capped = material.top.is_some() && (y < 2 || (y == 2 && next() < 0.45));
            let tones = if capped { material.top.unwrap() } else { material.body };
            out.push(tones[(next() * tones.len() as f64) as usize]);
        }
    }
    out
}

/// The icon as a square PNG, each block pixel scaled up crisply.
pub fn png(seed: &str, size: u32) -> Result<Vec<u8>> {
    let size = size.max(8) / 8 * 8;
    let block = size / 8;
    let colours = pixels(seed);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let c = colours[((y / block) * 8 + x / block) as usize];
            rgba.extend([(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xff]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| msg(format!("Couldn't make the icon: {e}")))?;
    writer.write_image_data(&rgba).map_err(|e| msg(format!("Couldn't make the icon: {e}")))?;
    writer.finish().map_err(|e| msg(format!("Couldn't make the icon: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pixels BlockIcon.svelte draws for these seeds, produced by running its code in Node.
    #[test]
    fn matches_the_interface_icon_exactly() {
        let cases = [
            ("survival-a1b2c3", "f7cde0,d48aa9,f0b8cf,f7cde0,d48aa9,e6a5c0,d48aa9,e6a5c0,f7cde0,d48aa9,f7cde0,d48aa9,e6a5c0,e6a5c0,f7cde0,f0b8cf,e6a5c0,f0b8cf,d48aa9,f0b8cf,d48aa9,f0b8cf,f0b8cf,d48aa9,e6a5c0,e6a5c0,f7cde0,e6a5c0,f0b8cf,d48aa9,f0b8cf,f0b8cf,d48aa9,d48aa9,d48aa9,f0b8cf,f0b8cf,d48aa9,d48aa9,d48aa9,f0b8cf,f0b8cf,f7cde0,d48aa9,f7cde0,f0b8cf,e6a5c0,d48aa9,f0b8cf,d48aa9,f7cde0,e6a5c0,f0b8cf,f0b8cf,f0b8cf,f0b8cf,d48aa9,e6a5c0,f0b8cf,f0b8cf,e6a5c0,d48aa9,f7cde0,e6a5c0"),
            ("neo-pack-9f1e2d", "55555b,3f3f45,36363b,55555b,4a4a50,36363b,4a4a50,55555b,55555b,4a4a50,4a4a50,55555b,55555b,36363b,36363b,55555b,55555b,36363b,3f3f45,3f3f45,36363b,3f3f45,4a4a50,55555b,36363b,3f3f45,3f3f45,4a4a50,4a4a50,36363b,3f3f45,55555b,36363b,4a4a50,55555b,4a4a50,36363b,4a4a50,55555b,4a4a50,36363b,55555b,4a4a50,4a4a50,36363b,4a4a50,36363b,4a4a50,36363b,4a4a50,3f3f45,55555b,36363b,4a4a50,3f3f45,55555b,4a4a50,3f3f45,4a4a50,55555b,3f3f45,4a4a50,55555b,4a4a50"),
        ];
        for (seed, expected) in cases {
            let ours: Vec<String> = pixels(seed).iter().map(|c| format!("{c:x}")).collect();
            assert_eq!(ours.join(","), expected, "{seed}");
        }
    }

    #[test]
    fn encodes_a_png() {
        let bytes = png("survival-a1b2c3", 256).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 256);
    }
}
