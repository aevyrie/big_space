//! Detect and update groups of nearby occupied cells.

use std::{hash::Hash, marker::PhantomData};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};
use bevy_utils::{
    hashbrown::{HashMap, HashSet},
    PassHash,
};

use super::{GridPrecision, SpatialHash, SpatialHashFilter, SpatialHashMap, SpatialSystem};

/// Adds support for spatial partitioning. Requires [`SpatialHashPlugin`](super::SpatialHashPlugin).
pub struct SpatialPartitionPlugin<P, F = ()>(PhantomData<(P, F)>)
where
    P: GridPrecision,
    F: SpatialHashFilter;

impl<P, F> Default for SpatialPartitionPlugin<P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<P, F> Plugin for SpatialPartitionPlugin<P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialPartitionMap<P, F>>()
            .add_systems(
                PostUpdate,
                SpatialPartitionMap::<P, F>::update
                    .in_set(SpatialSystem::UpdatePartition)
                    .after(SpatialSystem::UpdateMap),
            );
    }
}

/// Uniquely identifies a [`SpatialPartition`] in the [`SpatialPartitionMap`] resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialPartitionId(u64);

impl Hash for SpatialPartitionId {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// Global map of all [`SpatialPartition`]s. The map depends on and is built from a corresponding
/// [`SpatialHashMap`] with the same [`GridPrecision`] and [`SpatialHashFilter`].
#[derive(Resource)]
pub struct SpatialPartitionMap<P, F = ()>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    partitions: HashMap<SpatialPartitionId, SpatialPartition<P>>,
    reverse_map: HashMap<SpatialHash<P>, SpatialPartitionId, PassHash>,
    next_partition: u64,
    spooky: PhantomData<F>,
}

impl<P, F> Default for SpatialPartitionMap<P, F>
where
    P: GridPrecision,
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

