//! Change tracking for partitions. See [`PartitionChangePlugin`].

use crate::hash::component::CellHashMap;
use crate::hash::component::CellId;
use crate::hash::map::CellLookup;
use crate::hash::ChangedCells;
use crate::hash::SpatialHashFilter;
use crate::hash::SpatialHashSystems;
use crate::partition::map::PartitionLookup;
use crate::partition::PartitionId;
use bevy_app::prelude::*;
use bevy_ecs::entity::EntityHashMap;
use bevy_ecs::prelude::*;
use bevy_ecs::world::FromWorld;
use core::marker::PhantomData;

/// Adds support for spatial partitioning change tracking. Requires
/// [`GridHashPlugin`](crate::CellHashingPlugin) and [`PartitionPlugin`](crate::PartitionPlugin).
pub struct PartitionChangePlugin<F: SpatialHashFilter = ()>(PhantomData<F>);

impl<F: SpatialHashFilter> PartitionChangePlugin<F> {
    /// Create a new instance of [`PartitionChangePlugin`].
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
        app.init_resource::<PartitionEntities<F>>().add_systems(
            PostUpdate,
            PartitionEntities::<F>::update
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
pub struct PartitionEntities<F: SpatialHashFilter = ()> {
    /// Current mapping of an entity to its partition as of the last update.
    pub map: EntityHashMap<PartitionId>,
    /// Entities that have changed partition in the last update.
    pub changed: EntityHashMap<(Option<PartitionId>, Option<PartitionId>)>,
    spooky: PhantomData<F>,
}

impl<F: SpatialHashFilter> FromWorld for PartitionEntities<F> {
    fn from_world(_world: &mut World) -> Self {
        Self {
            map: Default::default(),
            changed: Default::default(),
            spooky: PhantomData,
        }
    }
}

impl<F: SpatialHashFilter> PartitionEntities<F> {
    fn update(
        mut this: ResMut<Self>,
        cells: Res<CellLookup<F>>,
        changed_cells: Res<ChangedCells<F>>,
        all_hashes: Query<(Entity, &CellId), F>,
        mut old_reverse: Local<CellHashMap<PartitionId>>,
        partitions: Res<PartitionLookup<F>>,
    ) {
        // 1. Clear the list of entities that have changed partitions
        this.changed.clear();

        // 2. Iterate through all entities that have moved between cells, using the already
        // optimized `ChangedCells` resource used for grid cells. Check these moved entities to see
        // if they have also changed partitions. This should also include spawned/despawned.
        for entity in changed_cells.iter() {
            match all_hashes.get(*entity) {
                Ok((entity_id, cell_hash)) => {
                    let new_pid = partitions.get(cell_hash).copied();
                    let old_pid = this.map.get(&entity_id).copied();
                    let partition_change = match (old_pid, new_pid) {
                        (Some(o), Some(n)) if o == n => None, // Partition unchanged
                        (None, None) => None,                 // Nonsensical
                        other => Some(other),                 // Valid change
                    };
                    if let Some((from, to)) = partition_change {
                        if let Some((existing_from, existing_to)) = this.changed.get_mut(entity) {
                            // Preserve the earliest known source if we already have one
                            if existing_from.is_none() {
                                *existing_from = from;
                            }
                            *existing_to = to;
                        } else {
                            this.changed.insert(*entity, (from, to));
                        }
                    }
                }
                // If the query fails, the entity no longer has a `CellId` because it was removed
                // or the entity was despawned.
                Err(_) => {
                    if let Some(prev_pid) = this.map.get(entity).copied() {
                        this.changed.insert(*entity, (Some(prev_pid), None));
                    }
                }
            }
        }

        // 3. Consider entities that have not moved, but their partition has changed out from
        // underneath them. This can happen when partitions merge and split - the entity did not
        // move but is now in a new partition.
        //
        // Check these changes at the cell level so we scale with the number of cells, not the
        // number of entities, and additionally avoid many lookups.
        //
        // It's important this is run after the moved-entity checks in step 2, because the entities
        // found from cell changes may *also* have moved, and we would miss that information if we
        // only used cell-level tracking.
        for (cell_id, entry) in cells.all_entries() {
            if let Some(&new_pid) = partitions.get(cell_id) {
                if let Some(&old_pid) = old_reverse.get(cell_id) {
                    if new_pid != old_pid {
                        for entity in entry.entities.iter().copied() {
                            this.changed
                                .entry(entity)
                                // Don't overwrite entities that moved cells, they have already been tracked.
                                .or_insert((Some(old_pid), Some(new_pid)));
                        }
                    }
                }
            }
        }

        // 4. Apply the changes to the entity partition map after all changes have been collected.
        let PartitionEntities { map, changed, .. } = this.as_mut();
        for (entity, (_source, destination)) in changed.iter() {
            match destination {
                Some(pid) => {
                    map.insert(*entity, *pid);
                }
                None => {
                    map.remove(entity);
                }
            };
        }

        // Snapshot the cell->partition mapping to compute deltas next update
        *old_reverse = partitions.reverse_map.clone();
    }
}
