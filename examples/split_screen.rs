//! Demonstrates how a single bevy world can contain multiple `big_space` hierarchies, each rendered
//! relative to a floating origin inside that big space.
//!
//! This takes the simplest approach, of simply duplicating the worlds and players for each split
//! screen, and synchronizing the player locations between both.

use bevy::{
    camera::{visibility::RenderLayers, Viewport},
    color::palettes,
    prelude::*,
    transform::TransformSystems,
};
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpaceDefaultPlugins,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, set_camera_viewports)
        .add_systems(
            PostUpdate,
            update_cameras
                .after(big_space::camera::camera_controller)
                .before(TransformSystems::Propagate),
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
        DirectionalLight::default(),
        Transform::default().looking_to(Vec3::NEG_ONE, Vec3::Y),
        RenderLayers::from_layers(&[1, 2]),
    ));

    // Big Space 1
    commands.spawn_big_space_default(|root| {
        root.spawn_spatial((
            Camera3d::default(),
            Transform::from_xyz(1_000_000.0 - 10.0, 100_005.0, 0.0)
                .looking_to(Vec3::NEG_X, Vec3::Y),
            BigSpaceCameraController::default().with_smoothness(0.8, 0.8),
            RenderLayers::layer(2),
            LeftCamera,
            FloatingOrigin,
        ))
        .with_child((
            Mesh3d(meshes.add(Cuboid::new(1.0, 2.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::YELLOW),
                ..default()
            })),
            RenderLayers::layer(2),
        ));

        root.spawn_spatial((
            RightCameraReplicated,
            Mesh3d(meshes.add(Cuboid::new(1.0, 2.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::FUCHSIA),
                ..default()
            })),
            RenderLayers::layer(2),
        ));

        root.spawn_spatial((
            Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::BLUE),
                ..default()
            })),
            Transform::from_xyz(1_000_000.0, 0.0, 0.0).with_scale(Vec3::splat(100_000.0)),
            RenderLayers::layer(2),
        ));

        root.spawn_spatial((
            Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::GREEN),
                ..default()
            })),
            Transform::from_xyz(-1_000_000.0, 0.0, 0.0).with_scale(Vec3::splat(100_000.0)),
            RenderLayers::layer(2),
        ));
    });

    // Big Space 2
    commands.spawn_big_space_default(|root| {
        root.spawn_spatial((
            Camera3d::default(),
            Camera {
                order: 1,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Transform::from_xyz(1_000_000.0, 100_005.0, 0.0).looking_to(Vec3::NEG_X, Vec3::Y),
            RenderLayers::layer(1),
            RightCamera,
            FloatingOrigin,
        ))
        .with_child((
            Mesh3d(meshes.add(Cuboid::new(1.0, 2.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::FUCHSIA),
                ..default()
            })),
            RenderLayers::layer(1),
        ));

        root.spawn_spatial((
            LeftCameraReplicated,
            Mesh3d(meshes.add(Cuboid::new(1.0, 2.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::YELLOW),
                ..default()
            })),
            RenderLayers::layer(1),
        ));

        root.spawn_spatial((
            Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::BLUE),
                ..default()
            })),
            Transform::from_xyz(1_000_000.0, 0.0, 0.0).with_scale(Vec3::splat(100_000.0)),
            RenderLayers::layer(1),
        ));

        root.spawn_spatial((
            Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::Srgba(palettes::css::GREEN),
                ..default()
            })),
            Transform::from_xyz(-1_000_000.0, 0.0, 0.0).with_scale(Vec3::splat(100_000.0)),
            RenderLayers::layer(1),
        ));
    });
}

#[allow(clippy::type_complexity)]
fn update_cameras(
    left: Query<CellTransformReadOnly, With<LeftCamera>>,
    mut left_rep: Query<
        CellTransform,
        (
            With<LeftCameraReplicated>,
            Without<RightCameraReplicated>,
            Without<LeftCamera>,
            Without<RightCamera>,
        ),
    >,
    right: Query<CellTransformReadOnly, With<RightCamera>>,
    mut right_rep: Query<
        CellTransform,
        (
            With<RightCameraReplicated>,
            Without<LeftCameraReplicated>,
            Without<LeftCamera>,
            Without<RightCamera>,
        ),
    >,
) -> Result {
    *left_rep.single_mut()?.cell = *left.single()?.cell;
    *left_rep.single_mut()?.transform = *left.single()?.transform;

    *right_rep.single_mut()?.cell = *right.single()?.cell;
    *right_rep.single_mut()?.transform = *right.single()?.transform;

    Ok(())
}

fn set_camera_viewports(
    windows: Query<&Window>,
    mut resize_events: MessageReader<bevy::window::WindowResized>,
    mut left_camera: Query<&mut Camera, (With<LeftCamera>, Without<RightCamera>)>,
    mut right_camera: Query<&mut Camera, With<RightCamera>>,
) -> Result {
    // We need to dynamically resize the camera's viewports whenever the window size changes
    // so then each camera always takes up half the screen.
    // A resize_event is sent when the window is first created, allowing us to reuse this system for initial setup.
    for resize_event in resize_events.read() {
        let window = windows.get(resize_event.window)?;
        let mut left_camera = left_camera.single_mut()?;
        left_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(0, 0),
            physical_size: UVec2::new(
                window.resolution.physical_width() / 2,
                window.resolution.physical_height(),
            ),
            ..default()
        });

        let mut right_camera = right_camera.single_mut()?;
        right_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(window.resolution.physical_width() / 2, 0),
            physical_size: UVec2::new(
                window.resolution.physical_width() / 2,
                window.resolution.physical_height(),
            ),
            ..default()
        });
    }

    Ok(())
}