impl<P, F> SpatialPartitionMap<P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    /// Returns a reference to the [`SpatialPartition`], if it exists.
    #[inline]
    pub fn get(&self, partition: &SpatialPartitionId) -> Option<&SpatialPartition<P>> {
        self.partitions.get(partition)
    }

    /// Searches for the [`SpatialPartition`] that contains this `hash`, returning the partition's
    /// [`SpatialPartitionId`] if the hash is found in any partition.
    #[inline]
    pub fn find(&self, hash: &SpatialHash<P>) -> Option<&SpatialPartitionId> {
        self.reverse_map.get(hash)
    }

    /// Iterates over all [`SpatialPartition`]s.
    pub fn partitions(&self) -> impl Iterator<Item = &SpatialPartition<P>> {
        self.partitions.values()
    }

    #[inline]
    fn insert(&mut self, partition: SpatialPartitionId, set: HashSet<SpatialHash<P>, PassHash>) {
        for hash in set.iter() {
            self.reverse_map.insert(*hash, partition);
        }
        self.partitions
            .insert(partition, SpatialPartition { tables: vec![set] });
    }

    #[inline]
    fn push(&mut self, partition: &SpatialPartitionId, hash: &SpatialHash<P>) {
        if let Some(partition) = self.partitions.get_mut(partition) {
            partition.insert(*hash)
        }
        self.reverse_map.insert(*hash, *partition);
    }

    #[inline]
    fn remove(&mut self, hash: &SpatialHash<P>) {
        let Some(old_id) = self.reverse_map.remove(hash) else {
            return;
        };
        if let Some(partition) = self.partitions.get_mut(&old_id) {
            partition.tables.iter_mut().any(|table| table.remove(hash));
        }
    }

    fn create(&mut self) -> SpatialPartitionId {
        let id = SpatialPartitionId(self.next_partition);
        self.partitions.insert(
            id,
            SpatialPartition {
                tables: Vec::default(),
            },
        );
        self.next_partition += 1;
        id
    }

    /// Merge the supplied set of partitions into a single partition.
    fn merge<'a>(&mut self, mut partitions: impl Iterator<Item = &'a SpatialPartitionId>) {
        let Some(first_partition) = partitions.find(|partition| self.get(partition).is_some())
        else {
            return;
        };

        for id in partitions.filter(|p| *p != first_partition) {
            let Some(partition) = self.partitions.remove(id) else {
                continue;
            };

            partition.iter().for_each(|hash| {
                self.reverse_map.insert(*hash, *first_partition);
            });

            self.partitions
                .get_mut(first_partition)
                .expect("partition should exist")
                .extend(partition);
        }
    }

    fn update(
        mut partitions: ResMut<Self>,
        map: Res<SpatialHashMap<P, F>>,
        // Scratch space allocations
        mut scratch_partitions: Local<Vec<SpatialPartitionId>>,
        mut affected_cells: Local<HashMap<SpatialPartitionId, HashSet<SpatialHash<P>, PassHash>>>,
        mut affected_cells_vec: Local<Vec<(SpatialPartitionId, HashSet<SpatialHash<P>, PassHash>)>>,
        mut split_results: Local<Vec<Vec<SplitResult<P>>>>,
    ) {
        for (added_cell, added_hash) in map
            .just_inserted()
            .iter()
            .filter_map(|cell| map.get(cell).zip(Some(cell)))
        {
            scratch_partitions.clear();
            scratch_partitions.extend(
                added_cell
                    .occupied_neighbors
                    .iter()
                    .filter_map(|neighbor| partitions.find(neighbor)),
            );

            if let Some(first_partition) = scratch_partitions.first() {
                // When the added cell is surrounded by other cells with at least one partition, add
                // the new cell to the first partition, then merge all adjacent partitions. Because
                // the added cell is the center, any neighboring cells are now connected through
                // this cell, thus their partitions are connected, and should be merged.
                partitions.push(first_partition, added_hash);
                partitions.merge(scratch_partitions.iter());
            } else {
                let new_partition = partitions.create();
                partitions.push(&new_partition, added_hash);
            }
        }

        // Track the cells neighboring removed cells. These may now be disconnected from the rest of
        // their partition.
        for removed_cell in map.just_removed().iter() {
            partitions.remove(removed_cell);
        }

        // Clean up empty tables and partitions
        partitions.partitions.retain(|_id, partition| {
            partition.tables.retain(|table| !table.is_empty());
            !partition.tables.is_empty()
        });

        for removed_cell in map.just_removed().iter() {
            // Group occupied neighbor cells by partition, so we can check if they are still
            // connected to each other after this removal.
            //
            // Note that this will only add values that exist in the map, which has already had
            // cells added and removed, and the partition, which has just been updated with added
            // cells.
            //
            // Unfortunately, it doesn't seem possible to do any early-out optimizations based on
            // the local neighborhood, because we don't have a full picture of the end state yet.
            // This is why we need to gather all potentially affected cells, and check for partition
            // splits once everything else has been added/removed.
            //
            // IMPORTANT: this is *intentionally* run in a second iterator after removing cells from
            // the partitions. This ensures that when we check the partitions for affected cells, we
            // aren't adding cells that were just removed but not yet processed.
            removed_cell
                .adjacent(1)
                .filter(|(hash, _)| map.contains(hash))
                .filter_map(|(hash, _)| partitions.find(&hash).zip(Some(hash)))
                .for_each(|(id, hash)| {
                    affected_cells.entry(*id).or_default().insert(hash);
                });
        }

        // Finally, we need to test for partitions being split apart by a removal (removing a bridge
        // in graph theory).
        *affected_cells_vec = affected_cells.drain().collect::<Vec<_>>();
        *split_results = affected_cells_vec.par_splat_map_mut(
            ComputeTaskPool::get(),
            None,
            |_, affected_cells| {
                let _task_span = tracing::info_span!("parallel partition split").entered();
                affected_cells
                    .iter_mut()
                    .filter_map(|(original_partition, affected_hashes)| {
                        let mut new_partitions = Vec::with_capacity(0);
                        let mut counter = 0;
                        while let Some(this_cell) = affected_hashes.iter().next().copied() {
                            for cell in map.flood(&this_cell, None) {
                                // Note: first visited cell is this_cell
                                affected_hashes.remove(&cell.0);
                                if affected_hashes.is_empty() {
                                    break;
                                }
                            }
                            // At this point, we have either visited all affected cells, or the
                            // flood fill ran out of cells to visit.
                            if affected_hashes.is_empty() && counter == 0 {
                                // If it only took a single iteration to connect all affected cells,
                                // it means the partition has not been split, and we can continue to
                                // the next // partition.
                                return None;
                            } else {
                                new_partitions
                                    .push(map.flood(&this_cell, None).map(|n| n.0).collect());
                            }
                            counter += 1;
                        }

                        Some(SplitResult {
                            original_partition: *original_partition,
                            new_partitions,
                        })
                    })
                    .collect::<Vec<_>>()
            },
        );

        for SplitResult {
            original_partition,
            ref mut new_partitions,
        } in split_results.iter_mut().flatten()
        {
            // We want the original partition to retain the most cells to ensure that the smaller
            // sets are the ones that are assigned a new partition ID.
            new_partitions.sort_unstable_by_key(|v| v.len());
            new_partitions.reverse();
            if let Some(partition) = new_partitions.pop() {
                if let Some(tables) = partitions
                    .partitions
                    .get_mut(original_partition)
                    .map(|p| &mut p.tables)
                {
                    // TODO: keep these in an object pool to reuse allocs
                    tables.drain(1..);
                    if let Some(table) = tables.get_mut(0) {
                        *table = partition;
                    } else {
                        tables.push(partition);
                    }
                }
            }

            // At this point the reverse map will be out of date. However, `partitions.insert()`
            // will update all hashes that now have a new partition, with their new ID.
            for partition_set in new_partitions.drain(..) {
                let new_id = partitions.create();
                partitions.insert(new_id, partition_set);
            }
        }
    }
}

