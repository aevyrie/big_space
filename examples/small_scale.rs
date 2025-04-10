//! `big_space` isn't only useful for objects that are large, it's useful any time you want to work
//! with big *differences* in scale. You might normally think of human scale and solar system scale
//! being mixed in games that use double precision (f64) worlds, but you can use this floating
//! origin plugin to work on almost any set of scales.
//!
//! In this example, we will be spawning spheres the size of protons, across the width of the
//! milky way galaxy.

use bevy::prelude::*;
use bevy_math::DVec3;
use big_space::prelude::*;
use tracing::info;

const UNIVERSE_DIA: f64 = 8.8e26; // Diameter of the observable universe
const PROTON_DIA: f32 = 1.68e-15; // Diameter of a proton

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::default(),
            FloatingOriginDebugPlugin::default(), // Draws cell AABBs and grids
            CameraControllerPlugin::default(),    // Compatible controller
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (bounce_atoms, toggle_cam_pos))
        .insert_resource(ClearColor(Color::BLACK))
        .run();
}

#[derive(Component)]
struct Proton;

fn setup_scene(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Because we are working on such small scales, we need to make the grid very small. This
    // ensures that the maximum floating point error is also very small, because no entities can
    // ever get farther than `SMALL_SCALE * 500` units from the origin.
    let small_grid = Grid::new(PROTON_DIA * 5_000.0, 0.0);

    commands.spawn_big_space(small_grid, |root_grid| {
        root_grid.spawn_spatial(DirectionalLight::default());

        // A proton at the origin
        root_grid.spawn_spatial((
            Proton,
            Mesh3d(meshes.add(Sphere::default())),
            MeshMaterial3d(materials.add(Color::WHITE)),
            Transform::from_scale(Vec3::splat(PROTON_DIA)),
        ));

        // Compute the grid cell for the far away objects
        let (grid_cell, cell_offset) = root_grid
            .grid()
            .translation_to_grid(DVec3::X * UNIVERSE_DIA);

        // A proton at the other side of the milky way
        root_grid.spawn_spatial((
            Proton,
            Mesh3d(meshes.add(Sphere::default())),
            MeshMaterial3d(materials.add(Color::WHITE)),
            Transform::from_translation(cell_offset).with_scale(Vec3::splat(PROTON_DIA)),
            grid_cell,
        ));

        root_grid.spawn_spatial((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                near: PROTON_DIA * 0.01, // Without this, the atom would be clipped
                ..Default::default()
            }),
            Transform::from_xyz(0.0, 0.0, PROTON_DIA * 2.0),
            grid_cell,
            FloatingOrigin,
            CameraController::default(),
        ));

        // A spaceship
        root_grid.spawn_spatial((
            SceneRoot(asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0")),
            Transform::from_xyz(0.0, 0.0, 2.5)
                .with_rotation(Quat::from_rotation_y(core::f32::consts::PI)),
            grid_cell,
        ));
    });

    commands.spawn(Text::new(format!(
        "Press `T` to teleport between the origin and ship {UNIVERSE_DIA}m away."
    )));
}

fn bounce_atoms(mut atoms: Query<&mut Transform, With<Proton>>, time: Res<Time>) {
    for mut atom in atoms.iter_mut() {
        atom.translation.y = time.elapsed_secs().sin() * PROTON_DIA;
    }
}

fn toggle_cam_pos(
    mut cam: Query<&mut GridCell, With<Camera>>,
    mut toggle: Local<bool>,
    grid: Query<&Grid>,
    keyboard: Res<ButtonInput<KeyCode>>,
    protons: Query<&GlobalTransform, With<Proton>>,
) -> Result {
    if !keyboard.just_pressed(KeyCode::KeyT) {
        return Ok(());
    }
    *cam.single_mut()? = if *toggle {
        grid.single()
            .unwrap()
            .translation_to_grid(DVec3::X * UNIVERSE_DIA)
            .0
    } else {
        GridCell::ZERO
    };
    *toggle = !*toggle;
    // To prove there is no funny business going on, let's print out the `GlobalTransform` of each
    // of the protons, to show that they truly are as far apart as we say they are.
    info!("Width of observable universe: {UNIVERSE_DIA}");
    for proton in &protons {
        info!("Proton x coord: {}", proton.translation().x);
    }
    Ok(())
}
