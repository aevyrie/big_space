//! Components and systems for optimizing stationary entities.
//!
//! See [`Stationary`], [`BigSpaceStationaryPlugin`].

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::{
    change_detection::Tick, lifecycle::HookContext, prelude::*, system::SystemChangeTick,
    world::DeferredWorld,
};
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;

/// A component that optimizes entities that do not move.
///
/// When an entity is marked as stationary, the plugin will skip most per-frame computations for it.
/// This includes grid recentering and spatial hashing updates. The `CellCoord` will only be
/// computed when the entity is spawned or when its parent changes.
///
/// # One-frame initialization delay
///
/// `Stationary` takes effect one full frame after insertion. During that first frame the entity
/// has `With<Stationary>` but not yet `With<StationaryInitialized>`, giving every system
/// (propagation, spatial hashing, etc.) one pass to process the entity before it goes to sleep.
///
/// - **Systems that need to run before sleep** should query
///   `(With<Stationary>, Without<StationaryInitialized>)`.
/// - **Systems that want to skip sleeping entities** should query
///   `With<StationaryInitialized>`.
///
/// # Important
///
/// Do **not** move a `Stationary` entity by mutating its [`Transform`] or [`CellCoord`].
/// Stationary entities are excluded from grid-cell recentering and spatial hash updates, so
/// changes to these components will not be picked up by the plugin. If you need to relocate a
/// stationary entity, remove the `Stationary` component first, move the entity, and then
/// re-add it.
///
/// Note that when a `Stationary` entity is first spawned, its [`Transform`] translation is
/// recentered into the correct grid cell (updating both [`CellCoord`] and [`Transform`]).
/// This one-time snap ensures the entity starts in a valid state regardless of the initial
/// translation magnitude.
#[derive(Debug, Clone, Reflect, Component, Default)]
#[component(on_remove = Stationary::on_remove)]
#[reflect(Component, Default)]
pub struct Stationary;

impl Stationary {
    /// Removes [`StationaryInitialized`] when [`Stationary`] is removed, so that the entity
    /// re-enters the normal update path for recentering and spatial hashing.
    fn on_remove(mut world: DeferredWorld, ctx: HookContext) {
        world
            .commands()
            .entity(ctx.entity)
            .try_remove::<StationaryInitialized>();
    }
}

/// Marker inserted by [`BigSpaceStationaryPlugin`] one frame after [`Stationary`] is added.
///
/// During the first frame an entity has `Stationary` but not yet `StationaryInitialized`,
/// giving all systems (propagation, spatial hashing, etc.) one pass to process the entity.
/// After that frame, `StationaryInitialized` is inserted and the entity is considered
/// *sleeping* - propagation skips recomputing its [`GlobalTransform`] unless the floating
/// origin moves, and [`CellHashingPlugin`] skips recomputing its [`CellId`].
///
/// See the docs on [`Stationary`] for the recommended query patterns.
#[derive(Debug, Clone, Reflect, Component, Default)]
#[reflect(Component, Default)]
pub struct StationaryInitialized;

/// Enables subtree pruning in [`Grid::propagate_high_precision`].
///
/// Auto-inserted on all [`Grid`] entities by [`BigSpaceStationaryPlugin`].
/// Absence means pruning is disabled for that grid (always treated as dirty).
///
/// Stores the last tick when any non-[`Stationary`] entity in this grid's subtree
/// had a changed [`Transform`], [`CellCoord`], or [`ChildOf`].
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct GridDirtyTick(u32);

impl GridDirtyTick {
    /// Returns `true` if this subtree has dirty non-stationary entities this frame.
    pub(crate) fn is_dirty(&self, system_ticks: SystemChangeTick) -> bool {
        Tick::new(self.0).is_newer_than(system_ticks.last_run(), system_ticks.this_run())
    }
}

