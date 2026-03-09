//! Contains the grid cell implementation

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::{DVec3, IVec3};
use bevy_platform::time::Instant;
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;

/// The integer coordinate of a cubic cell in a [`Grid`].
///
/// Locates an entity in a cell within its parent's [`Grid`]. The [`Transform`] of an entity with
/// this component is a transformation relative to the center of this cell.
///
/// All entities with a [`CellCoord`] must be children of an entity with a [`Grid`].
///
/// This component adds precision to the translation of an entity's [`Transform`]. In a
/// high-precision [`BigSpace`], the position of an entity is described by a [`Transform`] *and* a
/// [`CellCoord`]. This component is the index of a cell inside a large [`Grid`], and the
/// [`Transform`] is the floating point position of the entity relative to the center of this cell.
///
/// If an entity's [`Transform`] becomes large enough that the entity leaves the bounds of its cell,
/// the [`CellCoord`] and [`Transform`] will be automatically recomputed to keep the [`Transform`]
/// small.
#[derive(Component, Default, Debug, PartialEq, Eq, Clone, Copy, Hash, Reflect)]
#[reflect(Component, Default, PartialEq)]
#[require(Transform, GlobalTransform)]
pub struct CellCoord {
    /// X coordinate of a cell in its parent [`Grid`].
    pub x: GridPrecision,
    /// Y coordinate of a cell in its parent [`Grid`].
    pub y: GridPrecision,
    /// Z coordinate of a cell in its parent [`Grid`].
    pub z: GridPrecision,
}

impl CellCoord {
    /// Construct a new [`CellCoord`].
    pub fn new(x: GridPrecision, y: GridPrecision, z: GridPrecision) -> Self {
        Self { x, y, z }
    }

    /// The origin [`CellCoord`].
    pub const ZERO: Self = CellCoord { x: 0, y: 0, z: 0 };

    /// A unit value [`CellCoord`]. Useful for offsets.
    pub const ONE: Self = CellCoord { x: 1, y: 1, z: 1 };

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
        mut stats: Option<ResMut<crate::timing::PropagationStats>>,
        grids: Query<&Grid>,
        mut changed_transform: Query<
            (&mut Self, &mut Transform, &ChildOf),
            (Changed<Transform>, Without<Stationary>),
        >,
    ) {
        let start = Instant::now();
        changed_transform
            .par_iter_mut()
            .for_each(|(mut grid_pos, mut transform, parent)| {
                let Ok(grid) = grids.get(parent.parent()) else {
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
        if let Some(stats) = stats.as_mut() {
            stats.grid_recentering += start.elapsed();
        }
    }
}

impl core::ops::Add for CellCoord {
    type Output = CellCoord;

    fn add(self, rhs: Self) -> Self::Output {
        CellCoord {
            x: self.x.wrapping_add(rhs.x),
            y: self.y.wrapping_add(rhs.y),
            z: self.z.wrapping_add(rhs.z),
        }
    }
}

impl core::ops::Add<IVec3> for CellCoord {
    type Output = CellCoord;

    fn add(self, rhs: IVec3) -> Self::Output {
        CellCoord {
            x: self.x.wrapping_add(rhs.x as GridPrecision),
            y: self.y.wrapping_add(rhs.y as GridPrecision),
            z: self.z.wrapping_add(rhs.z as GridPrecision),
        }
    }
}

impl core::ops::Sub for CellCoord {
    type Output = CellCoord;

    fn sub(self, rhs: Self) -> Self::Output {
        CellCoord {
            x: self.x.wrapping_sub(rhs.x),
            y: self.y.wrapping_sub(rhs.y),
            z: self.z.wrapping_sub(rhs.z),
        }
    }
}

impl core::ops::Sub<IVec3> for CellCoord {
    type Output = CellCoord;

    fn sub(self, rhs: IVec3) -> Self::Output {
        CellCoord {
            x: self.x.wrapping_add(-rhs.x as GridPrecision),
            y: self.y.wrapping_add(-rhs.y as GridPrecision),
            z: self.z.wrapping_add(-rhs.z as GridPrecision),
        }
    }
}

impl core::ops::Add for &CellCoord {
    type Output = CellCoord;

    fn add(self, rhs: Self) -> Self::Output {
        (*self).add(*rhs)
    }
}

impl core::ops::Add<IVec3> for &CellCoord {
    type Output = CellCoord;

    fn add(self, rhs: IVec3) -> Self::Output {
        (*self).add(rhs)
    }
}

impl core::ops::Sub for &CellCoord {
    type Output = CellCoord;

    fn sub(self, rhs: Self) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl core::ops::Sub<IVec3> for &CellCoord {
    type Output = CellCoord;

    fn sub(self, rhs: IVec3) -> Self::Output {
        (*self).sub(rhs)
    }
}

impl core::ops::AddAssign for CellCoord {
    fn add_assign(&mut self, rhs: Self) {
        use core::ops::Add;
        *self = self.add(rhs);
    }
}

impl core::ops::AddAssign<IVec3> for CellCoord {
    fn add_assign(&mut self, rhs: IVec3) {
        use core::ops::Add;
        *self = self.add(rhs);
    }
}

impl core::ops::SubAssign for CellCoord {
    fn sub_assign(&mut self, rhs: Self) {
        use core::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl core::ops::SubAssign<IVec3> for CellCoord {
    fn sub_assign(&mut self, rhs: IVec3) {
        use core::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl core::ops::Mul<GridPrecision> for CellCoord {
    type Output = CellCoord;

    fn mul(self, rhs: GridPrecision) -> Self::Output {
        CellCoord {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl core::ops::Mul<GridPrecision> for &CellCoord {
    type Output = CellCoord;

    fn mul(self, rhs: GridPrecision) -> Self::Output {
        (*self).mul(rhs)
    }
}

impl core::ops::Div<GridPrecision> for CellCoord {
    type Output = CellCoord;

    fn div(self, rhs: GridPrecision) -> Self::Output {
        CellCoord {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl core::ops::Div<GridPrecision> for &CellCoord {
    type Output = CellCoord;

    fn div(self, rhs: GridPrecision) -> Self::Output {
        (*self).div(rhs)
    }
}
