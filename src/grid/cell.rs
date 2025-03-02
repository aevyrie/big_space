//! Contains the grid cell implementation

use crate::prelude::*;
use bevy_ecs::{component::ComponentId, prelude::*, world::DeferredWorld};
use bevy_hierarchy::prelude::*;
use bevy_math::{DVec3, IVec3};
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;
use bevy_utils::Instant;

/// Marks entities with any generic [`GridCell`] component. Allows you to query for high precision
/// spatial entities of any [`GridPrecision`].
///
/// Also useful for filtering. You might want to run queries on things without a grid cell, however
/// there could by many generic types of grid cell. `Without<GridCellAny>` will cover all of these
/// cases.
///
/// This is automatically added and removed by the component lifecycle hooks on [`GridCell`].
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct GridCellAny;

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
#[component(storage = "Table", on_add = Self::on_add, on_remove = Self::on_remove)]
pub struct GridCell<P: GridPrecision> {
    /// The x-index of the cell.
    pub x: P,
    /// The y-index of the cell.
    pub y: P,
    /// The z-index of the cell.
    pub z: P,
}

impl<P: GridPrecision> GridCell<P> {
    fn on_add(mut world: DeferredWorld, entity: Entity, _: ComponentId) {
        assert!(world.get::<GridCellAny>(entity).is_none(), "Adding multiple GridCell<P>s with different generic values on the same entity is not supported");
        world.commands().entity(entity).insert(GridCellAny);
    }

    fn on_remove(mut world: DeferredWorld, entity: Entity, _: ComponentId) {
        world.commands().entity(entity).remove::<GridCellAny>();
    }

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

    /// Convert this grid cell to a floating point translation within this `grid`.
    pub fn as_dvec3(&self, grid: &Grid<P>) -> DVec3 {
        DVec3 {
            x: self.x.as_f64() * grid.cell_edge_length() as f64,
            y: self.y.as_f64() * grid.cell_edge_length() as f64,
            z: self.z.as_f64() * grid.cell_edge_length() as f64,
        }
    }

    /// Returns a cell containing the minimum values for each element of self and rhs.
    ///
    /// In other words this computes [self.x.min(rhs.x), self.y.min(rhs.y), ..].
    pub fn min(&self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
            z: self.z.min(rhs.z),
        }
    }

    /// Returns a cell containing the maximum values for each element of self and rhs.
    ///
    /// In other words this computes [self.x.max(rhs.x), self.y.max(rhs.y), ..].
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
        grids: Query<&Grid<P>>,
        mut changed_transform: Query<(&mut Self, &mut Transform, &Parent), Changed<Transform>>,
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

impl<P: GridPrecision> std::ops::Add<IVec3> for GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: IVec3) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add_i32(rhs.x),
            y: self.y.wrapping_add_i32(rhs.y),
            z: self.z.wrapping_add_i32(rhs.z),
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

impl<P: GridPrecision> std::ops::Sub<IVec3> for GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: IVec3) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add_i32(-rhs.x),
            y: self.y.wrapping_add_i32(-rhs.y),
            z: self.z.wrapping_add_i32(-rhs.z),
        }
    }
}

impl<P: GridPrecision> std::ops::Add for &GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        (*self).add(*rhs)
    }
}

impl<P: GridPrecision> std::ops::Add<IVec3> for &GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: IVec3) -> Self::Output {
        (*self).add(rhs)
    }
}

impl<P: GridPrecision> std::ops::Sub for &GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl<P: GridPrecision> std::ops::Sub<IVec3> for &GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: IVec3) -> Self::Output {
        (*self).sub(rhs)
    }
}

impl<P: GridPrecision> std::ops::AddAssign for GridCell<P> {
    fn add_assign(&mut self, rhs: Self) {
        use std::ops::Add;
        *self = self.add(rhs);
    }
}

impl<P: GridPrecision> std::ops::AddAssign<IVec3> for GridCell<P> {
    fn add_assign(&mut self, rhs: IVec3) {
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

impl<P: GridPrecision> std::ops::SubAssign<IVec3> for GridCell<P> {
    fn sub_assign(&mut self, rhs: IVec3) {
        use std::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl<P: GridPrecision> std::ops::Mul<P> for GridCell<P> {
    type Output = GridCell<P>;

    fn mul(self, rhs: P) -> Self::Output {
        GridCell {
            x: GridPrecision::mul(self.x, rhs),
            y: GridPrecision::mul(self.y, rhs),
            z: GridPrecision::mul(self.z, rhs),
        }
    }
}

impl<P: GridPrecision> std::ops::Mul<P> for &GridCell<P> {
    type Output = GridCell<P>;

    fn mul(self, rhs: P) -> Self::Output {
        (*self).mul(rhs)
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    #[test]
    #[should_panic(
        expected = "Adding multiple GridCell<P>s with different generic values on the same entity is not supported"
    )]
    fn disallow_multiple_grid_cells_on_same_entity() {
        App::new()
            .add_systems(Startup, |mut commands: Commands| {
                commands
                    .spawn_empty()
                    .insert(super::GridCell::<i8>::default())
                    .insert(super::GridCell::<i16>::default());
            })
            .run();
    }
}
