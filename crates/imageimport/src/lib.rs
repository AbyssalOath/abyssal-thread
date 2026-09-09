//! Image -> crochet colorwork grid import.
//!
//! Two independent steps, matching how Stitch Fiddle's "convert picture"
//! flow works (and deliberately fixing the pain point of being stuck with
//! whatever size auto-detect picked):
//!   1. `resize_exact`/`resize_preserving_aspect` - pick the exact output
//!      stitch grid dimensions. Width and height are independent by
//!      default (no forced aspect ratio); "lock aspect" is a UI choice
//!      layered on top, not the library default.
//!   2. `quantize` - reduce to a small palette via k-means, so the result
//!      is a handful of solid colors (one yarn per color) rather than a
//!      noisy photo-realistic gradient.
//!
//! The output `ColorGrid` is a flat rectangular grid worked in rows (see
//! the doc comment on `ColorGrid` in `abyssal-thread-core` for why this is a
//! separate data model from `StitchGraph`).

use abyssal_thread_core::ColorGrid;
use image::{imageops::FilterType, DynamicImage, RgbImage};
use std::path::Path;

pub fn load_image(path: &Path) -> anyhow::Result<DynamicImage> {
    Ok(image::open(path)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeFilter {
    /// Blocky/crisp - good for logos, text, hard-edged graphics (like the
    /// "MERCI POUR LE VENIN" text example: no anti-aliasing haze at small sizes).
    Nearest,
    /// Smoothly blended - better for photos.
    Smooth,
}

impl ResizeFilter {
    fn to_image_filter(self) -> FilterType {
        match self {
            ResizeFilter::Nearest => FilterType::Nearest,
            ResizeFilter::Smooth => FilterType::Triangle,
        }
    }
}

/// Resamples to an EXACT `width` x `height` stitch grid. Always produces
/// precisely the requested dimensions - this is the direct fix for "these
/// are the dimensions that make it clear but I don't want it that big":
/// pick any width/height you want and re-run, independent of the source
/// image's proportions or resolution.
pub fn resize_exact(img: &DynamicImage, width: u32, height: u32, filter: ResizeFilter) -> RgbImage {
    img.resize_exact(width.max(1), height.max(1), filter.to_image_filter())
        .to_rgb8()
}

/// Like `resize_exact`, but derives the omitted dimension from the source
/// image's aspect ratio - used when a "lock aspect" toggle is on and the
/// user only dragged one of the two size sliders.
pub fn resize_preserving_aspect(
    img: &DynamicImage,
    width: Option<u32>,
    height: Option<u32>,
    filter: ResizeFilter,
) -> RgbImage {
    // Clamped to at least 1 before any division below - a genuinely
    // degenerate (0-width or 0-height) source image would otherwise divide
    // by zero, producing NaN/infinity that Rust's saturating float-to-int
    // cast turns into `u32::MAX` rather than panicking, which is worse: a
    // silent attempt to resize to several billion pixels wide/tall (a
    // hang or OOM) instead of a fast, clear failure. `image::open` on a
    // real file essentially never returns 0x0, but "essentially never"
    // isn't "never" for a value driven by untrusted file contents.
    let (src_w, src_h) = (img.width().max(1) as f32, img.height().max(1) as f32);
    let (w, h) = match (width, height) {
        (Some(w), None) => (w, ((w as f32) * src_h / src_w).round().max(1.0) as u32),
        (None, Some(h)) => (((h as f32) * src_w / src_h).round().max(1.0) as u32, h),
        (Some(w), Some(h)) => (w, h),
        (None, None) => (src_w.max(1.0) as u32, src_h.max(1.0) as u32),
    };
    resize_exact(img, w, h, filter)
}

/// Reduces `img` to at most `k` colors via a small fixed-iteration k-means
/// pass. No extra quantization dependency - target grids are small (tens of
/// thousands of pixels at most), so a handful of iterations is instant.
/// Centroids are seeded from evenly spaced pixels (not random), so the same
/// input always reproduces the same palette.
pub fn quantize(img: &RgbImage, k: usize) -> ColorGrid {
    let (width, height) = (img.width() as usize, img.height() as usize);
    let pixels: Vec<[f32; 3]> = img
        .pixels()
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();

    if pixels.is_empty() {
        return ColorGrid {
            width,
            height,
            cells: Vec::new(),
        };
    }
    let k = k.clamp(1, pixels.len());

    let mut centroids = farthest_point_seed(&pixels, k);
    let mut assignments = vec![0usize; pixels.len()];

    const ITERATIONS: usize = 12;
    for _ in 0..ITERATIONS {
        for (i, p) in pixels.iter().enumerate() {
            let mut best = 0;
            let mut best_dist = f32::MAX;
            for (c_idx, c) in centroids.iter().enumerate() {
                let d = dist2(*p, *c);
                if d < best_dist {
                    best_dist = d;
                    best = c_idx;
                }
            }
            assignments[i] = best;
        }

        let mut sums = vec![[0.0f32; 3]; k];
        let mut counts = vec![0u32; k];
        for (p, &a) in pixels.iter().zip(&assignments) {
            sums[a][0] += p[0];
            sums[a][1] += p[1];
            sums[a][2] += p[2];
            counts[a] += 1;
        }
        for c_idx in 0..k {
            if counts[c_idx] > 0 {
                let n = counts[c_idx] as f32;
                centroids[c_idx] = [sums[c_idx][0] / n, sums[c_idx][1] / n, sums[c_idx][2] / n];
            }
        }
    }

    let cells = assignments
        .iter()
        .map(|&a| {
            let c = centroids[a];
            [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8]
        })
        .collect();

    ColorGrid {
        width,
        height,
        cells,
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

/// Deterministic farthest-point ("greedy max-min") seeding: start from the
/// first pixel, then repeatedly add whichever remaining pixel is farthest
/// from every centroid chosen so far. Unlike naive evenly-spaced-index
/// seeding, this reliably finds small/rare color regions (e.g. a few dozen
/// red text pixels on a mostly-black background) instead of every seed
/// landing in the dominant color and never recovering - a real issue we hit
/// testing against a two-tone text image where naive seeding silently
/// dropped the accent color once k > 2.
fn farthest_point_seed(pixels: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    let mut centroids = Vec::with_capacity(k);
    centroids.push(pixels[0]);
    let mut min_dist: Vec<f32> = pixels.iter().map(|p| dist2(*p, centroids[0])).collect();

    while centroids.len() < k {
        let (farthest_idx, _) = min_dist
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .expect("pixels is non-empty");
        let new_centroid = pixels[farthest_idx];
        centroids.push(new_centroid);
        for (i, p) in pixels.iter().enumerate() {
            let d = dist2(*p, new_centroid);
            if d < min_dist[i] {
                min_dist[i] = d;
            }
        }
    }
    centroids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_image(w: u32, h: u32, color: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, image::Rgb(color)))
    }

    #[test]
    fn resize_exact_always_hits_the_requested_dimensions() {
        let img = solid_image(200, 90, [10, 20, 30]);
        let resized = resize_exact(&img, 40, 15, ResizeFilter::Nearest);
        assert_eq!((resized.width(), resized.height()), (40, 15));
    }

    #[test]
    fn resize_preserving_aspect_derives_missing_dimension() {
        let img = solid_image(200, 100, [0, 0, 0]); // 2:1 aspect
        let resized = resize_preserving_aspect(&img, Some(40), None, ResizeFilter::Nearest);
        assert_eq!((resized.width(), resized.height()), (40, 20));
    }

    #[test]
    fn resize_preserving_aspect_handles_a_degenerate_zero_size_source() {
        // A 0x0 source would otherwise divide by zero deriving the missing
        // dimension, producing NaN/infinity that Rust's saturating
        // float-to-int cast turns into `u32::MAX` - this should stay
        // small and finite instead of trying to resize to billions of
        // pixels wide.
        let img = solid_image(0, 0, [0, 0, 0]);
        let resized = resize_preserving_aspect(&img, Some(40), None, ResizeFilter::Nearest);
        assert_eq!(resized.width(), 40);
        assert!(
            resized.height() < 1000,
            "expected a small derived height, got {}",
            resized.height()
        );
    }

    #[test]
    fn quantize_handles_an_empty_image_without_panicking() {
        let img = solid_image(0, 0, [0, 0, 0]).to_rgb8();
        let grid = quantize(&img, 4);
        assert_eq!((grid.width, grid.height), (0, 0));
    }

    #[test]
    fn quantize_a_two_color_image_recovers_both_colors() {
        let mut img = RgbImage::from_pixel(10, 10, image::Rgb([250, 250, 250]));
        for y in 4..6 {
            for x in 0..10 {
                img.put_pixel(x, y, image::Rgb([200, 30, 30]));
            }
        }
        let grid = quantize(&img, 2);
        assert_eq!(grid.width, 10);
        assert_eq!(grid.height, 10);
        assert_eq!(grid.palette().len(), 2);
        // A stripe pixel should end up much closer to red than to white.
        let stripe_color = grid.get(0, 4);
        assert!(stripe_color[0] as i32 > stripe_color[1] as i32 + 50);
    }

    #[test]
    fn quantize_recovers_a_rare_color_on_a_dominant_background() {
        // Regression test: mostly-black image with a small red accent
        // region (~2% of pixels), asking for 3 colors. Naive evenly-spaced
        // seeding missed the red entirely here; farthest-point seeding
        // should find it.
        let mut img = RgbImage::from_pixel(60, 32, image::Rgb([5, 5, 5]));
        for y in 14..18 {
            for x in 10..40 {
                img.put_pixel(x, y, image::Rgb([180, 40, 40]));
            }
        }
        let grid = quantize(&img, 3);
        let has_reddish = grid
            .palette()
            .iter()
            .any(|c| c[0] as i32 > c[1] as i32 + 60);
        assert!(
            has_reddish,
            "expected a reddish color in palette, got {:?}",
            grid.palette()
        );
    }
}
