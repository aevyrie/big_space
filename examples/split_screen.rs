//! Demonstrates how a single bevy world can contain multiple big_space hierarchies, each rendered
//! relative to a floating origin inside that big space.
//!
//! This takes the simplest approach, of simply duplicating the worlds and players for each split
//! screen, and synchronizing the player locations between both.

use bevy::{
    prelude::*,
    render::{camera::Viewport, view::RenderLayers},
    transform::TransformSystem,
};
use bevy_color::palettes;
use big_space::{
    camera::{CameraController, CameraControllerPlugin},
    commands::BigSpaceCommands,
    reference_frame::ReferenceFrame,
    world_query::{GridTransform, GridTransformReadOnly},
    BigSpacePlugin, FloatingOrigin,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::<i32>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i32>::default(),
            CameraControllerPlugin::<i32>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, set_camera_viewports)
        .add_systems(
            PostUpdate,
            update_cameras
                .after(big_space::camera::camera_controller::<i32>)
                .before(TransformSystem::TransformPropagate),
        )
        .run();
}

#[derive(Component)]
struct LeftCamera;

#[derive(Component)]
struct RightCamera;

#[derive(Component)]
struct LeftCameraReplicated;

#[derive(Component)]
struct RightCameraReplicated;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DirectionalLightBundle {
            transform: Transform::default().looking_to(Vec3::NEG_ONE, Vec3::Y),
            ..default()
        },
        RenderLayers::from_layers(&[1, 2]),
    ));

    // Big Space 1
    commands.spawn_big_space(ReferenceFrame::<i32>::default(), |root_frame| {
        root_frame
            .spawn_spatial((
                Camera3dBundle {
                    transform: Transform::from_xyz(1_000_000.0 - 10.0, 100_005.0, 0.0)
                        .looking_to(Vec3::NEG_X, Vec3::Y),
                    ..default()
                },
                CameraController::default().with_smoothness(0.8, 0.8),
                RenderLayers::layer(2),
                LeftCamera,
                FloatingOrigin,
            ))
            .with_children(|child_builder| {
                child_builder.spawn((
                    PbrBundle {
                        mesh: meshes.add(Cuboid::new(1.0, 2.0, 1.0)),
                        material: materials.add(StandardMaterial {
                            base_color: Color::Srgba(palettes::css::YELLOW),
                            ..default()
                        }),
                        ..default()
                    },
                    RenderLayers::layer(2),
                ));
            });

        root_frame.spawn_spatial((
            RightCameraReplicated,
            PbrBundle {
                mesh: meshes.add(Cuboid::new(1.0, 2.0, 1.0)),
                material: materials.add(StandardMaterial {
                    base_color: Color::Srgba(palettes::css::FUCHSIA),
                    ..default()
                }),
                ..default()
            },
            RenderLayers::layer(2),
        ));

        root_frame.spawn_spatial((
            PbrBundle {
                mesh: meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap()),
                material: materials.add(StandardMaterial {
                    base_color: Color::Srgba(palettes::css::BLUE),
                    ..default()
                }),
                transform: Transform::from_xyz(1_000_000.0, 0.0, 0.0)
                    .with_scale(Vec3::splat(100_000.0)),
                ..default()
            },
            RenderLayers::layer(2),
        ));

        root_frame.spawn_spatial((
            PbrBundle {
                mesh: meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap()),
                material: materials.add(StandardMaterial {
                    base_color: Color::Srgba(palettes::css::GREEN),
                    ..default()
                }),
                transform: Transform::from_xyz(-1_000_000.0, 0.0, 0.0)
                    .with_scale(Vec3::splat(100_000.0)),
                ..default()
            },
            RenderLayers::layer(2),
        ));
    });

    // Big Space 2
    commands.spawn_big_space(ReferenceFrame::<i32>::default(), |root_frame| {
        root_frame
            .spawn_spatial((
                Camera3dBundle {
                    transform: Transform::from_xyz(1_000_000.0, 100_005.0, 0.0)
                        .looking_to(Vec3::NEG_X, Vec3::Y),
                    camera: Camera {
                        order: 1,
                        clear_color: ClearColorConfig::None,
                        ..default()
                    },
                    ..default()
                },
                RenderLayers::layer(1),
                RightCamera,
                FloatingOrigin,
            ))
            .with_children(|child_builder| {
                child_builder.spawn((
                    PbrBundle {
                        mesh: meshes.add(Cuboid::new(1.0, 2.0, 1.0)),
                        material: materials.add(StandardMaterial {
                            base_color: Color::Srgba(palettes::css::PINK),
                            ..default()
                        }),
                        ..default()
                    },
                    RenderLayers::layer(1),
                ));
            });

        root_frame.spawn_spatial((
            LeftCameraReplicated,
            PbrBundle {
                mesh: meshes.add(Cuboid::new(1.0, 2.0, 1.0)),
                material: materials.add(StandardMaterial {
                    base_color: Color::Srgba(palettes::css::YELLOW),
                    ..default()
                }),
                ..default()
            },
            RenderLayers::layer(1),
        ));

        root_frame.spawn_spatial((
            PbrBundle {
                mesh: meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap()),
                material: materials.add(StandardMaterial {
                    base_color: Color::Srgba(palettes::css::BLUE),
                    ..default()
                }),
                transform: Transform::from_xyz(1_000_000.0, 0.0, 0.0)
                    .with_scale(Vec3::splat(100_000.0)),
                ..default()
            },
            RenderLayers::layer(1),
        ));

        root_frame.spawn_spatial((
            PbrBundle {
                mesh: meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap()),
                material: materials.add(StandardMaterial {
                    base_color: Color::Srgba(palettes::css::GREEN),
                    ..default()
                }),
                transform: Transform::from_xyz(-1_000_000.0, 0.0, 0.0)
                    .with_scale(Vec3::splat(100_000.0)),
                ..default()
            },
            RenderLayers::layer(1),
        ));
    });
}

