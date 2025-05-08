use std::ops::{Add, AddAssign, Div, Mul};

use hydrolox_pga3d::prelude as pga;
use serde::{Deserialize, Serialize};
use vulkano::buffer::BufferContents;

#[derive(
    Debug, Default, Clone, Copy, PartialEq, PartialOrd, BufferContents, Serialize, Deserialize,
)]
#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const MIN: Self = Self::new(f32::MIN, f32::MIN, f32::MIN);
    pub const MAX: Self = Self::new(f32::MAX, f32::MAX, f32::MAX);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub const fn dot(&self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    pub const fn cross(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn min_components(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }
    pub fn max_components(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }

    pub const fn magnitude_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    pub fn magnitude(&self) -> f32 {
        self.magnitude_squared().sqrt()
    }

    pub fn normalized(&self) -> Vec3 {
        *self / self.magnitude()
    }
}
impl Add<Vec3> for Vec3 {
    type Output = Vec3;

    fn add(self, rhs: Vec3) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl AddAssign<Vec3> for Vec3 {
    fn add_assign(&mut self, rhs: Vec3) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}
impl Div<f32> for Vec3 {
    type Output = Vec3;
    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}
impl From<pga::Point> for Vec3 {
    fn from(value: pga::Point) -> Self {
        let scaled = value.scaled();
        Self::new(scaled.x, scaled.y, scaled.z)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}
impl AABB {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }
    pub fn from_verts<I: IntoIterator<Item = Vec3>>(verts: I) -> Self {
        let mut iter = verts.into_iter();

        if let Some(first) = iter.next() {
            let mut min = first;
            let mut max = first;

            for vert in iter {
                min.x = min.x.min(vert.x);
                min.y = min.y.min(vert.y);
                min.z = min.z.min(vert.z);

                max.x = max.x.max(vert.x);
                max.y = max.y.max(vert.y);
                max.z = max.z.max(vert.z);
            }

            Self::new(min, max)
        } else {
            Self::default()
        }
    }
}
