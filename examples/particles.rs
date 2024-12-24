//! Demonstration of using `bevy_hanabi` gpu particles with `big_space` to render a particle trail
//! that follows the camera even when it moves between cells.

use bevy::prelude::*;
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            bevy_hanabi::HanabiPlugin, // TODO fix once hanabi updates to bevy 0.15
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(
            PostUpdate,
            update_trail.after(TransformSystem::TransformPropagate),
        )
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut effects: ResMut<Assets<bevy_hanabi::EffectAsset>>,
) {
    let effect = effects.add(particle_effect());
    commands.spawn_big_space_default::<i64>(|root| {
        root.spawn_spatial(DirectionalLight::default());
        root.spawn_spatial((
            Mesh3d(meshes.add(Sphere::default())),
            MeshMaterial3d(materials.add(Color::BLACK)),
        ));

        root.spawn_spatial((
            Transform::from_xyz(0.0, 0.0, 50.0),
            Camera {
                hdr: true,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            Camera3d::default(),
            bevy::core_pipeline::bloom::Bloom {
                intensity: 0.2,
                ..default()
            },
            FloatingOrigin,
            big_space::camera::CameraController::default().with_smoothness(0.98, 0.9),
        ));

        // Because we want the trail to be fixed in the root grid, we spawn it here,
        // instead of on the camera itself.
        root.spawn_spatial((
            Name::new("effect"),
            bevy_hanabi::ParticleEffectBundle {
                effect: bevy_hanabi::ParticleEffect::new(effect.clone()),
                ..Default::default()
            },
        ));
    });
}

/// Update the trail with the latest camera position.
///
/// Working with `GlobalTransform` is preferred when working on a rendering feature like this with
/// big_space. This is because you will be working with the same coordinates that are being sent to
/// the GPU, allowing you to ignore GridCells and other implementation details of big_space.
///
/// To update our trail, all we need to do is update the latest position of the camera, from the
/// perspective of the emitter, which is simply `cam_translation - emitter_translation`.
///
/// IMPORTANT: The only thing this example is missing is handling when the object with a trail moves
/// far from the emitter. If you move too far away, you will need to spawn a new emitter at the
/// current location of the moving object, and keep the old emitter around until the trail fades
/// away. In other words, the object with a trail should leave behind a series of emitters behind
/// it, like breadcrumbs, as it moves across large distances.
fn update_trail(
    cam: Query<&GlobalTransform, With<Camera>>,
    query: Query<&GlobalTransform, With<bevy_hanabi::ParticleEffect>>,
    mut effect: Query<&mut bevy_hanabi::EffectProperties>,
) {
    let cam = cam.single();
    let Ok(mut properties) = effect.get_single_mut() else {
        return;
    };
    for emitter in query.iter() {
        let pos = cam.translation() - emitter.translation();
        properties.set("latest_pos", (pos).into());
    }
}

// Below is copied from bevy_hanabi's example. The one modification is that you always want to be
// using `SimulationSpace::Local`. Using the global space will not work with `big_space` when
// entities move between cells.

const LIFETIME: f32 = 10.0;
const TRAIL_SPAWN_RATE: f32 = 256.0;

fn particle_effect() -> bevy_hanabi::EffectAsset {
    use bevy_hanabi::prelude::*;
    use bevy_math::vec4;

    let writer = ExprWriter::new();

    let init_position_attr = SetAttributeModifier {
        attribute: Attribute::POSITION,
        value: writer.lit(Vec3::ZERO).expr(),
    };

    let init_velocity_attr = SetAttributeModifier {
        attribute: Attribute::VELOCITY,
        value: writer.lit(Vec3::ZERO).expr(),
    };

    let init_age_attr = SetAttributeModifier {
        attribute: Attribute::AGE,
        value: writer.lit(0.0).expr(),
    };

    let init_lifetime_attr = SetAttributeModifier {
        attribute: Attribute::LIFETIME,
        value: writer.lit(999999.0).expr(),
    };

    let init_size_attr = SetAttributeModifier {
        attribute: Attribute::SIZE,
        value: writer.lit(20.5).expr(),
    };

    let pos = writer.add_property("latest_pos", Vec3::ZERO.into());
    let pos = writer.prop(pos);

    let move_modifier = SetAttributeModifier {
        attribute: Attribute::POSITION,
        value: pos.expr(),
    };

    let render_color = ColorOverLifetimeModifier {
        gradient: Gradient::linear(vec4(3.0, 0.0, 0.0, 1.0), vec4(3.0, 0.0, 0.0, 0.0)),
    };

    EffectAsset::new(256, Spawner::once(1.0.into(), true), writer.finish())
        .with_ribbons(32768, 1.0 / TRAIL_SPAWN_RATE, LIFETIME, 0)
        .with_simulation_space(SimulationSpace::Local)
        .init_groups(init_position_attr, ParticleGroupSet::single(0))
        .init_groups(init_velocity_attr, ParticleGroupSet::single(0))
        .init_groups(init_age_attr, ParticleGroupSet::single(0))
        .init_groups(init_lifetime_attr, ParticleGroupSet::single(0))
        .init_groups(init_size_attr, ParticleGroupSet::single(0))
        .update_groups(move_modifier, ParticleGroupSet::single(0))
        .render(SizeOverLifetimeModifier {
            gradient: Gradient::from_keys([
                (0., Vec3::splat(0.0)),
                (0.1, Vec3::splat(0.0)),
                (0.2, Vec3::splat(200.0)),
                (1.0, Vec3::splat(0.0)),
            ]),
            ..default()
        })
        .render_groups(render_color, ParticleGroupSet::single(1))
}
