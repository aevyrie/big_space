//! Contains tools for debugging the floating origin.

use std::marker::PhantomData;

use bevy::prelude::*;

use crate::{
    precision::GridPrecision, reference_frame::local_origin::ReferenceFrames, FloatingOrigin,
    GridCell,
};

/// This plugin will render the bounds of occupied grid cells.
#[derive(Default)]
pub struct FloatingOriginDebugPlugin<P: GridPrecision>(PhantomData<P>);
impl<P: GridPrecision> Plugin for FloatingOriginDebugPlugin<P> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            update_debug_bounds::<P>.after(bevy::transform::TransformSystem::TransformPropagate),
        );
    }
}

/// Update the rendered debug bounds to only highlight occupied [`GridCell`]s.
pub fn update_debug_bounds<P: GridPrecision>(
    mut gizmos: Gizmos,
    reference_frames: ReferenceFrames<P>,
    occupied_cells: Query<(Entity, &GridCell<P>), Without<FloatingOrigin>>,
) {
    for (cell_entity, cell) in occupied_cells.iter() {
        let Some(frame) = reference_frames
            .reference_frame(cell_entity)
            .map(|handle| reference_frames.resolve_handle(handle))
        else {
            continue;
        };
        let transform = frame.global_transform(
            cell,
            &Transform::from_scale(Vec3::splat(frame.cell_edge_length() * 0.999)),
        );
        gizmos.cuboid(transform, Color::GREEN)
    }
}
