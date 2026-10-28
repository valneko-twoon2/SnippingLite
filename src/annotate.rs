//! Pen and highlighter marks drawn on top of a stored screenshot.

/// A freehand stroke in image pixel coordinates.
#[derive(Clone, Debug)]
pub struct Mark {
    pub highlight: bool,
    pub points: Vec<(f32, f32)>,
}

/// Pen strokes are opaque red; the highlighter is a wide translucent yellow.
const PEN: ([u8; 3], f32, f32) = ([0xe8, 0x11, 0x23], 1.0, 2.5);
const HIGHLIGHTER: ([u8; 3], f32, f32) = ([0xff, 0xe6, 0x00], 0.45, 10.0);

/// Brush color (RGB), opacity and radius in image pixels.
pub fn brush(highlight: bool) -> ([u8; 3], f32, f32) {
    if highlight {
        HIGHLIGHTER
    } else {
        PEN
    }
}

/// Draws the marks onto straight-alpha top-down BGRA pixels.
pub fn draw(pixels: &mut [u8], width: i32, height: i32, marks: &[Mark]) {
    for mark in marks {
        let (color, opacity, radius) = brush(mark.highlight);
        stroke(pixels, width, height, &mark.points, color, opacity, radius);
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
            points: vec![(5.0, 10.0), (15.0, 10.0), (5.0, 10.0)],
        }];
        draw(&mut pixels, 20, 20, &marks);
        let center = (10 * 20 + 10) * 4;
        // 45% of the blue channel (0) over white: 255 * 0.55.
        assert_eq!(pixels[center], 140);
        assert_eq!(pixels[center + 3], 255);
        let marks = [Mark {
            highlight: false,
            points: vec![(2.0, 2.0)],
        }];
        draw(&mut pixels, 20, 20, &marks);
        assert_eq!(&pixels[(2 * 20 + 2) * 4..][..4], &[0x23, 0x11, 0xe8, 255]);
    }
}
