#![cfg(test)]

use crate::hash::CellHashingPlugin;
use crate::partition::map::PartitionLookup;
use crate::partition::PartitionPlugin;
use crate::plugin::BigSpaceMinimalPlugins;
use crate::prelude::*;
use bevy_app::Update;
use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use bevy_ecs::world::World;

fn run_app_once(app: &mut App) {
    app.update();
}

#[test]
fn fresh_spawn_populates_map_without_changes() {
    // Setup app with plugins
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    // Spawn a basic world with two entities in distinct cells
    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            root.spawn_spatial(CellCoord::new(0, 0, 0));
            root.spawn_spatial(CellCoord::new(10, 0, 0));
        });
    };
    app.add_systems(Update, setup);

    run_app_once(&mut app);

    // Verify that the two spawned entities are mapped and recorded as spawned changes
    #[derive(Resource, Clone, Copy)]
    struct Spawned {
        e1: Entity,
        e2: Entity,
    }

    // Add a system to capture the entities we spawned
    let capture = |world: &mut World| {
        let mut q = world.query_filtered::<Entity, With<CellId>>();
        let mut iter = q.iter(world);
        let e1 = iter.next().unwrap();
        let e2 = iter.next().unwrap();
        world.insert_resource(Spawned { e1, e2 });
    };
    // Run capture in-place
    {
        let world_mut = &mut app.world_mut();
        capture(world_mut);
    }
    let spawned = app.world().resource::<Spawned>();
    let entity_partitions = app.world().resource::<PartitionEntities>();
    assert!(entity_partitions.map.get(&spawned.e1).is_some());
    assert!(entity_partitions.map.get(&spawned.e2).is_some());
    // Fresh spawns should be reported as (None, Some(_)) changes
    let (from1, to1) = *entity_partitions
        .changed
        .get(&spawned.e1)
        .expect("spawned entity should be in changed map");
    assert!(from1.is_none());
    assert!(to1.is_some());
    let (from2, to2) = *entity_partitions
        .changed
        .get(&spawned.e2)
        .expect("spawned entity should be in changed map");
    assert!(from2.is_none());
    assert!(to2.is_some());
}

#[test]
fn moving_between_partitions_records_change() {
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    #[derive(Resource, Clone, Copy)]
    struct Entities {
        a: Entity,
        b: Entity,
    }

    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            let a = root.spawn_spatial(CellCoord::new(0, 0, 0)).id();
            // Keep the destination cell occupied by a different entity so its partition exists
            let b = root.spawn_spatial(CellCoord::new(100, 0, 0)).id();
            root.commands().insert_resource(Entities { a, b });
        });
    };
    app.add_systems(Update, setup);

    run_app_once(&mut app); // establish initial state

    // Capture initial partition ids
    let entities = *app.world().resource::<Entities>();
    let (a_old_cell, b_cell) = {
        let world = app.world_mut();
        let mut q = world.query::<(&CellId,)>();
        let a_old = *q.get(world, entities.a).unwrap().0;
        let b = *q.get(world, entities.b).unwrap().0;
        (a_old, b)
    };
    let parts = app.world().resource::<PartitionLookup>();
    let pid_a0 = parts.get(&a_old_cell).copied().unwrap();
    let pid_b = parts.get(&b_cell).copied().unwrap();
    assert_ne!(pid_a0, pid_b, "initial partitions must be distinct");

    // Move entity A into B's cell (different partition)
    {
        let mut e = app.world_mut().entity_mut(entities.a);
        e.insert(CellCoord::new(100, 0, 0));
    }

    run_app_once(&mut app);

    // Assert change recorded for A
    let ep = app.world().resource::<PartitionEntities>();
    let (from, to) = ep
        .changed
        .get(&entities.a)
        .copied()
        .expect("A should have a recorded change");
    assert_eq!(from, Some(pid_a0));
    assert_eq!(to, Some(pid_b));
    // Map should also be updated
    assert_eq!(ep.map.get(&entities.a).copied(), Some(pid_b));

    // Next frame, changed should clear
    run_app_once(&mut app);
    let ep = app.world().resource::<PartitionEntities>();
    assert!(ep.changed.get(&entities.a).is_none());
}

