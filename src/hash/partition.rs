//! Detect and update groups of nearby occupied cells.

use std::{hash::Hash, marker::PhantomData, ops::Deref, time::Instant};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};
use bevy_utils::{
    hashbrown::{HashMap, HashSet},
    PassHash,
};

use super::{GridHash, GridHashMap, GridHashMapFilter, GridHashMapSystem, GridPrecision};

/// Adds support for spatial partitioning. Requires [`GridHashPlugin`](super::GridHashPlugin).
pub struct GridPartitionPlugin<P, F = ()>(PhantomData<(P, F)>)
where
    P: GridPrecision,
    F: GridHashMapFilter;

impl<P, F> Default for GridPartitionPlugin<P, F>
where
    P: GridPrecision,
    F: GridHashMapFilter,
{
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<P, F> Plugin for GridPartitionPlugin<P, F>
where
    P: GridPrecision,
    F: GridHashMapFilter,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GridPartitionMap<P, F>>().add_systems(
            PostUpdate,
            GridPartitionMap::<P, F>::update
                .in_set(GridHashMapSystem::UpdatePartition)
                .after(GridHashMapSystem::UpdateMap),
        );
    }
}

/// Uniquely identifies a [`GridPartition`] in the [`GridPartitionMap`] resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPartitionId(u64);

