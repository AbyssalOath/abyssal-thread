//! Approximate color naming: maps an arbitrary RGB color to the closest
//! entry in a small, curated set of common named colors, for a
//! human-readable legend alongside the exact hex code.
//!
//! This is nearest-match against a curated (not exhaustive) list, not a
//! precise color-identification system - two different hex codes can map
//! to the same name, and the "closest" name is sometimes a stretch for
//! colors that fall between named hues. It's meant to make a legend
//! skimmable ("B = #b43535 (~ Crimson)"), not to be authoritative about
//! what a yarn's actual color name is.

const NAMED_COLORS: &[(&str, [u8; 3])] = &[
    ("Black", [0, 0, 0]),
    ("Charcoal", [54, 54, 54]),
    ("Dark Gray", [90, 90, 90]),
    ("Gray", [128, 128, 128]),
    ("Silver", [192, 192, 192]),
    ("Light Gray", [211, 211, 211]),
    ("White", [255, 255, 255]),
    ("Ivory", [255, 255, 240]),
    ("Cream", [255, 253, 208]),
    ("Beige", [245, 245, 220]),
    ("Tan", [210, 180, 140]),
    ("Khaki", [195, 176, 145]),
    ("Brown", [101, 67, 33]),
    ("Chocolate", [123, 63, 0]),
    ("Maroon", [128, 0, 0]),
    ("Dark Red", [139, 0, 0]),
    ("Red", [220, 20, 20]),
    ("Crimson", [180, 30, 55]),
    ("Salmon", [250, 128, 114]),
    ("Coral", [255, 127, 80]),
    ("Peach", [255, 218, 185]),
    ("Pink", [255, 192, 203]),
    ("Hot Pink", [255, 105, 180]),
    ("Rose", [244, 194, 194]),
    ("Magenta", [220, 0, 220]),
    ("Plum", [142, 69, 133]),
    ("Purple", [128, 0, 128]),
    ("Lavender", [180, 160, 220]),
    ("Indigo", [75, 0, 130]),
    ("Navy", [0, 0, 100]),
    ("Blue", [30, 60, 200]),
    ("Royal Blue", [65, 105, 225]),
    ("Sky Blue", [135, 206, 235]),
    ("Denim", [90, 120, 160]),
    ("Teal", [0, 128, 128]),
    ("Turquoise", [64, 200, 190]),
    ("Cyan", [0, 220, 220]),
    ("Mint", [180, 230, 200]),
    ("Green", [30, 140, 30]),
    ("Forest Green", [34, 90, 34]),
    ("Dark Green", [10, 60, 10]),
    ("Olive", [110, 110, 20]),
    ("Lime", [180, 230, 40]),
    ("Gold", [212, 175, 55]),
    ("Mustard", [200, 165, 40]),
    ("Yellow", [230, 220, 30]),
    ("Orange", [235, 130, 30]),
    ("Dark Orange", [200, 90, 10]),
    // Muted/desaturated tones - photo-derived palettes land here often
    // (skin tones, worn fabric, shadows), and without these the nearest
    // *saturated* color can be a poor perceptual match even when it's the
    // closest by raw RGB distance (a real case we hit: a muted brick-red
    // #7c494b matched "Dark Gray" before these were added).
    ("Dusty Rose", [200, 150, 150]),
    ("Brick", [150, 70, 60]),
    ("Terracotta", [190, 100, 80]),
    ("Rust", [140, 70, 40]),
    ("Mauve", [150, 110, 120]),
    ("Slate", [110, 120, 130]),
    ("Taupe", [140, 130, 120]),
];

/// Nearest named color by squared Euclidean RGB distance.
pub fn nearest_color_name(c: [u8; 3]) -> &'static str {
    let [r, g, b] = [c[0] as i32, c[1] as i32, c[2] as i32];
    NAMED_COLORS
        .iter()
        .min_by_key(|(_, [nr, ng, nb])| {
            let (dr, dg, db) = (r - *nr as i32, g - *ng as i32, b - *nb as i32);
            dr * dr + dg * dg + db * db
        })
        .map(|(name, _)| *name)
        .unwrap_or("Unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_exact_matches() {
        assert_eq!(nearest_color_name([0, 0, 0]), "Black");
        assert_eq!(nearest_color_name([255, 255, 255]), "White");
    }

    #[test]
    fn picks_a_plausible_neighbor_for_an_inexact_color() {
        // Slightly off-black should still read as Black, not something wild.
        assert_eq!(nearest_color_name([12, 11, 10]), "Black");
        // A clear crimson-ish red should not come back as blue/green/etc.
        let name = nearest_color_name([180, 35, 55]);
        assert!(
            name == "Crimson" || name == "Dark Red" || name == "Red",
            "expected a red-family name, got {name}"
        );
    }
}
