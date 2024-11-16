//! This example demonstrates error accumulating from parent to children in nested reference frames.
use bevy::prelude::*;
use bevy_color::palettes::css::BLACK;
use bevy_hanabi::EffectAsset;
use bevy_render::primitives::Aabb;
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            bevy_hanabi::HanabiPlugin,
        ))
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut effects: ResMut<Assets<bevy_hanabi::EffectAsset>>,
) {
    commands.spawn_big_space(ReferenceFrame::<i64>::default(), |root_frame| {
        root_frame.spawn_spatial(DirectionalLightBundle::default());
        root_frame.spawn_spatial(PbrBundle {
            mesh: meshes.add(Sphere::default()),
            material: materials.add(Color::from(BLACK)),
            ..default()
        });
        root_frame.spawn_spatial((
            Camera3dBundle {
                transform: Transform::from_xyz(0.0, 0.0, 50.0),
                camera: Camera {
                    hdr: true,
                    clear_color: ClearColorConfig::Custom(Color::BLACK),
                    ..default()
                },
                ..Default::default()
            },
            bevy::core_pipeline::bloom::BloomSettings {
                intensity: 0.2,
                ..default()
            },
            FloatingOrigin,
            big_space::camera::CameraController::default(),
        ));

        let firework = effects.add(firework_effect());
        for i in 0..10 {
            let i = i as f32 * 100_000.0;
            root_frame.spawn_spatial((
                Name::new("firework"),
                bevy_hanabi::ParticleEffectBundle {
                    effect: bevy_hanabi::ParticleEffect::new(firework.clone()),
                    transform: Transform::from_xyz(i, 0.0, 0.0),
                    ..Default::default()
                },
                Aabb::default(),
            ));
        }
    });
}

fn firework_effect() -> EffectAsset {
    use bevy_hanabi::prelude::*;
    let mut color_gradient1 = Gradient::new();
    color_gradient1.add_key(0.0, Vec4::new(4.0, 4.0, 4.0, 1.0));
    color_gradient1.add_key(0.1, Vec4::new(4.0, 4.0, 0.0, 1.0));
    color_gradient1.add_key(0.6, Vec4::new(4.0, 0.0, 0.0, 1.0));
    color_gradient1.add_key(1.0, Vec4::new(4.0, 0.0, 0.0, 0.0));

    let mut size_gradient1 = Gradient::new();
    size_gradient1.add_key(0.0, Vec3::splat(0.05));
    size_gradient1.add_key(0.3, Vec3::splat(0.05));
    size_gradient1.add_key(1.0, Vec3::splat(0.0));

    let writer = ExprWriter::new();

    // Give a bit of variation by randomizing the age per particle. This will
    // control the starting color and starting size of particles.
    let age = writer.lit(0.).uniform(writer.lit(0.2)).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);

    // Give a bit of variation by randomizing the lifetime per particle
    let lifetime = writer.lit(0.8).normal(writer.lit(1.2)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    // Add constant downward acceleration to simulate gravity
    let accel = writer.lit(Vec3::Y * -16.).expr();
    let update_accel = AccelModifier::new(accel);

    // Add drag to make particles slow down a bit after the initial explosion
    let drag = writer.lit(4.).expr();
    let update_drag = LinearDragModifier::new(drag);

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(0.1).expr(),
        dimension: ShapeDimension::Volume,
    };

    // Give a bit of variation by randomizing the initial speed
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: (writer.rand(ScalarType::Float) * writer.lit(20.) + writer.lit(60.)).expr(),
    };

    // Clear the trail velocity so trail particles just stay in place as they fade
    // away
    let init_vel_trail =
        SetAttributeModifier::new(Attribute::VELOCITY, writer.lit(Vec3::ZERO).expr());

    let lead = ParticleGroupSet::single(0);
    let trail = ParticleGroupSet::single(1);

    let effect = EffectAsset::new(
        // 2k lead particles, with 32 trail particles each
        2048,
        Spawner::burst(2048.0.into(), 2.0.into()),
        writer.finish(),
    )
    .with_simulation_space(SimulationSpace::Local)
    // Tie together trail particles to make arcs. This way we don't need a lot of them, yet there's
    // a continuity between them.
    .with_ribbons(2048 * 32, 1.0 / 64.0, 0.2, 0)
    .with_name("firework")
    .init_groups(init_pos, lead)
    .init_groups(init_vel, lead)
    .init_groups(init_age, lead)
    .init_groups(init_lifetime, lead)
    .init_groups(init_vel_trail, trail)
    .update_groups(update_drag, lead)
    .update_groups(update_accel, lead)
    .render_groups(
        ColorOverLifetimeModifier {
            gradient: color_gradient1.clone(),
        },
        lead,
    )
    .render_groups(
        SizeOverLifetimeModifier {
            gradient: size_gradient1.clone(),
            screen_space_size: false,
        },
        lead,
    )
    .render_groups(
        ColorOverLifetimeModifier {
            gradient: color_gradient1,
        },
        trail,
    )
    .render_groups(
        SizeOverLifetimeModifier {
            gradient: size_gradient1,
            screen_space_size: false,
        },
        trail,
    );

    effect
}
