//! This example demonstrates error accumulating from parent to children in nested reference frames.
use bevy::{math::DVec3, prelude::*};
use bevy_color::palettes;
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
        ))
        .add_systems(Startup, setup_scene)
        .run();
}

// The nearby object is NEARBY meters away from us. The distance object is DISTANT meters away from
// us, and has a child that is DISTANT meters toward us (relative its parent) minus NEARBY meters.
//
// The result is two spheres that should perfectly overlap, even though one of those spheres is a
// child of an object more than one quadrillion meters away. This example intentionally results in a
// small amount of error, to demonstrate the scales and precision available even between different
// reference frames.
//
// Note that as you increase the distance further, there are still no rendering errors, and the
// green sphere does not vanish, however, as you move farther away, you will see that the green
// sphere will pop into neighboring cells due to rounding error.
const DISTANT: DVec3 = DVec3::new(1e17, 1e17, 1e17);
const SPHERE_RADIUS: f32 = 1.0;
const NEARBY: Vec3 = Vec3::new(SPHERE_RADIUS * 20.0, SPHERE_RADIUS * 20.0, 0.0);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_handle = meshes.add(Sphere::new(SPHERE_RADIUS).mesh());

    commands.spawn_big_space::<i64>(
        ReferenceFrame::new(SPHERE_RADIUS * 100.0, 0.0),
        |root_frame| {
            root_frame.spawn_spatial(PbrBundle {
                mesh: mesh_handle.clone(),
                material: materials.add(Color::from(palettes::css::BLUE)),
                transform: Transform::from_translation(NEARBY),
                ..default()
            });

            let parent = root_frame.frame().translation_to_grid(DISTANT);
            root_frame.with_frame(
                ReferenceFrame::new(SPHERE_RADIUS * 100.0, 0.0),
                |parent_frame| {
                    // This function introduces a small amount of error, because it can only work up
                    // to double precision floats. (f64).
                    let child = parent_frame
                        .frame()
                        .translation_to_grid(-DISTANT + NEARBY.as_dvec3());
                    parent_frame.insert(PbrBundle {
                        mesh: mesh_handle.clone(),
                        material: materials.add(Color::from(palettes::css::RED)),
                        transform: Transform::from_translation(parent.1),
                        ..default()
                    });
                    parent_frame.insert(parent.0);

                    // A green sphere that is a child of the sphere very far from the origin. This
                    // child is very far from its parent, and should be located exactly at the
                    // NEARBY position (if there was no floating point error). The distance from the
                    // green sphere to the blue sphere is the error caused by float imprecision.
                    // Note that the sphere does not have any rendering artifacts, its position just
                    // has a fixed error.
                    parent_frame.spawn((
                        PbrBundle {
                            mesh: mesh_handle,
                            material: materials.add(Color::from(palettes::css::GREEN)),
                            transform: Transform::from_translation(child.1),
                            ..default()
                        },
                        child.0,
                    ));
                },
            );

            root_frame.spawn_spatial(DirectionalLightBundle {
                transform: Transform::from_xyz(4.0, -10.0, -4.0),
                ..default()
            });

            root_frame.spawn_spatial((
                Camera3dBundle {
                    transform: Transform::from_translation(
                        NEARBY + Vec3::new(0.0, 0.0, SPHERE_RADIUS * 10.0),
                    )
                    .looking_at(NEARBY, Vec3::Y),
                    projection: Projection::Perspective(PerspectiveProjection {
                        near: (SPHERE_RADIUS * 0.1).min(0.1),
                        ..default()
                    }),
                    ..default()
                },
                FloatingOrigin,
                big_space::camera::CameraController::default() // Built-in camera controller
                    .with_speed_bounds([10e-18, 10e35])
                    .with_smoothness(0.9, 0.8)
                    .with_speed(1.0),
            ));
        },
    );
}