impl GridPartitionId {
    /// The inner partition id.
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl Hash for GridPartitionId {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// Groups connected [`GridCell`](crate::GridCell)s into [`GridPartition`]s.
///
/// Partitions divide space into independent groups of cells.
///
/// The map depends on and is built from a corresponding [`GridHashMap`] with the same
/// `P:`[`GridPrecision`] and `F:`[`GridHashMapFilter`].
#[derive(Resource)]
pub struct GridPartitionMap<P, F = ()>
where
    P: GridPrecision,
    F: GridHashMapFilter,
{
    partitions: HashMap<GridPartitionId, GridPartition<P>>,
    reverse_map: HashMap<GridHash<P>, GridPartitionId, PassHash>,
    next_partition: u64,
    spooky: PhantomData<F>,
}

impl<P, F> Default for GridPartitionMap<P, F>
where
    P: GridPrecision,
    F: GridHashMapFilter,
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

impl<P, F> Deref for GridPartitionMap<P, F>
where
    P: GridPrecision,
    F: GridHashMapFilter,
{
    type Target = HashMap<GridPartitionId, GridPartition<P>>;

    fn deref(&self) -> &Self::Target {
        &self.partitions
    }
}

impl<P, F> GridPartitionMap<P, F>
where
    P: GridPrecision,
    F: GridHashMapFilter,
{
    /// Returns a reference to the [`GridPartition`], if it exists.
    #[inline]
    pub fn resolve(&self, id: &GridPartitionId) -> Option<&GridPartition<P>> {
        self.partitions.get(id)
    }

    /// Searches for the [`GridPartition`] that contains this `hash`, returning the partition's
    /// [`GridPartitionId`] if the hash is found in any partition.
    #[inline]
    pub fn get(&self, hash: &GridHash<P>) -> Option<&GridPartitionId> {
        self.reverse_map.get(hash)
    }

    /// Iterates over all [`GridPartition`]s.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&GridPartitionId, &GridPartition<P>)> {
        self.partitions.iter()
    }

    #[inline]
    fn insert(&mut self, partition: GridPartitionId, set: HashSet<GridHash<P>, PassHash>) {
        let Some(hash) = set.iter().next() else {
            return;
        };
        for hash in set.iter() {
            self.reverse_map.insert(*hash, partition);
        }
        self.partitions.insert(
            partition,
            GridPartition {
                grid: hash.grid(),
                tables: vec![set],
            },
        );
    }

    #[inline]
    fn push(&mut self, partition: &GridPartitionId, hash: &GridHash<P>) {
        if let Some(partition) = self.partitions.get_mut(partition) {
            partition.insert(*hash)
        } else {
            return;
        }
        self.reverse_map.insert(*hash, *partition);
    }

    #[inline]
    fn remove(&mut self, hash: &GridHash<P>) {
        let Some(old_id) = self.reverse_map.remove(hash) else {
            return;
        };
        if let Some(partition) = self.partitions.get_mut(&old_id) {
            partition.tables.iter_mut().any(|table| table.remove(hash));
        }
    }

    #[inline]
    fn take_next_id(&mut self) -> GridPartitionId {
        let id = GridPartitionId(self.next_partition);
        self.next_partition += 1;
        id
    }

    /// Merge the supplied set of partitions into a single partition.
    fn merge(&mut self, partitions: &[GridPartitionId]) {
        let Some(largest_partition) = partitions
            .iter()
            .filter_map(|id| {
                self.resolve(id)
                    .map(|partition| partition.num_cells())
                    .zip(Some(id))
            })
            .reduce(|acc, elem| if elem.0 > acc.0 { elem } else { acc })
            .map(|(_cells, id)| id)
        else {
            return;
        };

        for id in partitions.iter().filter(|p| *p != largest_partition) {
            let Some(partition) = self.partitions.remove(id) else {
                continue;
            };

            partition.iter().for_each(|hash| {
                self.reverse_map.insert(*hash, *largest_partition);
            });

            self.partitions
                .get_mut(largest_partition)
                .expect("partition should exist")
                .extend(partition);
        }
    }

    fn update(
        mut partition_map: ResMut<Self>,
        mut timing: ResMut<crate::timing::GridHashStats>,
        hash_grid: Res<GridHashMap<P, F>>,
        // Scratch space allocations
        mut added_neighbors: Local<Vec<GridPartitionId>>,
        mut adjacent_to_removals: Local<HashMap<GridPartitionId, HashSet<GridHash<P>, PassHash>>>,
        mut split_candidates: Local<Vec<(GridPartitionId, HashSet<GridHash<P>, PassHash>)>>,
        mut split_results: Local<Vec<Vec<SplitResult<P>>>>,
    ) {
        let start = Instant::now();
        for added_hash in hash_grid.just_inserted().iter() {
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
                added_hash
                    .adjacent(1)
                    .filter_map(|hash| partition_map.get(&hash)),
            );

            if let Some(first_partition) = added_neighbors.first() {
                // When the added cell is surrounded by other cells with at least one partition, add
                // the new cell to the first partition, then merge all adjacent partitions. Because
                // the added cell is the center, any neighboring cells are now connected through
                // this cell, thus their partitions are connected, and should be merged.
                partition_map.push(first_partition, added_hash);
                partition_map.merge(&added_neighbors);
            } else {
                let new_partition = partition_map.take_next_id();
                partition_map.insert(new_partition, [*added_hash].into_iter().collect());
            }
        }

        // Track the cells neighboring removed cells. These may now be disconnected from the rest of
        // their partition.
        for removed_cell in hash_grid.just_removed().iter() {
            partition_map.remove(removed_cell);
        }

        // Clean up empty tables and partitions
        partition_map.partitions.retain(|_id, partition| {
            partition.tables.retain(|table| !table.is_empty());
            !partition.tables.is_empty()
        });

        for removed_cell in hash_grid.just_removed().iter() {
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
                .filter(|hash| hash_grid.contains(hash))
                .filter_map(|hash| partition_map.get(&hash).zip(Some(hash)))
                .for_each(|(id, hash)| {
                    adjacent_to_removals.entry(*id).or_default().insert(hash);
                });
        }

        // Finally, we need to test for partitions being split apart by a removal (removing a bridge
        // in graph theory).
        *split_candidates = adjacent_to_removals.drain().collect::<Vec<_>>();
        *split_results = split_candidates.par_splat_map_mut(
            ComputeTaskPool::get(),
            None,
            |_, affected_cells| {
                let _task_span = tracing::info_span!("parallel partition split").entered();
                affected_cells
                    .iter_mut()
                    .filter_map(|(original_partition, adjacent_hashes)| {
                        let mut new_partitions = Vec::with_capacity(0);
                        let mut counter = 0;
                        while let Some(this_cell) = adjacent_hashes.iter().next().copied() {
                            for cell in hash_grid.flood(&this_cell, None) {
                                // Note: first visited cell is this_cell
                                adjacent_hashes.remove(&cell.0);
                                if adjacent_hashes.is_empty() {
                                    break;
                                }
                            }
                            // At this point, we have either visited all affected cells, or the
                            // flood fill ran out of cells to visit.
                            if adjacent_hashes.is_empty() && counter == 0 {
                                // If it only took a single iteration to connect all affected cells,
                                // it means the partition has not been split, and we can continue to
                                // the next // partition.
                                return None;
                            } else {
                                new_partitions
                                    .push(hash_grid.flood(&this_cell, None).map(|n| n.0).collect());
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
            if let Some(partition) = new_partitions.pop() {
                if let Some(tables) = partition_map
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
                let new_id = partition_map.take_next_id();
                partition_map.insert(new_id, partition_set);
            }
        }
        timing.update_partition += start.elapsed();
    }
}

struct SplitResult<P: GridPrecision> {
    original_partition: GridPartitionId,
    new_partitions: Vec<HashSet<GridHash<P>, PassHash>>,
}

/// A group of nearby [`GridCell`](crate::GridCell)s in an island disconnected from all other
/// [`GridCell`](crate::GridCell)s.
#[derive(Debug)]
pub struct GridPartition<P: GridPrecision> {
    grid: Entity,
    tables: Vec<HashSet<GridHash<P>, PassHash>>,
}
impl<P: GridPrecision> GridPartition<P> {
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
    pub fn contains(&self, hash: &GridHash<P>) -> bool {
        self.tables.iter().any(|table| table.contains(hash))
    }

    /// Iterates over all [`GridHash`]s in this partition.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &GridHash<P>> {
        self.tables.iter().flat_map(|table| table.iter())
    }

    /// Returns the total number of cells in this partition.
    #[inline]
    pub fn num_cells(&self) -> usize {
        self.tables.iter().map(|t| t.len()).sum()
    }

    #[inline]
    fn insert(&mut self, cell: GridHash<P>) {
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
    fn extend(&mut self, mut partition: GridPartition<P>) {
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

    /// The grid this partition resides in.
    pub fn grid(&self) -> Entity {
        self.grid
    }
}
