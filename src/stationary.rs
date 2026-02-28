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
/// grid cell and spatial hash computed.
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
) {
    // Auto-insert on any Grid that doesn't have GridDirtyTick yet.
    // Commands are deferred, so newly inserted grids treat themselves as dirty (correct for
    // first GT initialization).
    for entity in grids_without.iter() {
        commands.entity(entity).insert(GridDirtyTick::default());
    }

    let current_tick = system_ticks.this_run().get();

    for parent_rel in changed.iter() {
        let mut ancestor = parent_rel.parent();
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
/// This plugin is **not** included in [`BigSpaceMinimalPlugins`] or [`BigSpaceDefaultPlugins`].
/// Add it manually when you want the optimization.
pub struct BigSpaceStationaryPlugin;

impl Plugin for BigSpaceStationaryPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Stationary>()
            .register_type::<StationaryComputed>()
            .register_type::<GridDirtyTick>();

        let configs = || {
            mark_dirty_subtrees
                .in_set(BigSpaceSystems::PropagateHighPrecision)
                .before(Grid::propagate_high_precision)
                .after(BigSpaceSystems::LocalFloatingOrigins)
        };
        app.add_systems(PostUpdate, configs())
            .add_systems(PostStartup, configs());
    }
}
