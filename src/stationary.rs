//! Components and systems for optimizing stationary entities.
//!
//! See [`Stationary`], [`BigSpaceStationaryPlugin`].

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::{change_detection::Tick, prelude::*, system::SystemChangeTick};
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;

/// A component that optimizes entities that do not move.
///
/// When an entity is marked as stationary, the plugin will skip most per-frame computations for it.
/// This includes grid recentering and spatial hashing updates. The `CellCoord` and `CellId`
/// will only be computed when the entity is spawned or when its parent changes.
#[derive(Debug, Clone, Reflect, Component, Default)]
#[reflect(Component, Default)]
pub struct Stationary;

/// Internal marker component used to identify [`Stationary`] entities that have had their initial
/// [`GlobalTransform`] computed.
///
/// Inserted by [`BigSpaceStationaryPlugin`] (via [`mark_stationary_computed`]) after the first
/// frame's [`Grid::propagate_high_precision`] run. When present, [`Grid::traverse_grid`] skips
/// recomputing the [`GlobalTransform`] for this entity unless the floating origin moves.
///
/// Also inserted by [`CellHashingPlugin`](crate::hash::CellHashingPlugin) after the first
/// spatial hash computation, so both plugins can be used independently without conflict.
#[derive(Debug, Clone, Reflect, Component, Default)]
#[reflect(Component, Default)]
pub struct StationaryComputed;

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

/// Inserts [`StationaryComputed`] on [`Stationary`] entities at the end of the frame.
///
/// Runs in [`Last`] to guarantee every [`PostUpdate`] system (including spatial hashing) has
/// had one full frame to observe entities with [`Stationary`] but without [`StationaryComputed`].
/// Placing this in [`Last`] avoids the Bevy auto-`apply_deferred` that would otherwise be
/// inserted mid-[`PostUpdate`] between this system (Commands writer) and any system that
/// filters `Without<StationaryComputed>`.
fn mark_stationary_computed(
    mut commands: Commands,
    uninitialized: Query<Entity, (With<Stationary>, Without<StationaryComputed>)>,
) {
    for entity in uninitialized.iter() {
        commands.entity(entity).insert(StationaryComputed);
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
/// This plugin also registers reflection for [`Stationary`], [`StationaryComputed`], and
/// [`GridDirtyTick`].
///
/// # Note
///
/// This plugin is included in [`BigSpaceDefaultPlugins`] but **not** in
/// [`BigSpaceMinimalPlugins`]. Add it manually alongside [`BigSpaceMinimalPlugins`] when you
/// want the optimization without the full default plugin set.
pub struct BigSpaceStationaryPlugin;

impl Plugin for BigSpaceStationaryPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Stationary>()
            .register_type::<StationaryComputed>()
            .register_type::<GridDirtyTick>();

        let dirty_configs = || {
            mark_dirty_subtrees
                .in_set(BigSpaceSystems::PropagateHighPrecision)
                .before(Grid::propagate_high_precision)
                .after(BigSpaceSystems::LocalFloatingOrigins)
        };
        // mark_stationary_computed runs in Last (not PostUpdate) so it cannot trigger
        // Bevy's auto-apply_deferred before any PostUpdate system that filters
        // Without<StationaryComputed> (e.g. CellId::initialize_stationary).
        app.add_systems(PostUpdate, dirty_configs())
            .add_systems(PostStartup, dirty_configs())
            .add_systems(Last, mark_stationary_computed);
    }
}
