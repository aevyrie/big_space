//! Contains tools for debugging the floating origin.

use std::marker::PhantomData;

use bevy::{prelude::*, utils::HashMap};

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

/// Update the rendered debug bounds to only highlight occupied [`GridCell`]s. [`DebugBounds`] are
/// spawned or hidden as needed.
pub fn update_debug_bounds<P: GridPrecision>(
    mut gizmos: Gizmos,
    settings: Res<FloatingOriginSettings>,
    occupied_cells: Query<(&GridCell<P>, Option<&FloatingOrigin>)>,
    origin_cells: Query<&GridCell<P>, With<FloatingOrigin>>,
) {
    let mut cells = HashMap::<_, (GridCell<P>, bool)>::new();
    let origin_cell = origin_cells.single();

    for (cell, this_is_origin) in occupied_cells.iter() {
        let (_, current_is_origin) = cells
            .entry((cell.x, cell.y, cell.z))
            .or_insert((*cell, this_is_origin.is_some()));

        *current_is_origin |= this_is_origin.is_some();
    }

    for (cell, has_origin) in cells.values() {
        let cell = cell - origin_cell;
        let scale = Vec3::splat(settings.grid_edge_length * 0.999);
        let translation = settings.grid_position(&cell, &Transform::IDENTITY);
        gizmos.cuboid(
            Transform::from_translation(translation).with_scale(scale),
            match *has_origin {
                true => Color::BLUE,
                false => Color::GREEN,
            },
        )
    }
}
