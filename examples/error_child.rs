//! This example demonstrates what floating point error in rendering looks like. You can press
//! spacebar to smoothly switch between enabling and disabling the floating origin.
//!
//! Instead of disabling the plugin outright, this example simply moves the floating origin
//! independently from the camera, which is equivalent to what would happen when moving far from the
//! origin when not using this plugin.

use bevy::prelude::*;
use big_space::{FloatingOrigin, GridCell};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::FloatingOriginPlugin::<i128>::default(),
            big_space::camera::CameraControllerPlugin::<i128>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i128>::default(),
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, cursor_grab_system)
        .run()
}

/// You can put things really, really far away from the origin. The distance we use here is actually
/// quite small, because we want the mesh to still be visible when the floating origin is far from
/// the camera. If you go much further than this, the mesh will simply disappear in a *POOF* of
/// floating point error.
///
/// This plugin can function much further from the origin without any issues. Try setting this to:
/// 10_000_000_000_000_000_000_000_000_000_000_000_000
const DISTANCE: f32 = 200_000_000f32;
const ORIGIN: f32 = 200.0;

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_handle = meshes.add(Sphere::new(1.5).mesh());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    commands.spawn((
        PbrBundle {
            mesh: mesh_handle.clone(),
            material: materials.add(Color::RED),
            transform: Transform::from_xyz(0.0, 0.0, ORIGIN),
            ..default()
        },
        GridCell::<i128>::default(),
    ));

    commands
        .spawn((
            PbrBundle {
                mesh: mesh_handle.clone(),
                material: matl_handle.clone(),
                transform: Transform::from_xyz(0.0, 0.0, DISTANCE),
                ..default()
            },
            GridCell::<i128>::default(),
        ))
        .with_children(|parent| {
            parent.spawn(PbrBundle {
                mesh: mesh_handle,
                material: materials.add(Color::GREEN),
                transform: Transform::from_xyz(0.0, 0.0, -DISTANCE + ORIGIN),
                ..default()
            });
        });
    // light
    commands.spawn((
        DirectionalLightBundle {
            transform: Transform::from_xyz(4.0, -10.0, -4.0),
            ..default()
        },
        GridCell::<i128>::default(),
    ));
    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(8.0, 0.0, ORIGIN).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        GridCell::<i128>::default(),
        FloatingOrigin,
        big_space::camera::CameraController::default() // Built-in camera controller
            .with_speed_bounds([10e-18, 10e35])
            .with_smoothness(0.9, 0.8)
            .with_speed(1.0),
    ));
}

fn cursor_grab_system(
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut cam: ResMut<big_space::camera::CameraInput>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let Some(mut window) = windows.get_single_mut().ok() else {
        return;
    };

    if btn.just_pressed(MouseButton::Left) {
        window.cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        window.cursor.visible = false;
        window.mode = bevy::window::WindowMode::BorderlessFullscreen;
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
        window.cursor.visible = true;
        window.mode = bevy::window::WindowMode::Windowed;
        cam.defaults_disabled = true;
    }
}
