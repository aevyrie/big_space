//! This example demonstrates what floating point error in rendering looks like. You can press
//! spacebar to smoothly switch between enabling and disabling the floating origin.
//!
//! Instead of disabling the plugin outright, this example simply moves the floating origin
//! independently from the camera, which is equivalent to what would happen when moving far from the
//! origin when not using this plugin.

use bevy::prelude::*;
use big_space::{FloatingOrigin, FloatingSpatialBundle, GridCell};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
        .add_plugin(big_space::FloatingOriginPlugin::<i64>::default())
        .add_startup_system(setup_scene)
        .add_startup_system(setup_ui)
        .add_system(rotator_system)
        .add_system(toggle_plugin)
        .run()
}

/// You can put things really, really far away from the origin. The distance we use here is actually
/// quite small, because we want the cubes to still be visible when the floating origin is far from
/// the camera. If you go much further than this, the cubes will simply disappear in a *POOF* of
/// floating point error.
///
/// This plugin can function much further from the origin without any issues. Try setting this to:
/// 10_000_000_000_000_000_000_000_000_000_000_000_000
const DISTANCE: f32 = 10_000_000.0;

/// Move the floating origin back to the "true" origin when the user presses the spacebar to emulate
/// disabling the plugin. Normally you would make your active camera the floating origin to avoid
/// this issue.
fn toggle_plugin(
    input: Res<Input<KeyCode>>,
    mut text: Query<&mut Text>,
    mut state: Local<bool>,
    mut floating_origin: Query<&mut GridCell<i64>, With<FloatingOrigin>>,
) {
    if input.just_pressed(KeyCode::Space) {
        *state = !*state;
    }

    let mut cell = floating_origin.single_mut();
    let cell_max = (DISTANCE / 10_000f32) as i64;
    let i = cell_max / 200;

    let msg = if *state {
        if 0 <= cell.x - i {
            cell.x = 0.max(cell.x - i);
            cell.y = 0.max(cell.y - i);
            cell.z = 0.max(cell.z - i);
            "Disabling..."
        } else {
            "Floating Origin Disabled"
        }
    } else {
        if cell_max >= cell.x + i {
            cell.x = i64::min(cell_max, cell.x + i);
            cell.y = i64::min(cell_max, cell.y + i);
            cell.z = i64::min(cell_max, cell.z + i);
            "Enabling..."
        } else {
            "Floating Origin Enabled"
        }
    };

    let dist = (cell_max - cell.x) * 10_000;

    text.single_mut().sections[0].value =
        format!("Press Spacebar to toggle: {msg}\nCamera distance to floating origin: {dist}")
}

#[derive(Component)]
struct Rotator;

fn rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<Rotator>>) {
    for mut transform in &mut query {
        transform.rotate_x(3.0 * time.delta_seconds());
    }
}

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/FiraMono-Regular.ttf");
    commands.spawn(TextBundle {
        style: Style {
            align_self: AlignSelf::FlexStart,
            flex_direction: FlexDirection::Column,
            ..Default::default()
        },
        text: Text {
            sections: vec![TextSection {
                value: "hello: ".to_string(),
                style: TextStyle {
                    font: font.clone(),
                    font_size: 30.0,
                    color: Color::WHITE,
                },
            }],
            ..Default::default()
        },
        ..Default::default()
    });
}

/// set up a simple scene with a "parent" cube and a "child" cube
fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube_handle = meshes.add(Mesh::from(shape::Cube { size: 2.0 }));
    let cube_material_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    // Normally, we would put the floating origin on the camera. However in this example, we want to
    // show what happens as the camera is far from the origin, to emulate what happens when this
    // plugin isn't used.
    commands.spawn((
        FloatingSpatialBundle::<i64> {
            transform: Transform::from_translation(Vec3::splat(DISTANCE)),
            ..default()
        },
        FloatingOrigin,
    ));

    // parent cube
    commands
        .spawn(PbrBundle {
            mesh: cube_handle.clone(),
            material: cube_material_handle.clone(),
            transform: Transform::from_translation(Vec3::splat(DISTANCE)),
            ..default()
        })
        .insert(GridCell::<i64>::default())
        .insert(Rotator)
        .with_children(|parent| {
            // child cube
            parent.spawn(PbrBundle {
                mesh: cube_handle,
                material: cube_material_handle,
                transform: Transform::from_xyz(0.0, 0.0, 3.0),
                ..default()
            });
        });
    // light
    commands
        .spawn(PointLightBundle {
            transform: Transform::from_xyz(DISTANCE + 4.0, DISTANCE - 10.0, DISTANCE - 4.0),
            ..default()
        })
        .insert(GridCell::<i64>::default());
    // camera
    commands
        .spawn(Camera3dBundle {
            transform: Transform::from_xyz(DISTANCE + 8.0, DISTANCE - 8.0, DISTANCE)
                .looking_at(Vec3::splat(DISTANCE), Vec3::Y),
            ..default()
        })
        .insert(GridCell::<i64>::default());
}
