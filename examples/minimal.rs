//! A minimal example of spawning meshes and floating origin cameras in big_spaces.

use bevy::prelude::*;
use bevy_math::DVec3;
use big_space::prelude::*;

// Spawn the camera and mesh really, stupidly, far from the origin .
const BIG_DISTANCE: f64 = 1_000_000_000_000_000_000.0;

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
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Using `spawn_big_space()` helps you avoid mistakes when building hierarchies. A world can
    // have multiple independent BigSpaces, with their own floating origins. This can come in handy
    // if you want to have two cameras very far from each other, like split screen, or portals.
    commands.spawn_big_space(ReferenceFrame::<i64>::default(), |root_frame| {
        // Because BIG_DISTANCE is so large, we want to avoid using bevy's f32 transforms alone.
        // Instead, we use this helper to convert an f64 position into a grid cell and offset.
        let (grid_cell, cell_offset) = root_frame
            .frame()
            .translation_to_grid(DVec3::splat(BIG_DISTANCE));

        // `spawn_spatial` will spawn a high-precision spatial entity with floating origin support
        root_frame.spawn_spatial(DirectionalLightBundle::default());

        // Spawn a sphere mesh with high precision
        root_frame.spawn_spatial((
            PbrBundle {
                mesh: meshes.add(Sphere::default()),
                material: materials.add(Color::WHITE),
                transform: Transform::from_translation(cell_offset),
                ..default()
            },
            grid_cell,
        ));

        // Spawning low-precision entities (without a GridCell) as children of high-precision
        // entities (with a GridCell), is also supported. We demonstrate this here by loading in a
        // GLTF scene, which will be added a child of this entity using only Transforms.
        root_frame.spawn_spatial((
            SceneBundle {
                scene: asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0"),
                transform: Transform::from_translation(cell_offset - 10.0),
                ..default()
            },
            grid_cell,
        ));

        // Any spatial entity can be the floating origin. Attaching it to the camera ensures the
        // camera will never see floating point precision rendering artifacts.
        root_frame.spawn_spatial((
            Camera3dBundle {
                transform: Transform::from_translation(cell_offset + Vec3::new(0.0, 0.0, 10.0)),
                ..Default::default()
            },
            grid_cell,
            FloatingOrigin,
            big_space::camera::CameraController::default(),
        ));
    });
}
