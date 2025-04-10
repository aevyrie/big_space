//! Minimal example of spawning meshes and a floating origin camera.

use bevy::prelude::*;
use bevy_math::DVec3;
use big_space::prelude::*;

// Spawn the camera and mesh really, stupidly, far from the origin .
const BIG_DISTANCE: f64 = 1_000_000_000_000_000_000.0;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::default(),
            FloatingOriginDebugPlugin::default(), // Draws cell AABBs and grids
            CameraControllerPlugin::default(),    // Compatible controller
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
    // Using `spawn_big_space` helps you avoid mistakes when building hierarchies.
    //
    // A world can have multiple independent BigSpaces, with their own floating origins. This can
    // come in handy if you want to have two cameras very far from each other, rendering at the same
    // time as split screen, or portals.
    commands.spawn_big_space_default(|root_grid| {
        // Because BIG_DISTANCE is so large, we want to avoid using bevy's f32 transforms alone and
        // experience rounding errors. Instead, we use this helper to convert f64 position into a
        // grid cell and f32 offset.
        let (grid_cell, cell_offset) = root_grid
            .grid()
            .translation_to_grid(DVec3::splat(BIG_DISTANCE));

        // `spawn_spatial` will spawn a high-precision spatial entity with floating origin support.
        root_grid.spawn_spatial(DirectionalLight::default());

        // Spawn a sphere mesh with high precision.
        root_grid.spawn_spatial((
            Mesh3d(meshes.add(Sphere::default())),
            MeshMaterial3d(materials.add(Color::WHITE)),
            Transform::from_translation(cell_offset),
            grid_cell,
        ));

        // Spawning low-precision entities (without a GridCell) as children of high-precision
        // entities (with a GridCell), is also supported. We demonstrate this here by loading in a
        // GLTF scene, which will be added as a child of this entity using low precision Transforms.
        root_grid.spawn_spatial((
            SceneRoot(asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0")),
            Transform::from_translation(cell_offset - 10.0),
            grid_cell,
        ));

        // Any spatial entity can be the floating origin. Attaching it to the camera ensures the
        // camera will never see floating point precision rendering artifacts.
        root_grid.spawn_spatial((
            Camera3d::default(),
            Transform::from_translation(cell_offset + Vec3::new(0.0, 0.0, 10.0)),
            grid_cell,
            FloatingOrigin,
            CameraController::default(),
        ));
    });
}
