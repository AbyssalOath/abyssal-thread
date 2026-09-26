//! Perceptual color matching: sRGB -> CIELAB, and CIEDE2000 color
//! difference between two Lab colors.
//!
//! Matching a photo's colors to real floss by plain RGB distance (what
//! `abyssal_thread_export::nearest_color_name` does for its approximate
//! names) picks visibly wrong threads - RGB distance treats a step in
//! blue the same as a step in green, while the eye doesn't. CIEDE2000 is
//! the standard "how different do these two colors look" formula, and is
//! what the floss catalog lookup in `threads.rs` ranks by.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB (D65) -> CIELAB (D65 reference white).
pub fn rgb_to_lab(rgb: [u8; 3]) -> Lab {
    let r = srgb_to_linear(rgb[0]);
    let g = srgb_to_linear(rgb[1]);
    let b = srgb_to_linear(rgb[2]);
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIEDE2000 color difference (kL = kC = kH = 1). Computed in f64
/// internally - the formula has several near-cancelling terms (hue angle
/// wraparound in particular) that f32 visibly loses precision on.
pub fn delta_e_2000(c1: Lab, c2: Lab) -> f32 {
    use std::f64::consts::PI;
    let (l1, a1, b1) = (c1.l as f64, c1.a as f64, c1.b as f64);
    let (l2, a2, b2) = (c2.l as f64, c2.a as f64, c2.b as f64);

    let c1_ab = (a1 * a1 + b1 * b1).sqrt();
    let c2_ab = (a2 * a2 + b2 * b2).sqrt();
    let c_bar = (c1_ab + c2_ab) / 2.0;
    let c_bar7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c_bar7 / (c_bar7 + 25f64.powi(7))).sqrt());
    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = (a1p * a1p + b1 * b1).sqrt();
    let c2p = (a2p * a2p + b2 * b2).sqrt();

    let hue = |b: f64, ap: f64| {
        if b == 0.0 && ap == 0.0 {
            0.0
        } else {
            let h = b.atan2(ap).to_degrees();
            if h < 0.0 {
                h + 360.0
            } else {
                h
            }
        }
    };
    let h1p = hue(b1, a1p);
    let h2p = hue(b2, a2p);

    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dh_deg = if c1p * c2p == 0.0 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let dh = 2.0 * (c1p * c2p).sqrt() * (dh_deg.to_radians() / 2.0).sin();

    let l_bar = (l1 + l2) / 2.0;
    let c_bar_p = (c1p + c2p) / 2.0;
    let h_bar_p = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };

    let t = 1.0 - 0.17 * (h_bar_p - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar_p).to_radians().cos()
        + 0.32 * (3.0 * h_bar_p + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar_p - 63.0).to_radians().cos();
    let d_theta = 30.0 * (-((h_bar_p - 275.0) / 25.0).powi(2)).exp();
    let c_bar_p7 = c_bar_p.powi(7);
    let r_c = 2.0 * (c_bar_p7 / (c_bar_p7 + 25f64.powi(7))).sqrt();
    let l_term = (l_bar - 50.0).powi(2);
    let s_l = 1.0 + 0.015 * l_term / (20.0 + l_term).sqrt();
    let s_c = 1.0 + 0.045 * c_bar_p;
    let s_h = 1.0 + 0.015 * c_bar_p * t;
    let r_t = -(2.0 * d_theta * PI / 180.0).sin() * r_c;

    let (tl, tc, th) = (dl / s_l, dc / s_c, dh / s_h);
    (tl * tl + tc * tc + th * th + r_t * tc * th).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab(l: f32, a: f32, b: f32) -> Lab {
        Lab { l, a, b }
    }

    /// Reference pairs from Sharma, Wu & Dalal, "The CIEDE2000
    /// Color-Difference Formula: Implementation Notes, Supplementary Test
    /// Data, and Mathematical Observations" (2005) - the standard test set
    /// for this formula, chosen specifically to exercise its edge cases.
    #[test]
    fn matches_sharma_reference_values() {
        let cases = [
            (
                lab(50.0, 2.6772, -79.7751),
                lab(50.0, 0.0, -82.7485),
                2.0425,
            ),
            (
                lab(50.0, 3.1571, -77.2803),
                lab(50.0, 0.0, -82.7485),
                2.8615,
            ),
            (
                lab(50.0, 2.8361, -74.0200),
                lab(50.0, 0.0, -82.7485),
                3.4412,
            ),
            (lab(50.0, 0.0, 0.0), lab(50.0, -1.0, 2.0), 2.3669),
            (lab(50.0, 2.5, 0.0), lab(73.0, 25.0, -18.0), 27.1492),
            (lab(50.0, 2.5, 0.0), lab(50.0, 3.1736, 0.5854), 1.0),
            (lab(50.0, 2.5, 0.0), lab(50.0, 3.2972, 0.0), 1.0),
            (lab(50.0, 2.5, 0.0), lab(50.0, 1.8634, 0.5757), 1.0),
            (lab(50.0, 2.5, 0.0), lab(50.0, 3.2592, 0.3350), 1.0),
        ];
        for (a, b, want) in cases {
            let got = delta_e_2000(a, b);
            assert!(
                (got - want).abs() < 1e-3,
                "{a:?} vs {b:?}: got {got}, want {want}"
            );
            let back = delta_e_2000(b, a);
            assert!((back - got).abs() < 1e-4, "not symmetric");
        }
    }

    #[test]
    fn rgb_to_lab_hits_known_anchor_points() {
        let white = rgb_to_lab([255, 255, 255]);
        assert!((white.l - 100.0).abs() < 0.05 && white.a.abs() < 0.05 && white.b.abs() < 0.05);
        let black = rgb_to_lab([0, 0, 0]);
        assert!(black.l.abs() < 0.01);
        // sRGB red is roughly L=53.24, a=80.09, b=67.20.
        let red = rgb_to_lab([255, 0, 0]);
        assert!((red.l - 53.24).abs() < 0.1);
        assert!((red.a - 80.09).abs() < 0.2);
        assert!((red.b - 67.20).abs() < 0.2);
    }
}
