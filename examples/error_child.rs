//! This example demonstrates error accumulating from parent to children in nested reference frames.
use bevy::{math::DVec3, prelude::*};
use big_space::{
    reference_frame::{ReferenceFrame, RootReferenceFrame},
    FloatingOrigin, GridCell,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::FloatingOriginPlugin::<i128>::default(),
            big_space::camera::CameraControllerPlugin::<i128>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i128>::default(),
        ))
        .add_systems(Startup, setup_scene)
        .run()
}

// The nearby object is 200 meters away from us. The distance object is 100 quadrillion meters away
// from us, and has a child that is 100 quadrillion meters toward us (relative its parent) minus 200
// meters. If we had infinite precision, the child of the distant object would be at the same
// position as the nearby object, only 200 meters away.
const DISTANT: DVec3 = DVec3::new(1e17, 1e17, 0.0);
const NEARBY: DVec3 = DVec3::new(200.0, 200.0, 0.0);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    root: Res<RootReferenceFrame<i128>>,
) {
    let mesh_handle = meshes.add(Sphere::new(0.5).mesh());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    // A red sphere located nearby
    commands.spawn((
        PbrBundle {
            mesh: mesh_handle.clone(),
            material: materials.add(Color::RED),
            transform: Transform::from_translation(NEARBY.as_vec3()),
            ..default()
        },
        GridCell::<i128>::default(),
    ));

    let parent = root.translation_to_grid(DISTANT);
    // This function introduces a small amount of error, because it can only work up to double
    // precision floats. (f64).
    let child = root.translation_to_grid(-DISTANT + NEARBY);

    commands
        .spawn((
            // A sphere very far from the origin
            PbrBundle {
                mesh: mesh_handle.clone(),
                material: matl_handle.clone(),
                transform: Transform::from_translation(parent.1),
                ..default()
            },
            parent.0,
            ReferenceFrame::<i128>::default(),
        ))
        .with_children(|parent| {
            // A green sphere that is a child of the sphere very far from the origin. This child is
            // very far from its parent, and should be located exactly at the origin (if there was
            // no floating point error). The distance from the green sphere to the red sphere is the
            // error caused by float imprecision. Note that the sphere does not have any rendering
            // artifacts, its position just has a fixed error.
            parent.spawn((
                PbrBundle {
                    mesh: mesh_handle,
                    material: materials.add(Color::GREEN),
                    transform: Transform::from_translation(child.1),
                    ..default()
                },
                child.0,
            ));
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
            transform: Transform::from_translation(NEARBY.as_vec3() + Vec3::new(0.0, 0.0, 20.0))
                .looking_at(NEARBY.as_vec3(), Vec3::Y),
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
