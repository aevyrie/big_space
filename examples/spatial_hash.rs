use std::{collections::VecDeque, iter::Sum, ops::Div};

use bevy::prelude::*;
use bevy_math::DVec3;
use bevy_utils::Instant;
use big_space::{
    camera::{CameraController, CameraControllerPlugin},
    spatial_hash::{SpatialHashMap, SpatialHashPlugin, SpatialHashStats},
    *,
};
use noise::{NoiseFn, Perlin};
use reference_frame::PropagationStats;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::<i32>::default(),
            SpatialHashPlugin::<i32>::default(),
            CameraControllerPlugin::<i32>::default(),
        ))
        .add_systems(Startup, (spawn, setup_ui))
        .add_systems(Update, (avg_stats, move_player).chain())
        .init_resource::<MaterialPresets>()
        .init_resource::<Avg<PropagationStats>>()
        .init_resource::<Avg<SpatialHashStats>>()
        .insert_resource(ClearColor(Color::BLACK))
        .run();
}

const HALF_WIDTH: f32 = 100.0;
const N_ENTITIES: usize = 100_000;

#[derive(Component)]
struct Player;

#[derive(Resource)]
struct MaterialPresets {
    default: Handle<StandardMaterial>,
    highlight: Handle<StandardMaterial>,
    flood: Handle<StandardMaterial>,
}

impl FromWorld for MaterialPresets {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        let mut d: StandardMaterial = Color::from(Srgba::new(0.9, 0.9, 0.9, 1.0)).into();
        d.unlit = true;
        let mut h: StandardMaterial = Color::from(Srgba::new(1.0, 0.0, 0.0, 1.0)).into();
        h.unlit = true;
        let mut f: StandardMaterial = Color::from(Srgba::new(0.0, 1.0, 0.0, 1.0)).into();
        f.unlit = true;

        Self {
            default: materials.add(d),
            highlight: materials.add(h),
            flood: materials.add(f),
        }
    }
}
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn move_player(
    time: Res<Time>,
    mut player: Query<(&mut Transform, &mut GridCell<i32>, &Parent), With<Player>>,
    mut non_player: Query<
        (&mut Transform, &mut GridCell<i32>, &Parent),
        (Without<Player>, With<Handle<Mesh>>),
    >,
    mut materials: Query<&mut Handle<StandardMaterial>, Without<Player>>,
    mut neighbors: Local<Vec<Entity>>,
    reference_frame: Query<&ReferenceFrame<i32>>,
    spatial_hash_map: Res<SpatialHashMap<i32>>,
    material_presets: Res<MaterialPresets>,
    mut text: Query<(&mut Text, &mut StatsText)>,
    hash_stats: Res<Avg<SpatialHashStats>>,
    prop_stats: Res<Avg<PropagationStats>>,
) {
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = materials.get_mut(*neighbor) {
            *material = material_presets.default.clone_weak();
        };
    }

    let t = time.elapsed_seconds() * 3.0;
    let scale = 1e5 / (N_ENTITIES as f32 * HALF_WIDTH);
    for (mut transform, _, _) in non_player.iter_mut() {
        transform.translation.x += t.sin() * scale;
        transform.translation.y += t.cos() * scale;
        transform.translation.z += (t * 2.3).sin() * scale;
    }

    let t = time.elapsed_seconds() * 0.1;
    let (mut transform, mut cell, parent) = player.single_mut();
    let absolute_pos = HALF_WIDTH * Vec3::new((5.0 * t).sin(), (7.0 * t).cos(), (20.0 * t).sin());
    (*cell, transform.translation) = reference_frame
        .get(parent.get())
        .unwrap()
        .imprecise_translation_to_grid(absolute_pos);

    neighbors.clear();

    spatial_hash_map
        .neighbors_contiguous(1, parent, *cell)
        .for_each(|(_hash, entry)| {
            for entity in &entry.entities {
                neighbors.push(*entity);
                if let Ok(mut material) = materials.get_mut(*entity) {
                    *material = material_presets.flood.clone_weak();
                };
            }
        });

    spatial_hash_map
        .neighbors_flat(1, parent, *cell)
        .for_each(|(_, _, entity)| {
            neighbors.push(entity);
            if let Ok(mut material) = materials.get_mut(entity) {
                *material = material_presets.highlight.clone_weak();
            };
        });

    // Time this separately, otherwise we just ending up timing how long allocations and pushing
    // to a vec take. Here, we just want to measure how long it takes to library to fulfill the
    // query, so we do as little extra computation as possible.
    //
    // The neighbor query is lazy, which means it only does work when we consume the iterator.
    let lookup_start = Instant::now();
    let total = spatial_hash_map
        .neighbors_contiguous(1, parent, *cell)
        .map(|(.., entry)| entry.entities.len())
        .sum::<usize>();
    let elapsed = lookup_start.elapsed().as_secs_f32();

    let (mut text, mut stats) = text.single_mut();
    stats.0.truncate(0);
    stats.0.push_front(elapsed);
    let avg = stats.0.iter().sum::<f32>() / stats.0.len() as f32;
    text.sections[0].value = format!(
        "\
Neighbor Lookup: {: >5.2}us
Neighboring Entities: {}

Spatial Hashing Update Cost:
Update Hashes: {: >8.2?}
Update Maps: {: >10.2?}

Local Origin Propagation: {: >10.2?}
Low Precision Propagation: {: >9.2?}
High Precision Propagation: {: >8.2?}",
        avg * 1e6,
        total,
        hash_stats.avg.hash_update_duration(),
        hash_stats.avg.map_update_duration(),
        prop_stats.avg.local_origin_propagation(),
        prop_stats.avg.low_precision_propagation(),
        prop_stats.avg.high_precision_propagation(),
    );
}