struct SplitResult<P: GridPrecision> {
    original_partition: SpatialPartitionId,
    new_partitions: Vec<HashSet<SpatialHash<P>, PassHash>>,
}

/// A set of [`crate::GridCell`]s in an island disconnected from all other [`crate::GridCell`]s.
#[derive(Debug)]
pub struct SpatialPartition<P: GridPrecision> {
    tables: Vec<HashSet<SpatialHash<P>, PassHash>>,
}
impl<P: GridPrecision> SpatialPartition<P> {
    /// Tables smaller than this will be drained into other tables when merging. Tables larger than
    /// this limit will instead be added to a list of tables. This prevents partitions ending up
    /// with many tables containing a few entries.
    ///
    /// Draining and extending a hash set is much slower than moving the entire hash set into a
    /// list. The tradeoff is that the more tables added, the more there are that need to be
    /// iterated over when searching for a cell.
    const MIN_TABLE_SIZE: usize = 128;

    /// Returns `true` if the `hash` is in this partition.
    #[inline]
    pub fn contains(&self, hash: &SpatialHash<P>) -> bool {
        self.tables.iter().any(|table| table.contains(hash))
    }

    /// Iterates over all [`SpatialHash`]s in this partition.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &SpatialHash<P>> {
        self.tables.iter().flat_map(|table| table.iter())
    }

    #[inline]
    fn insert(&mut self, cell: SpatialHash<P>) {
        if self.contains(&cell) {
            return;
        }
        if let Some(i) = self.smallest_table() {
            self.tables[i].insert(cell);
        } else {
            let mut table = HashSet::default();
            table.insert(cell);
            self.tables.push(table);
        }
    }

    #[inline]
    fn smallest_table(&self) -> Option<usize> {
        self.tables
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.len()))
            .min_by_key(|(_, len)| *len)
            .map(|(i, _len)| i)
    }

    #[inline]
    fn extend(&mut self, mut partition: SpatialPartition<P>) {
        for mut table in partition.tables.drain(..) {
            if table.len() < Self::MIN_TABLE_SIZE {
                if let Some(i) = self.smallest_table() {
                    self.tables[i].extend(table.drain());
                } else {
                    self.tables.push(table);
                }
            } else {
                self.tables.push(table);
            }
        }
    }
}
