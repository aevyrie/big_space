use std::collections::VecDeque;

/// Example with spheres at the scale and distance of the earth and moon around the sun, at 1:1
/// scale. The earth is rotating on its axis, and the camera is in this reference frame, to
/// demonstrate how high precision nested reference frames work at large scales.
use bevy::{
    core_pipeline::bloom::BloomSettings,
    math::DVec3,
    pbr::{CascadeShadowConfigBuilder, NotShadowCaster},
    prelude::*,
    render::camera::Exposure,
    transform::TransformSystem,
};
use bevy_color::palettes;
use big_space::{
    camera::{CameraController, CameraInput},
    commands::BigSpaceCommands,
    reference_frame::ReferenceFrame,
    FloatingOrigin,
};
use rand::Rng;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            // bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
            big_space::BigSpacePlugin::<i64>::new(true),
            // big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
        })
        .add_systems(Startup, spawn_solar_system)
        .add_systems(
            PostUpdate,
            (
                rotate,
                lighting
                    .in_set(TransformSystem::TransformPropagate)
                    .after(bevy::transform::systems::sync_simple_transforms)
                    .after(bevy::transform::systems::propagate_transforms)
                    .after(big_space::FloatingOriginSet::PropagateLowPrecision),
                cursor_grab_system,
                springy_ship
                    .after(big_space::camera::default_camera_inputs)
                    .before(big_space::camera::camera_controller::<i64>),
            ),
        )
        .register_type::<Sun>()
        .register_type::<Rotates>()
        .run();
}

const EARTH_ORBIT_RADIUS_M: f64 = 149.60e9;
const EARTH_RADIUS_M: f64 = 6.371e6;
const SUN_RADIUS_M: f64 = 695_508_000_f64;
const MOON_RADIUS_M: f64 = 1.7375e6;

#[derive(Component, Reflect)]
struct Sun;

#[derive(Component, Reflect)]
struct PrimaryLight;

#[derive(Component, Reflect)]
struct Spaceship;

#[derive(Component, Reflect)]
struct Rotates(f32);

fn rotate(mut rotate_query: Query<(&mut Transform, &Rotates)>) {
    for (mut transform, rotates) in rotate_query.iter_mut() {
        transform.rotate_local_y(rotates.0);
    }
}

fn lighting(
    mut light: Query<(&mut Transform, &mut GlobalTransform), With<PrimaryLight>>,
    sun: Query<&GlobalTransform, (With<Sun>, Without<PrimaryLight>)>,
) {
    let sun_pos = sun.single().translation();
    let (mut light_tr, mut light_gt) = light.single_mut();
    light_tr.look_at(-sun_pos, Vec3::Y);
    *light_gt = (*light_tr).into();
}

