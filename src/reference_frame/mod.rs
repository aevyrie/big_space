//! Adds the concept of hierarchical, nesting [`ReferenceFrame`]s, to group entities that move
//! through space together, like entities on a planet, rotating about the planet's axis, and,
//! orbiting a star.

use bevy_ecs::prelude::*;
use bevy_math::{prelude::*, Affine3A, DAffine3, DVec3};
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;

use crate::{precision::GridPrecision, GridCell};

use self::local_origin::LocalFloatingOrigin;

pub mod local_origin;
pub mod propagation;

/// A component that defines a reference frame for children of this entity with [`GridCell`]s. All
/// entities with a [`GridCell`] must be children of an entity with a [`ReferenceFrame`]. The
/// reference frame *defines* the grid that the `GridCell` indexes into.
///
/// ## Motivation
///
/// Reference frames are hierarchical, allowing more precision for objects with similar relative
/// velocities. All entities in the same reference frame will move together, like standard transform
/// propagation, but with much more precision. Entities in the same reference frame as the
/// [`crate::FloatingOrigin`] will be rendered with the most precision. Transforms are propagated
/// starting from the floating origin, ensuring that references frames in a similar point in the
/// hierarchy have accumulated the least error. Reference frames are transformed relative to each
/// other using 64 bit float transforms.
///
/// ## Example
///
/// You can use reference frames to ensure all entities on a planet, and the planet itself, are in
/// the same rotating reference frame, instead of moving rapidly through space around a star, or
/// worse, around the center of the galaxy.
#[derive(Debug, Clone, Reflect, Component)]
pub struct ReferenceFrame<P: GridPrecision + Reflect> {
    /// The high-precision position of the floating origin's current grid cell local to this
    /// reference frame.
    local_floating_origin: LocalFloatingOrigin<P>,
    /// Defines the uniform scale of the grid by the length of the edge of a grid cell.
    cell_edge_length: f32,
    /// How far an entity can move from the origin before its grid cell is recomputed.
    maximum_distance_from_origin: f32,
}

impl<P: GridPrecision> Default for ReferenceFrame<P> {
    fn default() -> Self {
        Self::new(2_000f32, 100f32)
    }
}

impl<P: GridPrecision> ReferenceFrame<P> {
    /// Construct a new [`ReferenceFrame`]. The properties of a reference frame cannot be changed
    /// after construction.
    pub fn new(cell_edge_length: f32, switching_threshold: f32) -> Self {
        Self {
            local_floating_origin: LocalFloatingOrigin::default(),
            cell_edge_length,
            maximum_distance_from_origin: cell_edge_length / 2.0 + switching_threshold,
        }
    }

    /// Get the position of the floating origin relative to the current reference frame.
    pub fn local_floating_origin(&self) -> &LocalFloatingOrigin<P> {
        &self.local_floating_origin
    }

    /// Get the size of each cell this reference frame's grid.
    pub fn cell_edge_length(&self) -> f32 {
        self.cell_edge_length
    }

    /// Get the reference frame's [`Self::maximum_distance_from_origin`].
    pub fn maximum_distance_from_origin(&self) -> f32 {
        self.maximum_distance_from_origin
    }

    /// Compute the double precision position of an entity's [`Transform`] with respect to the given
    /// [`GridCell`] within this reference frame.
    pub fn grid_position_double(&self, pos: &GridCell<P>, transform: &Transform) -> DVec3 {
        DVec3 {
            x: pos.x.as_f64() * self.cell_edge_length as f64 + transform.translation.x as f64,
            y: pos.y.as_f64() * self.cell_edge_length as f64 + transform.translation.y as f64,
            z: pos.z.as_f64() * self.cell_edge_length as f64 + transform.translation.z as f64,
        }
    }

    /// Compute the single precision position of an entity's [`Transform`] with respect to the given
    /// [`GridCell`].
    pub fn grid_position(&self, pos: &GridCell<P>, transform: &Transform) -> Vec3 {
        Vec3 {
            x: pos.x.as_f64() as f32 * self.cell_edge_length + transform.translation.x,
            y: pos.y.as_f64() as f32 * self.cell_edge_length + transform.translation.y,
            z: pos.z.as_f64() as f32 * self.cell_edge_length + transform.translation.z,
        }
    }

    /// Returns the floating point position of a [`GridCell`].
    pub fn grid_to_float(&self, pos: &GridCell<P>) -> DVec3 {
        DVec3 {
            x: pos.x.as_f64() * self.cell_edge_length as f64,
            y: pos.y.as_f64() * self.cell_edge_length as f64,
            z: pos.z.as_f64() * self.cell_edge_length as f64,
        }
    }

    /// Convert a large translation into a small translation relative to a grid cell.
    pub fn translation_to_grid(&self, input: impl Into<DVec3>) -> (GridCell<P>, Vec3) {
        let l = self.cell_edge_length as f64;
        let input = input.into();
        let DVec3 { x, y, z } = input;

        if input.abs().max_element() < self.maximum_distance_from_origin as f64 {
            return (GridCell::default(), input.as_vec3());
        }

        let x_r = (x / l).round();
        let y_r = (y / l).round();
        let z_r = (z / l).round();
        let t_x = x - x_r * l;
        let t_y = y - y_r * l;
        let t_z = z - z_r * l;

        (
            GridCell {
                x: P::from_f32(x_r as f32),
                y: P::from_f32(y_r as f32),
                z: P::from_f32(z_r as f32),
            },
            Vec3::new(t_x as f32, t_y as f32, t_z as f32),
        )
    }

    /// Convert a large translation into a small translation relative to a grid cell.
    pub fn imprecise_translation_to_grid(&self, input: Vec3) -> (GridCell<P>, Vec3) {
        self.translation_to_grid(input.as_dvec3())
    }

    /// Compute the [`GlobalTransform`] of an entity in this reference frame.
    pub fn global_transform(
        &self,
        local_cell: &GridCell<P>,
        local_transform: &Transform,
    ) -> GlobalTransform {
        // The reference frame transform from the floating origin's reference frame, to the local
        // reference frame.
        let transform_origin = self.local_floating_origin().reference_frame_transform();
        // The grid cell offset of this entity relative to the floating origin's cell in this local
        // reference frame.
        let cell_origin_relative = *local_cell - self.local_floating_origin().cell();
        let grid_offset = self.grid_to_float(&cell_origin_relative);
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
