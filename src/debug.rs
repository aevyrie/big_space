//! Contains tools for debugging the floating origin.

use std::marker::PhantomData;

use bevy::{prelude::*, utils::HashMap};
use bevy_polyline::prelude::*;

use crate::{precision::GridPrecision, FloatingOrigin, FloatingOriginSettings, GridCell};

/// This plugin will render the bounds of occupied grid cells.
#[derive(Default)]
pub struct FloatingOriginDebugPlugin<P: GridPrecision>(PhantomData<P>);
impl<P: GridPrecision> Plugin for FloatingOriginDebugPlugin<P> {
    fn build(&self, app: &mut App) {
        app.add_plugin(bevy_polyline::PolylinePlugin)
            .add_system_to_stage(CoreStage::Update, build_cube)
            .add_system_to_stage(
                CoreStage::PostUpdate,
                update_debug_bounds::<P>
                    .after(crate::recenter_transform_on_grid::<P>)
                    .before(crate::update_global_from_grid::<P>),
            );
    }
}

/// Marks entities that are used to render grid cell bounds.
#[derive(Component, Reflect)]
pub struct DebugBounds;

/// A resource that holds the handles to use for the debug bound polylines.
#[derive(Resource, Reflect)]
pub struct CubePolyline {
    polyline: Handle<Polyline>,
    material: Handle<PolylineMaterial>,
    origin_matl: Handle<PolylineMaterial>,
}

/// Update the rendered debug bounds to only highlight occupied [`GridCell`]s. [`DebugBounds`] are
/// spawned or hidden as needed.
pub fn update_debug_bounds<P: GridPrecision>(
    mut commands: Commands,
    cube_polyline: Res<CubePolyline>,
    occupied_cells: Query<(&GridCell<P>, Option<&FloatingOrigin>), Without<DebugBounds>>,
    mut debug_bounds: Query<
        (
            &mut GridCell<P>,
            &mut Handle<Polyline>,
            &mut Handle<PolylineMaterial>,
            &mut Visibility,
        ),
        With<DebugBounds>,
    >,
) {
    let mut occupied_cells = HashMap::from_iter(occupied_cells.iter()).into_iter();

    for (mut cell, mut polyline, mut matl, mut visibility) in &mut debug_bounds {
        if cube_polyline.is_changed() {
            *polyline = cube_polyline.polyline.clone();
        }
        if let Some((occupied_cell, has_origin)) = occupied_cells.next() {
            visibility.is_visible = true;
            *cell = *occupied_cell;
            if has_origin.is_some() {
                *matl = cube_polyline.origin_matl.clone();
            } else {
                *matl = cube_polyline.material.clone();
            }
        } else {
            // If there are more debug bounds than occupied cells, hide the extras.
            visibility.is_visible = false;
        }
    }

    // If there are still occupied cells but no more debug bounds, we need to spawn more.
    for (occupied_cell, has_origin) in occupied_cells {
        let material = if has_origin.is_some() {
            cube_polyline.origin_matl.clone()
        } else {
            cube_polyline.material.clone()
        };
        commands.spawn((
            SpatialBundle::default(),
            cube_polyline.polyline.clone(),
            material,
            occupied_cell.to_owned(),
            DebugBounds,
        ));
    }
}

/// Construct a polyline to match the [`FloatingOriginSettings`].
pub fn build_cube(
    settings: Res<FloatingOriginSettings>,
    mut commands: Commands,
    mut polyline_materials: ResMut<Assets<PolylineMaterial>>,
    mut polylines: ResMut<Assets<Polyline>>,
) {
    if !settings.is_changed() {
        return;
    }

    let s = settings.grid_edge_length / 2.001;

    /*
        (2)-----(3)               Y
         | \     | \              |
         |  (1)-----(0) MAX       o---X
         |   |   |   |             \
    MIN (6)--|--(7)  |              Z
           \ |     \ |
            (5)-----(4)
     */

    let indices = [
        0, 1, 1, 2, 2, 3, 3, 0, // Top ring
        4, 5, 5, 6, 6, 7, 7, 4, // Bottom ring
        0, 4, 8, 1, 5, 8, 2, 6, 8, 3, 7, // Verticals (8's are NaNs)
    ];

    let vertices = [
        Vec3::new(s, s, s),
        Vec3::new(-s, s, s),
        Vec3::new(-s, s, -s),
        Vec3::new(s, s, -s),
        Vec3::new(s, -s, s),
        Vec3::new(-s, -s, s),
        Vec3::new(-s, -s, -s),
        Vec3::new(s, -s, -s),
        Vec3::NAN,
    ];

    let vertices = [
        vertices[indices[0]],
        vertices[indices[1]],
        vertices[indices[2]],
        vertices[indices[3]],
        vertices[indices[4]],
        vertices[indices[5]],
        vertices[indices[6]],
        vertices[indices[7]],
        vertices[indices[8]],
        vertices[indices[9]],
        vertices[indices[10]],
        vertices[indices[11]],
        vertices[indices[12]],
        vertices[indices[13]],
        vertices[indices[14]],
        vertices[indices[15]],
        vertices[indices[16]],
        vertices[indices[17]],
        vertices[indices[18]],
        vertices[indices[19]],
        vertices[indices[20]],
        vertices[indices[21]],
        vertices[indices[22]],
        vertices[indices[23]],
        vertices[indices[24]],
        vertices[indices[25]],
        vertices[indices[26]],
    ];

    let polyline = polylines.add(Polyline {
        vertices: vertices.into(),
    });

    let material = polyline_materials.add(PolylineMaterial {
        width: 1.5,
        color: Color::rgb(2.0, 0.0, 0.0),
        perspective: false,
        ..Default::default()
    });

    let origin_matl = polyline_materials.add(PolylineMaterial {
        width: 1.5,
        color: Color::rgb(0.0, 0.0, 2.0),
        perspective: false,
        ..Default::default()
    });

    commands.insert_resource(CubePolyline {
        polyline,
        material,
        origin_matl,
    })
}