#[test]
fn move_within_same_partition_no_change() {
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    #[derive(Resource, Clone, Copy)]
    struct E(Entity);

    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            // Create two adjacent cells to ensure they are in a single partition
            root.spawn_spatial(CellCoord::new(0, 0, 0));
            let e = root.spawn_spatial(CellCoord::new(1, 0, 0)).id();
            root.commands().insert_resource(E(e));
        });
    };
    app.add_systems(Update, setup);

    run_app_once(&mut app);

    // Move entity from (1,0,0) -> (0,0,0), still within the same partition
    let e = app.world().resource::<E>().0;
    app.world_mut()
        .entity_mut(e)
        .insert(CellCoord::new(0, 0, 0));

    run_app_once(&mut app);

    let ep = app.world().resource::<PartitionEntities>();
    assert!(
        ep.changed.get(&e).is_none(),
        "No partition change expected within same partition"
    );
}

#[test]
fn remove_and_readd_triggers_partition_change() {
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    #[derive(Resource, Clone, Copy)]
    struct Ents {
        mover: Entity,
        dest: Entity,
    }

    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            let mover = root.spawn_spatial(CellCoord::new(0, 0, 0)).id();
            let dest = root.spawn_spatial(CellCoord::new(50, 0, 0)).id();
            root.commands().insert_resource(Ents { mover, dest });
        });
    };
    app.add_systems(Update, setup);
    run_app_once(&mut app);

    let parts = app.world().resource::<PartitionLookup>();
    let mover_cell = *app
        .world()
        .get::<CellId>(app.world().resource::<Ents>().mover)
        .unwrap();
    let dest_cell = *app
        .world()
        .get::<CellId>(app.world().resource::<Ents>().dest)
        .unwrap();
    let pid_from = parts.get(&mover_cell).copied().unwrap();
    let pid_to = parts.get(&dest_cell).copied().unwrap();
    assert_ne!(pid_from, pid_to);

    // Remove CellId/CellHash, change CellCoord to destination, then expect a partition change
    let mover = app.world().resource::<Ents>().mover;
    {
        let mut ent = app.world_mut().entity_mut(mover);
        ent.remove::<(CellId, CellHash)>();
        ent.insert(CellCoord::new(50, 0, 0));
    }

    run_app_once(&mut app);

    let ep = app.world().resource::<PartitionEntities>();
    let (from, to) = ep
        .changed
        .get(&mover)
        .copied()
        .expect("change expected after re-add");
    assert_eq!(from, Some(pid_from));
    assert_eq!(to, Some(pid_to));
}

#[test]
fn split_then_merge_back_same_frame_no_false_positive() {
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    #[derive(Resource, Clone, Copy)]
    struct R {
        root: Entity,
        left: Entity,
        mid: Entity,
        right: Entity,
    }

    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            let left = root.spawn_spatial(CellCoord::new(0, 0, 0)).id();
            let mid = root.spawn_spatial(CellCoord::new(1, 0, 0)).id();
            let right = root.spawn_spatial(CellCoord::new(2, 0, 0)).id();
            let root_id = root.id();
            root.commands().insert_resource(R {
                root: root_id,
                left,
                mid,
                right,
            });
        });
    };
    app.add_systems(Startup, setup);
    run_app_once(&mut app);

    // Schedule an Update system to remove mid and add a connector at (1,1,0) in the same frame
    fn do_ops(mut commands: Commands, r: Res<R>) {
        // Add connector in the same grid
        commands
            .grid(r.root, Grid::default())
            .spawn_spatial(CellCoord::new(1, 1, 0));
        // Remove mid to cause split
        commands.entity(r.mid).despawn();
    }
    app.add_systems(Update, do_ops);

    run_app_once(&mut app);

    // Verify no changes recorded for left or right (still in original partition id from before)
    let ep = app.world().resource::<PartitionEntities>();
    assert!(ep.changed.get(&app.world().resource::<R>().left).is_none());
    assert!(ep.changed.get(&app.world().resource::<R>().right).is_none());
}

