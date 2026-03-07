use crate::hash::ChangedCells;
use crate::plugin::{BigSpaceDefaultPlugins, BigSpaceMinimalPlugins};
use crate::prelude::*;
use bevy::prelude::*;

#[test]
fn changing_floating_origin_updates_global_transform() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);

    let first = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(150.0, 0.0, 0.0)),
            CellCoord::new(5, 0, 0),
            FloatingOrigin,
        ))
        .id();

    let second = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(0.0, 0.0, 300.0)),
            CellCoord::new(0, -15, 0),
        ))
        .id();

    app.world_mut()
        .spawn(BigSpaceRootBundle::default())
        .add_children(&[first, second]);

    app.update();

    app.world_mut().entity_mut(first).remove::<FloatingOrigin>();
    app.world_mut().entity_mut(second).insert(FloatingOrigin);

    app.update();

    let second_global_transform = app.world_mut().get::<GlobalTransform>(second).unwrap();

    assert_eq!(
        second_global_transform.translation(),
        Vec3::new(0.0, 0.0, 300.0)
    );
}

#[test]
fn child_global_transforms_are_updated_when_floating_origin_changes() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);

    let first = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(150.0, 0.0, 0.0)),
            CellCoord::new(5, 0, 0),
            FloatingOrigin,
        ))
        .id();

    let second = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(0.0, 0.0, 300.0)),
            CellCoord::new(0, -15, 0),
        ))
        .with_child(Transform::from_translation(Vec3::new(0.0, 0.0, 300.0)))
        .id();

    app.world_mut()
        .spawn(BigSpaceRootBundle::default())
        .add_children(&[first, second]);

    app.update();

    app.world_mut().entity_mut(first).remove::<FloatingOrigin>();
    app.world_mut().entity_mut(second).insert(FloatingOrigin);

    app.update();

    let child = app.world_mut().get::<Children>(second).unwrap()[0];
    let child_transform = app.world_mut().get::<GlobalTransform>(child).unwrap();

    assert_eq!(child_transform.translation(), Vec3::new(0.0, 0.0, 600.0));
}

#[test]
fn stationary_entities_do_not_recenter() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);

    let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    let stationary = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            CellCoord::new(0, 0, 0),
            Stationary,
        ))
        .set_parent_in_place(grid_entity)
        .id();

    app.update();

    // Move the stationary entity far away
    app.world_mut()
        .entity_mut(stationary)
        .get_mut::<Transform>()
        .unwrap()
        .translation = Vec3::new(100_000.0, 0.0, 0.0);

    app.update();

    // It should NOT have recentered
    let cell = app.world_mut().get::<CellCoord>(stationary).unwrap();
    assert_eq!(*cell, CellCoord::new(0, 0, 0));

    let transform = app.world_mut().get::<Transform>(stationary).unwrap();
    assert_eq!(transform.translation.x, 100_000.0);
}

