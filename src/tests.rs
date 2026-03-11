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
