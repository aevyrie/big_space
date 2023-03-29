use bevy::{
    math::Vec3A,
    prelude::*,
    render::primitives::{Aabb, Sphere},
    window::{CursorGrabMode, PrimaryWindow, Window, WindowMode},
};
use big_space::{
    camera::{CameraController, CameraInput},
    FloatingOrigin, GridCell,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
        .add_plugin(big_space::FloatingOriginPlugin::<i128>::default())
        .add_plugin(big_space::debug::FloatingOriginDebugPlugin::<i128>::default())
        .add_plugin(big_space::camera::CameraControllerPlugin::<i128>::default())
        .add_plugin(bevy_framepace::FramepacePlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .add_startup_system(setup)
        .add_system(cursor_grab_system)
        .add_system(ui_text_system)
        .add_startup_system(ui_setup)
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
            ..default()
        },
        GridCell::<i128>::default(), // All spatial entities need this component
        FloatingOrigin, // Important: marks this as the entity to use as the floating origin
        CameraController::default().with_max_speed(10e35), // Built-in camera controller
    ));

    let mesh_handle = meshes.add(
        shape::Icosphere {
            radius: 0.5,
            subdivisions: 32,
        }
        .try_into()
        .unwrap(),
    );
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::BLUE,
        perceptual_roughness: 0.8,
        reflectance: 1.0,
        ..default()
    });

    let mut translation = Vec3::ZERO;
    for i in 1..=37_i128 {
        let j = 10_f32.powf(i as f32 - 10.0);
        translation.x += j;
        commands.spawn((
            PbrBundle {
                mesh: mesh_handle.clone(),
                material: matl_handle.clone(),
                transform: Transform::from_scale(Vec3::splat(j)).with_translation(translation),
                ..default()
            },
            Aabb::from(Sphere {
                center: Vec3A::ZERO,
                radius: j / 2.0,
            }),
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

fn ui_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        TextBundle::from_section(
            "",
            TextStyle {
                font: asset_server.load("fonts/FiraMono-Regular.ttf"),
                font_size: 18.0,
                color: Color::WHITE,
            },
        )
        .with_text_alignment(TextAlignment::Left)
        .with_style(Style {
            position_type: PositionType::Absolute,
            position: UiRect {
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                ..default()
            },
            ..default()
        }),
        BigSpaceDebugText,
    ));
}

fn ui_text_system(
    mut text: Query<&mut Text, With<BigSpaceDebugText>>,
    time: Res<Time>,
    origin: Query<(&GridCell<i128>, &Transform), With<FloatingOrigin>>,
    camera: Query<&CameraController>,
    objects: Query<&Transform, With<Handle<Mesh>>>,
) {
    let (cell, transform) = origin.single();
    let translation = transform.translation;

    let grid_text = format!("Origin GridCell: {}x, {}y, {}z", cell.x, cell.y, cell.z);

    let translation_text = format!(
        "Origin Transform: {:>8.2}x, {:>8.2}y, {:>8.2}z",
        translation.x, translation.y, translation.z
    );

    let velocity = camera.single().velocity();
    let speed = velocity.0.length() / time.delta_seconds_f64();
    let camera_text = if speed > 3.0e8 {
        format!("Camera Speed: {:.0e} * speed of light", speed / 3.0e8)
    } else {
        format!("Camera Speed: {:.2e} m/s", speed)
    };

    let nearest_text = if let Some(nearest) = camera.single().nearest_object() {
        let dia = objects.get(nearest.0).unwrap().scale.max_element();
        let dia_fact = match dia {
            d if d > 8.8e26 => "(Greater than the diameter of the observable universe)",
            d if d > 1e21 => "(Greater than the diameter of the Milky Way galaxy)",
            d if d > 7e12 => "(Greater than the diameter of Pluto's orbit)",
            d if d > 1.4e9 => "(Greater than the diameter of the Sun)",
            d if d > 1.4e8 => "(Greater than the diameter of Earth)",
            d if d > 12e6 => "(Greater than the diameter of Earth)",
            d if d > 3e6 => "(Greater than the diameter of the Moon)",
            _ => "",
        };
        let dist = nearest.1;
        format!("Nearest sphere diameter: {dia:.0e} m    {dia_fact}\nNearest sphere distance: {dist:.0e} m",)
    } else {
        "".into()
    };

    text.single_mut().sections[0].value =
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