fn springy_ship(
    cam_input: Res<CameraInput>,
    mut ship: Query<&mut Transform, With<Spaceship>>,
    mut desired_dir: Local<(Vec3, Quat)>,
    mut smoothed_rot: Local<VecDeque<Vec3>>,
) {
    desired_dir.0 = DVec3::new(cam_input.right, cam_input.up, -cam_input.forward).as_vec3()
        * (1.0 + cam_input.boost as u8 as f32);

    smoothed_rot.truncate(15);
    smoothed_rot.push_front(DVec3::new(cam_input.pitch, cam_input.yaw, cam_input.roll).as_vec3());
    let avg_rot = smoothed_rot.iter().sum::<Vec3>() / smoothed_rot.len() as f32;

    use std::f32::consts::*;
    desired_dir.1 = Quat::IDENTITY.slerp(
        Quat::from_euler(
            EulerRot::XYZ,
            avg_rot.x.clamp(-FRAC_PI_4, FRAC_PI_4),
            avg_rot.y.clamp(-FRAC_PI_4, FRAC_PI_4),
            avg_rot.z.clamp(-FRAC_PI_4, FRAC_PI_4),
        ),
        0.2,
    ) * Quat::from_rotation_y(PI);

    ship.single_mut().translation = ship
        .single_mut()
        .translation
        .lerp(desired_dir.0 * Vec3::new(0.5, 0.5, -2.0), 0.02);

    ship.single_mut().rotation = ship.single_mut().rotation.slerp(desired_dir.1, 0.02);
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
    let ball_mesh_handle = meshes.add(Sphere::new(5.0).mesh().ico(5).unwrap());
    let plane_mesh_handle = meshes.add(Plane3d::new(Vec3::X, Vec2::splat(0.5)));

    commands.spawn((
        PrimaryLight,
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                color: Color::WHITE,
                illuminance: 120_000.,
                shadows_enabled: true,
                ..default()
            },
            cascade_shadow_config: CascadeShadowConfigBuilder {
                num_cascades: 4,
                minimum_distance: 0.1,
                maximum_distance: 10_000.0,
                first_cascade_far_bound: 100.0,
                overlap_proportion: 0.2,
            }
            .build(),
            ..default()
        },
    ));

    commands.spawn_big_space(ReferenceFrame::<i64>::default(), |root_frame| {
        root_frame.with_frame_default(|sun| {
            sun.insert((Sun, Name::new("Sun")));
            sun.spawn_spatial((
                PbrBundle {
                    mesh: sun_mesh_handle,
                    material: materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        emissive: LinearRgba::rgb(100000., 100000., 100000.),
                        ..default()
                    }),
                    ..default()
                },
                NotShadowCaster,
            ));

            let earth_pos = DVec3::Z * EARTH_ORBIT_RADIUS_M;
            let (earth_cell, earth_pos) = sun.frame().translation_to_grid(earth_pos);
            sun.with_frame_default(|earth| {
                earth.insert((
                    Name::new("Earth"),
                    earth_cell,
                    PbrBundle {
                        mesh: earth_mesh_handle,
                        material: materials.add(StandardMaterial {
                            base_color: Color::Srgba(palettes::css::BLUE),
                            perceptual_roughness: 0.8,
                            reflectance: 1.0,
                            ..default()
                        }),
                        transform: Transform::from_translation(earth_pos)
                            .with_scale(Vec3::splat(EARTH_RADIUS_M as f32)),
                        ..default()
                    },
                    Rotates(0.000001),
                ));

                let moon_orbit_radius_m = 385e6;
                let moon_pos = DVec3::NEG_Z * moon_orbit_radius_m;
                let (moon_cell, moon_pos) = earth.frame().translation_to_grid(moon_pos);
                earth.spawn_spatial((
                    Name::new("Moon"),
                    PbrBundle {
                        mesh: moon_mesh_handle,
                        material: materials.add(StandardMaterial {
                            base_color: Color::Srgba(palettes::css::GRAY),
                            perceptual_roughness: 1.0,
                            reflectance: 0.0,
                            ..default()
                        }),
                        transform: Transform::from_translation(moon_pos),
                        ..default()
                    },
                    moon_cell,
                ));

                let ball_pos =
                    DVec3::X * (EARTH_RADIUS_M + 1.0) + DVec3::NEG_Z * 30.0 + DVec3::Y * 10.0;
                let (ball_cell, ball_pos) = earth.frame().translation_to_grid(ball_pos);
                earth
                    .spawn_spatial((ball_cell, Transform::from_translation(ball_pos)))
                    .with_children(|children| {
                        children.spawn((PbrBundle {
                            mesh: ball_mesh_handle,
                            material: materials.add(StandardMaterial {
                                base_color: Color::WHITE,
                                ..default()
                            }),
                            ..default()
                        },));

                        children.spawn((PbrBundle {
                            mesh: plane_mesh_handle,
                            material: materials.add(StandardMaterial {
                                base_color: Color::Srgba(palettes::css::DARK_GREEN),
                                perceptual_roughness: 1.0,
                                reflectance: 0.0,
                                ..default()
                            }),
                            transform: Transform::from_scale(Vec3::splat(100.0))
                                .with_translation(Vec3::X * -5.0),
                            ..default()
                        },));
                    });

                let cam_pos = DVec3::X * (EARTH_RADIUS_M + 1.0);
                let (cam_cell, cam_pos) = earth.frame().translation_to_grid(cam_pos);
                earth.with_frame_default(|camera| {
                    camera.insert((
                        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::X),
                        CameraController::default() // Built-in camera controller
                            .with_speed_bounds([0.1, 10e35])
                            .with_smoothness(0.98, 0.98)
                            .with_speed(1.0),
                        cam_cell,
                    ));

                    camera.spawn_spatial((
                        FloatingOrigin,
                        Camera3dBundle {
                            transform: Transform::from_xyz(0.0, 4.0, 22.0),
                            camera: Camera {
                                hdr: true,
                                ..default()
                            },
                            exposure: Exposure::SUNLIGHT,
                            ..default()
                        },
                        BloomSettings::default(),
                    ));

                    camera.with_children(|camera| {
                        camera.spawn((
                            Spaceship,
                            SceneBundle {
                                scene: asset_server
                                    .load("models/low_poly_spaceship/scene.gltf#Scene0"),
                                transform: Transform::from_rotation(Quat::from_rotation_y(
                                    std::f32::consts::PI,
                                )),
                                ..default()
                            },
                        ));
                    });
                });
            });
        });

        let star_mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::rgb(2., 2., 2.),
            ..default()
        });
        let star_mesh_handle = meshes.add(Sphere::new(1e10).mesh().ico(5).unwrap());
        let mut rng = rand::thread_rng();
        (0..1000).for_each(|_| {
            root_frame.spawn_spatial((
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
}

fn cursor_grab_system(
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut cam: ResMut<big_space::camera::CameraInput>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let Some(mut window) = windows.get_single_mut().ok() else {
        return;
    };

    if btn.just_pressed(MouseButton::Right) {
        window.cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        window.cursor.visible = false;
        // window.mode = WindowMode::BorderlessFullscreen;
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
        window.cursor.visible = true;
        // window.mode = WindowMode::Windowed;
        cam.defaults_disabled = true;
    }
}
