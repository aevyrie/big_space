//! [`PartitionLookup`] map implementation for associating partitions and cells.

use crate::hash::component::CellHashMap;
use crate::hash::component::CellHashSet;
use crate::hash::component::CellId;
use crate::hash::map::CellLookup;
use crate::hash::SpatialHashFilter;
use crate::partition::{Partition, PartitionId};
use alloc::vec;
use alloc::vec::Vec;
use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use bevy_platform::hash::PassHash;
use bevy_platform::time::Instant;
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};
use core::marker::PhantomData;
use core::ops::Deref;

/// A resource for quickly finding connected groups of occupied grid cells in [`Partition`]s.
///
/// Partitions divide space into independent groups of cells.
///
/// The map is built from a [`CellLookup`] resource with the same `F:`[`SpatialHashFilter`].
///
/// Partitions are built on top of [`CellLookup`], only dealing with
/// [`CellCoord`](crate::grid::cell::CellCoord)s. For performance reasons, partitions do not track
/// grid occupancy at the `Entity` level. Instead, partitions are only concerned with which cells
/// are occupied. To find what entities are present, you will need to look up each of the
/// partition's [`CellId`]s in the [`CellLookup`]
#[derive(Resource)]
pub struct PartitionLookup<F = ()>
where
    F: SpatialHashFilter,
{
    partitions: HashMap<PartitionId, Partition, PassHash>,
    pub(crate) reverse_map: CellHashMap<PartitionId>,
    next_partition: u64,
    spooky: PhantomData<F>,
}

impl<F> Default for PartitionLookup<F>
where
    F: SpatialHashFilter,
{
    fn default() -> Self {
        Self {
            partitions: HashMap::default(),
            reverse_map: HashMap::default(),
            next_partition: 0,
            spooky: PhantomData,
        }
    }
}

impl<F> Deref for PartitionLookup<F>
where
    F: SpatialHashFilter,
{
    type Target = HashMap<PartitionId, Partition, PassHash>;

    fn deref(&self) -> &Self::Target {
        &self.partitions
    }
}

impl<F> PartitionLookup<F>
where
    F: SpatialHashFilter,
{
    /// Returns a reference to the [`Partition`] if it exists.
    #[inline]
    pub fn resolve(&self, id: &PartitionId) -> Option<&Partition> {
        self.partitions.get(id)
    }

    /// Searches for the [`Partition`] that contains this cell, returning the partition's
    /// [`PartitionId`] if the cell is found in any partition.
    #[inline]
    pub fn get(&self, hash: &CellId) -> Option<&PartitionId> {
        self.reverse_map.get(hash)
    }

    /// Iterates over all [`Partition`]s.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&PartitionId, &Partition)> {
        self.partitions.iter()
    }

    /// Searches for the [`Partition`] that contains this cell, returning the partition's
    /// [`PartitionId`] if the cell is found in any partition.
    #[inline]
    pub fn get_partition(&self, partition: &PartitionId) -> Option<&Partition> {
        self.partitions.get(partition)
    }
}