#[test]
fn remove_stationary_move_then_readd() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins)
        .add_plugins(BigSpaceStationaryPlugin)
        .add_plugins(CellHashingPlugin::default());

    let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    // FO at origin
    app.world_mut()
        .spawn((CellCoord::default(), FloatingOrigin))
        .set_parent_in_place(grid_entity);

    let entity = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::ZERO),
            CellCoord::new(1, 0, 0),
            Stationary,
        ))
        .set_parent_in_place(grid_entity)
        .id();

    // Stabilize
    app.update();
    app.update();

    let cell_before = *app.world().get::<CellCoord>(entity).unwrap();
    assert_eq!(cell_before, CellCoord::new(1, 0, 0));

    // Remove Stationary, move the entity far enough to trigger recentering, then re-add
    app.world_mut().entity_mut(entity).remove::<Stationary>();
    app.update(); // cleanup_removed_stationary removes StationaryComputed

    app.world_mut()
        .entity_mut(entity)
        .get_mut::<Transform>()
        .unwrap()
        .translation = Vec3::new(100_000.0, 0.0, 0.0);
    app.update(); // recentering runs because entity is no longer Stationary

    let cell_after_move = *app.world().get::<CellCoord>(entity).unwrap();
    assert_ne!(
        cell_after_move,
        CellCoord::new(1, 0, 0),
        "Entity should have been recentered into a new cell after removing Stationary"
    );

    // Re-add Stationary
    app.world_mut().entity_mut(entity).insert(Stationary);
    app.update();

    // Verify StationaryComputed is re-applied and the entity is in the CellLookup
    assert!(
        app.world().get::<StationaryComputed>(entity).is_some(),
        "StationaryComputed should be re-inserted after re-adding Stationary"
    );

    let cell_id = *app.world().get::<CellId>(entity).unwrap();
    let lookup = app.world().resource::<CellLookup<()>>();
    assert!(
        lookup
            .get(&cell_id)
            .unwrap()
            .entities()
            .any(|e| e == entity),
        "Entity should be in CellLookup after re-adding Stationary"
    );

    // Verify it no longer recenters
    let cell_snapshot = *app.world().get::<CellCoord>(entity).unwrap();
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<Transform>()
        .unwrap()
        .translation = Vec3::new(200_000.0, 0.0, 0.0);
    app.update();

    assert_eq!(
        *app.world().get::<CellCoord>(entity).unwrap(),
        cell_snapshot,
        "After re-adding Stationary, recentering should be skipped again"
    );
}

#[test]
fn stationary_entities_are_correctly_initialized() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);
    app.add_plugins(CellHashingPlugin::default());

    let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    let stationary = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            CellCoord::new(1, 2, 3),
            Stationary,
        ))
        .set_parent_in_place(grid_entity)
        .id();

    app.update();

    // Verify it got a CellId
    let cell_id = *app
        .world_mut()
        .get::<CellId>(stationary)
        .expect("Stationary entity should have a CellId after the first frame");
    assert_eq!(cell_id.coord(), CellCoord::new(1, 2, 3));

    // Verify it is in CellLookup
    let lookup = app.world().resource::<CellLookup<()>>();
    assert!(lookup.contains(&cell_id));
    assert!(lookup
        .get(&cell_id)
        .unwrap()
        .entities()
        .any(|e| e == stationary));
}

#[test]
fn stationary_entity_spawned_with_cellid_is_registered() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);
    app.add_plugins(CellHashingPlugin::default());

    let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    let coord = CellCoord::new(1, 2, 3);
    let cell_id = CellId::new_manual(grid_entity, &coord);
    let cell_hash = CellHash::from(cell_id);

    let stationary = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::ZERO),
            coord,
            cell_id,
            cell_hash,
            Stationary,
        ))
        .set_parent_in_place(grid_entity)
        .id();

    app.update();

    // Verify it is in CellLookup
    let lookup = app.world().resource::<CellLookup<()>>();
    assert!(
        lookup.contains(&cell_id),
        "Stationary entity spawned with CellId should be in CellLookup"
    );
    assert!(
        lookup
            .get(&cell_id)
            .unwrap()
            .entities()
            .any(|e| e == stationary),
        "Stationary entity should be found in CellLookup entry"
    );
}

#[test]
fn moving_entity_spawned_with_cellid_is_registered() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);
    app.add_plugins(CellHashingPlugin::default());

    let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    let coord = CellCoord::new(1, 2, 3);
    let cell_id = CellId::new_manual(grid_entity, &coord);
    let cell_hash = CellHash::from(cell_id);

    let _moving = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::ZERO),
            coord,
            cell_id,
            cell_hash,
        ))
        .set_parent_in_place(grid_entity)
        .id();

    app.update();

    // Verify it is in CellLookup
    let lookup = app.world().resource::<CellLookup<()>>();
    assert!(
        lookup.contains(&cell_id),
        "Moving entity spawned with CellId should be in CellLookup"
    );
}