/// Marks grid subtrees as dirty when non-[`Stationary`] entities change.
///
/// This pre-pass runs before [`Grid::propagate_high_precision`]. It walks the ancestors
/// of changed non-stationary entities and marks each ancestor [`Grid`] dirty via
/// [`GridDirtyTick`]. It also auto-inserts [`GridDirtyTick`] on any [`Grid`] that doesn't
/// have it yet.
///
/// Additionally, any [`Grid`] whose [`Children`] list changed this frame (entities added
/// or removed) marks itself and all ancestor grids dirty, ensuring newly spawned entities
/// (including [`Stationary`] ones excluded by `changed`) always get their initial
/// [`GlobalTransform`] computed even if the grid subtree was previously clean.
pub(crate) fn mark_dirty_subtrees(
    mut commands: Commands,
    system_ticks: SystemChangeTick,
    parents: Query<&ChildOf>,
    mut dirty_ticks: Query<&mut GridDirtyTick>,
    grids_without: Query<Entity, (With<Grid>, Without<GridDirtyTick>)>,
    changed: Query<
        &ChildOf,
        (
            Without<Stationary>,
            Or<(Changed<Transform>, Changed<CellCoord>, Changed<ChildOf>)>,
        ),
    >,
    // Catches grids that gained or lost children this frame (including newly spawned
    // Stationary entities excluded by `changed`) without scanning all CellCoord entities.
    grids_with_changed_children: Query<Entity, (With<Grid>, Changed<Children>)>,
) {
    // Auto-insert on any Grid that doesn't have GridDirtyTick yet.
    // Commands are deferred, so newly inserted grids treat themselves as dirty (correct for
    // first GT initialization).
    for entity in grids_without.iter() {
        commands.entity(entity).insert(GridDirtyTick::default());
    }

    let current_tick = system_ticks.this_run().get();

    for parent_rel in changed.iter() {
        mark_ancestor_grids(
            parent_rel.parent(),
            current_tick,
            &mut dirty_ticks,
            &parents,
        );
    }

    // Mark the grid itself (and its ancestors) dirty whenever its children list changes.
    // This ensures a freshly spawned child entity receives its initial GlobalTransform
    // even when the grid's subtree was otherwise clean.
    for grid_entity in grids_with_changed_children.iter() {
        mark_ancestor_grids(grid_entity, current_tick, &mut dirty_ticks, &parents);
    }
}

fn mark_ancestor_grids(
    start: Entity,
    current_tick: u32,
    dirty_ticks: &mut Query<&mut GridDirtyTick>,
    parents: &Query<&ChildOf>,
) {
    let mut ancestor = start;
    loop {
        let Ok(mut dirty) = dirty_ticks.get_mut(ancestor) else {
            break;
        };
        // bypass_change_detection to avoid spurious Changed<GridDirtyTick> noise
        let d = dirty.bypass_change_detection();
        // Early exit: if already marked this tick, all ancestors were marked too
        if d.0 == current_tick {
            break;
        }
        d.0 = current_tick;
        match parents.get(ancestor) {
            Ok(p) => ancestor = p.parent(),
            Err(_) => break,
        }
    }
}

/// Inserts [`StationaryInitialized`] on [`Stationary`] entities that have been present for at
/// least one full frame.
///
/// By waiting until [`Ref::is_added`] returns `false`, every system (propagation, spatial
/// hashing, etc.) gets one pass to process the entity before it goes to sleep.
///
/// Runs in [`Last`] so that its deferred commands cannot trigger Bevy's automatic
/// `apply_deferred` during [`PostUpdate`], which would interfere with systems that filter
/// `Without<StationaryInitialized>`.
fn mark_stationary_initialized(
    mut commands: Commands,
    uninitialized: Query<(Entity, Ref<Stationary>), Without<StationaryInitialized>>,
) {
    for (entity, stationary) in uninitialized.iter() {
        if !stationary.is_added() {
            commands.entity(entity).insert(StationaryInitialized);
        }
    }
}

