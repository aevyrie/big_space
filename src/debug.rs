//! Contains tools for debugging the floating origin.

use std::marker::PhantomData;

use bevy_app::prelude::*;
use bevy_color::prelude::*;
use bevy_ecs::prelude::*;
use bevy_gizmos::prelude::*;
use bevy_math::prelude::*;
use bevy_reflect::Reflect;
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
        app.init_gizmo_group::<BigSpaceGizmoConfig>()
            .add_systems(Startup, setup_gizmos)
            .add_systems(
                PostUpdate,
                (update_debug_bounds::<P>, update_reference_frame_axes::<P>)
                    .chain()
                    .after(bevy_transform::TransformSystem::TransformPropagate),
            );
    }
}

fn setup_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<BigSpaceGizmoConfig>();
    config.line_perspective = false;
    config.line_joints = GizmoLineJoint::Round(4);
    config.line_width = 1.0;
}

/// Update the rendered debug bounds to only highlight occupied [`GridCell`]s.
fn update_debug_bounds<P: GridPrecision>(
    mut gizmos: Gizmos<BigSpaceGizmoConfig>,
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
            gizmos.cuboid(transform, Color::linear_rgb(0.0, 1.0, 0.0))
        } else {
            // gizmos.cuboid(transform, Color::rgba(0.0, 0.0, 1.0, 0.5))
        }
    }
}

#[derive(Default, Reflect)]
struct BigSpaceGizmoConfig;

impl GizmoConfigGroup for BigSpaceGizmoConfig {}

/// Draw axes for reference frames.
fn update_reference_frame_axes<P: GridPrecision>(
    mut gizmos: Gizmos<BigSpaceGizmoConfig>,
    frames: Query<(&GlobalTransform, &ReferenceFrame<P>)>,
) {
    for (transform, frame) in frames.iter() {
        let start = transform.translation();
        // Scale with distance
        let len = (start.length().powf(0.9)).max(frame.cell_edge_length()) * 0.5;
        gizmos.ray(
            start,
            transform.right() * len,
            Color::linear_rgb(1.0, 0.0, 0.0),
        );
        gizmos.ray(
            start,
            transform.up() * len,
            Color::linear_rgb(0.0, 1.0, 0.0),
        );
        gizmos.ray(
            start,
            transform.back() * len,
            Color::linear_rgb(0.0, 0.0, 1.0),
        );
    }
}
