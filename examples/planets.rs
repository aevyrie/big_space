//! A practical example of a spare ship on a planet, in a solar system, surrounded by stars.
extern crate alloc;

use alloc::collections::VecDeque;

use bevy::{
    color::palettes,
    core_pipeline::bloom::Bloom,
    math::DVec3,
    pbr::{CascadeShadowConfigBuilder, NotShadowCaster},
    prelude::*,
    render::camera::Exposure,
    transform::TransformSystem,
};
use big_space::prelude::*;
use turborand::{rng::Rng, TurboRand};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::default().validate(),
            CameraControllerPlugin::default(),
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
            ..Default::default()
        })
        .add_systems(Startup, spawn_solar_system)
        .add_systems(
            PostUpdate,
            (
                rotate,
                lighting
                    .in_set(TransformSystem::TransformPropagate)
                    .after(FloatingOriginSystem::PropagateLowPrecision),
                cursor_grab_system,
                springy_ship
                    .after(big_space::camera::default_camera_inputs)
                    .before(big_space::camera::camera_controller),
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
) -> Result {
    let sun_pos = sun.single()?.translation();
    let (mut light_tr, mut light_gt) = light.single_mut()?;
    light_tr.look_at(-sun_pos, Vec3::Y);
    *light_gt = (*light_tr).into();
    Ok(())
}

fn springy_ship(
    cam_input: Res<big_space::camera::CameraInput>,
    mut ship: Query<&mut Transform, With<Spaceship>>,
    mut desired_dir: Local<(Vec3, Quat)>,
    mut smoothed_rot: Local<VecDeque<Vec3>>,
) -> Result {
    desired_dir.0 = DVec3::new(cam_input.right, cam_input.up, -cam_input.forward).as_vec3()
        * (1.0 + cam_input.boost as u8 as f32);

    smoothed_rot.truncate(15);
    smoothed_rot.push_front(DVec3::new(cam_input.pitch, cam_input.yaw, cam_input.roll).as_vec3());
    let avg_rot = smoothed_rot.iter().sum::<Vec3>() / smoothed_rot.len() as f32;

    use core::f32::consts::*;
    desired_dir.1 = Quat::IDENTITY.slerp(
        Quat::from_euler(
            EulerRot::XYZ,
            avg_rot.x.clamp(-FRAC_PI_4, FRAC_PI_4),
            avg_rot.y.clamp(-FRAC_PI_4, FRAC_PI_4),
            avg_rot.z.clamp(-FRAC_PI_4, FRAC_PI_4),
        ),
        0.2,
    ) * Quat::from_rotation_y(PI);

    ship.single_mut()?.translation = ship
        .single_mut()?
        .translation
        .lerp(desired_dir.0 * Vec3::new(0.5, 0.5, -2.0), 0.02);

    ship.single_mut()?.rotation = ship.single_mut()?.rotation.slerp(desired_dir.1, 0.02);

    Ok(())
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
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 120_000.,
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 0.1,
            maximum_distance: 10_000.0,
            first_cascade_far_bound: 100.0,
            overlap_proportion: 0.2,
        }
        .build(),
    ));

    commands.spawn_big_space_default(|root_grid| {
        root_grid.with_grid_default(|sun| {
            sun.insert((Sun, Name::new("Sun")));
            sun.spawn_spatial((
                Mesh3d(sun_mesh_handle),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    emissive: LinearRgba::rgb(1000., 1000., 1000.),
                    ..default()
                })),
                NotShadowCaster,
            ));

            let earth_pos = DVec3::Z * EARTH_ORBIT_RADIUS_M;
            let (earth_cell, earth_pos) = sun.grid().translation_to_grid(earth_pos);
            sun.with_grid_default(|earth| {
                earth.insert((
                    Name::new("Earth"),
                    earth_cell,
                    Mesh3d(earth_mesh_handle),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::Srgba(palettes::css::BLUE),
                        perceptual_roughness: 0.8,
                        reflectance: 1.0,
                        ..default()
                    })),
                    Transform::from_translation(earth_pos)
                        .with_scale(Vec3::splat(EARTH_RADIUS_M as f32)),
                    Rotates(0.000001),
                ));

                let moon_orbit_radius_m = 385e6;
                let moon_pos = DVec3::NEG_Z * moon_orbit_radius_m;
                let (moon_cell, moon_pos) = earth.grid().translation_to_grid(moon_pos);
                earth.spawn_spatial((
                    Name::new("Moon"),
                    Mesh3d(moon_mesh_handle),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::Srgba(palettes::css::GRAY),
                        perceptual_roughness: 1.0,
                        reflectance: 0.0,
                        ..default()
                    })),
                    Transform::from_translation(moon_pos),
                    moon_cell,
                ));

                let ball_pos =
                    DVec3::X * (EARTH_RADIUS_M + 1.0) + DVec3::NEG_Z * 30.0 + DVec3::Y * 10.0;
                let (ball_cell, ball_pos) = earth.grid().translation_to_grid(ball_pos);
                earth
                    .spawn_spatial((ball_cell, Transform::from_translation(ball_pos)))
                    .with_children(|children| {
                        children.spawn((
                            Mesh3d(ball_mesh_handle),
                            MeshMaterial3d(materials.add(StandardMaterial {
                                base_color: Color::WHITE,
                                ..default()
                            })),
                        ));

                        children.spawn((
                            Mesh3d(plane_mesh_handle),
                            MeshMaterial3d(materials.add(StandardMaterial {
                                base_color: Color::Srgba(palettes::css::DARK_GREEN),
                                perceptual_roughness: 1.0,
                                reflectance: 0.0,
                                ..default()
                            })),
                            Transform::from_scale(Vec3::splat(100.0))
                                .with_translation(Vec3::X * -5.0),
                        ));
                    });

                let cam_pos = DVec3::X * (EARTH_RADIUS_M + 1.0);
                let (cam_cell, cam_pos) = earth.grid().translation_to_grid(cam_pos);
                earth.with_grid_default(|camera| {
                    camera.insert((
                        FloatingOrigin,
                        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::X),
                        CameraController::default() // Built-in camera controller
                            .with_speed_bounds([0.1, 10e35])
                            .with_smoothness(0.98, 0.98)
                            .with_speed(1.0),
                        cam_cell,
                    ));

                    camera.spawn_spatial((
                        Camera3d::default(),
                        Transform::from_xyz(0.0, 4.0, 22.0),
                        Camera {
                            hdr: true,
                            ..default()
                        },
                        Exposure::SUNLIGHT,
                        Bloom::NATURAL,
                        bevy::core_pipeline::post_process::ChromaticAberration {
                            intensity: 0.01,
                            ..Default::default()
                        },
                        bevy::core_pipeline::motion_blur::MotionBlur::default(),
                    ));

                    camera.with_child((
                        Spaceship,
                        SceneRoot(asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0")),
                        Transform::from_rotation(Quat::from_rotation_y(core::f32::consts::PI)),
                    ));
                });
            });
        });

        let star_mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::rgb(2., 2., 2.),
            ..default()
        });
        let star_mesh_handle = meshes.add(Sphere::new(1e10).mesh().ico(5).unwrap());
        let rng = Rng::new();
        (0..1000).for_each(|_| {
            root_grid.spawn_spatial((
                Mesh3d(star_mesh_handle.clone()),
                MeshMaterial3d(star_mat.clone()),
                Transform::from_xyz(
                    (rng.f32() - 0.5) * 1e14,
                    (rng.f32() - 0.5) * 1e14,
                    (rng.f32() - 0.5) * 1e14,
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
) -> Result<()> {
    let mut window = windows.single_mut()?;

    if btn.just_pressed(MouseButton::Right) {
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        window.cursor_options.visible = false;
        // window.mode = WindowMode::BorderlessFullscreen;
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
        window.cursor_options.visible = true;
        // window.mode = WindowMode::Windowed;
        cam.defaults_disabled = true;
    }

    Ok(())
}
