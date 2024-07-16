//! Contains tools for debugging the floating origin.

use std::marker::PhantomData;

use bevy_app::prelude::*;
use bevy_color::{
    palettes::css::{BLUE, GREEN, RED},
    prelude::*,
};
use bevy_ecs::prelude::*;
use bevy_gizmos::prelude::*;
use bevy_math::prelude::*;
use bevy_transform::prelude::*;

use crate::{
    precision::GridPrecision,
    reference_frame::{local_origin::ReferenceFrames, ReferenceFrame},
    FloatingOrigin, GridCell,
};

/// This plugin will render the bounds of occupied grid cells.
#[derive(Default)]
pub struct FloatingOriginDebugPlugin<P: GridPrecision>(PhantomData<P>);
impl<P: GridPrecision> Plugin for FloatingOriginDebugPlugin<P> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (update_debug_bounds::<P>, update_reference_frame_axes::<P>)
                .chain()
                .after(bevy_transform::TransformSystem::TransformPropagate),
        );
    }
}

/// Update the rendered debug bounds to only highlight occupied [`GridCell`]s.
pub fn update_debug_bounds<P: GridPrecision>(
    mut gizmos: Gizmos,
    reference_frames: ReferenceFrames<P>,
    occupied_cells: Query<(Entity, &GridCell<P>, Option<&FloatingOrigin>)>,
) {
    for (cell_entity, cell, origin) in occupied_cells.iter() {
        let Some(frame) = reference_frames.parent_frame(cell_entity) else {
            continue;
        };
        let transform = frame.global_transform(
            cell,
            &Transform::from_scale(Vec3::splat(frame.cell_edge_length() * 0.999)),
        );
        if origin.is_none() {
            gizmos.cuboid(transform, Color::Srgba(GREEN))
        } else {
            // gizmos.cuboid(transform, Color::rgba(0.0, 0.0, 1.0, 0.5))
        }
    }
}

/// Draw axes for reference frames.
pub fn update_reference_frame_axes<P: GridPrecision>(
    mut gizmos: Gizmos,
    frames: Query<(&GlobalTransform, &ReferenceFrame<P>)>,
) {
    for (transform, frame) in frames.iter() {
        let start = transform.translation();
        let len = frame.cell_edge_length() * 2.0;
        gizmos.ray(start, transform.right() * len, Color::Srgba(RED));
        gizmos.ray(start, transform.up() * len, Color::Srgba(GREEN));
        gizmos.ray(start, transform.back() * len, Color::Srgba(BLUE));
    }
}
