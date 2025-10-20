//! Demonstrates using the plugin over a wide range of scales, from protons to the universe.

use bevy::{
    color::palettes,
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpaceDefaultPlugins,
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, (setup, ui_setup))
        .add_systems(PreUpdate, (cursor_grab_system, ui_text_system))
        .add_systems(
            PostUpdate,
            highlight_nearest_sphere.after(TransformSystems::Propagate),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn_big_space_default(|root| {
        root.spawn_spatial((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                near: 1e-18,
                ..default()
            }),
            Transform::from_xyz(0.0, 0.0, 8.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            FloatingOrigin, // Important: marks the floating origin entity for rendering.
            BigSpaceCameraController::default() // Built-in camera controller
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

            root.spawn_spatial((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(matl_handle.clone()),
                Transform::from_scale(Vec3::splat(j)).with_translation(translation),
            ));
        }

        // light
        root.spawn_spatial(DirectionalLight {
            illuminance: 10_000.0,
            ..default()
        });
    });
}

#[derive(Component, Reflect)]
struct BigSpaceDebugText;

#[derive(Component, Reflect)]
struct FunFactText;

fn ui_setup(mut commands: Commands) {
    commands.spawn((
        Text::default(),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::new_with_justify(Justify::Left),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        BigSpaceDebugText,
    ));

    commands.spawn((
        Text::default(),
        TextFont {
            font_size: 52.0,
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            right: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        FunFactText,
    ));
}

fn highlight_nearest_sphere(
    cameras: Query<&BigSpaceCameraController>,
    objects: Query<&GlobalTransform>,
    mut gizmos: Gizmos,
) -> Result {
    let Some((entity, _)) = cameras.single()?.nearest_object() else {
        return Ok(());
    };
    let transform = objects.get(entity)?;
    gizmos
        .sphere(
            transform.translation(),
            transform.scale().x * 0.505,
            Color::Srgba(palettes::basic::RED),
        )
        .resolution(128);
    Ok(())
}

#[allow(clippy::type_complexity)]
fn ui_text_system(
    mut debug_text: Query<&mut Text, (With<BigSpaceDebugText>, Without<FunFactText>)>,
    mut fun_text: Query<&mut Text, (With<FunFactText>, Without<BigSpaceDebugText>)>,
    grids: Grids,
    time: Res<Time>,
    origin: Query<(Entity, CellTransformReadOnly), With<FloatingOrigin>>,
    camera: Query<&BigSpaceCameraController>,
    objects: Query<&Transform, With<Mesh3d>>,
) -> Result {
    let (origin_entity, origin_pos) = origin.single()?;
    let translation = origin_pos.transform.translation;

    let grid_text = format!(
        "GridCell: {}x, {}y, {}z",
        origin_pos.cell.x, origin_pos.cell.y, origin_pos.cell.z
    );

    let translation_text = format!(
        "Transform: {}x, {}y, {}z",
        translation.x, translation.y, translation.z
    );

    let Some(grid) = grids.parent_grid(origin_entity) else {
        return Ok(());
    };

    let real_position = grid.grid_position_double(origin_pos.cell, origin_pos.transform);
    let real_position_f64_text = format!(
        "Combined (f64): {}x, {}y, {}z",
        real_position.x, real_position.y, real_position.z
    );
    let real_position_f32_text = format!(
        "Combined (f32): {}x, {}y, {}z",
        real_position.x as f32, real_position.y as f32, real_position.z as f32
    );

    let velocity = camera.single()?.velocity();
    let speed = velocity.0.length() / time.delta_secs_f64();
    let camera_text = if speed > 3.0e8 {
        format!("Speed: {:.0e} * speed of light", speed / 3.0e8)
    } else {
        format!("Speed: {speed:.2e} m/s")
    };

    let (nearest_text, fact_text) = if let Some(nearest) = camera.single()?.nearest_object() {
        let dia = objects.get(nearest.0)?.scale.max_element();
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

    debug_text.single_mut()?.0 = format!(
        "{grid_text}\n{translation_text}\n\n{real_position_f64_text}\n{real_position_f32_text}\n\n{camera_text}\n{nearest_text}"
    );

    fun_text.single_mut()?.0 = fact_text;

    Ok(())
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
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut cam: ResMut<big_space::camera::BigSpaceCameraInput>,
    btn: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) -> Result {
    let mut cursor_options = windows.single_mut()?;

    if btn.just_pressed(MouseButton::Left) {
        cursor_options.grab_mode = CursorGrabMode::Locked;
        cursor_options.visible = false;
        cam.defaults_disabled = false;
    }

    if key.just_pressed(KeyCode::Escape) {
        cursor_options.grab_mode = CursorGrabMode::None;
        cursor_options.visible = true;
        // window.mode = WindowMode::Windowed;
        cam.defaults_disabled = true;
    }

    Ok(())
}
