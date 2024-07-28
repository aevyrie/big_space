use bevy::prelude::*;
use big_space::{
    camera::{CameraController, CameraControllerPlugin},
    spatial_hash::{SpatialHashMap, SpatialHashPlugin},
    *,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::<i32>::default(),
            SpatialHashPlugin::<i32>::default(),
            CameraControllerPlugin::<i32>::default(),
        ))
        .add_systems(Startup, spawn)
        .add_systems(Update, move_player)
        .init_resource::<MaterialPresets>()
        .run();
}

const HALF_WIDTH: f32 = 20.0;
const N_ENTITIES: usize = 40_000;

#[derive(Component)]
struct Player;

#[derive(Resource)]
struct MaterialPresets {
    default: Handle<StandardMaterial>,
    highlight: Handle<StandardMaterial>,
}

impl FromWorld for MaterialPresets {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        Self {
            default: materials.add(Color::from(Srgba::new(0.9, 0.9, 0.9, 0.1))),
            highlight: materials.add(Color::from(Srgba::new(1.0, 0.0, 0.0, 1.0))),
        }
    }
}

fn move_player(
    time: Res<Time>,
    mut player: Query<(&mut Transform, &mut GridCell<i32>, &Parent), With<Player>>,
    mut meshes: Query<&mut Handle<StandardMaterial>, Without<Player>>,
    mut neighbors: Local<Vec<Entity>>,
    reference_frame: Query<&ReferenceFrame<i32>>,
    spatial_hash_map: Res<SpatialHashMap<i32>>,
    material_presets: Res<MaterialPresets>,
) {
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = meshes.get_mut(*neighbor) {
            *material = material_presets.default.clone();
        };
    }
    let t = time.elapsed_seconds() * 0.03;
    if let Ok((mut transform, mut cell, parent)) = player.get_single_mut() {
        let absolute_pos =
            HALF_WIDTH * Vec3::new((5.0 * t).sin(), (7.0 * t).cos(), (3.0 * t).sin());
        (*cell, transform.translation) = reference_frame
            .get(parent.get())
            .unwrap()
            .imprecise_translation_to_grid(absolute_pos);
        *neighbors = spatial_hash_map
            .neighbors_flat(1, parent, *cell)
            .map(|(_, _, set)| set)
            .collect();
    }
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = meshes.get_mut(*neighbor) {
            *material = material_presets.highlight.clone();
        };
    }
}

fn spawn(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    material_presets: Res<MaterialPresets>,
) {
    use turborand::prelude::*;
    let rng = Rng::with_seed(342525);
    let values: Vec<_> = std::iter::repeat_with(|| {
        Vec3::new(
            rng.f32_normalized() * HALF_WIDTH,
            rng.f32_normalized() * HALF_WIDTH,
            rng.f32_normalized() * HALF_WIDTH,
        )
    })
    .take(N_ENTITIES)
    .collect();

    let sphere = meshes.add(Sphere::new(0.1));

    commands.spawn_big_space(ReferenceFrame::<i32>::new(4.0, 0.0), |root| {
        root.spawn_spatial(DirectionalLightBundle::default());
        root.spawn_spatial((
            Camera3dBundle::default(),
            CameraController::default(),
            FloatingOrigin,
            GridCell::new(0, 0, HALF_WIDTH as i32 * 2),
        ));
        root.with_children(|root_builder| {
            for (i, value) in values.iter().enumerate() {
                let mut child_commands = root_builder.spawn((
                    BigSpatialBundle::<i32> {
                        transform: Transform::from_xyz(value.x, value.y, value.z),
                        ..default()
                    },
                    material_presets.default.clone(),
                    sphere.clone(),
                ));
                if i == 0 {
                    child_commands
                        .insert(Player)
                        .insert(materials.add(Color::from(Srgba::new(0.0, 0.0, 1.0, 1.0))));
                }
            }
        });
    });
}
