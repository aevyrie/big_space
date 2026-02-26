use crate::hash::ChangedCells;
use crate::plugin::BigSpaceMinimalPlugins;
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
    let cell_id = CellId::__new_manual(grid_entity, &coord);
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
    let cell_id = CellId::__new_manual(grid_entity, &coord);
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
    app.add_plugins(BigSpaceMinimalPlugins);
    app.add_plugins(CellHashingPlugin::default());

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
