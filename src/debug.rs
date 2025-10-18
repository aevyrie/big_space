//! Contains tools for debugging the floating origin.

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_color::prelude::*;
use bevy_ecs::prelude::*;
use bevy_gizmos::prelude::*;
use bevy_math::prelude::*;
use bevy_reflect::Reflect;
use bevy_transform::prelude::*;

/// This plugin will render the bounds of occupied grid cells.
pub struct BigSpaceDebugPlugin;
impl Plugin for BigSpaceDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<BigSpaceGizmoConfig>()
            .add_systems(Startup, setup_gizmos)
            .add_systems(
                PostUpdate,
                (update_debug_bounds, update_grid_axes)
                    .chain()
                    .after(TransformSystems::Propagate),
            );
    }
}

fn setup_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<BigSpaceGizmoConfig>();
    config.line.perspective = false;
    config.line.joints = GizmoLineJoint::Round(4);
    config.line.width = 1.0;
}

/// Update the rendered debug bounds to only highlight occupied [`CellCoord`]s.
fn update_debug_bounds(
    mut gizmos: Gizmos<BigSpaceGizmoConfig>,
    grids: Grids,
    occupied_cells: Query<(Entity, &CellCoord, Option<&FloatingOrigin>)>,
) {
    for (cell_entity, cell, origin) in occupied_cells.iter() {
        let Some(grid) = grids.parent_grid(cell_entity) else {
            continue;
        };
        let transform = grid.global_transform(
            cell,
            &Transform::from_scale(Vec3::splat(grid.cell_edge_length() * 0.999)),
        );
        if origin.is_none() {
            gizmos.cuboid(transform, Color::linear_rgb(0.0, 1.0, 0.0));
        } else {
            // gizmos.cuboid(transform, Color::rgba(0.0, 0.0, 1.0, 0.5))
        }
    }
}

#[derive(Default, Reflect)]
struct BigSpaceGizmoConfig;

impl GizmoConfigGroup for BigSpaceGizmoConfig {}

/// Draw axes for grids.
fn update_grid_axes(
    mut gizmos: Gizmos<BigSpaceGizmoConfig>,
    grids: Query<(&GlobalTransform, &Grid)>,
) {
    for (transform, grid) in grids.iter() {
        let start = transform.translation();
        // Scale with distance
        let len = (start.length().powf(0.9)).max(grid.cell_edge_length()) * 0.5;
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
