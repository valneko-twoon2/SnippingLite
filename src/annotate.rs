//! Pen and highlighter marks drawn on top of a stored screenshot.

/// A freehand stroke in image pixel coordinates.
#[derive(Clone, Debug)]
pub struct Mark {
    pub highlight: bool,
    pub color: [u8; 3],
    /// Brush radius in image pixels.
    pub radius: f32,
    pub points: Vec<(f32, f32)>,
}

/// Colors offered for strokes (RGB).
pub const COLORS: [[u8; 3]; 5] = [
    [0xe8, 0x11, 0x23],
    [0xff, 0xe6, 0x00],
    [0x16, 0xc6, 0x0c],
    [0x00, 0x78, 0xd4],
    [0x10, 0x10, 0x10],
];
/// Brush radii for the pen and the highlighter, thin to thick.
pub const PEN_RADII: [f32; 3] = [1.5, 2.5, 4.5];
pub const HIGHLIGHTER_RADII: [f32; 3] = [6.0, 10.0, 16.0];

/// Border widths in image pixels for screenshots, thin to thick.
pub const OUTLINE_WIDTHS: [i32; 3] = [1, 2, 4];

/// Paints an opaque border of `border` pixels just inside the edges of top-down
/// BGRA pixels. A border wider than half the image covers all of it.
pub fn outline(pixels: &mut [u8], width: i32, height: i32, border: i32, color: [u8; 3]) {
    if width <= 0
        || height <= 0
        || border <= 0
        || pixels.len() != width as usize * height as usize * 4
    {
        return;
    }
    let value = [color[2], color[1], color[0], 255];
    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (index as i32 % width, index as i32 / width);
        if x < border || y < border || x >= width - border || y >= height - border {
            *pixel = value;
        }
    }
}

/// The image pixel under a point given in pixels from the image's top-left corner.
/// Rounding goes down, so a point just left of or above the image is outside it.
pub fn pixel_of(x: f32, y: f32) -> (i32, i32) {
    (x.floor() as i32, y.floor() as i32)
}

/// The color of a pixel of top-down BGRA pixels, or `None` outside the image.
pub fn color_at(pixels: &[u8], width: i32, height: i32, (x, y): (i32, i32)) -> Option<[u8; 3]> {
    if x < 0 || y < 0 || x >= width || y >= height {
        return None;
    }
    let at = (y as usize * width as usize + x as usize) * 4;
    let pixel = pixels.get(at..at + 4)?;
    Some([pixel[2], pixel[1], pixel[0]])
}

/// A color as `#RRGGBB`.
pub fn hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// The highlighter is translucent, the pen is opaque.
pub fn opacity(highlight: bool) -> f32 {
    if highlight {
        0.45
    } else {
        1.0
    }
}

/// Cuts the area out of top-down BGRA pixels; the area is clipped to the image.
pub fn crop(
    pixels: &[u8],
    width: i32,
    height: i32,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
) -> Option<(i32, i32, Vec<u8>)> {
    let (x0, y0) = (x0.clamp(0, width), y0.clamp(0, height));
    let (x1, y1) = (x1.clamp(0, width), y1.clamp(0, height));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let row = (x1 - x0) as usize * 4;
    let mut out = Vec::with_capacity(row * (y1 - y0) as usize);
    for y in y0..y1 {
        let start = (y as usize * width as usize + x0 as usize) * 4;
        out.extend_from_slice(&pixels[start..start + row]);
    }
    Some((x1 - x0, y1 - y0, out))
}

/// Draws the marks onto straight-alpha top-down BGRA pixels.
pub fn draw(pixels: &mut [u8], width: i32, height: i32, marks: &[Mark]) {
    for mark in marks {
        let opacity = opacity(mark.highlight);
        stroke(
            pixels,
            width,
            height,
            &mark.points,
            mark.color,
            opacity,
            mark.radius,
        );
    }
}