#[test]
fn stationary_entities_do_not_trigger_unnecessary_updates() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins)
        .add_plugins(BigSpaceStationaryPlugin)
        .add_plugins(CellHashingPlugin::default());

    let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    let mut stationary_entities = Vec::new();
    for i in 0..100 {
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
                CellCoord::new(i, 0, 0),
                Stationary,
            ))
            .set_parent_in_place(grid_entity)
            .id();
        stationary_entities.push(entity);
    }

    app.update();

    // After first frame, they all should have CellId and be in ChangedCells
    {
        let changed_cells = app.world().resource::<ChangedCells<()>>();
        assert_eq!(changed_cells.len(), 100);
    }

    // Second frame - nothing should change
    app.update();
    {
        let changed_cells = app.world().resource::<ChangedCells<()>>();
        assert_eq!(
            changed_cells.len(),
            0,
            "No updates should happen for stationary entities after the first frame"
        );
    }

    // Now move them - Transform changes, but they are Stationary so they don't recenter and don't change CellCoord
    for entity in &stationary_entities {
        app.world_mut()
            .entity_mut(*entity)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x = 1000.0;
    }

    app.update();
    {
        let changed_cells = app.world().resource::<ChangedCells<()>>();
        assert_eq!(
            changed_cells.len(),
            0,
            "Stationary entities should skip updates even if their Transform changes"
        );
    }

    // Manually change CellCoord for one of them - it should STILL skip updates because of Without<Stationary>
    app.world_mut()
        .entity_mut(stationary_entities[0])
        .get_mut::<CellCoord>()
        .unwrap()
        .x = 500;

    app.update();
    {
        let changed_cells = app.world().resource::<ChangedCells<()>>();
        assert_eq!(
            changed_cells.len(),
            0,
            "Stationary entities should skip updates even if their CellCoord changes"
        );
    }
}

/// Verifies that a [`Stationary`] entity's [`GlobalTransform`] is updated when the floating
/// origin moves to a new cell, even though the grid's dirty tick says the subtree is clean.
///
/// This exercises the `!is_local_origin_unchanged()` override path in the pruning logic.
#[test]
fn stationary_entity_gt_updates_when_fo_moves() {
    let mut app = App::new();
    // Use the stationary plugin so GridDirtyTick is active
    app.add_plugins(BigSpaceMinimalPlugins)
        .add_plugins(BigSpaceStationaryPlugin);

    let root = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    // FO starts at cell (0, 0, 0)
    let fo = app
        .world_mut()
        .spawn((CellCoord::default(), FloatingOrigin))
        .set_parent_in_place(root)
        .id();

    // Stationary entity at cell (2, 0, 0) - 2 * 2000 = 4000 from the FO
    let stationary = app
        .world_mut()
        .spawn((CellCoord::new(2, 0, 0), Stationary))
        .set_parent_in_place(root)
        .id();

    // Let the world stabilize so GridDirtyTick is in "clean" state
    app.update(); // frame 1: GTs computed, GridDirtyTick inserted
    app.update(); // frame 2: subtree clean

    let gt_before = app
        .world()
        .get::<GlobalTransform>(stationary)
        .unwrap()
        .translation();
    assert_eq!(
        gt_before,
        Vec3::new(4000.0, 0.0, 0.0),
        "Stationary entity should be at 4000 with FO at cell 0"
    );

    // Move the FO to cell (1, 0, 0) - now entity is only 2000 away
    app.world_mut()
        .entity_mut(fo)
        .get_mut::<CellCoord>()
        .unwrap()
        .x = 1;
    app.update();

    let gt_after = app
        .world()
        .get::<GlobalTransform>(stationary)
        .unwrap()
        .translation();
    assert_eq!(
        gt_after,
        Vec3::new(2000.0, 0.0, 0.0),
        "Stationary entity GT must update when floating origin moves, even in a clean subtree"
    );
}

