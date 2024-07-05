use bevy::{
    prelude::*,
    transform::TransformSystem,
    window::{CursorGrabMode, PrimaryWindow},
};
use bevy_color::palettes;
use big_space::{
    camera::{CameraController, CameraInput},
    commands::BigSpaceCommands,
    reference_frame::{local_origin::ReferenceFrames, ReferenceFrame},
    world_query::GridTransformReadOnly,
    FloatingOrigin,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::BigSpacePlugin::<i128>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i128>::default(),
            big_space::camera::CameraControllerPlugin::<i128>::default(),
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, (setup, ui_setup))
        .add_systems(PreUpdate, (cursor_grab_system, ui_text_system))
        .add_systems(
            PostUpdate,
            highlight_nearest_sphere.after(TransformSystem::TransformPropagate),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn_big_space(ReferenceFrame::<i128>::default(), |root| {
        root.spawn_spatial((
            Camera3dBundle {
                transform: Transform::from_xyz(0.0, 0.0, 8.0)
                    .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
                projection: Projection::Perspective(PerspectiveProjection {
                    near: 1e-18,
                    ..default()
                }),
                ..default()
            },
            FloatingOrigin, // Important: marks the floating origin entity for rendering.
            CameraController::default() // Built-in camera controller
                .with_speed_bounds([10e-18, 10e35])
                .with_smoothness(0.9, 0.8)
                .with_speed(1.0),
        ));

        let mesh_handle = meshes.add(Sphere::new(0.5).mesh().ico(32).unwrap());
        let matl_handle = materials.add(StandardMaterial {
            base_color: Color::Srgba(palettes::basic::BLUE),
            perceptual_roughness: 0.8,
            reflectance: 1.0,
            ..default()
        });

        let mut translation = Vec3::ZERO;
        for i in -16..=27 {
            let j = 10_f32.powf(i as f32);
            let k = 10_f32.powf((i - 1) as f32);
            translation.x += j / 2.0 + k;
            translation.y = j / 2.0;

            root.spawn_spatial(PbrBundle {
                mesh: mesh_handle.clone(),
                material: matl_handle.clone(),
                transform: Transform::from_scale(Vec3::splat(j)).with_translation(translation),
                ..default()
            });
        }

        // light
        root.spawn_spatial(DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 10_000.0,
                ..default()
            },
            ..default()
        });
    });
}

#[derive(Component, Reflect)]
pub struct BigSpaceDebugText;

#[derive(Component, Reflect)]
pub struct FunFactText;

fn ui_setup(mut commands: Commands) {
    commands.spawn((
        TextBundle::from_section(
            "",
            TextStyle {
                font_size: 18.0,
                color: Color::WHITE,
                ..default()
            },
        )
        .with_text_justify(JustifyText::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        }),
        BigSpaceDebugText,
    ));

    commands.spawn((
        TextBundle::from_section(
            "",
            TextStyle {
                font_size: 52.0,
                color: Color::WHITE,
                ..default()
            },
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            right: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        })
        .with_text_justify(JustifyText::Center),
        FunFactText,
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
    // Ignore rotation due to panicking in gizmos, as of bevy 0.13
    let (scale, _, translation) = transform.to_scale_rotation_translation();
    gizmos
        .sphere(
            translation,
            Quat::IDENTITY, // Bevy likes to explode on non-normalized quats in gizmos,
            scale.x * 0.505,
            Color::Srgba(palettes::basic::RED),
        )
        .resolution(128);
}