fn stroke(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    points: &[(f32, f32)],
    color: [u8; 3],
    opacity: f32,
    radius: f32,
) {
    if points.is_empty() || width <= 0 || height <= 0 {
        return;
    }
    // Coverage is collected first so overlapping stamps of one stroke are not blended twice.
    let mut coverage = vec![0u8; width as usize * height as usize];
    let mut stamp = |cx: f32, cy: f32| {
        let reach = radius + 1.0;
        let x0 = ((cx - reach).floor() as i32).max(0);
        let x1 = ((cx + reach).ceil() as i32).min(width - 1);
        let y0 = ((cy - reach).floor() as i32).max(0);
        let y1 = ((cy + reach).ceil() as i32).min(height - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let distance =
                    ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let value = ((radius + 0.5 - distance).clamp(0.0, 1.0) * 255.0) as u8;
                let slot = &mut coverage[y as usize * width as usize + x as usize];
                *slot = (*slot).max(value);
            }
        }
    };
    let step = (radius / 2.0).max(0.5);
    let mut last = points[0];
    stamp(last.0, last.1);
    for &next in &points[1..] {
        let (dx, dy) = (next.0 - last.0, next.1 - last.1);
        let steps = ((dx * dx + dy * dy).sqrt() / step).ceil().max(1.0) as u32;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            stamp(last.0 + dx * t, last.1 + dy * t);
        }
        last = next;
    }
    for (pixel, &cover) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(&coverage) {
        if cover == 0 {
            continue;
        }
        let source = opacity * cover as f32 / 255.0;
        let behind = pixel[3] as f32 / 255.0;
        let alpha = source + behind * (1.0 - source);
        // The pixels are BGRA while the color is RGB.
        for (channel, value) in pixel[..3].iter_mut().zip(color.iter().rev()) {
            let mixed =
                (*value as f32 * source + *channel as f32 * behind * (1.0 - source)) / alpha;
            *channel = mixed.round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighter_blends_once_and_pen_covers() {
        let mut pixels = vec![255u8; 20 * 20 * 4];
        // A stroke that doubles back over itself must not darken twice.
        let marks = [Mark {
            highlight: true,
            color: [0xff, 0xe6, 0x00],
            radius: 10.0,
            points: vec![(5.0, 10.0), (15.0, 10.0), (5.0, 10.0)],
        }];
        draw(&mut pixels, 20, 20, &marks);
        let center = (10 * 20 + 10) * 4;
        // 45% of the blue channel (0) over white: 255 * 0.55.
        assert_eq!(pixels[center], 140);
        assert_eq!(pixels[center + 3], 255);
        let marks = [Mark {
            highlight: false,
            color: [0xe8, 0x11, 0x23],
            radius: 2.5,
            points: vec![(2.0, 2.0)],
        }];
        draw(&mut pixels, 20, 20, &marks);
        assert_eq!(&pixels[(2 * 20 + 2) * 4..][..4], &[0x23, 0x11, 0xe8, 255]);
        // Cropping clips to the image and rejects empty areas.
        let (w, h, out) = crop(&pixels, 20, 20, (-5, 0, 4, 3)).unwrap();
        assert_eq!((w, h, out.len()), (4, 3, 48));
        assert!(crop(&pixels, 20, 20, (5, 5, 5, 9)).is_none());
    }

    fn painted(width: i32, height: i32, border: i32) -> Vec<bool> {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        outline(&mut pixels, width, height, border, [1, 2, 3]);
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                if *p != [0, 0, 0, 0] {
                    assert_eq!(*p, [3, 2, 1, 255], "BGRA order, opaque");
                    true
                } else {
                    false
                }
            })
            .collect()
    }

    #[test]
    fn outline_paints_exactly_the_edge_pixels() {
        // Equivalence: a 6x5 image with a 1 pixel border leaves a 4x3 inside.
        let marked = painted(6, 5, 1);
        for y in 0..5 {
            for x in 0..6 {
                let edge = x == 0 || y == 0 || x == 5 || y == 4;
                assert_eq!(marked[y * 6 + x], edge, "{x},{y}");
            }
        }
        // Every width of the settings keeps the corners and the center apart.
        for border in OUTLINE_WIDTHS {
            let marked = painted(40, 30, border);
            let b = border as usize;
            assert!(marked[0] && marked[40 * 30 - 1]);
            assert!(marked[b - 1 + 40 * 15] && !marked[b + 40 * 15]);
            assert!(!marked[20 + 15 * 40]);
            assert_eq!(
                marked.iter().filter(|m| **m).count(),
                40 * 30 - (40 - 2 * b) * (30 - 2 * b)
            );
        }
    }

    #[test]
    fn color_at_reads_rgb_and_stops_at_the_edges() {
        // 3x2 image; each pixel is BGRA with a distinct color.
        let pixels: Vec<u8> = (0..6u8).flat_map(|i| [i, i + 10, i + 20, 255]).collect();
        assert_eq!(color_at(&pixels, 3, 2, (0, 0)), Some([20, 10, 0]));
        assert_eq!(color_at(&pixels, 3, 2, (2, 0)), Some([22, 12, 2]));
        assert_eq!(color_at(&pixels, 3, 2, (0, 1)), Some([23, 13, 3]));
        assert_eq!(color_at(&pixels, 3, 2, (2, 1)), Some([25, 15, 5]));
        // Boundary: one step outside on every side.
        for outside in [
            (-1, 0),
            (0, -1),
            (3, 0),
            (0, 2),
            (3, 2),
            (i32::MIN, 0),
            (0, i32::MAX),
        ] {
            assert_eq!(color_at(&pixels, 3, 2, outside), None, "{outside:?}");
        }
        // Errors: an empty image, a zero size and pixels shorter than the size claims.
        assert_eq!(color_at(&[], 0, 0, (0, 0)), None);
        assert_eq!(color_at(&pixels, 0, 2, (0, 0)), None);
        assert_eq!(color_at(&pixels[..20], 3, 2, (2, 1)), None);
    }

    #[test]
    fn pixel_of_rounds_down_so_the_left_edge_stays_outside() {
        assert_eq!(pixel_of(0.0, 0.0), (0, 0));
        assert_eq!(pixel_of(0.99, 1.0), (0, 1));
        assert_eq!(pixel_of(-0.01, -0.5), (-1, -1));
        assert_eq!(pixel_of(2.5, 7.999), (2, 7));
    }

    #[test]
    fn hex_has_six_uppercase_digits() {
        assert_eq!(hex([0, 0, 0]), "#000000");
        assert_eq!(hex([255, 255, 255]), "#FFFFFF");
        assert_eq!(hex([0xe8, 0x11, 0x23]), "#E81123");
        assert_eq!(hex([1, 10, 171]), "#010AAB");
    }

    #[test]
    fn outline_handles_tiny_and_invalid_input() {
        // Boundary: the border meets in the middle, then exceeds the image.
        assert!(painted(4, 4, 2).iter().all(|m| *m));
        assert!(painted(4, 4, 3).iter().all(|m| *m));
        assert!(painted(1, 1, 1)[0]);
        assert_eq!(painted(5, 1, 1), vec![true; 5]);
        // Errors: nothing changes for a zero, negative or mismatched request.
        for (w, h, border, len) in [
            (4, 4, 0, 64),
            (4, 4, -1, 64),
            (0, 4, 1, 0),
            (4, 0, 1, 0),
            (4, 4, 1, 60),
        ] {
            let mut pixels = vec![9u8; len];
            outline(&mut pixels, w, h, border, [1, 2, 3]);
            assert!(pixels.iter().all(|p| *p == 9), "{w}x{h} border {border}");
        }
    }
}
