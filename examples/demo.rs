use bevy::{
    math::Vec3A,
    prelude::*,
    render::primitives::{Aabb, Sphere},
};
use big_space::{camera::CameraController, FloatingOrigin, GridCell};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
        .add_plugin(big_space::FloatingOriginPlugin::<i128>::default())
        .add_plugin(big_space::debug::FloatingOriginDebugPlugin::<i128>::default())
        .add_plugin(big_space::camera::CameraControllerPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .add_startup_system(setup)
        .add_system(cursor_grab_system)
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
            max_speed: 10e12,
            smoothness: 0.9,
            ..default()
        }, // Built-in camera controller
    ));

    let mesh_handle = meshes.add(
        shape::Icosphere {
            radius: 0.5,
            subdivisions: 16,
        }
        .try_into()
        .unwrap(),
    );
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::YELLOW,
        unlit: false,
        ..default()
    });

    let mut translation = Vec3::ZERO;
    for i in 1..100i128 {
        let j = i.pow(10) as f32 + 1.0;
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

fn cursor_grab_system(
    mut windows: ResMut<Windows>,
    btn: Res<Input<MouseButton>>,
    key: Res<Input<KeyCode>>,
) {
    let window = windows.get_primary_mut().unwrap();

    use bevy::window::CursorGrabMode;
    if btn.just_pressed(MouseButton::Left) {
        window.set_cursor_grab_mode(CursorGrabMode::Locked);
        window.set_cursor_visibility(false);
    }

    if key.just_pressed(KeyCode::Escape) {
        window.set_cursor_grab_mode(CursorGrabMode::None);
        window.set_cursor_visibility(true);
    }
}
