//! Adds the concept of hierarchical, nesting [`Grid`]s, to group entities that move through space
//! together, like entities on a planet, rotating about the planet's axis, and, orbiting a star.

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::{prelude::*, Affine3A, DAffine3, DVec3};
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;

use local_origin::LocalFloatingOrigin;

pub mod cell;
pub mod local_origin;
pub mod propagation;

/// A component that defines a spatial grid that child entities are located on. Child entities are
/// located on this grid with the [`GridCell`] component.
///
/// All entities with a [`GridCell`] must be children of an entity with a [`Grid`].
///
/// Grids are hierarchical, allowing more precision for objects with similar relative velocities.
/// All entities in the same grid will move together, like standard transform propagation, but with
/// much more precision.
///
/// Entities in the same grid as the [`FloatingOrigin`] will be rendered with the most precision.
/// Transforms are propagated starting from the floating origin, ensuring that grids in a similar
/// point in the hierarchy have accumulated the least error. Grids are transformed relative to each
/// other using 64-bit float transforms.
#[derive(Debug, Clone, Reflect, Component)]
#[reflect(Component)]
// We do not require the Transform, GlobalTransform, or GridCell, because these are not required in
// all cases: e.g. BigSpace should not have a Transform or GridCell.
pub struct Grid {
    /// The high-precision position of the floating origin's current grid cell local to this grid.
    local_floating_origin: LocalFloatingOrigin,
    /// Defines the uniform scale of the grid by the length of the edge of a grid cell.
    cell_edge_length: f32,
    /// How far an entity can move from the origin before its grid cell is recomputed.
    maximum_distance_from_origin: f32,
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(2_000f32, 100f32)
    }
}

impl Grid {
    /// Construct a new [`Grid`]. The properties of a grid cannot be changed after construction.
    pub fn new(cell_edge_length: f32, switching_threshold: f32) -> Self {
        Self {
            local_floating_origin: LocalFloatingOrigin::default(),
            cell_edge_length,
            maximum_distance_from_origin: cell_edge_length / 2.0 + switching_threshold,
        }
    }

    /// Get the position of the floating origin relative to the current grid.
    #[inline]
    pub fn local_floating_origin(&self) -> &LocalFloatingOrigin {
        &self.local_floating_origin
    }

    /// Get the size of each cell this grid's grid.
    #[inline]
    pub fn cell_edge_length(&self) -> f32 {
        self.cell_edge_length
    }

    /// Get the grid's [`Self::maximum_distance_from_origin`].
    #[inline]
    pub fn maximum_distance_from_origin(&self) -> f32 {
        self.maximum_distance_from_origin
    }

    /// Compute the double precision position of an entity's [`Transform`] with respect to the given
    /// [`GridCell`] within this grid.
    #[inline]
    pub fn grid_position_double(&self, pos: &GridCell, transform: &Transform) -> DVec3 {
        DVec3 {
            x: pos.x as f64 * self.cell_edge_length as f64 + transform.translation.x as f64,
            y: pos.y as f64 * self.cell_edge_length as f64 + transform.translation.y as f64,
            z: pos.z as f64 * self.cell_edge_length as f64 + transform.translation.z as f64,
        }
    }

    /// Compute the single precision position of an entity's [`Transform`] with respect to the given
    /// [`GridCell`].
    #[inline]
    pub fn grid_position(&self, pos: &GridCell, transform: &Transform) -> Vec3 {
        Vec3 {
            x: pos.x as f64 as f32 * self.cell_edge_length + transform.translation.x,
            y: pos.y as f64 as f32 * self.cell_edge_length + transform.translation.y,
            z: pos.z as f64 as f32 * self.cell_edge_length + transform.translation.z,
        }
    }

    /// Returns the floating point position of a [`GridCell`].
    pub fn cell_to_float(&self, pos: &GridCell) -> DVec3 {
        DVec3 {
            x: pos.x as f64,
            y: pos.y as f64,
            z: pos.z as f64,
        } * self.cell_edge_length as f64
    }

    /// Convert a large translation into a small translation relative to a grid cell.
    #[inline]
    pub fn translation_to_grid(&self, input: impl Into<DVec3>) -> (GridCell, Vec3) {
        let l = self.cell_edge_length as f64;
        let input = input.into();
        let DVec3 { x, y, z } = input;

        if input.abs().max_element() < self.maximum_distance_from_origin as f64 {
            return (GridCell::default(), input.as_vec3());
        }

        let x_r = round(x / l);
        let y_r = round(y / l);
        let z_r = round(z / l);
        let t_x = x - x_r * l;
        let t_y = y - y_r * l;
        let t_z = z - z_r * l;

        (
            GridCell {
                x: x_r as GridPrecision,
                y: y_r as GridPrecision,
                z: z_r as GridPrecision,
            },
            Vec3::new(t_x as f32, t_y as f32, t_z as f32),
        )
    }

    /// Convert a large translation into a small translation relative to a grid cell.
    #[inline]
    pub fn imprecise_translation_to_grid(&self, input: Vec3) -> (GridCell, Vec3) {
        self.translation_to_grid(input.as_dvec3())
    }

    /// Compute the [`GlobalTransform`] of an entity in this grid.
    #[inline]
    pub fn global_transform(
        &self,
        local_cell: &GridCell,
        local_transform: &Transform,
    ) -> GlobalTransform {
        // The grid transform from the floating origin's grid, to the local grid.
        let transform_origin = self.local_floating_origin().grid_transform();
        // The grid cell offset of this entity relative to the floating origin's cell in this local
        // grid.
        let cell_origin_relative = *local_cell - self.local_floating_origin().cell();
        let grid_offset = self.cell_to_float(&cell_origin_relative);
        let local_transform = DAffine3::from_scale_rotation_translation(
            local_transform.scale.as_dvec3(),
            local_transform.rotation.as_dquat(),
            local_transform.translation.as_dvec3() + grid_offset,
        );
        let global_64 = transform_origin * local_transform;

        Affine3A {
            matrix3: global_64.matrix3.as_mat3().into(),
            translation: global_64.translation.as_vec3a(),
        }
        .into()
    }
}

fn round(x: f64) -> f64 {
    #[cfg(feature = "libm")]
    {
        libm::round(x)
    }

    #[cfg(all(not(feature = "libm"), feature = "std"))]
    {
        x.round()
    }

    #[cfg(all(not(feature = "libm"), not(feature = "std")))]
    {
        compile_error!("Must enable the `libm` and/or `std` feature.");
        f64::NAN
    }
}