#[derive(Resource)]
struct Avg<T>
where
    for<'a> T: FromWorld + Sum<&'a T> + Div<u32, Output = T>,
{
    queue: VecDeque<T>,
    avg: T,
}

impl<T> FromWorld for Avg<T>
where
    for<'a> T: FromWorld + Sum<&'a T> + Div<u32, Output = T>,
{
    fn from_world(world: &mut World) -> Self {
        Avg {
            queue: VecDeque::new(),
            avg: T::from_world(world),
        }
    }
}

impl<T> Avg<T>
where
    for<'a> T: FromWorld + Sum<&'a T> + Div<u32, Output = T>,
{
    fn push(&mut self, value: T) -> &mut Self {
        self.queue.truncate(63);
        self.queue.push_front(value);
        self
    }

    fn compute_avg(&mut self) -> &mut Self {
        self.avg = self.queue.iter().sum::<T>() / self.queue.len() as u32;
        self
    }
}

fn avg_stats(
    hash_stats: Res<SpatialHashStats>,
    mut avg_hash_stats: ResMut<Avg<SpatialHashStats>>,
    prop_stats: Res<PropagationStats>,
    mut avg_prop_stats: ResMut<Avg<PropagationStats>>,
) {
    avg_hash_stats.push(hash_stats.clone()).compute_avg();
    avg_prop_stats.push(prop_stats.clone()).compute_avg();
}

fn spawn(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    material_presets: Res<MaterialPresets>,
) {
    use turborand::prelude::*;
    let rng = Rng::with_seed(342525);
    let noise = Perlin::new(345612);

    let rng = || loop {
        let noise_scale = 5.0;
        let threshold = 0.5;
        let rng_val = || rng.f64_normalized() * noise_scale;
        let coord = [rng_val(), rng_val(), rng_val()];
        if noise.get(coord) > threshold {
            return DVec3::from_array(coord).as_vec3() * HALF_WIDTH / noise_scale as f32;
        }
    };

    let values: Vec<_> = std::iter::repeat_with(rng).take(N_ENTITIES).collect();

    let sphere = meshes.add(Sphere::new(HALF_WIDTH / 600.0));

    commands.spawn_big_space(ReferenceFrame::<i32>::new(4.0, 0.0), |root| {
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
                    material_presets.default.clone_weak(),
                    sphere.clone(),
                ));
                // child_commands.remove::<Visibility>();
                // child_commands.remove::<InheritedVisibility>();
                // child_commands.remove::<ViewVisibility>();
                if i == 0 {
                    let mut matl: StandardMaterial =
                        Color::from(Srgba::new(1.0, 1.0, 0.0, 1.0)).into();
                    matl.unlit = true;
                    child_commands
                        .insert(meshes.add(Sphere::new(1.0)))
                        .insert(Player)
                        .insert(materials.add(matl));
                }
            }
        });
    });
}

#[derive(Component)]
struct StatsText(VecDeque<f32>);

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((NodeBundle {
            style: Style {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                padding: UiRect::all(Val::Px(24.)),
                ..default()
            },
            ..default()
        },))
        .with_children(|parent| {
            parent.spawn((
                TextBundle::from_section(
                    "",
                    TextStyle {
                        font: asset_server.load("fonts/FiraMono-Regular.ttf"),
                        font_size: 18.0,
                        ..default()
                    },
                ),
                StatsText(Default::default()),
            ));
        });
}
