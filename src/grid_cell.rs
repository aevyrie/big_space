//! Contains the grid cell implementation

use bevy::prelude::*;

use crate::precision::GridPrecision;

/// Defines the grid cell this entity's `Transform` is relative to.
///
/// This component is generic over a few integer types to allow you to select the grid size you
/// need. These correspond to a total usable volume of a cube with the following edge lengths:
///
/// **Assuming you are using a grid cell edge length of 10,000 meters, and `1.0` == 1 meter, which
/// gives you a worst case precision of 0.5mm**
///
/// - i8: 2,560 km = 74% of the diameter of the Moon
/// - i16: 655,350 km = 85% of the diameter of the Moon's orbit around Earth
/// - i32: 0.0045 light years = ~4 times the width of the solar system
/// - i64: 19.5 million light years = ~100 times the width of the milky way galaxy
/// - i128: 3.6e+26 light years = ~3.9e+15 times the width of the observable universe
///
/// where
///
/// `usable_edge_length = 2^(integer_bits) * grid_cell_edge_length`
///
/// # Note
///
/// Be sure you are using the same grid index precision everywhere. It might be a good idea to
/// define a type alias!
///
/// ```
/// # use big_space::GridCell;
/// type GalacticGrid = GridCell<i64>;
/// ```
///
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