/// Opt-in plugin that enables the stationary entity subtree-pruning optimization.
///
/// Add this plugin to enable dirty-tick tracking for [`Grid`] subtrees. When active,
/// [`Grid::propagate_high_precision`] skips entire subtrees where both:
/// - The grid's local floating origin has not changed, **and**
/// - No non-[`Stationary`] entity in the subtree has a changed [`Transform`],
///   [`CellCoord`], or [`ChildOf`] this frame.
///
/// Without this plugin, all grid subtrees are visited every frame (correct, just less efficient
/// for worlds with many stationary entities spread across many grids).
///
/// This plugin also registers reflection for [`Stationary`], [`StationaryInitialized`], and
/// [`GridDirtyTick`].
///
/// # Note
///
/// This plugin is included in [`BigSpaceDefaultPlugins`] but **not** in
/// [`BigSpaceMinimalPlugins`](crate::plugin::BigSpaceMinimalPlugins). Add it manually alongside
/// [`BigSpaceMinimalPlugins`](crate::plugin::BigSpaceMinimalPlugins) when you want the
/// optimization without the full default plugin set.
pub struct BigSpaceStationaryPlugin;

impl Plugin for BigSpaceStationaryPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Stationary>()
            .register_type::<StationaryInitialized>()
            .register_type::<GridDirtyTick>();

        #[cfg(feature = "std")]
        let dirty_configs = || {
            mark_dirty_subtrees
                .in_set(BigSpaceSystems::PropagateHighPrecision)
                .before(Grid::propagate_high_precision_channeled)
                .after(BigSpaceSystems::LocalFloatingOrigins)
        };
        #[cfg(not(feature = "std"))]
        let dirty_configs = || {
            mark_dirty_subtrees
                .in_set(BigSpaceSystems::PropagateHighPrecision)
                .before(Grid::propagate_high_precision)
                .after(BigSpaceSystems::LocalFloatingOrigins)
        };
        // mark_stationary_initialized runs in Last (not PostUpdate) so it cannot trigger
        // Bevy's auto-apply_deferred before any PostUpdate system that filters
        // Without<StationaryInitialized> (e.g. CellId::compute_stationary_cell).
        app.add_systems(PostUpdate, dirty_configs())
            .add_systems(PostStartup, dirty_configs())
            .add_systems(Last, mark_stationary_initialized);
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::ChangedCells;
    use crate::plugin::BigSpaceMinimalPlugins;
    use crate::prelude::*;
    use bevy::prelude::*;

    /// Stationary entities must not be recentered into a new grid cell, even when their
    /// [`Transform`] translation is large enough to trigger recentering for normal entities.
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

    /// Removing `Stationary`, moving the entity, and re-adding `Stationary` must correctly
    /// recenter the entity, update its [`CellId`], and resume skipping updates.
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
        app.update(); // cleanup_removed_stationary removes StationaryInitialized

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
        app.update(); // Frame 1: systems process the entity (one-frame delay)
        app.update(); // Frame 2: mark_stationary_initialized inserts StationaryInitialized

        // Verify StationaryInitialized is re-applied and the entity is in the CellLookup
        assert!(
            app.world().get::<StationaryInitialized>(entity).is_some(),
            "StationaryInitialized should be re-inserted after re-adding Stationary"
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

    /// A newly spawned `Stationary` entity gets a [`CellId`] computed and appears in
    /// [`CellLookup`] after the first frame.
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

    /// A `Stationary` entity spawned with an existing [`CellId`] is still picked up by
    /// [`CellLookup`] and receives [`StationaryInitialized`] after stabilization.
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

    /// After initialization, `Stationary` entities must not appear in [`ChangedCells`] on
    /// subsequent frames, even if their [`Transform`] is mutated.
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

        // Second frame - StationaryInitialized not yet inserted (one-frame delay), so
        // compute_stationary_cell runs again (idempotent but populates ChangedCells).
        app.update();
        {
            let changed_cells = app.world().resource::<ChangedCells<()>>();
            assert_eq!(
                changed_cells.len(),
                100,
                "Extra frame before StationaryInitialized"
            );
        }

        // Third frame - StationaryInitialized now present, nothing should change
        app.update();
        {
            let changed_cells = app.world().resource::<ChangedCells<()>>();
            assert_eq!(
                changed_cells.len(),
                0,
                "No updates should happen for stationary entities after initialization"
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

    /// Verifies that [`BigSpaceStationaryPlugin`] is not accidentally included in
    /// [`BigSpaceMinimalPlugins`], which would impose the optimization overhead on all users.
    #[test]
    fn stationary_plugin_excluded_from_minimal_plugins() {
        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins);
        let root = app.world_mut().spawn(BigSpaceRootBundle::default()).id();
        app.update();
        assert!(
            app.world().get::<GridDirtyTick>(root).is_none(),
            "GridDirtyTick should not be auto-inserted without BigSpaceStationaryPlugin"
        );
    }

    /// After removing [`Stationary`] and changing [`CellCoord`], the entity should re-enter the
    /// spatial hash update path and be tracked in [`CellLookup`] at its new cell.
    ///
    /// This catches timing issues where [`StationaryInitialized`] lingers or [`Changed<CellCoord>`]
    /// detection fails after archetype changes from removing Stationary/StationaryInitialized.
    #[test]
    fn entity_reenters_spatial_hash_after_stationary_removed() {
        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_plugins(BigSpaceStationaryPlugin)
            .add_plugins(CellHashingPlugin::default());

        let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

        // FO at origin
        app.world_mut()
            .spawn((CellCoord::default(), FloatingOrigin))
            .set_parent_in_place(grid_entity);

        // Spawn entity with Stationary
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(Vec3::ZERO),
                CellCoord::new(1, 0, 0),
                Stationary,
            ))
            .set_parent_in_place(grid_entity)
            .id();

        // Frame 1: compute_stationary_cell adds CellId
        app.update();
        assert!(
            app.world().get::<CellId>(entity).is_some(),
            "Entity should have CellId after first frame"
        );

        // Frame 2: mark_stationary_initialized adds StationaryInitialized (runs in Last)
        app.update();
        assert!(
            app.world().get::<StationaryInitialized>(entity).is_some(),
            "StationaryInitialized should be present after stabilization"
        );

        let old_cell_id = *app.world().get::<CellId>(entity).unwrap();

        // Verify entity is in CellLookup at old cell
        let lookup = app.world().resource::<CellLookup<()>>();
        assert!(
            lookup
                .get(&old_cell_id)
                .unwrap()
                .entities()
                .any(|e| e == entity),
            "Entity should be in CellLookup at original cell"
        );

        // Simulate wake-up: remove Stationary and change CellCoord in the same operation.
        app.world_mut().entity_mut(entity).remove::<Stationary>();
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CellCoord>()
            .unwrap()
            .x = 5;

        // This frame should: remove StationaryInitialized (via Stationary on_remove hook),
        // then CellId::update detects Changed<CellCoord> + Without<Stationary> → updates CellId.
        app.update();

        // Verify StationaryInitialized was removed
        assert!(
            app.world().get::<StationaryInitialized>(entity).is_none(),
            "StationaryInitialized should be removed after Stationary is removed"
        );
        // Verify Stationary was removed
        assert!(
            app.world().get::<Stationary>(entity).is_none(),
            "Stationary should be removed"
        );

        // Verify CellId was updated to the new cell
        let new_cell_id = *app.world().get::<CellId>(entity).unwrap();
        assert_ne!(
            old_cell_id, new_cell_id,
            "CellId should have been updated to reflect the new CellCoord"
        );
        assert_eq!(
            new_cell_id.coord(),
            CellCoord::new(5, 0, 0),
            "CellId should reflect cell (5, 0, 0)"
        );

        // Verify entity is in CellLookup at the NEW cell
        let lookup = app.world().resource::<CellLookup<()>>();
        assert!(
            lookup
                .get(&new_cell_id)
                .unwrap()
                .entities()
                .any(|e| e == entity),
            "Entity should be tracked in CellLookup at its new cell after wake-up"
        );

        // Verify entity is NOT in CellLookup at the OLD cell
        assert!(
            lookup
                .get(&old_cell_id)
                .is_none_or(|entry| !entry.entities().any(|e| e == entity)),
            "Entity should no longer be in CellLookup at old cell"
        );

        // Verify the new cell appears in newly_occupied
        assert!(
            lookup.newly_occupied().contains(&new_cell_id),
            "New cell should be in newly_occupied set"
        );
    }

    /// `CellCoord` is changed while the entity still has Stationary, then Stationary removal
    /// happens via deferred commands in the same frame. The spatial hash must detect the
    /// `CellCoord` change on the next frame after Stationary is gone.
    #[test]
    fn stationary_removal_with_cellcoord_change_detected() {
        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_plugins(BigSpaceStationaryPlugin)
            .add_plugins(CellHashingPlugin::default());

        let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

        app.world_mut()
            .spawn((CellCoord::default(), FloatingOrigin))
            .set_parent_in_place(grid_entity);

        // Spawn with Stationary
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(Vec3::ZERO),
                CellCoord::new(1, 0, 0),
                Stationary,
            ))
            .set_parent_in_place(grid_entity)
            .id();

        // Stabilize: CellId created, StationaryInitialized added
        app.update();
        app.update();

        assert!(app.world().get::<StationaryInitialized>(entity).is_some());
        assert!(app.world().get::<Stationary>(entity).is_some());
        let old_cell_id = *app.world().get::<CellId>(entity).unwrap();

        // Change CellCoord while entity still has Stationary
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CellCoord>()
            .unwrap()
            .x = 5;

        // Queue Stationary removal via deferred commands
        app.world_mut()
            .commands()
            .entity(entity)
            .remove::<Stationary>();

        app.update();

        // Verify the chain completed
        assert!(
            app.world().get::<Stationary>(entity).is_none(),
            "Stationary should be removed"
        );
        assert!(
            app.world().get::<StationaryInitialized>(entity).is_none(),
            "StationaryInitialized should be removed via hook chain"
        );

        // THE CRITICAL CHECK: CellId must reflect the new CellCoord
        let new_cell_id = *app.world().get::<CellId>(entity).unwrap();
        assert_ne!(
            old_cell_id, new_cell_id,
            "CellId should have been updated to the new cell"
        );
        assert_eq!(
            new_cell_id.coord(),
            CellCoord::new(5, 0, 0),
            "CellId should reflect cell (5, 0, 0)"
        );

        // Verify entity is in CellLookup at the new cell
        let lookup = app.world().resource::<CellLookup<()>>();
        assert!(
            lookup
                .get(&new_cell_id)
                .is_some_and(|entry| entry.entities().any(|e| e == entity)),
            "Entity must be tracked in CellLookup at new cell after waking up"
        );

        // Verify entity is NOT in CellLookup at the old cell
        assert!(
            lookup
                .get(&old_cell_id)
                .is_none_or(|entry| !entry.entities().any(|e| e == entity)),
            "Entity should no longer be in CellLookup at old cell"
        );
    }

    /// A system running after spatial hash systems changes `CellCoord` and removes Stationary
    /// via deferred commands. Next frame's `CellId::update` must detect the cross-frame
    /// Changed<CellCoord>.
    #[test]
    fn late_cellcoord_change_with_deferred_stationary_removal() {
        #[derive(Component)]
        struct TestBody;

        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_plugins(BigSpaceStationaryPlugin)
            .add_plugins(CellHashingPlugin::<With<TestBody>>::new());

        let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

        app.world_mut()
            .spawn((CellCoord::default(), FloatingOrigin))
            .set_parent_in_place(grid_entity);

        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(Vec3::ZERO),
                CellCoord::new(1, 0, 0),
                TestBody,
                Stationary,
            ))
            .set_parent_in_place(grid_entity)
            .id();

        // Stabilize
        app.update();
        app.update();

        assert!(app.world().get::<CellId>(entity).is_some());
        assert!(app.world().get::<StationaryInitialized>(entity).is_some());
        let old_cell_id = *app.world().get::<CellId>(entity).unwrap();

        // Verify entity is in the filtered CellLookup
        {
            let lookup = app.world().resource::<CellLookup<With<TestBody>>>();
            assert!(
                lookup
                    .get(&old_cell_id)
                    .is_some_and(|e| e.entities().any(|e| e == entity)),
                "Entity should be in filtered CellLookup before wake-up"
            );
        }

        // System changes CellCoord after spatial hash systems ran
        fn late_cellcoord_change(
            mut query: Query<&mut CellCoord, With<TestBody>>,
            mut commands: Commands,
            with_stationary: Query<Entity, (With<TestBody>, With<Stationary>)>,
        ) {
            for mut coord in query.iter_mut() {
                if coord.x == 1 {
                    coord.x = 5;
                }
            }
            for entity in with_stationary.iter() {
                commands.entity(entity).remove::<Stationary>();
            }
        }

        app.add_systems(
            PostUpdate,
            late_cellcoord_change.after(SpatialHashSystems::UpdateCellLookup),
        );

        app.update();
        app.update();

        // Verify the chain completed
        assert!(
            app.world().get::<Stationary>(entity).is_none(),
            "Stationary should be removed"
        );
        assert!(
            app.world().get::<StationaryInitialized>(entity).is_none(),
            "StationaryInitialized should be removed"
        );

        // CellId must reflect the new CellCoord
        let new_cell_id = *app.world().get::<CellId>(entity).unwrap();
        assert_ne!(
            old_cell_id, new_cell_id,
            "CellId should have been updated to the new cell"
        );
        assert_eq!(
            new_cell_id.coord(),
            CellCoord::new(5, 0, 0),
            "CellId should reflect cell (5, 0, 0)"
        );

        let lookup = app.world().resource::<CellLookup<With<TestBody>>>();
        assert!(
            lookup
                .get(&new_cell_id)
                .is_some_and(|entry| entry.entities().any(|e| e == entity)),
            "Entity must be tracked in filtered CellLookup at new cell"
        );

        assert!(
            lookup
                .get(&old_cell_id)
                .is_none_or(|entry| !entry.entities().any(|e| e == entity)),
            "Entity should no longer be in filtered CellLookup at old cell"
        );
    }

    /// Regression test for a bug where an entity that gained `Stationary` + `StationaryInitialized`
    /// in the same frame cycle was never processed by `compute_stationary_cell`. When `CellCoord`
    /// was also changed in that frame, `CellId::update` (which has `Without<Stationary>`) skipped
    /// the entity, and `compute_stationary_cell` (which has `Without<StationaryInitialized>`)
    /// also skipped it. The entity's `CellId` became permanently stale.
    ///
    /// Fixed by delaying `StationaryInitialized` insertion until the frame after `Stationary`
    /// is added, giving all systems one full pass to process the entity.
    #[test]
    fn late_stationary_insert_updates_cellid() {
        #[derive(Component)]
        struct TestBody;

        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_plugins(BigSpaceStationaryPlugin)
            .add_plugins(CellHashingPlugin::<With<TestBody>>::new())
            .add_plugins(PartitionPlugin::<With<TestBody>>::new())
            .add_plugins(PartitionChangePlugin::<With<TestBody>>::new());

        let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

        app.world_mut()
            .spawn((CellCoord::default(), FloatingOrigin))
            .set_parent_in_place(grid_entity);

        // Spawn entity at cell (1, 0, 0)
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(Vec3::ZERO),
                CellCoord::new(1, 0, 0),
                TestBody,
            ))
            .set_parent_in_place(grid_entity)
            .id();

        // Stabilize
        app.update();
        app.update();

        let old_cid = *app.world().get::<CellId>(entity).unwrap();
        assert_eq!(old_cid.coord(), CellCoord::new(1, 0, 0));

        // Simulate a late CellCoord change + Stationary insertion after spatial hash
        // systems already ran in the same PostUpdate.
        #[derive(Resource)]
        struct LateStationaryTarget(Entity);

        fn late_stationary_insert(
            mut commands: Commands,
            target: Res<LateStationaryTarget>,
            mut cell_coords: Query<&mut CellCoord>,
            mut ran: Local<bool>,
        ) {
            if *ran {
                return;
            }
            // Write a new CellCoord (entity moved to cell 5,0,0)
            if let Ok(mut coord) = cell_coords.get_mut(target.0) {
                *coord = CellCoord::new(5, 0, 0);
            }
            // Insert Stationary after spatial hash systems already ran
            commands.entity(target.0).insert(Stationary);
            *ran = true;
        }

        app.insert_resource(LateStationaryTarget(entity));
        app.add_systems(
            PostUpdate,
            late_stationary_insert.after(SpatialHashSystems::UpdatePartitionChange),
        );

        // Frame 1: late_stationary_insert writes CellCoord(5,0,0) and inserts Stationary.
        app.update();

        // Frame 2: compute_stationary_cell sees Without<StationaryInitialized> still satisfied
        // (one-frame delay) so it re-processes the entity and updates CellId to match the
        // new CellCoord. StationaryInitialized is inserted in Last this frame.
        app.update();

        // Verify the entity has both Stationary and StationaryInitialized
        assert!(
            app.world().get::<Stationary>(entity).is_some(),
            "Entity should have Stationary"
        );
        assert!(
            app.world().get::<StationaryInitialized>(entity).is_some(),
            "Entity should have StationaryInitialized"
        );

        // Previously CellId would be stuck at (1,0,0) here; now it must match CellCoord.
        let current_cid = *app.world().get::<CellId>(entity).unwrap();
        let current_coord = *app.world().get::<CellCoord>(entity).unwrap();
        assert_eq!(
            current_coord,
            CellCoord::new(5, 0, 0),
            "CellCoord was written by the late system"
        );
        assert_eq!(
            current_cid.coord(),
            current_coord,
            "CellId ({:?}) must match CellCoord ({:?}). Previously failed when the entity \
             gained Stationary + StationaryInitialized before compute_stationary_cell could \
             process the CellCoord change.",
            current_cid.coord(),
            current_coord,
        );
    }

    /// Regression test: with multiple `CellHashingPlugin<F>` instances, a stationary entity
    /// matching both filters must appear in both `ChangedCells<F1>` and `ChangedCells<F2>`.
    ///
    /// Previously, the first filter's `compute_stationary_cell` inserted `StationaryInitialized`,
    /// causing the second filter's version to skip the entity. Now `StationaryInitialized` is
    /// inserted by `mark_stationary_initialized` with a one-frame delay, so all filters get a
    /// chance to process the entity.
    #[test]
    fn stationary_populates_all_changed_cells_with_multiple_filters() {
        #[derive(Component)]
        struct TagA;
        #[derive(Component)]
        struct TagB;

        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_plugins(BigSpaceStationaryPlugin)
            .add_plugins(CellHashingPlugin::<With<TagA>>::new())
            .add_plugins(CellHashingPlugin::<With<TagB>>::new());

        let grid_entity = app.world_mut().spawn(BigSpaceRootBundle::default()).id();

        app.world_mut()
            .spawn((CellCoord::default(), FloatingOrigin))
            .set_parent_in_place(grid_entity);

        // Entity matches BOTH filters
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(Vec3::ZERO),
                CellCoord::new(1, 0, 0),
                TagA,
                TagB,
                Stationary,
            ))
            .set_parent_in_place(grid_entity)
            .id();

        app.update();

        let changed_a = app.world().resource::<ChangedCells<With<TagA>>>();
        assert!(
            changed_a.iter().any(|&e| e == entity),
            "Stationary entity must appear in ChangedCells<With<TagA>>"
        );

        let changed_b = app.world().resource::<ChangedCells<With<TagB>>>();
        assert!(
            changed_b.iter().any(|&e| e == entity),
            "Stationary entity must appear in ChangedCells<With<TagB>>"
        );
    }
}
