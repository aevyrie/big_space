//! This example demonstrates what floating point error in rendering looks like. You can press
//! spacebar to smoothly switch between enabling and disabling the floating origin.
//!
//! Instead of disabling the plugin outright, this example simply moves the floating origin
//! independently from the camera, which is equivalent to what would happen when moving far from the
//! origin when not using this plugin.

use bevy::prelude::*;
use big_space::{
    commands::BigSpaceCommands,
    reference_frame::{local_origin::ReferenceFrames, ReferenceFrame},
    FloatingOrigin, GridCell,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::BigSpacePlugin::<i128>::default(),
        ))
        .add_systems(Startup, (setup_scene, setup_ui))
        .add_systems(Update, (rotator_system, toggle_plugin))
        .run();
}

/// You can put things really, really far away from the origin. The distance we use here is actually
/// quite small, because we want the mesh to still be visible when the floating origin is far from
/// the camera. If you go much further than this, the mesh will simply disappear in a *POOF* of
/// floating point error when we disable this plugin.
///
/// This plugin can function much further from the origin without any issues. Try setting this to:
/// 10_000_000_000_000_000_000_000_000_000_000_000_000
const DISTANCE: i128 = 2_000_000;

/// Move the floating origin back to the "true" origin when the user presses the spacebar to emulate
/// disabling the plugin. Normally you would make your active camera the floating origin to avoid
/// this issue.
fn toggle_plugin(
    input: Res<ButtonInput<KeyCode>>,
    ref_frames: ReferenceFrames<i128>,
    mut text: Query<&mut Text>,
    mut disabled: Local<bool>,
    mut floating_origin: Query<(Entity, &mut GridCell<i128>), With<FloatingOrigin>>,
) {
    if input.just_pressed(KeyCode::Space) {
        *disabled = !*disabled;
    }

    let this_frame = ref_frames.parent_frame(floating_origin.single().0).unwrap();

    let mut origin_cell = floating_origin.single_mut().1;
    let index_max = DISTANCE / this_frame.cell_edge_length() as i128;
    let increment = index_max / 100;

    let msg = if *disabled {
        if origin_cell.x > 0 {
            origin_cell.x = 0.max(origin_cell.x - increment);
            origin_cell.y = 0.max(origin_cell.y - increment);
            origin_cell.z = 0.max(origin_cell.z - increment);

            "Disabling..."
        } else {
            "Floating Origin Disabled"
        }
    } else if origin_cell.x < index_max {
        origin_cell.x = index_max.min(origin_cell.x.saturating_add(increment));
        origin_cell.y = index_max.min(origin_cell.y.saturating_add(increment));
        origin_cell.z = index_max.min(origin_cell.z.saturating_add(increment));
        "Enabling..."
    } else {
        "Floating Origin Enabled"
    };

    let dist = index_max.saturating_sub(origin_cell.x) * this_frame.cell_edge_length() as i128;

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
        transform.rotate_y(time.delta_seconds());
    }
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(
        TextBundle::from_section(
            "",
            TextStyle {
                font_size: 30.0,
                ..default()
            },
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        }),
    );
}

fn setup_scene(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn_big_space(ReferenceFrame::<i128>::default(), |root| {
        let d = DISTANCE / root.frame().cell_edge_length() as i128;
        let distant_grid_cell = GridCell::<i128>::new(d, d, d);

        // Normally, we would put the floating origin on the camera. However in this example, we
        // want to show what happens as the camera is far from the origin, to emulate what
        // happens when this plugin isn't used.
        root.spawn_spatial((distant_grid_cell, FloatingOrigin));

        root.spawn_spatial((
            SceneBundle {
                scene: asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0"),
                transform: Transform::from_scale(Vec3::splat(0.2)),
                ..default()
            },
            distant_grid_cell,
            Rotator,
        ))
        .with_children(|parent| {
            parent.spawn(SceneBundle {
                scene: asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0"),
                transform: Transform::from_xyz(0.0, 0.0, 20.0),
                ..default()
            });
        });
        // light
        root.spawn_spatial((
            DirectionalLightBundle {
                transform: Transform::from_xyz(4.0, -10.0, -4.0),
                ..default()
            },
            distant_grid_cell,
        ));
        // camera
        root.spawn_spatial((
            Camera3dBundle {
                transform: Transform::from_xyz(8.0, 8.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
                ..default()
            },
            distant_grid_cell,
        ));
    });
}
