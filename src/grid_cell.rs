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

/// The cell index an entity within a [`crate::ReferenceFrame`]'s grid. The [`Transform`] of an
/// entity with this component is a transformation from the center of this cell.
///
/// This component adds precision to the translation of an entity's [`Transform`]. In a
/// high-precision [`BigSpace`] world, the position of an entity is described by a [`Transform`]
/// *and* a [`GridCell`]. This component is the index of a cell inside a large grid defined by the
/// [`ReferenceFrame`], and the transform is the position of the entity relative to the center of
/// that cell.
///
/// Entities and an entity hierarchies are only allowed to have a single type of `GridCell`, you
/// cannot mix [`GridPrecision`]s.
#[derive(Component, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Reflect)]
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

    /// Convert this grid cell to a floating point translation within this `reference_frame`.
    pub fn as_dvec3(&self, reference_frame: &ReferenceFrame<P>) -> DVec3 {
        DVec3 {
            x: self.x.as_f64() * reference_frame.cell_edge_length() as f64,
            y: self.y.as_f64() * reference_frame.cell_edge_length() as f64,
            z: self.z.as_f64() * reference_frame.cell_edge_length() as f64,
        }
    }

    /// If an entity's transform translation becomes larger than the limit specified in its
    /// [`ReferenceFrame`], it will be relocated to the nearest grid cell to reduce the size of the
    /// transform.
    pub fn recenter_large_transforms(
        mut stats: ResMut<crate::timing::PropagationStats>,
        reference_frames: Query<&ReferenceFrame<P>>,
        mut changed_transform: Query<(&mut Self, &mut Transform, &Parent), Changed<Transform>>,
    ) {
        let start = Instant::now();
        changed_transform
            .par_iter_mut()
            .for_each(|(mut grid_pos, mut transform, parent)| {
                let Ok(reference_frame) = reference_frames.get(parent.get()) else {
                    return;
                };
                if transform
                    .bypass_change_detection()
                    .translation
                    .abs()
                    .max_element()
                    > reference_frame.maximum_distance_from_origin()
                {
                    let (grid_cell_delta, translation) = reference_frame
                        .imprecise_translation_to_grid(
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
