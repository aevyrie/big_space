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
/// [`GridHashPlugin`](crate::CellHashingPlugin) and [`PartitionPlugin`](crate::PartitionPlugin).
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
    pub changed: EntityHashMap<(Option<PartitionId>, Option<PartitionId>)>,
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
            if let Some(&new_pid) = partitions.get(cell_hash) {
                // Only mark entities whose previous recorded partition differs from the new one
                for entity in entry.entities.iter().copied() {
                    if let Some(prev_pid) = entity_partitions.map.get(&entity).copied() {
                        if prev_pid != new_pid {
                            if let Some((_from, to)) = entity_partitions.changed.get_mut(&entity) {
                                *to = Some(new_pid);
                            } else {
                                entity_partitions
                                    .changed
                                    .insert(entity, (Some(prev_pid), Some(new_pid)));
                            }
                        }
                    }
                }
            }
        }

        // Compute entity-level partition changes for entities that moved cells this frame,
        // were newly spawned (added cell), or had cell removed (despawned/removed component).
        for entity in changed_cells.iter() {
            match all_hashes.get(*entity) {
                // Entity currently has a CellId
                Ok((entity_id, cell_hash)) => {
                    let new_pid = partitions.get(cell_hash).copied();
                    let old_pid = entity_partitions.map.get(&entity_id).copied();
                    let record = match (old_pid, new_pid) {
                        (Some(o), Some(n)) if o == n => None, // Partition unchanged
                        (None, None) => None,                 // Nonsensical
                        other => Some(other),
                    };
                    if let Some((from, to)) = record {
                        if let Some((existing_from, existing_to)) =
                            entity_partitions.changed.get_mut(entity)
                        {
                            // Preserve the earliest known source if we already have one
                            if existing_from.is_none() {
                                *existing_from = from;
                            }
                            *existing_to = to;
                        } else {
                            entity_partitions.changed.insert(*entity, (from, to));
                        }
                    }
                }
                // Entity no longer has a CellId -> removed/despawned
                Err(_) => {
                    if let Some(prev_pid) = entity_partitions.map.get(entity).copied() {
                        entity_partitions
                            .changed
                            .insert(*entity, (Some(prev_pid), None));
                    }
                }
            }
        }

        // Apply the delta only after all changes have been collected.
        let mut apply_list: Vec<(Entity, Option<PartitionId>)> =
            Vec::with_capacity(entity_partitions.changed.len());
        for (entity, (_from, to)) in entity_partitions.changed.iter() {
            apply_list.push((*entity, *to));
        }
        for (entity, to) in apply_list {
            match to {
                Some(pid) => {
                    entity_partitions.map.insert(entity, pid);
                }
                None => {
                    entity_partitions.map.remove(&entity);
                }
            }
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
