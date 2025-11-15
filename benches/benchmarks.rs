//! `big_space` benchmarks.
#![allow(clippy::type_complexity)]
#![allow(missing_docs)]
#![allow(clippy::unit_arg)]

use bevy::prelude::*;
use big_space::plugin::BigSpaceMinimalPlugins;
use big_space::prelude::*;
use core::{hint::black_box, iter::repeat_with, ops::Neg};
use criterion::{criterion_group, criterion_main, Criterion};
use turborand::prelude::*;

criterion_group!(
    benches,
    global_transform,
    spatial_hashing,
    hash_filtering,
    deep_hierarchy,
    wide_hierarchy,
    vs_bevy,
    partition_change_tracking,
);
criterion_main!(benches);

#[allow(clippy::unit_arg)]
fn global_transform(c: &mut Criterion) {
    let mut group = c.benchmark_group("propagation");
    group.bench_function("global_transform", |b| {
        let grid = Grid::default();
        let local_cell = CellCoord { x: 1, y: 1, z: 1 };
        let local_transform = Transform::from_xyz(9.0, 200.0, 500.0);
        b.iter(|| {
            black_box(grid.global_transform(&local_cell, &local_transform));
        });
    });
}

#[allow(clippy::unit_arg)]
fn deep_hierarchy(c: &mut Criterion) {
    /// Total number of entities to spawn
    const N_SPAWN: usize = 100;

    let mut group = c.benchmark_group(format!("deep_hierarchy {N_SPAWN}"));

    fn setup(mut commands: Commands) {
        commands.spawn_big_space(Grid::new(10000.0, 0.0), |root| {
            let mut parent = root.spawn_grid_default(()).id();
            for _ in 0..N_SPAWN {
                let child = root.commands().spawn(BigGridBundle::default()).id();
                root.commands().entity(parent).add_child(child);
                parent = child;
            }
            root.spawn_spatial(FloatingOrigin);
        });
    }

    fn translate(mut transforms: Query<&mut Transform>) {
        transforms.iter_mut().for_each(|mut transform| {
            transform.translation += Vec3::ONE;
        });
    }

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
    ))
    .add_systems(Startup, setup)
    .add_systems(Update, translate)
    .update();

    group.bench_function("Baseline", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });
}

#[allow(clippy::unit_arg)]
fn wide_hierarchy(c: &mut Criterion) {
    /// Total number of entities to spawn
    const N_SPAWN: usize = 100_000;

    let mut group = c.benchmark_group(format!("wide_hierarchy {N_SPAWN}"));

    fn setup(mut commands: Commands) {
        commands.spawn_big_space(Grid::new(10000.0, 0.0), |root| {
            for _ in 0..N_SPAWN {
                root.spawn_spatial(());
            }
            root.spawn_spatial(FloatingOrigin);
        });
    }

    fn translate(mut transforms: Query<&mut Transform>) {
        transforms.iter_mut().for_each(|mut transform| {
            transform.translation += Vec3::ONE;
        });
    }

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
    ))
    .add_systems(Startup, setup)
    .add_systems(Update, translate)
    .update();

    group.bench_function("Baseline", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });
}

