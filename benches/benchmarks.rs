#![allow(clippy::type_complexity)]

use bevy::prelude::*;
use big_space::{
    spatial_hash::{SpatialHashMap, SpatialHashPlugin},
    *,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::iter::repeat_with;
use turborand::prelude::*;

criterion_group!(benches, spatial_hashing,);
criterion_main!(benches);

#[allow(clippy::unit_arg)]
fn spatial_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_hashing");

    const WIDTH: i32 = 50;
    const N_SPAWN: usize = 10_000;
    const N_MOVE: usize = 1_000;

    let setup = |mut commands: Commands| {
        commands.spawn_big_space(ReferenceFrame::<i32>::new(1.0, 0.0), |root| {
            let rng = Rng::new();
            let values: Vec<_> = repeat_with(|| {
                IVec3::new(
                    rng.i32(-WIDTH..=WIDTH),
                    rng.i32(-WIDTH..=WIDTH),
                    rng.i32(-WIDTH..=WIDTH),
                )
            })
            .take(N_SPAWN)
            .collect();

            for pos in values {
                root.spawn_spatial(GridCell::new(pos.x, pos.y, pos.z));
            }
        });
    };

    let translate = |mut cells: Query<&mut GridCell<i32>>| {
        let rng = Rng::new();
        let values: Vec<_> = repeat_with(|| {
            IVec3::new(
                rng.i32(-WIDTH..=WIDTH),
                rng.i32(-WIDTH..=WIDTH),
                rng.i32(-WIDTH..=WIDTH),
            )
        })
        .take(N_MOVE)
        .collect();
        cells
            .iter_mut()
            .enumerate()
            .take(N_MOVE)
            .for_each(|(i, mut cell)| {
                *cell = GridCell::ZERO + values[i];
            })
    };

    let mut app = App::new();
    app.add_plugins(SpatialHashPlugin::<i32>::default())
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

    let map = app.world().resource::<SpatialHashMap<i32>>();
    let first = map.iter().next().unwrap();
    group.bench_function("SpatialHashMap::get", |b| {
        b.iter(|| {
            black_box(map.get(first.0));
        });
    });

    let ent = *first.1.iter().next().unwrap();
    group.bench_function("Find entity", |b| {
        b.iter(|| {
            black_box(map.get(first.0).map(|set| set.get(&ent)));
        });
    });

    app.update();

    let parent = app
        .world_mut()
        .query::<&Parent>()
        .get(app.world(), ent)
        .unwrap();
    let map = app.world().resource::<SpatialHashMap<i32>>();

    group.bench_function("Neighbors radius 1", |b| {
        b.iter(|| {
            black_box(map.neighbors(1, parent, GridCell::new(0, 0, 0)).count());
        });
    });

    group.bench_function("Neighbors radius 4", |b| {
        b.iter(|| {
            black_box(map.neighbors(4, parent, GridCell::new(0, 0, 0)).count());
        });
    });

    group.bench_function("Neighbors radius all", |b| {
        b.iter(|| {
            black_box(
                map.neighbors(WIDTH as u8, parent, GridCell::new(0, 0, 0))
                    .count(),
            );
        });
    });

    group.bench_function("Neighbors flood 1", |b| {
        b.iter(|| {
            black_box(
                map.neighbors_flood(1, parent, GridCell::ZERO)
                    .iter()
                    .count(),
            );
        });
    });
}
