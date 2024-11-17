//! Big spaces are infinite, looping back on themselves smoothly.

use bevy::prelude::*;
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(), // Replaced by big_space
            BigSpacePlugin::<i8>::default(),
            FloatingOriginDebugPlugin::<i8>::default(), // Draws cell AABBs and reference frames
            big_space::camera::CameraControllerPlugin::<i8>::default(), // Compatible controller
        ))
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sphere = meshes.add(Sphere::default());
    let matl = materials.add(Color::WHITE);

    commands.spawn_big_space(ReferenceFrame::<i8>::default(), |root_frame| {
        let range = || -8..8;
        for x in range() {
            for y in range() {
                for z in range() {
                    root_frame.spawn_spatial((
                        PbrBundle {
                            mesh: sphere.clone(),
                            material: matl.clone(),
                            ..default()
                        },
                        GridCell::<i8> {
                            x: x * 16,
                            y: y * 16,
                            z: z * 16,
                        },
                    ));
                }
            }
        }
        root_frame.spawn_spatial(DirectionalLightBundle::default());
        root_frame.spawn_spatial((
            Camera3dBundle {
                transform: Transform::from_xyz(0.0, 0.0, 10.0),
                ..Default::default()
            },
            FloatingOrigin,
            big_space::camera::CameraController::default()
                .with_slowing(false)
                .with_speed(1000.)
                .with_smoothness(0.99, 0.95),
        ));
    });
}
