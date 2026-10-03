//! Colour parsing and the album-art accent.

/// Straight (non-premultiplied) RGBA, each channel `0.0..=1.0`.
pub type Rgba = [f32; 4];

/// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA`. The `#` is optional.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let hex = s.trim().trim_start_matches('#');
    if !hex.is_ascii() {
        return None;
    }
    let channel = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .ok()
            .map(|v| f32::from(v) / 255.0)
    };
    match hex.len() {
        3 => {
            let nibble = |i: usize| {
                u8::from_str_radix(&hex[i..=i], 16)
                    .ok()
                    .map(|v| f32::from(v * 17) / 255.0)
            };
            Some([nibble(0)?, nibble(1)?, nibble(2)?, 1.0])
        }
        6 => Some([channel(0)?, channel(2)?, channel(4)?, 1.0]),
        8 => Some([channel(0)?, channel(2)?, channel(4)?, channel(6)?]),
        _ => None,
    }
}

/// `#RRGGBB`, dropping alpha.
pub fn to_hex(c: Rgba) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", byte(c[0]), byte(c[1]), byte(c[2]))
}

pub fn with_alpha(c: Rgba, alpha: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * alpha]
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Hue in degrees, saturation and value in `0.0..=1.0`.
pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let channel = |n: f32| {
        let k = (n + h / 60.0).rem_euclid(6.0);
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    (channel(5.0), channel(3.0), channel(1.0))
}

const HUE_BUCKETS: usize = 12;
/// Roughly how many pixels are sampled, however large the image is.
const SAMPLE_TARGET: usize = 4096;

/// Picks a vivid colour from album art, for text and bars drawn over a dark card.
///
/// `pixels` is BGRA, 4 bytes each. Pixels vote for their hue weighted by how
/// colourful they are, the winning hue's pixels are averaged, and the result
/// is lifted to a readable brightness. Grey artwork returns `fallback`.
pub fn accent_from_bgra(pixels: &[u8], fallback: Rgba) -> Rgba {
    let mut weight = [0.0f32; HUE_BUCKETS];
    let mut sum = [[0.0f32; 3]; HUE_BUCKETS];
    let step = (pixels.len() / 4 / SAMPLE_TARGET).max(1);

    for px in pixels.as_chunks::<4>().0.iter().step_by(step) {
        let (r, g, b) = (
            f32::from(px[2]) / 255.0,
            f32::from(px[1]) / 255.0,
            f32::from(px[0]) / 255.0,
        );
        let (hue, saturation, value) = rgb_to_hsv(r, g, b);
        if saturation < 0.2 || value < 0.2 {
            continue;
        }
        let bucket = (hue / 360.0 * HUE_BUCKETS as f32) as usize % HUE_BUCKETS;
        let w = saturation * value;
        weight[bucket] += w;
        for (acc, channel) in sum[bucket].iter_mut().zip([r, g, b]) {
            *acc += channel * w;
        }
    }

    let best = (0..HUE_BUCKETS)
        .max_by(|&a, &b| weight[a].total_cmp(&weight[b]))
        .unwrap_or(0);
    let sampled = pixels.len() / 4 / step;
    // Under 2% colourful pixels: the art is effectively monochrome.
    if weight[best] <= sampled as f32 * 0.02 {
        return fallback;
    }

    let [r, g, b] = sum[best].map(|channel| channel / weight[best]);
    let (hue, saturation, value) = rgb_to_hsv(r, g, b);
    let (r, g, b) = hsv_to_rgb(hue, saturation.clamp(0.45, 0.85), value.max(0.9));
    [r, g, b, 1.0]
}

