use std::{collections::VecDeque, time::Duration};

use bevy::prelude::*;
use bevy_math::DVec3;
use bevy_utils::Instant;
use big_space::{
    camera::{CameraController, CameraControllerPlugin},
    spatial_hash::{SpatialHashMap, SpatialHashPlugin},
    timing::PropagationStats,
    BigSpaceCommands, BigSpacePlugin, BigSpatialBundle, FloatingOrigin, GridCell, ReferenceFrame,
    SmoothedStat, SpatialHashStats,
};
use noise::{NoiseFn, Perlin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::<i32>::default(),
            SpatialHashPlugin::<i32>::default(),
            CameraControllerPlugin::<i32>::default(),
        ))
        .add_systems(Startup, (spawn, setup_ui))
        .add_systems(Update, move_player)
        .insert_resource(ClearColor(Color::BLACK))
        .init_resource::<MaterialPresets>()
        .run();
}

const HALF_WIDTH: f32 = 100.0;
const N_ENTITIES: usize = 50_000;
// How fast the entities should move, causing them to move into neighboring cells.
const MOVEMENT_SPEED: f32 = 4e5;
const PERCENT_STATIC: f32 = 0.9;

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
    hash_stats: Res<SmoothedStat<SpatialHashStats>>,
    prop_stats: Res<SmoothedStat<PropagationStats>>,
) {
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = materials.get_mut(*neighbor) {
            *material = material_presets.default.clone_weak();
        };
    }

    let t = time.elapsed_seconds() * 3.0;
    let scale = MOVEMENT_SPEED / (N_ENTITIES as f32 * HALF_WIDTH);
    if scale.abs() > 0.0 {
        // Avoid change detection
        for (i, (mut transform, _, _)) in non_player.iter_mut().enumerate() {
            if i > (PERCENT_STATIC * N_ENTITIES as f32) as usize {
                transform.translation.x += t.sin() * scale;
                transform.translation.y += t.cos() * scale;
                transform.translation.z += (t * 2.3).sin() * scale;
            }
        }
    }

    let t = time.elapsed_seconds() * 0.01;
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
    let elapsed = lookup_start.elapsed();

    let (mut text, mut stats) = text.single_mut();
    stats.0.truncate(0);
    stats.0.push_front(elapsed);
    let avg = stats
        .0
        .iter()
        .sum::<Duration>()
        .div_f32(stats.0.len() as f32);
    text.sections[0].value = format!(
        "\
Neighbor Flood Fill: {: >8.1?}
Neighbors: {: >9} Entities

Spatial Hashing
Moved Cells: {: >7?} Entities
Compute Hashes: {: >13.1?}
Update Maps: {: >16.1?}

Transform Propagation
Cell Recentering: {: >11.1?}
LP Root: {: >20.1?}
Frame Origin: {: >15.1?}
LP Propagation: {: >13.1?}
HP Propagation: {: >13.1?}
Total: {: >22.1?}",
        avg,
        total,
        //
        hash_stats.avg().moved_cell_entities(),
        hash_stats.avg().hash_update_duration(),
        hash_stats.avg().map_update_duration(),
        //
        prop_stats.avg().grid_recentering(),
        prop_stats.avg().low_precision_root_tagging(),
        prop_stats.avg().local_origin_propagation(),
        prop_stats.avg().low_precision_propagation(),
        prop_stats.avg().high_precision_propagation(),
        prop_stats.avg().total(),
    );
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

    let sphere = meshes.add(
        Sphere::new(HALF_WIDTH / (N_ENTITIES as f32).powf(0.33) * 0.1)
            .mesh()
            .ico(1)
            .unwrap(),
    );

    commands.spawn_big_space(ReferenceFrame::<i32>::new(4.0, 0.0), |root| {
        root.spawn_spatial((
            Camera3dBundle::default(),
            CameraController::default(),
            FloatingOrigin,
            GridCell::new(0, 0, HALF_WIDTH as i32),
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
                    child_commands.insert((
                        meshes.add(Sphere::new(1.0)),
                        Player,
                        materials.add(matl),
                        Transform::from_scale(Vec3::splat(1.0)),
                    ));
                }
            }
        });
    });
}

#[derive(Component)]
struct StatsText(VecDeque<Duration>);

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((NodeBundle {
            style: Style {
                width: Val::Auto,
                height: Val::Auto,
                padding: UiRect::all(Val::Px(16.)),
                margin: UiRect::all(Val::Px(12.)),
                border: UiRect::all(Val::Px(1.)),
                ..default()
            },
            border_radius: BorderRadius::all(Val::Px(8.0)),
            border_color: Color::linear_rgba(0.03, 0.03, 0.03, 0.95).into(),
            background_color: Color::linear_rgba(0.012, 0.012, 0.012, 0.95).into(),
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
