use bevy::{
    core_pipeline::bloom::BloomSettings,
    prelude::*,
    transform::TransformSystem,
    window::{CursorGrabMode, PrimaryWindow, Window, WindowMode},
};
use big_space::{
    camera::{CameraController, CameraInput},
    world_query::GridTransformReadOnly,
    FloatingOrigin, FloatingOriginSettings, GridCell,
};

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
        .add_systems(Startup, (setup, ui_setup))
        .add_systems(Update, (cursor_grab_system, ui_text_system))
        .add_systems(
            PostUpdate,
            highlight_nearest_sphere.after(TransformSystem::TransformPropagate),
        )
        .run()
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    space: Res<FloatingOriginSettings>,
) {
    let mut sphere = |radius| {
        meshes.add(
            shape::Icosphere {
                radius,
                subdivisions: 32,
            }
            .try_into()
            .unwrap(),
        )
    };
    let sun_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(100., 100., 100.),
        ..default()
    });

    let sun_radius_m = 695_508_000.0;

    commands
        .spawn((
            GridCell::<i64>::ZERO,
            PointLightBundle {
                point_light: PointLight {
                    intensity: 35.73e24,
                    range: 1e20,
                    radius: sun_radius_m,
                    shadows_enabled: true,
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|builder| {
            builder.spawn((
                PbrBundle {
                    mesh: sphere(sun_radius_m),
                    material: sun_mat,
                    ..default()
                },
                GridCell::<i64>::ZERO,
            ));
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
        space.imprecise_translation_to_grid(Vec3::X * earth_orbit_radius_m);

    commands.spawn((
        PbrBundle {
            mesh: sphere(earth_radius_m),
            material: earth_mat,
            transform: Transform::from_translation(earth_pos),
            ..default()
        },
        earth_cell,
    ));

    let moon_orbit_radius_m = 385e6;
    let moon_radius_m = 1.7375e6;

    let moon_mat = materials.add(StandardMaterial {
        base_color: Color::DARK_GRAY,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });

    let (moon_cell, moon_pos): (GridCell<i64>, _) =
        space.imprecise_translation_to_grid(Vec3::Z * moon_orbit_radius_m);

    commands.spawn((
        PbrBundle {
            mesh: sphere(moon_radius_m),
            material: moon_mat,
            transform: Transform::from_translation(earth_pos + moon_pos),
            ..default()
        },
        moon_cell + earth_cell,
    ));

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(686., -181., 80.)
                .looking_to(-Vec3::Z * 0.6 - Vec3::X - Vec3::Y * 0.1, Vec3::Y),
            camera: Camera {
                hdr: true,
                ..default()
            },
            ..default()
        },
        BloomSettings::default(),
        GridCell::<i64>::new(74899712, 45839, 232106),
        FloatingOrigin, // Important: marks the floating origin entity for rendering.
        CameraController::default() // Built-in camera controller
            .with_speed_bounds([10e-18, 10e35])
            .with_smoothness(0.9, 0.8)
            .with_speed(1.0),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.05,
    });
}

#[derive(Component, Reflect)]
pub struct BigSpaceDebugText;

fn ui_setup(mut commands: Commands) {
    commands.spawn((
        TextBundle::from_section(
            "",
            TextStyle {
                font_size: 28.0,
                color: Color::WHITE,
                ..default()
            },
        )
        .with_text_alignment(TextAlignment::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        }),
        BigSpaceDebugText,
    ));
}

fn highlight_nearest_sphere(
    cameras: Query<&CameraController>,
    objects: Query<&GlobalTransform>,
    mut gizmos: Gizmos,
) {
    let Some((entity, _)) = cameras.single().nearest_object() else {
        return;
    };
    let Ok(transform) = objects.get(entity) else {
        return;
    };
    let (scale, rotation, translation) = transform.to_scale_rotation_translation();
    gizmos
        .sphere(translation, rotation, scale.x * 0.505, Color::RED)
        .circle_segments(128);
}

fn ui_text_system(
    mut debug_text: Query<&mut Text, With<BigSpaceDebugText>>,
    time: Res<Time>,
    origin: Query<GridTransformReadOnly<i64>, With<FloatingOrigin>>,
    camera: Query<&CameraController>,
    objects: Query<&Transform, With<Handle<Mesh>>>,
) {
    let origin = origin.single();
    let translation = origin.transform.translation;

    let grid_text = format!(
        "GridCell: {}x, {}y, {}z",
        origin.cell.x, origin.cell.y, origin.cell.z
    );

    let translation_text = format!(
        "Transform: {:>8.2}x, {:>8.2}y, {:>8.2}z",
        translation.x, translation.y, translation.z
    );

    let velocity = camera.single().velocity();
    let speed = velocity.0.length() / time.delta_seconds_f64();
    let camera_text = if speed > 3.0e8 {
        format!("Speed: {:.0e} * speed of light", speed / 3.0e8)
    } else {
        format!("Speed: {:.2e} m/s", speed)
    };

    let nearest_text = if let Some(nearest) = camera.single().nearest_object() {
        let dia = objects.get(nearest.0).unwrap().scale.max_element();
        let dist = nearest.1;
        format!("\nNearest sphere distance: {dist:.0e} m\nNearest sphere diameter: {dia:.0e} m",)
    } else {
        "".into()
    };

    debug_text.single_mut().sections[0].value =
        format!("{grid_text}\n{translation_text}\n{camera_text}\n{nearest_text}");
}

fn cursor_grab_system(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut cam: ResMut<CameraInput>,
    btn: Res<Input<MouseButton>>,
    key: Res<Input<KeyCode>>,
) {
    let Some(mut window) = windows.get_single_mut().ok() else {
        return;
    };

    if btn.just_pressed(MouseButton::Left) {
        window.cursor.grab_mode = CursorGrabMode::Locked;
        window.cursor.visible = false;
        window.mode = WindowMode::BorderlessFullscreen;
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
        window.mode = WindowMode::Windowed;
        cam.defaults_disabled = true;
    }
}