/// Private methods
impl<F> PartitionLookup<F>
where
    F: SpatialHashFilter,
{
    /// Inserts a partition into the map, replacing existing data; if the provided `set` is empty,
    /// the partition will be removed from the map. In either case, the previous value will be
    /// returned.
    #[inline]
    fn insert(&mut self, partition: PartitionId, set: CellHashSet) -> Option<Partition> {
        let Some(hash) = set.iter().next() else {
            // The set is empty. We will remove the partition entirely.
            return self.partitions.remove(&partition);
        };
        let mut min = hash.coord();
        let mut max = hash.coord();
        for hash in set.iter() {
            self.reverse_map.insert(*hash, partition);
            min = min.min(hash.coord());
            max = max.max(hash.coord());
        }
        self.partitions
            .insert(partition, Partition::new(hash.grid(), vec![set], min, max))
    }

    /// Add a cell to the partition.
    #[inline]
    fn push(&mut self, partition: &PartitionId, cell: &CellId) {
        if let Some(partition) = self.partitions.get_mut(partition) {
            partition.insert(*cell);
        } else {
            return;
        }
        self.reverse_map.insert(*cell, *partition);
    }

    /// Remove a cell from the partition.
    #[inline]
    fn remove(&mut self, cell: &CellId) {
        let Some(old_id) = self.reverse_map.remove(cell) else {
            return;
        };
        let mut empty = false;
        if let Some(partition) = self.partitions.get_mut(&old_id) {
            if partition.remove(cell) && partition.is_empty() {
                empty = true;
            }
        }
        if empty {
            self.partitions.remove(&old_id);
        }
    }

    /// Get the next available partition ID.
    #[inline]
    fn take_next_id(&mut self) -> PartitionId {
        let id = PartitionId(self.next_partition);
        self.next_partition += 1;
        id
    }

    /// Merge the supplied set of partitions into a single partition.
    fn merge(&mut self, partitions: &[PartitionId]) {
        let Some(largest_partition) = partitions
            .iter()
            .filter_map(|id| self.resolve(id).map(Partition::num_cells).zip(Some(id)))
            .reduce(|acc, elem| if elem.0 > acc.0 { elem } else { acc })
            .map(|(_cells, id)| id)
        else {
            return;
        };

        for id in partitions.iter().filter(|p| *p != largest_partition) {
            let Some(partition) = self.partitions.remove(id) else {
                continue;
            };

            partition.iter().for_each(|cell_guid| {
                self.reverse_map.insert(*cell_guid, *largest_partition);
            });

            self.partitions
                .get_mut(largest_partition)
                .expect("partition should exist")
                .extend(partition);
        }
    }

    pub(super) fn update(
        mut partitions: ResMut<Self>,
        mut timing: Option<ResMut<crate::timing::GridHashStats>>,
        cells: Res<CellLookup<F>>,
        // Scratch space allocations
        mut added_neighbors: Local<Vec<PartitionId>>,
        mut split_candidates_map: Local<HashMap<PartitionId, CellHashSet, PassHash>>,
        mut split_candidates: Local<Vec<(PartitionId, CellHashSet)>>,
        mut split_results: Local<Vec<Vec<SplitResult>>>,
    ) {
        let start = Instant::now();

        for newly_occupied in cells.newly_occupied().iter() {
            added_neighbors.clear();
            added_neighbors.extend(
                // This intentionally checks the partition map which is out of date, not the spatial
                // hash map. Consider the case of a single entity moving through space, between
                // cells. If we used the spatial hash map's `occupied_neighbors` on the added cell
                // position, it would return no results, the old partition would be removed, and a
                // new one created. As the entity moves through space, it is constantly reassigned a
                // new partition.
                //
                // By using the partition map, we will be able to see the previously occupied cell
                // before it is removed, merge with that partition, then remove it later.
                newly_occupied
                    .adjacent(1)
                    .filter_map(|hash| partitions.get(&hash)),
            );

            if let Some(first_partition) = added_neighbors.first() {
                // When the added cell is surrounded by other cells with at least one partition, add
                // the new cell to the first partition, then merge all adjacent partitions. Because
                // the added cell is the center, any neighboring cells are now connected through
                // this cell, thus their partitions are connected and should be merged.
                partitions.push(first_partition, newly_occupied);
                partitions.merge(&added_neighbors);
            } else {
                let new_id = partitions.take_next_id();
                partitions.insert(new_id, [*newly_occupied].into_iter().collect());
            }
        }

        // Track the cells neighboring removed cells. These may now be disconnected from the rest of
        // their partition.
        for removed_cell in cells.newly_emptied().iter() {
            partitions.remove(removed_cell);
        }

        for removed_cell in cells.newly_emptied().iter() {
            // Group occupied neighbor cells by partition, so we can check if they are still
            // connected to each other after this removal.
            //
            // Note that this will only add values that exist in the map, which has already had
            // cells added and removed, and the partition, which has just been updated with added
            // cells.
            //
            // Unfortunately, it doesn't seem possible to do any early-out optimizations based on
            // the local neighborhood, because we don't have a full picture of the end state yet.
            // This is why we need to gather all potentially affected cells and check for partition
            // splits once everything else has been added/removed.
            //
            // IMPORTANT: this is *intentionally* run in a second iterator after removing cells from
            // the partitions. This ensures that when we check the partitions for affected cells, we
            // aren't adding cells that were just removed but not yet processed.
            removed_cell
                .adjacent(1)
                .filter(|cell_guid| cells.contains(cell_guid))
                .filter_map(|cell_guid| partitions.get(&cell_guid).zip(Some(cell_guid)))
                .for_each(|(partition_id, cell_guid)| {
                    split_candidates_map
                        .entry(*partition_id)
                        .or_default()
                        .insert(cell_guid);
                });
        }

        // Finally, we need to check for partitions being split apart by a removal (removing a
        // bridge in graph theory).
        split_candidates.clear();
        split_candidates.extend(split_candidates_map.drain());
        *split_results = split_candidates.par_splat_map_mut(
            ComputeTaskPool::get(),
            None,
            |_index, split_candidates| {
                let _task_span = bevy_log::info_span!("parallel partition split").entered();
                split_candidates
                    .iter_mut()
                    .filter_map(|(id, adjacent_hashes)| {
                        let mut new_partitions = Vec::with_capacity(0);
                        let mut counter = 0;
                        while let Some(this_cell) = adjacent_hashes.iter().next().copied() {
                            for neighbor_cell in cells.flood(&this_cell, None) {
                                // Note: the first visited cell is this_cell
                                adjacent_hashes.remove(&neighbor_cell.0);
                                if adjacent_hashes.is_empty() {
                                    break;
                                }
                            }
                            // At this point, we have either visited all affected cells, or the
                            // flood fill ran out of cells to visit.
                            if adjacent_hashes.is_empty() && counter == 0 {
                                // If it only took a single iteration to connect all affected cells,
                                // it means the partition has not been split, and we can continue to
                                // the next partition.
                                return None;
                            }
                            new_partitions
                                .push(cells.flood(&this_cell, None).map(|n| n.0).collect());

                            counter += 1;
                        }

                        Some(SplitResult {
                            original_partition_id: *id,
                            new_partitions,
                        })
                    })
                    .collect::<Vec<_>>()
            },
        );

        for SplitResult {
            original_partition_id,
            ref mut new_partitions,
        } in split_results.iter_mut().flatten()
        {
            // We want the original partition to retain the most cells to ensure that the smaller
            // sets are the ones that are assigned a new partition ID.
            new_partitions.sort_unstable_by_key(CellHashSet::len);
            if let Some(largest_partition) = new_partitions.pop() {
                partitions.insert(*original_partition_id, largest_partition);
            }

            // At this point the reverse map will be out of date. However, `partitions.insert()`
            // will update all hashes that now have a new partition with their new ID.
            for partition_set in new_partitions.drain(..) {
                let new_id = partitions.take_next_id();
                partitions.insert(new_id, partition_set);
            }
        }

        if let Some(ref mut timing) = timing {
            timing.update_partition += start.elapsed();
        }
    }
}

pub(super) struct SplitResult {
    original_partition_id: PartitionId,
    new_partitions: Vec<CellHashSet>,
}
