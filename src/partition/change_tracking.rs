//! Change tracking for partitions. See [`PartitionChangePlugin`].

use crate::hash::component::CellHashMap;
use crate::hash::component::CellId;
use crate::hash::map::CellLookup;
use crate::hash::ChangedCells;
use crate::hash::SpatialHashFilter;
use crate::hash::SpatialHashSystems;
use crate::partition::map::PartitionLookup;
use crate::partition::PartitionId;
use alloc::vec::Vec;
use bevy_app::prelude::*;
use bevy_ecs::entity::EntityHashMap;
use bevy_ecs::prelude::*;
use bevy_ecs::world::FromWorld;
use core::marker::PhantomData;

/// Adds support for spatial partitioning change tracking. Requires
/// [`GridHashPlugin`](super::CellHashingPlugin) and [`PartitionPlugin`](super::PartitionPlugin).
pub struct PartitionChangePlugin<F: SpatialHashFilter = ()>(PhantomData<F>);

impl<F: SpatialHashFilter> PartitionChangePlugin<F> {
    /// Create a new instance of [`crate::prelude::PartitionChangePlugin`].
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl Default for PartitionChangePlugin<()> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F: SpatialHashFilter> Plugin for PartitionChangePlugin<F> {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartitionChange<F>>().add_systems(
            PostUpdate,
            PartitionChange::<F>::update
                .in_set(SpatialHashSystems::UpdatePartitionChange)
                .after(SpatialHashSystems::UpdatePartitionLookup),
        );
    }
}

/// Tracks which partition an entity belongs to, and which entities changed partitions this frame.
/// Updated in [`SpatialHashSystems::UpdatePartitionChange`] in [`PostUpdate`].
///
/// This only works if the [`PartitionChangePlugin`] has been added.
#[derive(Resource)]
pub struct PartitionChange<F: SpatialHashFilter = ()> {
    /// Current mapping of an entity to its partition as of the last update.
    pub map: EntityHashMap<PartitionId>,
    /// Entities that have changed partition in the last update.
    pub changed: EntityHashMap<(PartitionId, PartitionId)>,
    spooky: PhantomData<F>,
}

impl<F: SpatialHashFilter> FromWorld for PartitionChange<F> {
    fn from_world(_world: &mut World) -> Self {
        Self {
            map: Default::default(),
            changed: Default::default(),
            spooky: PhantomData,
        }
    }
}

impl<F: SpatialHashFilter> PartitionChange<F> {
    fn update(
        mut entity_partitions: ResMut<Self>,
        cells: Res<CellLookup<F>>,
        changed_cells: Res<ChangedCells<F>>,
        all_hashes: Query<(Entity, &CellId), F>,
        mut old_reverse: Local<CellHashMap<PartitionId>>,
        partitions: Res<PartitionLookup>,
    ) {
        entity_partitions.changed.clear();

        // Compute cell-level partition changes: cells that remained occupied but changed partitions.
        for (cell_hash, entry) in cells.all_entries() {
            if let (Some(old_pid), Some(new_pid)) =
                (old_reverse.get(cell_hash), partitions.get(cell_hash))
            {
                if old_pid != new_pid {
                    // All entities in this cell have changed partition without moving cells.
                    for entity in entry.entities.iter().copied() {
                        // Preserve the original source if already present; update only destination.
                        if let Some((_from, to)) = entity_partitions.changed.get_mut(&entity) {
                            // Keep existing source, just update destination
                            *to = *new_pid;
                        } else {
                            entity_partitions
                                .changed
                                .insert(entity, (*old_pid, *new_pid));
                        }
                    }
                }
            }
        }

        // Compute entity-level partition changes for entities that moved cells this frame.
        let get_old_new_pids =
            |entity: &Entity, changed: &PartitionChange<F>| -> Option<(PartitionId, PartitionId)> {
                let (entity_id, cell_hash) = all_hashes.get(*entity).ok()?;
                let new_pid = *partitions.get(cell_hash)?;
                let old_pid = changed.map.get(&entity_id).copied()?;
                (old_pid != new_pid).then_some((old_pid, new_pid))
            };
        for entity in changed_cells.iter() {
            let Some((prev_pid, new_pid)) = get_old_new_pids(entity, &entity_partitions) else {
                continue;
            };
            if let Some((_, existing_new_pid)) = entity_partitions.changed.get_mut(entity) {
                *existing_new_pid = new_pid; // Only update the destination partition
            } else {
                entity_partitions
                    .changed
                    .insert(*entity, (prev_pid, new_pid));
            }
        }

        // Apply the delta only after all changes have been collected.
        let mut apply_list: Vec<(Entity, PartitionId)> =
            Vec::with_capacity(entity_partitions.changed.len());
        for (entity, (_from, to)) in entity_partitions.changed.iter() {
            apply_list.push((*entity, *to));
        }
        for (entity, to) in apply_list {
            entity_partitions.map.insert(entity, to);
        }
        // Entities present but not in the map yet are inserted with their current partition.
        for (entity_id, cell_hash) in all_hashes.iter() {
            if !entity_partitions.map.contains_key(&entity_id) {
                if let Some(pid) = partitions.get(cell_hash) {
                    entity_partitions.map.insert(entity_id, *pid);
                }
            }
        }

        // Snapshot the cell->partition mapping to compute deltas next update
        *old_reverse = partitions.reverse_map.clone();
    }
}
