//! Detect and update groups of nearby occupied cells.

use core::{hash::Hash, marker::PhantomData, ops::Deref};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_platform_support::prelude::*;
use bevy_platform_support::{
    collections::{HashMap, HashSet},
    hash::PassHash,
    time::Instant,
};
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};

use super::{GridCell, GridHash, GridHashMap, GridHashMapFilter, GridHashMapSystem};

pub use private::GridPartition;

/// Adds support for spatial partitioning. Requires [`GridHashPlugin`](super::GridHashPlugin).
pub struct GridPartitionPlugin<F = ()>(PhantomData<F>)
where
    F: GridHashMapFilter;

impl<F> Default for GridPartitionPlugin<F>
where
    F: GridHashMapFilter,
{
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F> Plugin for GridPartitionPlugin<F>
where
    F: GridHashMapFilter,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GridPartitionMap<F>>().add_systems(
            PostUpdate,
            GridPartitionMap::<F>::update
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
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// Groups connected [`GridCell`]s into [`GridPartition`]s.
///
/// Partitions divide space into independent groups of cells.
///
/// The map depends on and is built from a corresponding [`GridHashMap`] with the same
/// `F:`[`GridHashMapFilter`].
#[derive(Resource)]
pub struct GridPartitionMap<F = ()>
where
    F: GridHashMapFilter,
{
    partitions: HashMap<GridPartitionId, GridPartition>,
    reverse_map: HashMap<GridHash, GridPartitionId, PassHash>,
    next_partition: u64,
    spooky: PhantomData<F>,
}

impl<F> Default for GridPartitionMap<F>
where
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

impl<F> Deref for GridPartitionMap<F>
where
    F: GridHashMapFilter,
{
    type Target = HashMap<GridPartitionId, GridPartition>;

    fn deref(&self) -> &Self::Target {
        &self.partitions
    }
}

impl<F> GridPartitionMap<F>
where
    F: GridHashMapFilter,
{
    /// Returns a reference to the [`GridPartition`], if it exists.
    #[inline]
    pub fn resolve(&self, id: &GridPartitionId) -> Option<&GridPartition> {
        self.partitions.get(id)
    }

    /// Searches for the [`GridPartition`] that contains this `hash`, returning the partition's
    /// [`GridPartitionId`] if the hash is found in any partition.
    #[inline]
    pub fn get(&self, hash: &GridHash) -> Option<&GridPartitionId> {
        self.reverse_map.get(hash)
    }

    /// Iterates over all [`GridPartition`]s.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&GridPartitionId, &GridPartition)> {
        self.partitions.iter()
    }

    #[inline]
    fn insert(&mut self, partition: GridPartitionId, set: HashSet<GridHash, PassHash>) {
        let Some(hash) = set.iter().next() else {
            return;
        };
        let mut min = hash.cell();
        let mut max = hash.cell();
        for hash in set.iter() {
            self.reverse_map.insert(*hash, partition);
            min = min.min(hash.cell());
            max = max.max(hash.cell());
        }
        self.partitions.insert(
            partition,
            GridPartition::new(hash.grid(), vec![set], min, max),
        );
    }

    #[inline]
    fn push(&mut self, partition: &GridPartitionId, hash: &GridHash) {
        if let Some(partition) = self.partitions.get_mut(partition) {
            partition.insert(*hash);
        } else {
            return;
        }
        self.reverse_map.insert(*hash, *partition);
    }

    #[inline]
    fn remove(&mut self, hash: &GridHash) {
        let Some(old_id) = self.reverse_map.remove(hash) else {
            return;
        };
        let mut empty = false;
        if let Some(partition) = self.partitions.get_mut(&old_id) {
            if partition.remove(hash) && partition.is_empty() {
                empty = true;
            }
        }
        if empty {
            self.partitions.remove(&old_id);
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
            .filter_map(|id| self.resolve(id).map(GridPartition::num_cells).zip(Some(id)))
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
        hash_grid: Res<GridHashMap<F>>,
        // Scratch space allocations
        mut added_neighbors: Local<Vec<GridPartitionId>>,
        mut adjacent_to_removals: Local<HashMap<GridPartitionId, HashSet<GridHash, PassHash>>>,
        mut split_candidates: Local<Vec<(GridPartitionId, HashSet<GridHash, PassHash>)>>,
        mut split_results: Local<Vec<Vec<SplitResult>>>,
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
                    .filter_map(|(id, adjacent_hashes)| {
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
                                // the next partition.
                                return None;
                            }
                            new_partitions
                                .push(hash_grid.flood(&this_cell, None).map(|n| n.0).collect());

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
            new_partitions.sort_unstable_by_key(HashSet::len);
            if let Some(largest_partition) = new_partitions.pop() {
                partition_map.insert(*original_partition_id, largest_partition);
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

struct SplitResult {
    original_partition_id: GridPartitionId,
    new_partitions: Vec<HashSet<GridHash, PassHash>>,
}

/// A private module to ensure the internal fields of the partition are not accessed directly.
/// Needed to ensure invariants are upheld.
mod private {
    use super::{GridCell, GridHash};
    use crate::precision::GridPrecision;
    use bevy_ecs::prelude::*;
    use bevy_platform_support::{collections::HashSet, hash::PassHash, prelude::*};

    /// A group of nearby [`GridCell`]s on an island disconnected from all other [`GridCell`]s.
    #[derive(Debug)]
    pub struct GridPartition {
        grid: Entity,
        tables: Vec<HashSet<GridHash, PassHash>>,
        min: GridCell,
        max: GridCell,
    }

    impl GridPartition {
        /// Returns `true` if the `hash` is in this partition.
        #[inline]
        pub fn contains(&self, hash: &GridHash) -> bool {
            self.tables.iter().any(|table| table.contains(hash))
        }

        /// Iterates over all [`GridHash`]s in this partition.
        #[inline]
        pub fn iter(&self) -> impl Iterator<Item = &GridHash> {
            self.tables.iter().flat_map(|table| table.iter())
        }

        /// Returns the total number of cells in this partition.
        #[inline]
        pub fn num_cells(&self) -> usize {
            self.tables.iter().map(HashSet::len).sum()
        }

        /// The grid this partition resides in.
        #[inline]
        pub fn grid(&self) -> Entity {
            self.grid
        }

        /// The maximum grid cell extent of the partition.
        pub fn max(&self) -> GridCell {
            self.max
        }

        /// The minimum grid cell extent of the partition.
        pub fn min(&self) -> GridCell {
            self.min
        }

        /// Frees up any unused memory. Returns `false` if the partition is completely empty.
        pub fn is_empty(&self) -> bool {
            self.tables.is_empty()
        }
    }

    /// Private internal methods
    impl GridPartition {
        pub(crate) fn new(
            grid: Entity,
            tables: Vec<HashSet<GridHash, PassHash>>,
            min: GridCell,
            max: GridCell,
        ) -> Self {
            Self {
                grid,
                tables,
                min,
                max,
            }
        }

        /// Tables smaller than this will be drained into other tables when merging. Tables larger than
        /// this limit will instead be added to a list of tables. This prevents partitions ending up
        /// with many tables containing a few entries.
        ///
        /// Draining and extending a hash set is much slower than moving the entire hash set into a
        /// list. The tradeoff is that the more tables added, the more there are that need to be
        /// iterated over when searching for a cell.
        const MIN_TABLE_SIZE: usize = 20_000;

        #[inline]
        pub(crate) fn insert(&mut self, cell: GridHash) {
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
            self.min = self.min.min(cell.cell());
            self.max = self.max.max(cell.cell());
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
        pub(crate) fn extend(&mut self, mut other: GridPartition) {
            assert_eq!(self.grid, other.grid);

            for other_table in other.tables.drain(..) {
                if other_table.len() < Self::MIN_TABLE_SIZE {
                    if let Some(i) = self.smallest_table() {
                        self.tables[i].reserve(other_table.len());
                        self.tables[i].extend(other_table);
                    } else {
                        self.tables.push(other_table);
                    }
                } else {
                    self.tables.push(other_table);
                }
            }
            self.min = self.min.min(other.min);
            self.max = self.max.max(other.max);
        }

        /// Removes a grid hash from the partition. Returns whether the value was present.
        #[inline]
        pub(crate) fn remove(&mut self, hash: &GridHash) -> bool {
            let Some(i_table) = self
                .tables
                .iter_mut()
                .enumerate()
                .find_map(|(i, table)| table.remove(hash).then_some(i))
            else {
                return false;
            };
            if self.tables[i_table].is_empty() {
                self.tables.swap_remove(i_table);
            }

            let (cell, min, max) = (hash.cell(), self.min, self.max);
            // Only need to recompute the bounds if the removed cell was touching the boundary.
            if min.x == cell.x || min.y == cell.y || min.z == cell.z {
                self.compute_min();
            }
            // Note this is not an `else if`. The cell might be on the max bound in one axis, and the
            // min bound in another.
            if max.x == cell.x || max.y == cell.y || max.z == cell.z {
                self.compute_max();
            }
            true
        }

        /// Computes the minimum bounding coordinate. Requires linearly scanning over entries in the
        /// partition.
        #[inline]
        fn compute_min(&mut self) {
            if let Some(min) = self.iter().map(GridHash::cell).reduce(|acc, e| acc.min(e)) {
                self.min = min;
            } else {
                self.min = GridCell::ONE * 1e10f64 as GridPrecision;
            }
        }

        /// Computes the maximum bounding coordinate. Requires linearly scanning over entries in the
        /// partition.
        #[inline]
        fn compute_max(&mut self) {
            if let Some(max) = self.iter().map(GridHash::cell).reduce(|acc, e| acc.max(e)) {
                self.max = max;
            } else {
                self.min = GridCell::ONE * -1e10 as GridPrecision;
            }
        }
    }
}