#[allow(clippy::unit_arg)]
fn spatial_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_hashing");

    const HALF_WIDTH: i64 = 100;
    /// Total number of entities to spawn
    const N_SPAWN: usize = 10_000;
    /// Number of entities that move into a different cell each update
    const N_MOVE: usize = 1_000;

    fn setup(mut commands: Commands) {
        commands.spawn_big_space(Grid::new(1.0, 0.0), |root| {
            let rng = Rng::with_seed(342525);
            let values: Vec<_> = repeat_with(|| {
                [
                    rng.i64(-HALF_WIDTH..=HALF_WIDTH) as GridPrecision,
                    rng.i64(-HALF_WIDTH..=HALF_WIDTH) as GridPrecision,
                    rng.i64(-HALF_WIDTH..=HALF_WIDTH) as GridPrecision,
                ]
            })
            .take(N_SPAWN)
            .collect();

            for pos in values {
                root.spawn_spatial(CellCoord::new(pos[0], pos[1], pos[2]));
            }
        });
    }

    fn translate(mut cells: Query<&mut CellCoord>) {
        cells.iter_mut().take(N_MOVE).for_each(|mut cell| {
            *cell += CellCoord::ONE;
        });
    }

    let mut app = App::new();
    app.add_plugins(CellHashingPlugin::default())
        .add_systems(Startup, setup)
        .update();

    group.bench_function("Baseline", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    app.add_systems(Update, translate).update();
    group.bench_function("Translation and rehashing", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    let map = app.world().resource::<CellLookup>();
    let first = map
        .all_entries()
        .find(|(_, entry)| !entry.entities.is_empty())
        .unwrap();
    group.bench_function("GridHashMap::get", |b| {
        b.iter(|| {
            black_box(map.get(first.0).unwrap());
        });
    });

    let ent = *first.1.entities.iter().next().unwrap();
    group.bench_function("Find entity", |b| {
        b.iter(|| {
            black_box(
                map.get(first.0)
                    .map(|entry| entry.entities.iter().find(|e| *e == &ent)),
            );
        });
    });

    // let parent = app .world_mut() .query::<&GridHash>() .get(app.world(), ent)
    //     .unwrap(); let map = app.world().resource::<GridHashMap>(); let entry =
    //     map.get(parent).unwrap();

    // group.bench_function("Neighbors radius: 4", |b| {
    //     b.iter(|| {
    //         black_box(map.neighbors(entry).count());
    //     });
    // });

    // group.bench_function(format!("Neighbors radius: {}", HALF_WIDTH), |b| {
    //     b.iter(|| {
    //         black_box(
    //             map.neighbors(entry)x
    //                 .count(),
    //         );
    //     });
    // });

    fn setup_uniform<const HALF_EXTENT: GridPrecision>(mut commands: Commands) {
        commands.spawn_big_space(Grid::new(1.0, 0.0), |root| {
            for x in HALF_EXTENT.neg()..HALF_EXTENT {
                for y in HALF_EXTENT.neg()..HALF_EXTENT {
                    for z in HALF_EXTENT.neg()..HALF_EXTENT {
                        root.spawn_spatial(CellCoord::new(x, y, z));
                    }
                }
            }
        });
    }

    // Uniform Grid Population 1_000

    let mut app = App::new();
    app.add_plugins(CellHashingPlugin::default())
        .add_systems(Startup, setup_uniform::<5>)
        .update();

    let parent = app
        .world_mut()
        .query_filtered::<Entity, With<BigSpace>>()
        .single(app.world())
        .unwrap();
    let spatial_map = app.world().resource::<CellLookup>();
    let hash = CellId::__new_manual(parent, &CellCoord { x: 0, y: 0, z: 0 });
    let entry = spatial_map.get(&hash).unwrap();

    assert_eq!(spatial_map.nearby(entry).count(), 27);
    group.bench_function("nearby 1 population 1_000", |b| {
        b.iter(|| {
            black_box(spatial_map.nearby(entry).count());
        });
    });

    assert_eq!(spatial_map.flood(&hash, None).count(), 1_000);
    let flood = || spatial_map.flood(&hash, None).count();
    group.bench_function("nearby flood population 1_000", |b| {
        b.iter(|| black_box(flood()));
    });

    // Uniform Grid Population 1_000_000

    let mut app = App::new();
    app.add_plugins(CellHashingPlugin::default())
        .add_systems(Startup, setup_uniform::<50>)
        .update();

    let parent = app
        .world_mut()
        .query_filtered::<Entity, With<BigSpace>>()
        .single(app.world())
        .unwrap();
    let spatial_map = app.world().resource::<CellLookup>();
    let hash = CellId::__new_manual(parent, &CellCoord { x: 0, y: 0, z: 0 });
    let entry = spatial_map.get(&hash).unwrap();

    assert_eq!(spatial_map.nearby(entry).count(), 27);
    group.bench_function("nearby 1 population 1_000_000", |b| {
        b.iter(|| {
            black_box(spatial_map.nearby(entry).count());
        });
    });

    assert_eq!(spatial_map.flood(&hash, None).count(), 1_000_000);
    group.bench_function("nearby flood population 1_000_000", |b| {
        b.iter(|| black_box(spatial_map.flood(&hash, None).count()));
    });
}

#[allow(clippy::unit_arg)]
fn hash_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_filtering");

    const N_ENTITIES: usize = 100_000;
    const N_PLAYERS: usize = 100;
    const N_MOVE: usize = 1_000;
    const HALF_WIDTH: i64 = 100;

    #[derive(Component)]
    struct Player;

    fn setup(mut commands: Commands) {
        let rng = Rng::with_seed(342525);
        let values: Vec<_> = repeat_with(|| {
            [
                rng.i64(-HALF_WIDTH..=HALF_WIDTH) as GridPrecision,
                rng.i64(-HALF_WIDTH..=HALF_WIDTH) as GridPrecision,
                rng.i64(-HALF_WIDTH..=HALF_WIDTH) as GridPrecision,
            ]
        })
        .take(N_ENTITIES)
        .collect();

        commands.spawn_big_space_default(|root| {
            for (i, pos) in values.iter().enumerate() {
                let mut cmd = root.spawn_spatial(CellCoord::new(pos[0], pos[1], pos[2]));
                if i < N_PLAYERS {
                    cmd.insert(Player);
                }
            }
        });
    }

    fn translate(mut cells: Query<&mut CellCoord>) {
        cells.iter_mut().take(N_MOVE).for_each(|mut cell| {
            *cell += IVec3::ONE;
        });
    }

    let mut app = App::new();
    app.add_systems(Startup, setup)
        .add_systems(Update, translate)
        .update();
    app.update();
    app.add_plugins((CellHashingPlugin::default(),));
    group.bench_function("No Filter Plugin", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    let mut app = App::new();
    app.add_systems(Startup, setup)
        .add_systems(Update, translate)
        .update();
    app.update();
    app.add_plugins((CellHashingPlugin::<With<Player>>::new(),));
    group.bench_function("With Player Plugin", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    let mut app = App::new();
    app.add_systems(Startup, setup)
        .add_systems(Update, translate)
        .update();
    app.update();
    app.add_plugins((CellHashingPlugin::<Without<Player>>::new(),));
    group.bench_function("Without Player Plugin", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    let mut app = App::new();
    app.add_systems(Startup, setup)
        .add_systems(Update, translate)
        .update();
    app.update();
    app.add_plugins((CellHashingPlugin::default(),))
        .add_plugins((CellHashingPlugin::<With<Player>>::new(),))
        .add_plugins((CellHashingPlugin::<Without<Player>>::new(),));
    group.bench_function("All Plugins", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });
}

#[allow(clippy::unit_arg)]
fn vs_bevy(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform_prop");

    use bevy::prelude::*;

    const N_ENTITIES: usize = 1_000_000;

    fn setup_bevy(mut commands: Commands) {
        commands
            .spawn((Transform::default(), Visibility::default()))
            .with_children(|builder| {
                for _ in 0..N_ENTITIES {
                    builder.spawn((Transform::default(), Visibility::default()));
                }
            });
    }

    fn setup_big(mut commands: Commands) {
        commands.spawn_big_space_default(|root| {
            for _ in 0..N_ENTITIES {
                root.spawn_spatial(());
            }
            root.spawn_spatial(FloatingOrigin);
        });
    }

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .add_systems(Startup, setup_bevy)
        .update();

    group.bench_function("Bevy Propagation Static", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, BigSpaceMinimalPlugins))
        .add_systems(Startup, setup_big)
        .update();

    group.bench_function("Big Space Propagation Static", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    fn translate(mut transforms: Query<&mut Transform>) {
        transforms.iter_mut().for_each(|mut transform| {
            transform.translation += 1.0;
        });
    }

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .add_systems(Startup, setup_bevy)
        .add_systems(Update, translate)
        .update();

    group.bench_function("Bevy Propagation", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, BigSpaceMinimalPlugins))
        .add_systems(Startup, setup_big)
        .add_systems(Update, translate)
        .update();

    group.bench_function("Big Space Propagation", |b| {
        b.iter(|| {
            black_box(app.update());
        });
    });
}

fn partition_change_tracking(c: &mut Criterion) {
    use partitions::*;

    let mut group = c.benchmark_group("partition_change_tracking");

    // Ensure the benchmarked app has a floating origin in the same grid as the scenario
    fn spawn_bench_floating_origin(mut commands: Commands, grids: Query<Entity, With<Grid>>) {
        if let Ok(grid_entity) = grids.single() {
            // Attach a floating origin entity under the scenario's grid
            commands.entity(grid_entity).with_children(|b| {
                b.spawn((FloatingOrigin, Transform::default(), CellCoord::default()));
            });
        }
    }

    fn build_app(config: ScenarioConfig) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, BigSpaceMinimalPlugins))
            .add_plugins((CellHashingPlugin::default(), PartitionPlugin::default()));
        add_partition_perf(&mut app, config);
        // Add floating origin for the benchmark's big space after setup has created the grid
        app.add_systems(PostStartup, spawn_bench_floating_origin);
        // Warm up the world and apply startup
        app.update();
        app
    }

    // Axis 1: scaling with static entities (no movement)
    for &n in &[100usize, 10_000, 100_000] {
        let mut app = build_app(ScenarioConfig {
            n_entities: n,
            percent_moving: 0.0,
            density: Density::Dense,
        });
        group.bench_function(format!("static_n={}", n), |b| {
            b.iter(|| black_box(app.update()));
        });
    }

    // Axis 2: 10k entities, varying percent moving
    for &(label, pct) in &[("25%", 0.25f32), ("50%", 0.5), ("100%", 1.0)] {
        let mut app = build_app(ScenarioConfig {
            n_entities: 10_000,
            percent_moving: pct,
            density: Density::Dense,
        });
        group.bench_function(format!("n=10000_moving={}", label), |b| {
            b.iter(|| black_box(app.update()));
        });
    }

    // Axis 3: 10k, 25% moving, Sparse vs Dense
    for &(label, density) in &[("sparse", Density::Sparse), ("dense", Density::Dense)] {
        let mut app = build_app(ScenarioConfig {
            n_entities: 10_000,
            percent_moving: 0.25,
            density,
        });
        group.bench_function(format!("n=10000_25pct_{}", label), |b| {
            b.iter(|| black_box(app.update()));
        });
    }
}

