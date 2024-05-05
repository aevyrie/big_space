//! This example demonstrates what floating point error in rendering looks like. You can press
//! spacebar to smoothly switch between enabling and disabling the floating origin.
//!
//! Instead of disabling the plugin outright, this example simply moves the floating origin
//! independently from the camera, which is equivalent to what would happen when moving far from the
//! origin when not using this plugin.

use bevy::prelude::*;
use big_space::{
    reference_frame::RootReferenceFrame, FloatingOrigin, GridCell, IgnoreFloatingOrigin,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::FloatingOriginPlugin::<i128>::new(10.0, 1.0),
        ))
        .add_systems(Startup, (setup_scene, setup_ui))
        .add_systems(Update, (rotator_system, toggle_plugin))
        .run()
}

/// You can put things really, really far away from the origin. The distance we use here is actually
/// quite small, because we want the mesh to still be visible when the floating origin is far from
/// the camera. If you go much further than this, the mesh will simply disappear in a *POOF* of
/// floating point error.
///
/// This plugin can function much further from the origin without any issues. Try setting this to:
/// 10_000_000_000_000_000_000_000_000_000_000_000_000
const DISTANCE: i128 = 21_000_000;

/// Move the floating origin back to the "true" origin when the user presses the spacebar to emulate
/// disabling the plugin. Normally you would make your active camera the floating origin to avoid
/// this issue.
fn toggle_plugin(
    input: Res<ButtonInput<KeyCode>>,
    settings: Res<RootReferenceFrame<i128>>,
    mut text: Query<&mut Text>,
    mut disabled: Local<bool>,
    mut floating_origin: Query<&mut GridCell<i128>, With<FloatingOrigin>>,
) {
    if input.just_pressed(KeyCode::Space) {
        *disabled = !*disabled;
    }

    let mut origin_cell = floating_origin.single_mut();
    let index_max = DISTANCE / settings.cell_edge_length() as i128;
    let increment = index_max / 100;

    let msg = if *disabled {
        if origin_cell.x > 0 {
            origin_cell.x = 0.max(origin_cell.x - increment);
            "Disabling..."
        } else {
            "Floating Origin Disabled"
        }
    } else if origin_cell.x < index_max {
        origin_cell.x = index_max.min(origin_cell.x.saturating_add(increment));
        "Enabling..."
    } else {
        "Floating Origin Enabled"
    };

    let dist = index_max.saturating_sub(origin_cell.x) * settings.cell_edge_length() as i128;

    let thousands = |num: i128| {
        num.to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(std::str::from_utf8)
            .collect::<Result<Vec<&str>, _>>()
            .unwrap()
            .join(",") // separator
    };

    text.single_mut().sections[0].value =
        format!("Press Spacebar to toggle: {msg}\nCamera distance to floating origin: {}\nMesh distance from origin: {}", thousands(dist), thousands(DISTANCE))
}

#[derive(Component)]
struct Rotator;

fn rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<Rotator>>) {
    for mut transform in &mut query {
        transform.rotate_x(time.delta_seconds());
    }
}

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        TextBundle {
            style: Style {
                align_self: AlignSelf::FlexStart,
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            text: Text {
                sections: vec![TextSection {
                    value: "hello: ".to_string(),
                    style: TextStyle {
                        font_size: 30.0,
                        color: Color::WHITE,
                        ..default()
                    },
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        IgnoreFloatingOrigin::<0>,
    ));
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    reference_frame: Res<RootReferenceFrame<i128>>,
) {
    let mesh_handle = meshes.add(Sphere::new(1.5).mesh());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    let d = DISTANCE / reference_frame.cell_edge_length() as i128;
    let distant_grid_cell = GridCell::<i128>::new(d, d, d);

    // Normally, we would put the floating origin on the camera. However in this example, we want to
    // show what happens as the camera is far from the origin, to emulate what happens when this
    // plugin isn't used.
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::default().mesh()),
            material: materials.add(StandardMaterial::from(Color::RED)),
            transform: Transform::from_scale(Vec3::splat(10000.0)),
            ..default()
        },
        distant_grid_cell,
        FloatingOrigin::<0>,
    ));

    commands
        .spawn((
            PbrBundle {
                mesh: mesh_handle.clone(),
                material: matl_handle.clone(),
                ..default()
            },
            distant_grid_cell,
            Rotator,
        ))
        .with_children(|parent| {
            parent.spawn(PbrBundle {
                mesh: mesh_handle,
                material: matl_handle,
                transform: Transform::from_xyz(0.0, 0.0, 4.0),
                ..default()
            });
        });
    // light
    commands.spawn((
        DirectionalLightBundle {
            transform: Transform::from_xyz(4.0, -10.0, -4.0),
            ..default()
        },
        distant_grid_cell,
    ));
    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(8.0, -8.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        distant_grid_cell,
    ));
}