#[test]
fn partition_split_records_changes_for_stationary_entities() {
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    #[derive(Resource, Clone, Copy)]
    struct R {
        left: Entity,
        mid: Entity,
        right: Entity,
    }

    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            let left = root.spawn_spatial(CellCoord::new(0, 0, 0)).id();
            let mid = root.spawn_spatial(CellCoord::new(1, 0, 0)).id();
            let right = root.spawn_spatial(CellCoord::new(2, 0, 0)).id();
            root.commands().insert_resource(R { left, mid, right });
        });
    };
    app.add_systems(Startup, setup);

    for _ in 0..10 {
        run_app_once(&mut app);
    }

    let (left, right, mid) = {
        let r = app.world().resource::<R>();
        (r.left, r.right, r.mid)
    };

    let (pid_left, pid_mid, pid_right) = {
        let parts = app.world().resource::<PartitionLookup>();
        let cell_left = *app.world().get::<CellId>(left).unwrap();
        let cell_mid = *app.world().get::<CellId>(mid).unwrap();
        let cell_right = *app.world().get::<CellId>(right).unwrap();
        (
            parts.get(&cell_left).copied().unwrap(),
            parts.get(&cell_mid).copied().unwrap(),
            parts.get(&cell_right).copied().unwrap(),
        )
    };
    assert_eq!(pid_left, pid_mid);
    assert_eq!(pid_mid, pid_right);

    // Remove the middle cell to cause a split
    app.world_mut().commands().entity(mid).despawn();

    let mut left_changed = false;
    let mut right_changed = false;
    let mut left_pid_entities = None;
    let mut right_pid_entities = None;

    for _ in 0..100 {
        run_app_once(&mut app);
        let ep = app.world().resource::<PartitionEntities>();
        if ep.changed.contains_key(&left) {
            left_changed = true;
        }
        if ep.changed.contains_key(&right) {
            right_changed = true;
        }
        left_pid_entities = ep.map.get(&left).copied();
        right_pid_entities = ep.map.get(&right).copied();
    }

    let parts = app.world().resource::<PartitionLookup>();
    let cell_left = *app.world().get::<CellId>(left).unwrap();
    let cell_right = *app.world().get::<CellId>(right).unwrap();
    let left_pid_actual = parts.get(&cell_left).copied().unwrap();
    let right_pid_actual = parts.get(&cell_right).copied().unwrap();

    assert_ne!(
        left_pid_actual, right_pid_actual,
        "Partitions should have split in PartitionLookup"
    );
    assert_ne!(
        left_pid_entities, right_pid_entities,
        "Partitions should have split in PartitionEntities"
    );

    assert!(
        left_changed || right_changed,
        "At least one entity should have recorded a partition change"
    );
}

#[test]
fn partition_merge_records_changes_for_stationary_entities() {
    let mut app = App::new();
    app.add_plugins((
        BigSpaceMinimalPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ));

    #[derive(Resource, Clone, Copy)]
    struct R {
        root: Entity,
        left: Entity,
        right: Entity,
    }

    let setup = |mut commands: Commands| {
        commands.spawn_big_space_default(|root| {
            let left = root.spawn_spatial(CellCoord::new(0, 0, 0)).id();
            let right = root.spawn_spatial(CellCoord::new(2, 0, 0)).id();
            let root_id = root.id();
            root.commands().insert_resource(R {
                root: root_id,
                left,
                right,
            });
        });
    };
    app.add_systems(Startup, setup);

    for _ in 0..10 {
        run_app_once(&mut app);
    }

    let (left, right) = {
        let r = app.world().resource::<R>();
        (r.left, r.right)
    };

    let ep = app.world().resource::<PartitionEntities>();
    let left_pid_initial = *ep.map.get(&left).expect("left not mapped");
    let right_pid_initial = *ep.map.get(&right).expect("right not mapped");
    assert_ne!(left_pid_initial, right_pid_initial);

    // Add a connector to merge them
    let root_id = app.world().resource::<R>().root;
    app.world_mut()
        .commands()
        .grid(root_id, Grid::default())
        .spawn_spatial(CellCoord::new(1, 0, 0));

    let mut left_changed = false;
    let mut right_changed = false;
    let mut left_pid_entities = None;
    let mut right_pid_entities = None;

    for _ in 0..100 {
        run_app_once(&mut app);
        let ep = app.world().resource::<PartitionEntities>();
        if ep.changed.contains_key(&left) {
            left_changed = true;
        }
        if ep.changed.contains_key(&right) {
            right_changed = true;
        }
        left_pid_entities = ep.map.get(&left).copied();
        right_pid_entities = ep.map.get(&right).copied();
    }

    // After merge, both should be in the same partition
    assert_eq!(
        left_pid_entities, right_pid_entities,
        "Partitions should have merged in PartitionEntities"
    );

    // At least one of them MUST have changed its partition ID to match the other
    assert!(
        left_changed || right_changed,
        "At least one entity should have recorded a partition change during merge"
    );
}