pub mod partitions {
    pub use super::*;

    /// Density of initial entity arrangement.
    #[derive(Clone, Copy, Debug)]
    pub enum Density {
        /// Entities are placed with at least one empty cell between any two occupied cells,
        /// which keeps all entities in independent partitions initially.
        Sparse,
        /// Entities are placed contiguously so that many share the same partition (often one big one).
        Dense,
    }

    /// Configuration for a partition perf scenario.
    #[derive(Resource, Clone, Copy, Debug)]
    pub struct ScenarioConfig {
        /// Total number of entities to spawn.
        pub n_entities: usize,
        /// Fraction of entities that move each frame in [0, 1].
        pub percent_moving: f32,
        /// Initial arrangement of entities.
        pub density: Density,
    }

    impl Default for ScenarioConfig {
        fn default() -> Self {
            Self {
                n_entities: 10_000,
                percent_moving: 0.25,
                density: Density::Dense,
            }
        }
    }

    /// Marker for movers.
    #[derive(Component)]
    pub struct Mover;

    /// Resource that stores the root grid entity for the active scenario.
    #[derive(Resource, Clone, Copy, Debug)]
    pub struct ScenarioRoot(pub Entity);

    /// Adds systems to the app that set up and then update a partition perf scenario.
    ///
    /// You can use this from an example or a benchmark.
    pub fn add_partition_perf(app: &mut App, config: ScenarioConfig) {
        app.insert_resource(config)
            .add_systems(Startup, setup_scenario)
            .add_systems(Update, move_movers);
    }

