/// Example with spheres at the scale and distance of the earth and moon around the sun, at 1:1
/// scale. The earth is rotating on its axis, and the camera is in this reference frame, to
/// demonstrate how high precision nested reference frames work at large scales.
use bevy::{
    core_pipeline::bloom::BloomSettings, math::DVec3, pbr::NotShadowCaster, prelude::*,
    render::camera::Exposure,
};
use big_space::{
    camera::CameraController, commands::BigSpaceCommandExt, reference_frame::ReferenceFrame,
    FloatingOrigin,
};
use rand::Rng;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
            big_space::BigSpacePlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            bevy_framepace::FramepacePlugin,
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
        })
        .add_systems(Startup, spawn_solar_system)
        .add_systems(Update, (rotate, lighting))
        .register_type::<Sun>()
        .register_type::<Earth>()
        .register_type::<Moon>()
        .register_type::<Rotates>()
        .run()
}

const EARTH_ORBIT_RADIUS_M: f64 = 149.60e9;
const EARTH_RADIUS_M: f64 = 6.371e6;
const SUN_RADIUS_M: f64 = 695_508_000_f64;
const MOON_RADIUS_M: f64 = 1.7375e6;

#[derive(Component, Reflect)]
struct Earth;

#[derive(Component, Reflect)]
struct Sun;

#[derive(Component, Reflect)]
struct Moon;

#[derive(Component, Reflect)]
struct PrimaryLight;

#[derive(Component, Reflect)]
struct Rotates(f32);

fn rotate(mut rotate_query: Query<(&mut Transform, &Rotates)>) {
    for (mut transform, rotates) in rotate_query.iter_mut() {
        transform.rotate_local_y(rotates.0);
    }
}

fn lighting(
    mut light: Query<&mut Transform, With<PrimaryLight>>,
    sun: Query<&GlobalTransform, With<Sun>>,
) {
    let sun_pos = sun.single().translation();
    light.single_mut().look_at(-sun_pos, Vec3::Y);
}