/// Box-averages a BGRA image down to `side` by `side` pixels and softens the
/// result. Stretched back up with linear filtering it reads as a heavy blur of
/// the original, at none of the cost of a real one.
pub fn downsample_bgra(pixels: &[u8], w: u32, h: u32, side: u32) -> Vec<u8> {
    let (w, h, side) = (w as usize, h as usize, side as usize);
    if w == 0 || h == 0 || side == 0 || pixels.len() < w * h * 4 {
        return vec![0; side * side * 4];
    }

    let mut cells = vec![[0.0f32; 4]; side * side];
    for (index, cell) in cells.iter_mut().enumerate() {
        let (cx, cy) = (index % side, index / side);
        // A cell always covers at least one source pixel, even when upscaling.
        let (x0, y0) = (cx * w / side, cy * h / side);
        let x1 = ((cx + 1) * w / side).clamp(x0 + 1, w);
        let y1 = ((cy + 1) * h / side).clamp(y0 + 1, h);
        for y in y0..y1 {
            for px in pixels[(y * w + x0) * 4..(y * w + x1) * 4]
                .as_chunks::<4>()
                .0
            {
                for (sum, &channel) in cell.iter_mut().zip(px) {
                    *sum += f32::from(channel);
                }
            }
        }
        let count = ((x1 - x0) * (y1 - y0)) as f32;
        *cell = cell.map(|sum| sum / count);
    }

    // One 3x3 box pass, clamped at the edges, hides the cell boundaries.
    let last = side as isize - 1;
    let at =
        |x: isize, y: isize| cells[(y.clamp(0, last) * side as isize + x.clamp(0, last)) as usize];
    let mut out = Vec::with_capacity(side * side * 4);
    for index in 0..(side * side) as isize {
        let (x, y) = (index % side as isize, index / side as isize);
        for channel in 0..4 {
            let sum: f32 = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| at(x + dx, y + dy)[channel])
                .sum();
            out.push((sum / 9.0).round() as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];

    fn image(bgra: [u8; 4], count: usize) -> Vec<u8> {
        bgra.repeat(count)
    }

    #[test]
    fn parses_the_three_hex_forms() {
        assert_eq!(parse_hex("#FF0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex("00ff00"), Some([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(parse_hex("#fff"), Some(WHITE));
        assert_eq!(parse_hex("#0000FF80").unwrap()[3], 128.0 / 255.0);
    }

    #[test]
    fn rejects_malformed_hex() {
        for bad in ["", "#12", "#GGGGGG", "auto", "#12345", "#ééé"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(to_hex(parse_hex("#1DB954").unwrap()), "#1DB954");
    }

    #[test]
    fn hsv_round_trips() {
        for (r, g, b) in [(1.0, 0.0, 0.0), (0.2, 0.6, 0.4), (0.1, 0.1, 0.9)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((r - r2).abs() < 1e-5 && (g - g2).abs() < 1e-5 && (b - b2).abs() < 1e-5);
        }
    }

    #[test]
    fn accent_follows_the_dominant_colourful_hue() {
        // Mostly dark grey with a red patch: red should still win.
        let mut pixels = image([40, 40, 40, 255], 3000);
        pixels.extend(image([20, 30, 200, 255], 1000));
        let accent = accent_from_bgra(&pixels, WHITE);
        assert!(accent[0] > 0.8 && accent[0] > accent[1] * 2.0 && accent[0] > accent[2] * 2.0);
    }

    #[test]
    fn accent_is_lifted_to_a_readable_brightness() {
        let accent = accent_from_bgra(&image([90, 20, 20, 255], 1000), WHITE);
        assert!(accent[2] >= 0.89, "dark blue stayed dark: {accent:?}");
    }

    #[test]
    fn grey_or_empty_art_falls_back() {
        assert_eq!(
            accent_from_bgra(&image([128, 128, 128, 255], 500), WHITE),
            WHITE
        );
        assert_eq!(accent_from_bgra(&[], WHITE), WHITE);
    }
    #[test]
    fn downsampling_keeps_a_flat_image_flat() {
        let small = downsample_bgra(&image([10, 20, 30, 255], 64 * 64), 64, 64, 8);
        assert_eq!(small.len(), 8 * 8 * 4);
        assert!(
            small
                .as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [10, 20, 30, 255])
        );
    }

    #[test]
    fn downsampling_keeps_left_and_right_apart() {
        // Left half blue, right half red, 32 pixels wide and 2 tall.
        let row: Vec<u8> = (0..32)
            .flat_map(|x| {
                if x < 16 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        let small = downsample_bgra(&row.repeat(2), 32, 2, 8);
        assert!(small[0] > 200 && small[2] < 50, "left edge should be blue");
        assert!(
            small[7 * 4] < 50 && small[7 * 4 + 2] > 200,
            "right edge should be red"
        );
    }

    #[test]
    fn downsampling_survives_bad_input() {
        assert_eq!(downsample_bgra(&[], 0, 0, 4), vec![0; 64]);
        assert_eq!(downsample_bgra(&[1, 2, 3], 10, 10, 2), vec![0; 16]);
        // Upscaling a single pixel repeats it.
        assert_eq!(
            downsample_bgra(&[9, 8, 7, 255], 1, 1, 2),
            [9, 8, 7, 255].repeat(4)
        );
    }
}
