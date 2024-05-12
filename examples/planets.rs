/// Example with spheres at the scale and distance of the earth and moon around the sun, at 1:1
/// scale. The earth is rotating on its axis, and the camera is in this reference frame, to
/// demonstrate how high precision nested reference frames work at large scales.
use bevy::{core_pipeline::bloom::BloomSettings, prelude::*, render::camera::Exposure};
use big_space::{
    bundles::{BigSpaceBundle, BigSpatialBundle},
    camera::CameraController,
    reference_frame::{BigSpace, ReferenceFrame},
    FloatingOrigin, GridCell,
};
use rand::Rng;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
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
        .add_systems(
            Startup,
            (
                spawn_solar_system,
                spawn_camera,
                // spawn_starfield
            )
                .chain(),
        )
        .add_systems(Update, rotate)
        .register_type::<Sun>()
        .register_type::<Earth>()
        .register_type::<Moon>()
        .register_type::<Rotates>()
        .run()
}

const EARTH_ORBIT_RADIUS_M: f32 = 149.60e9;
const EARTH_RADIUS_M: f32 = 6.371e6;
const SUN_RADIUS_M: f32 = 695_508_000_f32;

#[derive(Component, Reflect)]
struct Earth;

#[derive(Component, Reflect)]
struct Sun;

#[derive(Component, Reflect)]
struct Moon;

#[derive(Component, Reflect)]
struct Rotates(f32);

fn rotate(mut rotate_query: Query<(&mut Transform, &Rotates)>) {
    for (mut transform, rotates) in rotate_query.iter_mut() {
        transform.rotate_local_y(rotates.0);
    }
}

fn spawn_camera(mut commands: Commands, earth: Query<(Entity, &ReferenceFrame<i64>), With<Earth>>) {
    let (earth, earth_frame) = earth.single();
    let (cam_cell, cam_pos): (GridCell<i64>, _) =
        earth_frame.imprecise_translation_to_grid(Vec3::X * (EARTH_RADIUS_M + 1.0));

    // camera
    let camera_entity = commands
        .spawn((
            Camera3dBundle {
                transform: Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::X),
                camera: Camera {
                    hdr: true,
                    ..default()
                },
                exposure: Exposure::SUNLIGHT,
                ..default()
            },
            BloomSettings::default(),
            CameraController::default() // Built-in camera controller
                .with_speed_bounds([10e-18, 10e35])
                .with_smoothness(0.9, 0.8)
                .with_speed(1.0),
        ))
        .insert(BigSpatialBundle {
            cell: cam_cell,
            ..default()
        })
        .insert(FloatingOrigin)
        .id();

    commands.entity(earth).add_child(camera_entity);
}

fn spawn_starfield(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut sphere = |radius| meshes.add(Sphere::new(radius).mesh().ico(32).unwrap());

    let star_mesh = sphere(1e10);
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(100000., 100000., 100000.),
        ..default()
    });
    let mut rng = rand::thread_rng();
    (0..500).for_each(|_| {
        commands.spawn((PbrBundle {
            mesh: star_mesh.clone(),
            material: star_mat.clone(),
            transform: Transform::from_xyz(
                (rng.gen::<f32>() - 0.5) * 1e14,
                (rng.gen::<f32>() - 0.5) * 1e14,
                (rng.gen::<f32>() - 0.5) * 1e14,
            ),
            ..default()
        },));
    });
}

fn spawn_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut sphere = |radius| meshes.add(Sphere::new(radius).mesh().ico(32).unwrap());

    let space_frame = ReferenceFrame::<i64>::default();

    commands
        .spawn(BigSpaceBundle::<i64>::default())
        .with_children(|space| {
            let sun_frame = ReferenceFrame::<i64>::default();
            space
                .spawn((
                    Sun,
                    GridCell::<i64>::ZERO,
                    PbrBundle {
                        mesh: sphere(SUN_RADIUS_M),
                        material: materials.add(StandardMaterial {
                            base_color: Color::WHITE,
                            emissive: Color::rgb_linear(10000000., 10000000., 10000000.),
                            ..default()
                        }),
                        ..default()
                    },
                ))
                .with_children(|sun| {
                    sun.spawn(PointLightBundle {
                        point_light: PointLight {
                            intensity: 35.73e27,
                            range: 1e20,
                            radius: SUN_RADIUS_M,
                            shadows_enabled: true,
                            ..default()
                        },
                        ..default()
                    });

                    let (earth_cell, earth_pos): (GridCell<i64>, _) =
                        sun_frame.imprecise_translation_to_grid(Vec3::Z * EARTH_ORBIT_RADIUS_M);

                    let earth_frame = ReferenceFrame::<i64>::default();

                    sun.spawn((
                        Earth,
                        PbrBundle {
                            mesh: sphere(EARTH_RADIUS_M),
                            material: materials.add(StandardMaterial {
                                base_color: Color::BLUE,
                                perceptual_roughness: 0.8,
                                reflectance: 1.0,
                                ..default()
                            }),
                            transform: Transform::from_translation(earth_pos),
                            ..default()
                        },
                        earth_cell,
                        Rotates(0.001),
                    ))
                    .with_children(|earth| {
                        let moon_orbit_radius_m = 385e6;
                        let moon_radius_m = 1.7375e6;

                        let moon_mat = materials.add(StandardMaterial {
                            base_color: Color::GRAY,
                            perceptual_roughness: 1.0,
                            reflectance: 0.0,
                            ..default()
                        });

                        let (moon_cell, moon_pos): (GridCell<i64>, _) = earth_frame
                            .imprecise_translation_to_grid(Vec3::X * moon_orbit_radius_m);

                        earth.spawn((
                            Moon,
                            PbrBundle {
                                mesh: sphere(moon_radius_m),
                                material: moon_mat,
                                transform: Transform::from_translation(moon_pos),
                                ..default()
                            },
                            moon_cell,
                        ));

                        let (ball_cell, ball_pos): (GridCell<i64>, _) = earth_frame
                            .imprecise_translation_to_grid(
                                Vec3::X * (EARTH_RADIUS_M + 1.0) + Vec3::NEG_Z * 5.0,
                            );

                        earth.spawn((
                            PbrBundle {
                                mesh: sphere(1.0),
                                material: materials.add(StandardMaterial {
                                    base_color: Color::FUCHSIA,
                                    perceptual_roughness: 1.0,
                                    reflectance: 0.0,
                                    ..default()
                                }),
                                transform: Transform::from_translation(ball_pos),
                                ..default()
                            },
                            ball_cell,
                        ));
                    })
                    .insert(earth_frame);
                })
                .insert(sun_frame);
        })
        .insert(space_frame);
}
