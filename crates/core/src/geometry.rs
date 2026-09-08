//! Minimal 3D vector math. We intentionally avoid pulling in `nalgebra`/`glam`
//! for now so the core crate stays dependency-light; swap this out later if
//! the layout engine grows more sophisticated (quaternions, mesh relaxation).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn scale(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn distance(self, o: Vec3) -> f32 {
        (self - o).length()
    }

    /// Point on a circle of `radius` in the XZ plane at `angle` radians,
    /// offset by `height` on Y. This is the workhorse for ring/round layout.
    pub fn on_circle(radius: f32, angle: f32, height: f32) -> Vec3 {
        Vec3::new(radius * angle.cos(), height, radius * angle.sin())
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Unit-length copy of this vector; returns itself unchanged if already
    /// (near) zero-length, to avoid propagating NaNs into the 3D viewport's
    /// camera math (see `gui::viewport`).
    pub fn normalized(self) -> Vec3 {
        let len = self.length();
        if len < 1e-6 {
            self
        } else {
            self.scale(1.0 / len)
        }
    }
}

// Addition/subtraction are `+`/`-` via these trait impls, not inherent
// `.add(...)`/`.sub(...)` methods - clippy's `should_implement_trait` lint
// (`geometry.rs`, previously) fires on an inherent method whose name/
// signature matches a trait method *regardless* of whether the trait is
// also implemented separately, so keeping both around was never going to
// satisfy it; the actual fix is to only have the one. Every call site
// across the workspace (`layout::relax`, `gui::viewport`) was updated to
// `+`/`-` accordingly.
impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