/// Verifies that a [`Stationary`] entity spawned *after* the grid has already stabilized
/// (i.e., [`GridDirtyTick`] is present and clean) still receives its initial
/// [`GlobalTransform`] on the very next frame.
///
/// This is a regression test for the bug where newly spawned stationary entities were
/// permanently stuck at [`GlobalTransform::IDENTITY`] because `mark_dirty_subtrees`
/// excluded them from the dirty walk via `Without<Stationary>`.
#[test]
fn dynamically_spawned_stationary_entity_gets_gt() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins)
        .add_plugins(BigSpaceStationaryPlugin);

    let root = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

    app.world_mut()
        .spawn((CellCoord::default(), FloatingOrigin))
        .set_parent_in_place(root);

    // Let the grid settle: GridDirtyTick is inserted and the subtree becomes clean.
    app.update(); // frame 1: GridDirtyTick deferred-inserted, GT computed for initial entities
    app.update(); // frame 2: grid is now "clean" (no changed non-stationary entities)

    // Spawn a Stationary entity into the now-stable grid.
    let late_stationary = app
        .world_mut()
        .spawn((CellCoord::new(1, 0, 0), Stationary))
        .set_parent_in_place(root)
        .id();

    // One more frame: mark_dirty_subtrees detects Changed<Children> on the grid and marks it dirty.
    app.update();

    let gt = app
        .world()
        .get::<GlobalTransform>(late_stationary)
        .unwrap()
        .translation();
    assert_ne!(
        gt,
        Vec3::ZERO,
        "Dynamically spawned Stationary entity must have its GT computed (not stuck at IDENTITY)"
    );
    assert_eq!(
        gt,
        Vec3::new(2000.0, 0.0, 0.0),
        "Stationary entity at CellCoord(1,0,0) should have GT = 2000 with FO at cell 0"
    );
}

/// Verifies that [`BigSpaceStationaryPlugin`] and no plugin produce identical
/// [`GlobalTransform`]s for the same world state after several frames of activity.
///
/// This is an equivalence/regression guard for the tree-walk rewrite: adding the
/// stationary optimization must never change the computed GT values.
#[test]
fn plugin_and_no_plugin_produce_same_gts() {
    fn build_and_run(with_stationary_plugin: bool) -> (Vec3, Vec3) {
        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins);
        if with_stationary_plugin {
            app.add_plugins(BigSpaceStationaryPlugin);
        }

        let root = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

        let fo = app
            .world_mut()
            .spawn((CellCoord::default(), FloatingOrigin))
            .set_parent_in_place(root)
            .id();

        let moving = app
            .world_mut()
            .spawn((
                CellCoord::new(1, 0, 0),
                Transform::from_xyz(100.0, 0.0, 0.0),
            ))
            .set_parent_in_place(root)
            .id();

        let stationary = app
            .world_mut()
            .spawn((CellCoord::new(3, 0, 0), Stationary))
            .set_parent_in_place(root)
            .id();

        app.update(); // frame 1
        app.update(); // frame 2: grid clean

        // Move the floating origin, forcing all GTs to recompute
        app.world_mut()
            .entity_mut(fo)
            .get_mut::<CellCoord>()
            .unwrap()
            .x = 1;

        app.update(); // frame 3: FO moved

        let gt_moving = app
            .world()
            .get::<GlobalTransform>(moving)
            .unwrap()
            .translation();
        let gt_stationary = app
            .world()
            .get::<GlobalTransform>(stationary)
            .unwrap()
            .translation();
        (gt_moving, gt_stationary)
    }

    let (gt_moving_no_plugin, gt_stationary_no_plugin) = build_and_run(false);
    let (gt_moving_with_plugin, gt_stationary_with_plugin) = build_and_run(true);

    assert_eq!(
        gt_moving_no_plugin, gt_moving_with_plugin,
        "Moving entity GT must be identical with and without BigSpaceStationaryPlugin"
    );
    assert_eq!(
        gt_stationary_no_plugin, gt_stationary_with_plugin,
        "Stationary entity GT must be identical with and without BigSpaceStationaryPlugin"
    );
}