#[allow(clippy::type_complexity)]
fn ui_text_system(
    mut debug_text: Query<
        (&mut Text, &GlobalTransform),
        (With<BigSpaceDebugText>, Without<FunFactText>),
    >,
    mut fun_text: Query<&mut Text, (With<FunFactText>, Without<BigSpaceDebugText>)>,
    ref_frames: ReferenceFrames<i128>,
    time: Res<Time>,
    origin: Query<(Entity, GridTransformReadOnly<i128>), With<FloatingOrigin>>,
    camera: Query<&CameraController>,
    objects: Query<&Transform, With<Handle<Mesh>>>,
) {
    let (origin_entity, origin_pos) = origin.single();
    let translation = origin_pos.transform.translation;

    let grid_text = format!(
        "GridCell: {}x, {}y, {}z",
        origin_pos.cell.x, origin_pos.cell.y, origin_pos.cell.z
    );

    let translation_text = format!(
        "Transform: {}x, {}y, {}z",
        translation.x, translation.y, translation.z
    );

    let Some(ref_frame) = ref_frames.parent_frame(origin_entity) else {
        return;
    };

    let real_position = ref_frame.grid_position_double(origin_pos.cell, origin_pos.transform);
    let real_position_f64_text = format!(
        "Combined (f64): {}x, {}y, {}z",
        real_position.x, real_position.y, real_position.z
    );
    let real_position_f32_text = format!(
        "Combined (f32): {}x, {}y, {}z",
        real_position.x as f32, real_position.y as f32, real_position.z as f32
    );

    let velocity = camera.single().velocity();
    let speed = velocity.0.length() / time.delta_seconds_f64();
    let camera_text = if speed > 3.0e8 {
        format!("Speed: {:.0e} * speed of light", speed / 3.0e8)
    } else {
        format!("Speed: {:.2e} m/s", speed)
    };

    let (nearest_text, fact_text) = if let Some(nearest) = camera.single().nearest_object() {
        let dia = objects.get(nearest.0).unwrap().scale.max_element();
        let (fact_dia, fact) = closest(dia);
        let dist = nearest.1;
        let multiple = dia / fact_dia;
        (
            format!(
                "\nNearest sphere distance: {dist:.0e} m\nNearest sphere diameter: {dia:.0e} m",
            ),
            format!("{multiple:.1}x {fact}"),
        )
    } else {
        ("".into(), "".into())
    };

    let mut debug_text = debug_text.single_mut();

    debug_text.0.sections[0].value = format!(
        "{grid_text}\n{translation_text}\n\n{real_position_f64_text}\n{real_position_f32_text}\n\n{camera_text}\n{nearest_text}"
    );

    fun_text.single_mut().sections[0].value = fact_text
}

fn closest<'a>(diameter: f32) -> (f32, &'a str) {
    let items = vec![
        (8.8e26, "diameter of the observable universe"),
        (9e25, "length of the Hercules-Corona Borealis Great Wall"),
        (1e24, "diameter of the Local Supercluster"),
        (9e22, "diameter of the Local Group"),
        (1e21, "diameter of the Milky Way galaxy"),
        (5e16, "length of the Pillars of Creation"),
        (1.8e14, "diameter of Messier 87"),
        (7e12, "diameter of Pluto's orbit"),
        (24e9, "diameter of Sagittarius A"),
        (1.4e9, "diameter of the Sun"),
        (1.4e8, "diameter of Jupiter"),
        (12e6, "diameter of Earth"),
        (3e6, "diameter of the Moon"),
        (9e3, "height of Mt. Everest"),
        (3.8e2, "height of the Empire State Building"),
        (2.5e1, "length of a train car"),
        (1.8, "height of a human"),
        (1e-1, "size of a cat"),
        (1e-2, "size of a mouse"),
        (1e-3, "size of an insect"),
        (1e-4, "diameter of a eukaryotic cell"),
        (1e-5, "width of a human hair"),
        (1e-6, "diameter of a bacteria"),
        (5e-8, "size of a phage"),
        (5e-9, "size of a transistor"),
        (1e-10, "diameter of a carbon atom"),
        (4e-11, "diameter of a hydrogen atom"),
        (4e-12, "diameter of an electron"),
        (1.9e-15, "diameter of a proton"),
    ];

    let mut min = items[0];
    for item in items.iter() {
        if (item.0 - diameter).abs() < (min.0 - diameter).abs() {
            item.clone_into(&mut min);
        }
    }
    min
}

fn cursor_grab_system(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut cam: ResMut<CameraInput>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let Some(mut window) = windows.get_single_mut().ok() else {
        return;
    };

    if btn.just_pressed(MouseButton::Left) {
        window.cursor.grab_mode = CursorGrabMode::Locked;
        window.cursor.visible = false;
        // window.mode = WindowMode::BorderlessFullscreen;
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
        // window.mode = WindowMode::Windowed;
        cam.defaults_disabled = true;
    }
}