#[allow(clippy::type_complexity)]
fn update_cameras(
    left: Query<GridTransformReadOnly<i32>, With<LeftCamera>>,
    mut left_rep: Query<
        GridTransform<i32>,
        (
            With<LeftCameraReplicated>,
            Without<RightCameraReplicated>,
            Without<LeftCamera>,
            Without<RightCamera>,
        ),
    >,
    right: Query<GridTransformReadOnly<i32>, With<RightCamera>>,
    mut right_rep: Query<
        GridTransform<i32>,
        (
            With<RightCameraReplicated>,
            Without<LeftCameraReplicated>,
            Without<LeftCamera>,
            Without<RightCamera>,
        ),
    >,
) {
    *left_rep.single_mut().cell = *left.single().cell;
    *left_rep.single_mut().transform = *left.single().transform;

    *right_rep.single_mut().cell = *right.single().cell;
    *right_rep.single_mut().transform = *right.single().transform;
}

fn set_camera_viewports(
    windows: Query<&Window>,
    mut resize_events: EventReader<bevy::window::WindowResized>,
    mut left_camera: Query<&mut Camera, (With<LeftCamera>, Without<RightCamera>)>,
    mut right_camera: Query<&mut Camera, With<RightCamera>>,
) {
    // We need to dynamically resize the camera's viewports whenever the window size changes
    // so then each camera always takes up half the screen.
    // A resize_event is sent when the window is first created, allowing us to reuse this system for initial setup.
    for resize_event in resize_events.read() {
        let window = windows.get(resize_event.window).unwrap();
        let mut left_camera = left_camera.single_mut();
        left_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(0, 0),
            physical_size: UVec2::new(
                window.resolution.physical_width() / 2,
                window.resolution.physical_height(),
            ),
            ..default()
        });

        let mut right_camera = right_camera.single_mut();
        right_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(window.resolution.physical_width() / 2, 0),
            physical_size: UVec2::new(
                window.resolution.physical_width() / 2,
                window.resolution.physical_height(),
            ),
            ..default()
        });
    }
}
