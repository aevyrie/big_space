//! Contains the grid cell implementation

use crate::prelude::*;
use bevy_ecs::{prelude::*, relationship::Relationship};
use bevy_math::{DVec3, IVec3};
use bevy_platform_support::time::Instant;
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;

/// Locates an entity in a cell within its parent's [`Grid`]. The [`Transform`] of an entity with
/// this component is a transformation from the center of this cell.
///
/// All entities with a [`GridCell`] must be children of an entity with a [`Grid`].
///
/// This component adds precision to the translation of an entity's [`Transform`]. In a
/// high-precision [`BigSpace`], the position of an entity is described by a [`Transform`] *and* a
/// [`GridCell`]. This component is the index of a cell inside a large [`Grid`], and the
/// [`Transform`] is the floating point position of the entity relative to the center of this cell.
///
/// If an entity's [`Transform`] becomes large enough that the entity leaves the bounds of its cell,
/// the [`GridCell`] and [`Transform`] will be automatically recomputed to keep the [`Transform`]
/// small.
///
/// [`BigSpace`]s are only allowed to have a single type of `GridCell`, you cannot mix
/// [`GridPrecision`]s.
#[derive(Component, Default, Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect)]
#[reflect(Component, Default, PartialEq)]
#[require(Transform, GlobalTransform)]
pub struct GridCell {
    /// The x-index of the cell.
    pub x: GridPrecision,
    /// The y-index of the cell.
    pub y: GridPrecision,
    /// The z-index of the cell.
    pub z: GridPrecision,
}

impl GridCell {
    /// Construct a new [`GridCell`].
    pub fn new(x: GridPrecision, y: GridPrecision, z: GridPrecision) -> Self {
        Self { x, y, z }
    }

    /// The origin [`GridCell`].
    pub const ZERO: Self = GridCell { x: 0, y: 0, z: 0 };

    /// A unit value [`GridCell`]. Useful for offsets.
    pub const ONE: Self = GridCell { x: 1, y: 1, z: 1 };

    /// Convert this grid cell to a floating point translation within this `grid`.
    pub fn as_dvec3(&self, grid: &Grid) -> DVec3 {
        DVec3 {
            x: self.x as f64 * grid.cell_edge_length() as f64,
            y: self.y as f64 * grid.cell_edge_length() as f64,
            z: self.z as f64 * grid.cell_edge_length() as f64,
        }
    }

    /// Returns a cell containing the minimum values for each element of self and rhs.
    ///
    /// In other words this computes [self.x.min(rhs.x), self.y.min(rhs.y), ...].
    pub fn min(&self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
            z: self.z.min(rhs.z),
        }
    }

    /// Returns a cell containing the maximum values for each element of self and rhs.
    ///
    /// In other words this computes [self.x.max(rhs.x), self.y.max(rhs.y), ...].
    pub fn max(&self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
            z: self.z.max(rhs.z),
        }
    }

    /// If an entity's transform translation becomes larger than the limit specified in its
    /// [`Grid`], it will be relocated to the nearest grid cell to reduce the size of the transform.
    pub fn recenter_large_transforms(
        mut stats: ResMut<crate::timing::PropagationStats>,
        grids: Query<&Grid>,
        mut changed_transform: Query<(&mut Self, &mut Transform, &ChildOf), Changed<Transform>>,
    ) {
        let start = Instant::now();
        changed_transform
            .par_iter_mut()
            .for_each(|(mut grid_pos, mut transform, parent)| {
                let Ok(grid) = grids.get(parent.get()) else {
                    return;
                };
                if transform
                    .bypass_change_detection()
                    .translation
                    .abs()
                    .max_element()
                    > grid.maximum_distance_from_origin()
                {
                    let (grid_cell_delta, translation) = grid.imprecise_translation_to_grid(
                        transform.bypass_change_detection().translation,
                    );
                    *grid_pos += grid_cell_delta;
                    transform.translation = translation;
                }
            });
        stats.grid_recentering += start.elapsed();
    }
}

impl core::ops::Add for GridCell {
    type Output = GridCell;

    fn add(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add(rhs.x),
            y: self.y.wrapping_add(rhs.y),
            z: self.z.wrapping_add(rhs.z),
        }
    }
}

impl core::ops::Add<IVec3> for GridCell {
    type Output = GridCell;

    fn add(self, rhs: IVec3) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add(rhs.x as GridPrecision),
            y: self.y.wrapping_add(rhs.y as GridPrecision),
            z: self.z.wrapping_add(rhs.z as GridPrecision),
        }
    }
}

impl core::ops::Sub for GridCell {
    type Output = GridCell;

    fn sub(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_sub(rhs.x),
            y: self.y.wrapping_sub(rhs.y),
            z: self.z.wrapping_sub(rhs.z),
        }
    }
}

impl core::ops::Sub<IVec3> for GridCell {
    type Output = GridCell;

    fn sub(self, rhs: IVec3) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add(-rhs.x as GridPrecision),
            y: self.y.wrapping_add(-rhs.y as GridPrecision),
            z: self.z.wrapping_add(-rhs.z as GridPrecision),
        }
    }
}

impl core::ops::Add for &GridCell {
    type Output = GridCell;

    fn add(self, rhs: Self) -> Self::Output {
        (*self).add(*rhs)
    }
}

impl core::ops::Add<IVec3> for &GridCell {
    type Output = GridCell;

    fn add(self, rhs: IVec3) -> Self::Output {
        (*self).add(rhs)
    }
}

impl core::ops::Sub for &GridCell {
    type Output = GridCell;

    fn sub(self, rhs: Self) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl core::ops::Sub<IVec3> for &GridCell {
    type Output = GridCell;

    fn sub(self, rhs: IVec3) -> Self::Output {
        (*self).sub(rhs)
    }
}

impl core::ops::AddAssign for GridCell {
    fn add_assign(&mut self, rhs: Self) {
        use core::ops::Add;
        *self = self.add(rhs);
    }
}

impl core::ops::AddAssign<IVec3> for GridCell {
    fn add_assign(&mut self, rhs: IVec3) {
        use core::ops::Add;
        *self = self.add(rhs);
    }
}

impl core::ops::SubAssign for GridCell {
    fn sub_assign(&mut self, rhs: Self) {
        use core::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl core::ops::SubAssign<IVec3> for GridCell {
    fn sub_assign(&mut self, rhs: IVec3) {
        use core::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl core::ops::Mul<GridPrecision> for GridCell {
    type Output = GridCell;

    fn mul(self, rhs: GridPrecision) -> Self::Output {
        GridCell {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl core::ops::Mul<GridPrecision> for &GridCell {
    type Output = GridCell;

    fn mul(self, rhs: GridPrecision) -> Self::Output {
        (*self).mul(rhs)
    }
}