/// Verifies that a non-stationary entity changing in a nested sub-grid correctly propagates
/// dirty-marking up through ancestor grids, and that its [`GlobalTransform`] is updated.
///
/// Exercises the `mark_dirty_subtrees` ancestor walk for deeply nested hierarchies.
#[test]
fn nested_sub_grid_entity_gt_updates_correctly() {
    #[derive(Component)]
    struct Marker;

    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins)
        .add_plugins(BigSpaceStationaryPlugin)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn_big_space_default(|root| {
                root.spawn_spatial(FloatingOrigin);
                // Sub-grid at (1000, 0, 0) in root.
                root.with_grid_default(|sub_grid| {
                    sub_grid.insert(Transform::from_xyz(1000.0, 0.0, 0.0));
                    // Entity inside sub-grid at (500, 0, 0) → total GT = 1000 + 500 = 1500
                    sub_grid.spawn_spatial((Transform::from_xyz(500.0, 0.0, 0.0), Marker));
                });
            });
        });

    app.update(); // frame 1: initial GTs computed

    let entity = app
        .world_mut()
        .query_filtered::<Entity, With<Marker>>()
        .single(app.world())
        .unwrap();

    let gt_initial = app
        .world()
        .get::<GlobalTransform>(entity)
        .unwrap()
        .translation();
    assert_eq!(
        gt_initial,
        Vec3::new(1500.0, 0.0, 0.0),
        "Initial GT should be sub-grid pos + entity pos = 1500"
    );

    app.update(); // frame 2: subtree clean

    // Move the entity within the sub-grid
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<Transform>()
        .unwrap()
        .translation
        .x = 600.0;

    app.update(); // frame 3: mark_dirty_subtrees must mark sub_grid AND root dirty

    let gt_after = app
        .world()
        .get::<GlobalTransform>(entity)
        .unwrap()
        .translation();
    assert_eq!(
        gt_after,
        Vec3::new(1600.0, 0.0, 0.0),
        "GT must update when entity moves inside a sub-grid: 1000 + 600 = 1600"
    );
}

/// Verifies that [`BigSpaceStationaryPlugin`] is not accidentally included in
/// [`BigSpaceMinimalPlugins`], which would impose the optimization overhead on all users.
#[test]
fn stationary_plugin_excluded_from_minimal_plugins() {
    let mut app = App::new();
    app.add_plugins(BigSpaceMinimalPlugins);
    // BigSpaceStationaryPlugin registers GridDirtyTick for reflection.
    // If it were accidentally included, GridDirtyTick would be registered.
    // We verify by checking that no Grid entity automatically gets a GridDirtyTick
    // after an update (the auto-insertion only happens when the plugin is active).
    let root = app.world_mut().spawn(BigSpaceRootBundle::default()).id();
    app.update();
    assert!(
        app.world().get::<GridDirtyTick>(root).is_none(),
        "GridDirtyTick should not be auto-inserted without BigSpaceStationaryPlugin"
    );
}

/// Verifies that [`BigSpaceStationaryPlugin`] is included in [`BigSpaceDefaultPlugins`].
#[test]
fn stationary_plugin_included_in_default_plugins() {
    let mut app = App::new();
    app.add_plugins(BigSpaceDefaultPlugins);
    let root = app.world_mut().spawn(BigSpaceRootBundle::default()).id();
    app.update();
    assert!(
        app.world().get::<GridDirtyTick>(root).is_some(),
        "GridDirtyTick should be auto-inserted when BigSpaceStationaryPlugin is active via BigSpaceDefaultPlugins"
    );
}
