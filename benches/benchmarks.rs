#![allow(clippy::type_complexity)]

use bevy::prelude::*;
use big_space::{
    spatial_hash::{SpatialHashMap, SpatialHashPlugin},
    *,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::Rng;

criterion_group!(benches, spatial_hashing,);
criterion_main!(benches);

#[allow(clippy::unit_arg)]
fn spatial_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial_hashing");

    const WIDTH: i32 = 5;
    const N_SPAWN: usize = 100_000;
    const N_MOVE: usize = 10_000;

    let setup = |mut commands: Commands| {
        commands.spawn_big_space(ReferenceFrame::<i32>::new(1.0, 0.0), |root| {
            let mut rng = rand::thread_rng();
            let mut new_coord = || rng.gen_range(-WIDTH..=WIDTH);
            for _i in 0..N_SPAWN {
                root.spawn_spatial(GridCell::new(new_coord(), new_coord(), new_coord()));
            }
        });
    };

    let translate = |mut cells: Query<&mut GridCell<i32>>| {
        let new_coord = || rand::thread_rng().gen_range(-WIDTH..=WIDTH);
        cells
            .iter_mut()
            .enumerate()
            .take(N_MOVE)
            .for_each(|(_, mut cell)| {
                *cell = GridCell::new(new_coord(), new_coord(), new_coord());
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

    let map = app.world.resource::<SpatialHashMap<i32>>();
    let first = map.iter().next().unwrap();
    group.bench_function("SpatialHashMap::get", |b| {
        b.iter(|| {
            black_box(map.get(first.0));
        });
    });

    let ent = first.1.iter().next().unwrap();
    group.bench_function("Find entity", |b| {
        b.iter(|| {
            black_box(map.get(first.0).map(|set| set.get(ent)));
        });
    });
}