    fn setup_scenario(mut commands: Commands, config: Res<ScenarioConfig>) {
        // Grid with default settings is fine; large cell length to keep visuals consistent in example.
        commands.spawn_big_space(Grid::new(10_000.0, 0.0), |root| {
            // Spawn many entities as children of the root grid for better spawn throughput.
            let grid_entity = root.id();
            // Expose the scenario's root grid entity so examples can attach cameras to the same space.
            root.commands().insert_resource(ScenarioRoot(grid_entity));
            let n_movers =
                ((config.percent_moving.clamp(0.0, 1.0)) * config.n_entities as f32) as usize;

            match config.density {
                Density::Sparse => {
                    // Distribute sparsely in 3D with a gap of 1 cell between occupied cells
                    // along each axis to avoid initial merges (independent partitions).
                    let n = config.n_entities as i64;
                    let edge = (f64::cbrt(n as f64).ceil() as i64).max(1);
                    let mut i = 0usize;
                    'outer: for z in 0..edge {
                        for y in 0..edge {
                            for x in 0..edge {
                                if i >= config.n_entities {
                                    break 'outer;
                                }
                                // Multiply by 2 to leave one empty cell between any two occupied cells
                                let cell = CellCoord::new(
                                    (x * 2) as GridPrecision,
                                    (y * 2) as GridPrecision,
                                    (z * 2) as GridPrecision,
                                );
                                let mut ec = root.spawn_spatial(());
                                ec.insert(cell);
                                if i < n_movers {
                                    ec.insert(Mover);
                                }
                                i += 1;
                            }
                        }
                    }
                }
                Density::Dense => {
                    let n = config.n_entities as i64;
                    let edge = (f64::cbrt(n as f64).ceil() as i64).max(1);
                    let mut i = 0usize;
                    'outer: for z in 0..edge {
                        for y in 0..edge {
                            for x in 0..edge {
                                if i >= config.n_entities {
                                    break 'outer;
                                }
                                let cell = CellCoord::new(
                                    x as GridPrecision,
                                    y as GridPrecision,
                                    z as GridPrecision,
                                );
                                // Spawn as a spatial child of the grid and only set CellCoord
                                let mut ec = root.spawn_spatial(());
                                ec.insert(cell);
                                if i < n_movers {
                                    ec.insert(Mover);
                                }
                                i += 1;
                            }
                        }
                    }
                }
            }
        });
    }

    fn move_movers(mut q: Query<&mut CellCoord, With<Mover>>, mut flip: Local<bool>) {
        *flip = !*flip;
        let dx = if *flip { 1 } else { -1 };
        for mut cell in q.iter_mut() {
            cell.x += dx as GridPrecision;
        }
    }
}
