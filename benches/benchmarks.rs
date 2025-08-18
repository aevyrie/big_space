//! `big_space` benchmarks.
#![allow(clippy::type_complexity)]
#![allow(missing_docs)]

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
