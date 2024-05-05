use bevy::{
    prelude::*,
    transform::TransformSystem,
    window::{CursorGrabMode, PrimaryWindow},
};
use big_space::{
    camera::{CameraController, CameraInput},
    propagation::IgnoreFloatingOrigin,
    world_query::GridTransformReadOnly,
    FloatingOrigin, GridCell,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            big_space::FloatingOriginPlugin::<i128>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i128>::default(),
            big_space::camera::CameraControllerPlugin::<i128>::default(),
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
) {
    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 0.0, 8.0)
                .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                near: 1e-18,
                ..default()
            }),
            ..default()
        },
        GridCell::<i128>::default(), // All spatial entities need this component
        FloatingOrigin::<0>,              // Important: marks the floating origin entity for rendering.
        CameraController::default() // Built-in camera controller
            .with_speed_bounds([10e-18, 10e35])
            .with_smoothness(0.9, 0.8)
            .with_speed(1.0),
    ));

    let mesh_handle = meshes.add(Sphere::new(0.5).mesh().ico(32).unwrap());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::BLUE,
        perceptual_roughness: 0.8,
        reflectance: 1.0,
        ..default()
    });

    let mut translation = Vec3::ZERO;
    for i in -16..=27 {
        let j = 10_f32.powf(i as f32);
        let k = 10_f32.powf((i - 1) as f32);
        translation.x += j / 2.0 + k;
        commands.spawn((
            PbrBundle {
                mesh: mesh_handle.clone(),
                material: matl_handle.clone(),
                transform: Transform::from_scale(Vec3::splat(j)).with_translation(translation),
                ..default()
            },
            GridCell::<i128>::default(),
        ));
    }

    // light
    commands.spawn((DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 100_000.0,
            ..default()
        },
        ..default()
    },));
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
                font_size: 28.0,
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
        IgnoreFloatingOrigin::<0>,
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
        IgnoreFloatingOrigin::<0>,
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
        .sphere(translation, Quat::IDENTITY, scale.x * 0.505, Color::RED)
        .circle_segments(128);
}

#[allow(clippy::type_complexity)]
fn ui_text_system(
    mut debug_text: Query<
        (&mut Text, &GlobalTransform),
        (With<BigSpaceDebugText>, Without<FunFactText>),
    >,
    mut fun_text: Query<&mut Text, (With<FunFactText>, Without<BigSpaceDebugText>)>,
    time: Res<Time>,
    origin: Query<GridTransformReadOnly<i128, 0>, With<FloatingOrigin>>,
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

    debug_text.0.sections[0].value =
        format!("{grid_text}\n{translation_text}\n{camera_text}\n{nearest_text}");

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
            min = item.to_owned();
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
