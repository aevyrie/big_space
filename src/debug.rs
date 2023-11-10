//! Contains tools for debugging the floating origin.

use std::marker::PhantomData;

use bevy::prelude::*;

use crate::{precision::GridPrecision, FloatingOrigin, FloatingOriginSettings, GridCell};

/// This plugin will render the bounds of occupied grid cells.
#[derive(Default)]
pub struct FloatingOriginDebugPlugin<P: GridPrecision>(PhantomData<P>);
impl<P: GridPrecision> Plugin for FloatingOriginDebugPlugin<P> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            update_debug_bounds::<P>
                .after(crate::recenter_transform_on_grid::<P>)
                .before(crate::update_global_from_grid::<P>),
        );
    }
}

/// Update the rendered debug bounds to only highlight occupied [`GridCell`]s.
pub fn update_debug_bounds<P: GridPrecision>(
    mut gizmos: Gizmos,
    settings: Res<FloatingOriginSettings>,
    occupied_cells: Query<&GridCell<P>, Without<FloatingOrigin>>,
    origin_cells: Query<&GridCell<P>, With<FloatingOrigin>>,
) {
    let origin_cell = origin_cells.single();
    for cell in occupied_cells.iter() {
        let cell = cell - origin_cell;
        let scale = Vec3::splat(settings.grid_edge_length * 0.999);
        let translation = settings.grid_position(&cell, &Transform::IDENTITY);
        gizmos.cuboid(
            Transform::from_translation(translation).with_scale(scale),
            Color::GREEN,
        )
    }
}
