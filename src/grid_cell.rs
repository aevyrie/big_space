//! Contains the grid cell implementation

use bevy::prelude::*;

use crate::precision::GridPrecision;

/// The cell index an entity within a [`crate::ReferenceFrame`]'s grid. The [`Transform`] of an
/// entity with this component is a transformation from the center of this cell.
///
/// This component adds precision to the translation of an entity's [`Transform`]. In a
/// high-precision [`big_space`](crate) world, the position of an entity is described by a
/// [`Transform`] *and* a [`GridCell`]. This component is the index of a cell inside a large grid
/// defined by a reference frame, and the transform is the position of the entity relative to the
/// center of that cell.
#[derive(Component, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Reflect)]
#[reflect(Component, Default, PartialEq)]
pub struct GridCell<P: GridPrecision> {
    /// The x-index of the cell.
    pub x: P,
    /// The y-index of the cell.
    pub y: P,
    /// The z-index of the cell.
    pub z: P,
}

impl<P: GridPrecision> GridCell<P> {
    /// Construct a new [`GridCell`].
    pub fn new(x: P, y: P, z: P) -> Self {
        Self { x, y, z }
    }

    /// The origin [`GridCell`].
    pub const ZERO: Self = GridCell {
        x: P::ZERO,
        y: P::ZERO,
        z: P::ZERO,
    };

    /// A unit value [`GridCell`]. Useful for offsets.
    pub const ONE: Self = GridCell {
        x: P::ONE,
        y: P::ONE,
        z: P::ONE,
    };
}
impl<P: GridPrecision> std::ops::Add for GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add(rhs.x),
            y: self.y.wrapping_add(rhs.y),
            z: self.z.wrapping_add(rhs.z),
        }
    }
}
impl<P: GridPrecision> std::ops::Sub for GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_sub(rhs.x),
            y: self.y.wrapping_sub(rhs.y),
            z: self.z.wrapping_sub(rhs.z),
        }
    }
}
impl<P: GridPrecision> std::ops::Add for &GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        (*self).add(*rhs)
    }
}
impl<P: GridPrecision> std::ops::Sub for &GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl<P: GridPrecision> std::ops::AddAssign for GridCell<P> {
    fn add_assign(&mut self, rhs: Self) {
        use std::ops::Add;
        *self = self.add(rhs);
    }
}

impl<P: GridPrecision> std::ops::SubAssign for GridCell<P> {
    fn sub_assign(&mut self, rhs: Self) {
        use std::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl<P: GridPrecision> std::ops::Mul<P> for GridCell<P> {
    type Output = GridCell<P>;

    fn mul(self, rhs: P) -> Self::Output {
        GridCell {
            x: self.x.mul(rhs),
            y: self.y.mul(rhs),
            z: self.z.mul(rhs),
        }
    }
}

impl<P: GridPrecision> std::ops::Mul<P> for &GridCell<P> {
    type Output = GridCell<P>;

    fn mul(self, rhs: P) -> Self::Output {
        (*self).mul(rhs)
    }
}
