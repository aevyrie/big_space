/// Example with spheres at the scale and distance of the earth and moon around the sun, at 1:1
/// scale. The earth is rotating on its axis, and the camera is in this reference frame, to
/// demonstrate how high precision nested reference frames work at large scales.
use bevy::{core_pipeline::bloom::BloomSettings, prelude::*, render::camera::Exposure};
use big_space::{
    camera::CameraController,
    reference_frame::{ReferenceFrame, RootReferenceFrame},
    FloatingOrigin, GridCell,
};
use rand::Rng;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::FloatingOriginPlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            bevy_framepace::FramepacePlugin,
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 100.0,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, rotate)
        .run()
}

#[derive(Component)]
struct Rotates(f32);

fn rotate(mut rotate_query: Query<(&mut Transform, &Rotates)>) {
    for (mut transform, rotates) in rotate_query.iter_mut() {
        transform.rotate_local_y(rotates.0);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    space: Res<RootReferenceFrame<i64>>,
) {
    let mut sphere = |radius| meshes.add(Sphere::new(radius).mesh().ico(32).unwrap());

    let star = sphere(1e10);
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(100000., 100000., 100000.),
        ..default()
    });
    let mut rng = rand::thread_rng();
    for _ in 0..500 {
        commands.spawn((
            GridCell::<i64>::new(
                ((rng.gen::<f32>() - 0.5) * 1e11) as i64,
                ((rng.gen::<f32>() - 0.5) * 1e11) as i64,
                ((rng.gen::<f32>() - 0.5) * 1e11) as i64,
            ),
            PbrBundle {
                mesh: star.clone(),
                material: star_mat.clone(),
                ..default()
            },
        ));
    }

    let sun_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(10000000., 10000000., 10000000.),
        ..default()
    });
    let sun_radius_m = 695_508_000.0;

    commands
        .spawn((
            GridCell::<i64>::ZERO,
            PointLightBundle {
                point_light: PointLight {
                    intensity: 35.73e27,
                    range: 1e20,
                    radius: sun_radius_m,
                    shadows_enabled: true,
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|builder| {
            builder.spawn((PbrBundle {
                mesh: sphere(sun_radius_m),
                material: sun_mat,
                ..default()
            },));
        });

    let earth_orbit_radius_m = 149.60e9;
    let earth_radius_m = 6.371e6;

    let earth_mat = materials.add(StandardMaterial {
        base_color: Color::BLUE,
        perceptual_roughness: 0.8,
        reflectance: 1.0,
        ..default()
    });

    let (earth_cell, earth_pos): (GridCell<i64>, _) =
        space.imprecise_translation_to_grid(Vec3::Z * earth_orbit_radius_m);

    commands
        .spawn((
            PbrBundle {
                mesh: sphere(earth_radius_m),
                material: earth_mat,
                transform: Transform::from_translation(earth_pos),
                ..default()
            },
            earth_cell,
            ReferenceFrame::<i64>::default(),
            Rotates(0.001),
        ))
        .with_children(|commands| {
            let moon_orbit_radius_m = 385e6;
            let moon_radius_m = 1.7375e6;

            let moon_mat = materials.add(StandardMaterial {
                base_color: Color::GRAY,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            });

            let (moon_cell, moon_pos): (GridCell<i64>, _) =
                space.imprecise_translation_to_grid(Vec3::X * moon_orbit_radius_m);

            commands.spawn((
                PbrBundle {
                    mesh: sphere(moon_radius_m),
                    material: moon_mat,
                    transform: Transform::from_translation(moon_pos),
                    ..default()
                },
                moon_cell,
            ));

            let (cam_cell, cam_pos): (GridCell<i64>, _) =
                space.imprecise_translation_to_grid(Vec3::X * (earth_radius_m + 1.0));

            // camera
            commands.spawn((
                Camera3dBundle {
                    transform: Transform::from_translation(cam_pos)
                        .looking_to(Vec3::NEG_Z, Vec3::X),
                    camera: Camera {
                        hdr: true,
                        ..default()
                    },
                    exposure: Exposure::SUNLIGHT,
                    ..default()
                },
                BloomSettings::default(),
                cam_cell,
                FloatingOrigin, // Important: marks the floating origin entity for rendering.
                CameraController::default() // Built-in camera controller
                    .with_speed_bounds([10e-18, 10e35])
                    .with_smoothness(0.9, 0.8)
                    .with_speed(1.0),
            ));

            let (ball_cell, ball_pos): (GridCell<i64>, _) = space.imprecise_translation_to_grid(
                Vec3::X * (earth_radius_m + 1.0) + Vec3::NEG_Z * 5.0,
            );

            let ball_mat = materials.add(StandardMaterial {
                base_color: Color::FUCHSIA,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            });

            commands.spawn((
                PbrBundle {
                    mesh: sphere(1.0),
                    material: ball_mat,
                    transform: Transform::from_translation(ball_pos),
                    ..default()
                },
                ball_cell,
            ));
        });
}
