use bevy::{
    math::Vec3A,
    prelude::*,
    render::primitives::{Aabb, Sphere},
};
use big_space::{
    camera::{CameraController, CameraInput, CameraVelocity},
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
        CameraController {
            max_speed: 10e35,
            smoothness: 0.8,
            ..default()
        }, // Built-in camera controller
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
        base_color: Color::MIDNIGHT_BLUE,
        perceptual_roughness: 0.8,
        reflectance: 1.0,
        ..default()
    });

    let mut translation = Vec3::ZERO;
    for i in 1..=100i128 {
        let j = i.pow(14) as f32;
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
        .with_text_alignment(TextAlignment::TOP_LEFT)
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
    velocity: Res<CameraVelocity>,
) {
    let (cell, transform) = origin.single();
    let translation = transform.translation;

    let grid_text = format!("Origin GridCell: {}x, {}y, {}z", cell.x, cell.y, cell.z);

    let translation_text = format!(
        "Origin Transform: {:>8.2}x, {:>8.2}y, {:>8.2}z",
        translation.x, translation.y, translation.z
    );

    let speed = velocity.translation().length() / time.delta_seconds_f64();
    let camera_text = if speed > 3.0e8 {
        format!("Camera Speed: {:.0e} x speed of light", speed / 3.0e8)
    } else {
        format!("Camera Speed: {:.2e}", speed)
    };

    text.single_mut().sections[0].value = format!("{grid_text}\n{translation_text}\n{camera_text}");
}

fn cursor_grab_system(
    mut windows: ResMut<Windows>,
    mut cam: ResMut<CameraInput>,
    btn: Res<Input<MouseButton>>,
    key: Res<Input<KeyCode>>,
) {
    use bevy::window::CursorGrabMode;
    let window = windows.get_primary_mut().unwrap();

    if btn.just_pressed(MouseButton::Left) {
        window.set_cursor_grab_mode(CursorGrabMode::Locked);
        window.set_cursor_visibility(false);
        window.set_mode(WindowMode::BorderlessFullscreen);
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.set_cursor_grab_mode(CursorGrabMode::None);
        window.set_cursor_visibility(true);
        window.set_mode(WindowMode::Windowed);
        cam.defaults_disabled = true;
    }
}
