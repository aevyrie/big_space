//! `big_space` isn't only useful for objects that are large, it's useful any time you want to work
//! with big *differences* in scale. You might normally think of human scale and solar system scale
//! being mixed in games that use double precision (f64) worlds, but you can use this floating
//! origin plugin to work on almost any set of scales.
//!
//! In this example, we will be spawning spheres the size of carbon atoms, across the width of the
//! milky way galaxy.

use bevy::prelude::*;
use bevy_math::DVec3;
use big_space::prelude::*;

const BIG_DISTANCE: f64 = 100_000_000_000_000_000_000.0; // Diameter of the milky way galaxy
const SMALL_SCALE: f32 = 0.000_000_000_154; // Diameter of a carbon atom

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(), // Replaced by big_space
            BigSpacePlugin::<i128>::default(),
            FloatingOriginDebugPlugin::<i128>::default(), // Draws cell AABBs and reference frames
            big_space::camera::CameraControllerPlugin::<i128>::default(), // Compatible controller
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (bounce_atoms, toggle_cam_pos))
        .insert_resource(ClearColor(Color::BLACK))
        .run();
}

#[derive(Component)]
struct Atom;

fn setup_scene(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Because we are working on such small scales, we need to make the reference frame's grid very
    // small. This ensures that the maximum floating point error is also very small, because no
    // entities can ever get farther than `SMALL_SCALE * 500` units from the origin.
    let small_reference_frame_grid = ReferenceFrame::<i128>::new(SMALL_SCALE * 1_000.0, 0.0);

    commands.spawn_big_space(small_reference_frame_grid, |root_frame| {
        root_frame.spawn_spatial(DirectionalLightBundle::default());

        // A carbon atom at the origin
        root_frame.spawn_spatial((
            Atom,
            PbrBundle {
                mesh: meshes.add(Sphere::default()),
                material: materials.add(Color::WHITE),
                transform: Transform::from_scale(Vec3::splat(SMALL_SCALE)),
                ..default()
            },
        ));

        // Compute the grid cell for the far away objects
        let (grid_cell, cell_offset) = root_frame
            .frame()
            .translation_to_grid(DVec3::X * BIG_DISTANCE);

        // A carbon atom at the other side of the milky way
        root_frame.spawn_spatial((
            Atom,
            PbrBundle {
                mesh: meshes.add(Sphere::default()),
                material: materials.add(Color::WHITE),
                transform: Transform::from_translation(cell_offset)
                    .with_scale(Vec3::splat(SMALL_SCALE)),
                ..default()
            },
            grid_cell,
        ));

        root_frame.spawn_spatial((
            Camera3dBundle {
                projection: Projection::Perspective(PerspectiveProjection {
                    near: SMALL_SCALE * 0.01, // Without this, the atom would be clipped
                    ..Default::default()
                }),
                transform: Transform::from_xyz(0.0, 0.0, SMALL_SCALE * 2.0),
                ..Default::default()
            },
            grid_cell,
            FloatingOrigin,
            big_space::camera::CameraController::default(),
        ));

        // A space ship
        root_frame.spawn_spatial((
            SceneBundle {
                scene: asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0"),
                transform: Transform::from_xyz(0.0, 0.0, 2.5)
                    .with_rotation(Quat::from_rotation_y(3.14)),
                ..default()
            },
            grid_cell,
        ));
    });

    commands.spawn(TextBundle {
        text: Text::from_section(
            format!("Press `T` to teleport between the origin and ship {BIG_DISTANCE}m away."),
            TextStyle::default(),
        ),
        ..Default::default()
    });
}

fn bounce_atoms(mut atoms: Query<&mut Transform, With<Atom>>, time: Res<Time>) {
    for mut atom in atoms.iter_mut() {
        atom.translation.y = time.elapsed_seconds().sin() * SMALL_SCALE;
    }
}

fn toggle_cam_pos(
    mut cam: Query<&mut GridCell<i128>, With<Camera>>,
    mut toggle: Local<bool>,
    frame: Query<&ReferenceFrame<i128>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyT) {
        return;
    }
    *cam.single_mut() = if *toggle {
        frame
            .single()
            .translation_to_grid(DVec3::X * BIG_DISTANCE)
            .0
    } else {
        GridCell::ZERO
    };
    *toggle = !*toggle;
}
