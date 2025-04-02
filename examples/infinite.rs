//! Big spaces are infinite, looping back on themselves smoothly.

use bevy::prelude::*;
use big_space::prelude::*;

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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sphere = Mesh3d(meshes.add(Sphere::default()));
    let matl = MeshMaterial3d(materials.add(Color::WHITE));

    commands.spawn_big_space(Grid::default(), |root_grid| {
        let width = || -8..8;
        for (x, y, z) in width()
            .flat_map(|x| width().map(move |y| (x, y)))
            .flat_map(|(x, y)| width().map(move |z| (x, y, z)))
        {
            root_grid.spawn_spatial((
                sphere.clone(),
                matl.clone(),
                GridCell {
                    x: x * 16,
                    y: y * 16,
                    z: z * 16,
                },
            ));
        }
        root_grid.spawn_spatial(DirectionalLight::default());
        root_grid.spawn_spatial((
            Camera3d::default(),
            Transform::from_xyz(0.0, 0.0, 10.0),
            FloatingOrigin,
            big_space::camera::CameraController::default()
                .with_speed(10.)
                .with_smoothness(0.99, 0.95),
        ));
    });
}
