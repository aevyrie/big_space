//! This example demonstrates error accumulating from parent to children in nested reference frames.
use bevy::{math::DVec3, prelude::*};
use big_space::{
    bundles::BigSpaceBundle, reference_frame::ReferenceFrame, FloatingOrigin, GridCell,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
            big_space::BigSpacePlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
        ))
        .add_systems(Startup, setup_scene)
        .run()
}

// The nearby object is 200 meters away from us. The distance object is 1 million kilometers away
// from us, and has a child that is 1 million kilometers toward us (relative its parent) minus 200
// meters.
const DISTANT: DVec3 = DVec3::new(1e9, 1e9, 1e9);
const NEARBY: DVec3 = DVec3::new(200.0, 200.0, 0.0);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_handle = meshes.add(Sphere::new(0.01).mesh());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    let root_frame = ReferenceFrame::<i64>::default();

    commands
        .spawn(BigSpaceBundle::<i64>::default())
        .with_children(|root| {
            // A red sphere located nearby
            root.spawn((
                PbrBundle {
                    mesh: mesh_handle.clone(),
                    material: materials.add(Color::RED),
                    transform: Transform::from_translation(NEARBY.as_vec3()),
                    ..default()
                },
                GridCell::<i64>::default(),
            ));

            let parent = root_frame.translation_to_grid(DISTANT);
            // This function introduces a small amount of error, because it can only work up to
            // double precision floats. (f64).
            let child = root_frame.translation_to_grid(-DISTANT + NEARBY);

            root.spawn((
                // A sphere very far from the origin
                PbrBundle {
                    mesh: mesh_handle.clone(),
                    material: matl_handle.clone(),
                    transform: Transform::from_translation(parent.1),
                    ..default()
                },
                parent.0,
                ReferenceFrame::<i64>::default(),
            ))
            .with_children(|parent| {
                // A green sphere that is a child of the sphere very far from the origin. This child
                // is very far from its parent, and should be located exactly at the origin (if
                // there was no floating point error). The distance from the green sphere to the red
                // sphere is the error caused by float imprecision. Note that the sphere does not
                // have any rendering artifacts, its position just has a fixed error.
                parent.spawn((
                    PbrBundle {
                        mesh: mesh_handle,
                        material: materials.add(Color::GREEN),
                        transform: Transform::from_translation(child.1),
                        ..default()
                    },
                    child.0,
                ));
                // todo!("Seems like there is an error here. The green sphere seems to follow the camera when it changes grid cells, but the red one does not.")
            });
            // light
            root.spawn((
                DirectionalLightBundle {
                    transform: Transform::from_xyz(4.0, -10.0, -4.0),
                    ..default()
                },
                GridCell::<i64>::default(),
            ));
            // camera
            root.spawn((
                Camera3dBundle {
                    transform: Transform::from_translation(
                        NEARBY.as_vec3() + Vec3::new(0.0, 0.0, 0.5),
                    )
                    .looking_at(NEARBY.as_vec3(), Vec3::Y),
                    ..default()
                },
                GridCell::<i64>::default(),
                FloatingOrigin,
                big_space::camera::CameraController::default() // Built-in camera controller
                    .with_speed_bounds([10e-18, 10e35])
                    .with_smoothness(0.9, 0.8)
                    .with_speed(1.0),
            ));
        })
        .insert(root_frame);
}
