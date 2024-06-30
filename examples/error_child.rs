//! This example demonstrates error accumulating from parent to children in nested reference frames.
use bevy::{math::DVec3, prelude::*};
use bevy_color::palettes;
use big_space::{commands::BigSpaceCommands, reference_frame::ReferenceFrame, FloatingOrigin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            // bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
            big_space::BigSpacePlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
        ))
        .add_systems(Startup, setup_scene)
        .run();
}

// The nearby object is NEARBY meters away from us. The distance object is DISTANT meters away from
// us, and has a child that is DISTANT meters toward us (relative its parent) minus NEARBY meters.
const DISTANT: DVec3 = DVec3::new(1e10, 1e10, 1e10);
const SPHERE_RADIUS: f32 = 10.0;
const NEARBY: Vec3 = Vec3::new(SPHERE_RADIUS * 20.0, SPHERE_RADIUS * 20.0, 0.0);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_handle = meshes.add(Sphere::new(SPHERE_RADIUS).mesh());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.7, 0.6),
        ..default()
    });

    commands.spawn_big_space(
        ReferenceFrame::<i64>::new(SPHERE_RADIUS * 100.0, 0.0),
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
                        material: matl_handle.clone(),
                        transform: Transform::from_translation(parent.1),
                        ..default()
                    });
                    parent_frame.insert(parent.0);

                    parent_frame.with_children(|child_builder| {
                        // A green sphere that is a child of the sphere very far from the origin.
                        // This child is very far from its parent, and should be located exactly at
                        // the origin (if there was no floating point error). The distance from the
                        // green sphere to the red sphere is the error caused by float imprecision.
                        // Note that the sphere does not have any rendering artifacts, its position
                        // just has a fixed error.
                        child_builder.spawn((
                            PbrBundle {
                                mesh: mesh_handle,
                                material: materials.add(Color::from(palettes::css::GREEN)),
                                transform: Transform::from_translation(child.1),
                                ..default()
                            },
                            child.0,
                        ));
                    });
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
