//! Example of dynamic spawn by big space.
use bevy::prelude::*;
use bevy_math::{dvec3, DVec3};
use big_space::prelude::*;
use turborand::{rng::Rng, TurboRand};

// Spawn the camera and mesh really, stupidly, far from the origin .
const BIG_DISTANCE: f64 = 1_000_000_000_000_000.0;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpaceDefaultPlugins.build(),
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(PostUpdate, dynamic_spawn_grid_in_root)
        .add_systems(PostUpdate, dynamic_spawn_spatial_in_root)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Using `spawn_big_space` helps you avoid mistakes when building hierarchies. Most notably,
    // it will allow you to only write out the `GridPrecision` generic value (i64 in this case)
    // once, without needing to repeat this generic when spawning `GridCell<i64>`s
    //
    // A world can have multiple independent BigSpaces, with their own floating origins. This can
    // come in handy if you want to have two cameras very far from each other, rendering at the same
    // time like split screen, or portals.
    commands.spawn_big_space_default(|root_grid| {
        // Because BIG_DISTANCE is so large, we want to avoid using bevy's f32 transforms alone and
        // experience rounding errors. Instead, we use this helper to convert an f64 position into a
        // grid cell and f32 offset.
        let (grid_cell, cell_offset) = root_grid
            .grid()
            .translation_to_grid(DVec3::splat(BIG_DISTANCE));

        // `spawn_spatial` will spawn a high-precision spatial entity with floating origin support.
        root_grid.spawn_spatial(DirectionalLight::default());

        // Spawn a sphere mesh with high precision.
        root_grid.spawn_spatial((
            Mesh3d(meshes.add(Sphere::new(500.0))),
            MeshMaterial3d(materials.add(Color::WHITE)),
            Transform::from_translation(cell_offset),
            grid_cell,
        ));

        // Any spatial entity can be the floating origin. Attaching it to the camera ensures the
        // camera will never see floating point precision rendering artifacts.
        root_grid.spawn_spatial((
            Camera3d::default(),
            Transform::from_translation(cell_offset + Vec3::new(0.0, 0.0, 3000.0)),
            grid_cell,
            FloatingOrigin,
            BigSpaceCameraController::default(),
        ));
    });

    commands.spawn(Text::new(format!(
        "Press `P` to dynamic spawn new grid. \nPress `O` to dynamic spawn new grid cell."
    )));
}

fn dynamic_spawn_grid_in_root(
    commands: Commands,
    grids: Grids,
    root: Query<Entity, With<BigSpace>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    // spawn grid
    if key.just_pressed(KeyCode::KeyP) {
        let root_entity = root.single().unwrap();
        root_entity.spawn_grid_commands(commands, grids, |root_grid| {
            let rng = Rng::new();
            let offset = dvec3(
                (rng.f64() - 0.5) * 4000.0,
                (rng.f64() - 0.5) * 4000.0,
                (rng.f64() - 0.5) * 4000.0,
            );
            let (grid_cell, cell_offset) = root_grid
                .grid()
                .translation_to_grid(DVec3::splat(BIG_DISTANCE) + offset);
            root_grid.with_grid_default(|child_grid| {
                child_grid.insert((
                    Mesh3d(meshes.add(Sphere::new(500.0))),
                    MeshMaterial3d(materials.add(Color::WHITE)),
                    Transform::from_translation(cell_offset),
                    grid_cell,
                ));
            });
        });
    }
}

fn dynamic_spawn_spatial_in_root(
    commands: Commands,
    grids: Grids,
    root: Query<Entity, With<BigSpace>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    // spawn spatial
    if key.just_pressed(KeyCode::KeyO) {
        let root_entity = root.single().unwrap();
        root_entity.spawn_grid_commands(commands, grids, |root_grid| {
            let rng = Rng::new();
            let offset = dvec3(
                (rng.f64() - 0.5) * 2000.0,
                (rng.f64() - 0.5) * 2000.0,
                (rng.f64() - 0.5) * 2000.0,
            );
            let (grid_cell, cell_offset) = root_grid
                .grid()
                .translation_to_grid(DVec3::splat(BIG_DISTANCE) + offset);
            root_grid.spawn_spatial((
                Mesh3d(meshes.add(Sphere::new(300.0))),
                MeshMaterial3d(materials.add(Color::linear_rgb(1.0, 1.0, 0.0))),
                Transform::from_translation(cell_offset),
                grid_cell,
            ));
        });
    }
}