fn spawn_solar_system(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sun_mesh_handle = meshes.add(Sphere::new(SUN_RADIUS_M as f32).mesh().ico(6).unwrap());
    let earth_mesh_handle = meshes.add(Sphere::new(1.0).mesh().ico(35).unwrap());
    let moon_mesh_handle = meshes.add(Sphere::new(MOON_RADIUS_M as f32).mesh().ico(15).unwrap());
    let ball_mesh_handle = meshes.add(Sphere::new(1.0).mesh().ico(5).unwrap());
    let plane_mesh_handle = meshes.add(Plane3d::new(Vec3::Y));
    let star_mesh_handle = meshes.add(Sphere::new(1e10).mesh().ico(5).unwrap());

    commands.spawn((
        PrimaryLight,
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                color: Color::WHITE,
                illuminance: 120_000.,
                shadows_enabled: true,
                ..default()
            },
            ..default()
        },
    ));

    commands.spawn_big_space(ReferenceFrame::<i64>::default(), |root_frame| {
        root_frame.spawn_frame_default(|_root_frame, sun_frame| {
            sun_frame.insert(Sun);
            sun_frame.spawn_spatial(|_sun_frame, sun_mesh| {
                sun_mesh.insert((
                    PbrBundle {
                        mesh: sun_mesh_handle,
                        material: materials.add(StandardMaterial {
                            base_color: Color::WHITE,
                            emissive: Color::rgb_linear(100000000., 100000000., 100000000.),
                            ..default()
                        }),
                        ..default()
                    },
                    NotShadowCaster,
                ));
            });
            sun_frame.spawn_frame_default(|sun_frame, earth_frame| {
                let earth_pos = DVec3::Z * EARTH_ORBIT_RADIUS_M;
                let (earth_cell, earth_pos) = sun_frame.translation_to_grid(earth_pos);
                earth_frame.insert((
                    Earth,
                    earth_cell,
                    PbrBundle {
                        mesh: earth_mesh_handle,
                        material: materials.add(StandardMaterial {
                            base_color: Color::BLUE,
                            perceptual_roughness: 0.8,
                            reflectance: 1.0,
                            ..default()
                        }),
                        transform: Transform::from_translation(earth_pos)
                            .with_scale(Vec3::splat(EARTH_RADIUS_M as f32)),
                        ..default()
                    },
                    Rotates(0.0001),
                ));
                earth_frame.spawn_spatial(|earth_frame, moon| {
                    let moon_orbit_radius_m = 385e6;
                    let moon_pos = DVec3::X * moon_orbit_radius_m;
                    let (moon_cell, moon_pos) = earth_frame.translation_to_grid(moon_pos);
                    let moon_matl = materials.add(StandardMaterial {
                        base_color: Color::GRAY,
                        perceptual_roughness: 1.0,
                        reflectance: 0.0,
                        ..default()
                    });
                    moon.insert((
                        Moon,
                        PbrBundle {
                            mesh: moon_mesh_handle,
                            material: moon_matl,
                            transform: Transform::from_translation(moon_pos),
                            ..default()
                        },
                        moon_cell,
                    ));
                });
                earth_frame.spawn_spatial(|earth_frame, ball| {
                    let ball_pos =
                        DVec3::X * (EARTH_RADIUS_M + 1.0) + DVec3::NEG_Z * 30.0 + DVec3::Y * 10.0;
                    let (ball_cell, ball_pos) = earth_frame.translation_to_grid(ball_pos);
                    ball.insert((ball_cell, Transform::from_translation(ball_pos)));
                    ball.with_children(|children| {
                        children.spawn((PbrBundle {
                            mesh: ball_mesh_handle,
                            material: materials.add(StandardMaterial {
                                base_color: Color::FUCHSIA,
                                perceptual_roughness: 1.0,
                                reflectance: 0.0,
                                ..default()
                            }),
                            ..default()
                        },));
                        children.spawn((PbrBundle {
                            mesh: plane_mesh_handle,
                            material: materials.add(StandardMaterial {
                                base_color: Color::GREEN,
                                perceptual_roughness: 1.0,
                                reflectance: 0.0,
                                ..default()
                            }),
                            transform: Transform::from_scale(Vec3::splat(10.0)),
                            ..default()
                        },));
                    });
                });
                earth_frame.spawn_frame_default(|earth_frame, camera_frame| {
                    let cam_pos = DVec3::X * (EARTH_RADIUS_M + 1.0);
                    let (cam_cell, cam_pos) = earth_frame.translation_to_grid(cam_pos);
                    camera_frame.insert((
                        FloatingOrigin,
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
                        CameraController::default() // Built-in camera controller
                            .with_speed_bounds([10e-18, 10e35])
                            .with_smoothness(0.9, 0.8)
                            .with_speed(1.0),
                        cam_cell,
                    ));
                    camera_frame.with_children(|_camera_frame, camera| {
                        camera.spawn(SceneBundle {
                            scene: asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0"),
                            transform: Transform::from_xyz(0.0, -4.0, -24.0)
                                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                            ..default()
                        });
                    });
                    // camera_frame.spawn_spatial(|_, camera| {
                    //     camera.insert((
                    //         PrimaryLight,
                    //         DirectionalLightBundle {
                    //             directional_light: DirectionalLight {
                    //                 color: Color::WHITE,
                    //                 illuminance: 120_000.,
                    //                 shadows_enabled: true,
                    //                 ..default()
                    //             },
                    //             ..default()
                    //         },
                    //     ));
                    // });
                });
            });
        });

        let star_mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: Color::rgb_linear(100000., 100000., 100000.),
            ..default()
        });
        let mut rng = rand::thread_rng();
        (0..500).for_each(|_| {
            root_frame.spawn_spatial(|_root_frame, star| {
                star.insert((
                    star_mesh_handle.clone(),
                    star_mat.clone(),
                    Transform::from_xyz(
                        (rng.gen::<f32>() - 0.5) * 1e14,
                        (rng.gen::<f32>() - 0.5) * 1e14,
                        (rng.gen::<f32>() - 0.5) * 1e14,
                    ),
                ));
            });
        });
    });
}
