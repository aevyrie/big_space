//! A bare minimum example of spawning big_spaces.

use bevy::prelude::*;
use big_space::prelude::*;

const BIG_DIST: f32 = 100_000_000.0; // Spawn stuff really far from the origin to show it works.

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(), // Replaced by big_space
            BigSpacePlugin::<i64>::default(),
            FloatingOriginDebugPlugin::<i64>::default(), // Draws cell AABBs and reference frames
            big_space::camera::CameraControllerPlugin::<i64>::default(), // Compatible controller
        ))
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Using `spawn_big_space()` helps you avoid mistakes when building a hierarchy.
    // You can have multiple independent BigSpaces in your world, with their own floating origins.
    commands.spawn_big_space(ReferenceFrame::<i64>::default(), |root_frame| {
        // spawn_spatial will spawn a high-precision spatial entity with floating origin support
        root_frame.spawn_spatial(DirectionalLightBundle::default());
        root_frame.spawn_spatial(PbrBundle {
            mesh: meshes.add(Sphere::default()),
            material: materials.add(Color::WHITE),
            transform: Transform::from_translation(Vec3::splat(BIG_DIST)),
            ..default()
        });
        root_frame.spawn_spatial((
            // Any spatial entity can be the floating origin. Attaching it to the camera ensures the
            // camera will never see floating point precision rendering artifacts.
            Camera3dBundle {
                transform: Transform::from_translation(BIG_DIST + Vec3::new(0.0, 0.0, 10.0)),
                ..Default::default()
            },
            FloatingOrigin,
            big_space::camera::CameraController::default(),
        ));
    });
}
