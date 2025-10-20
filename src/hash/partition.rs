//! Detect and update groups of nearby occupied cells.

use core::{hash::Hash, marker::PhantomData, ops::Deref};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_platform::prelude::*;
use bevy_platform::{collections::HashMap, time::Instant};
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};

use super::component::{CellHashMap, CellHashSet};
use super::{CellCoord, CellId, CellLookup, SpatialHashFilter, SpatialHashSystems};

pub use private::Partition;

/// Adds support for spatial partitioning. Requires [`GridHashPlugin`](super::CellHashingPlugin).
pub struct PartitionPlugin<F = ()>(PhantomData<F>)
where
    F: SpatialHashFilter;

impl<F> PartitionPlugin<F>
where
    F: SpatialHashFilter,
{
    /// Create a new instance of [`PartitionPlugin`].
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl Default for PartitionPlugin<()> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F> Plugin for PartitionPlugin<F>
where
    F: SpatialHashFilter,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<PartitionLookup<F>>().add_systems(
            PostUpdate,
            PartitionLookup::<F>::update
                .in_set(SpatialHashSystems::UpdatePartitionLookup)
                .after(SpatialHashSystems::UpdateCellLookup),
        );
    }
}

/// Uniquely identifies a [`Partition`] in the [`PartitionLookup`] resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionId(u64);

impl PartitionId {
    /// The inner partition id.
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl Hash for PartitionId {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// A resource for quickly finding connected groups of occupied grid cells in [`Partition`]s.
///
/// The map is built from a [`CellLookup`] resource with the same `F:`[`SpatialHashFilter`].
#[derive(Resource)]
pub struct PartitionLookup<F = ()>
where
    F: SpatialHashFilter,
{
    partitions: HashMap<PartitionId, Partition>,
    reverse_map: CellHashMap<PartitionId>,
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
    type Target = HashMap<PartitionId, Partition>;

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

    fn update(
        mut partitions: ResMut<Self>,
        mut timing: ResMut<crate::timing::GridHashStats>,
        cells: Res<CellLookup<F>>,
        // Scratch space allocations
        mut added_neighbors: Local<Vec<PartitionId>>,
        mut split_candidates_map: Local<HashMap<PartitionId, CellHashSet>>,
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
        timing.update_partition += start.elapsed();
    }
}

struct SplitResult {
    original_partition_id: PartitionId,
    new_partitions: Vec<CellHashSet>,
}

/// A private module to ensure the internal fields of the partition are not accessed directly.
/// Needed to ensure invariants are upheld.
mod private {
    use super::{CellCoord, CellId};
    use crate::hash::component::CellHashSet;
    use crate::precision::GridPrecision;
    use bevy_ecs::prelude::*;
    use bevy_platform::prelude::*;

    /// A group of nearby grid cells, within the same grid, disconnected from all other cells in
    /// that grid. Accessed via [`CellPartitionLookup`](super::PartitionLookup).
    #[derive(Debug)]
    pub struct Partition {
        grid: Entity,
        tables: Vec<CellHashSet>,
        min: CellCoord,
        max: CellCoord,
    }

    impl Partition {
        /// Returns `true` if the `hash` is in this partition.
        #[inline]
        pub fn contains(&self, hash: &CellId) -> bool {
            self.tables.iter().any(|table| table.contains(hash))
        }

        /// Iterates over all [`CellId`]s in this partition.
        #[inline]
        pub fn iter(&self) -> impl Iterator<Item = &CellId> {
            self.tables.iter().flat_map(|table| table.iter())
        }

        /// Returns the total number of cells in this partition.
        #[inline]
        pub fn num_cells(&self) -> usize {
            self.tables.iter().map(CellHashSet::len).sum()
        }

        /// The grid this partition resides in.
        #[inline]
        pub fn grid(&self) -> Entity {
            self.grid
        }

        /// The maximum grid cell extent of the partition.
        pub fn max(&self) -> CellCoord {
            self.max
        }

        /// The minimum grid cell extent of the partition.
        pub fn min(&self) -> CellCoord {
            self.min
        }

        /// Returns `true` if the partition is completely empty.
        pub fn is_empty(&self) -> bool {
            self.tables.is_empty()
        }
    }

    /// Private internal methods
    impl Partition {
        pub(crate) fn new(
            grid: Entity,
            tables: Vec<CellHashSet>,
            min: CellCoord,
            max: CellCoord,
        ) -> Self {
            Self {
                grid,
                min,
                max,
                tables,
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

        /// Tables smaller than this will be drained into other tables when merging. Tables larger than
        /// this limit will instead be added to a list of tables. This prevents partitions ending up
        /// with many tables containing a few entries.
        ///
        /// Draining and extending a hash set is much slower than moving the entire hash set into a
        /// list. The tradeoff is that the more tables added, the more there are that need to be
        /// iterated over when searching for a cell.
        const MIN_TABLE_SIZE: usize = 20_000;

        #[inline]
        pub(crate) fn insert(&mut self, cell: CellId) {
            if self.contains(&cell) {
                return;
            }
            if let Some(i) = self.smallest_table() {
                self.tables[i].insert(cell);
            } else {
                let mut table = CellHashSet::default();
                table.insert(cell);
                self.tables.push(table);
            }
            self.min = self.min.min(cell.coord());
            self.max = self.max.max(cell.coord());
        }

        #[inline]
        pub(crate) fn extend(&mut self, mut other: Partition) {
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

        /// Removes a cell from the partition. Returns `true` if the cell was present.
        #[inline]
        pub(crate) fn remove(&mut self, cell: &CellId) -> bool {
            let Some(i_table) = self
                .tables
                .iter_mut()
                .enumerate()
                .find_map(|(i, table)| table.remove(cell).then_some(i))
            else {
                return false;
            };
            if self.tables[i_table].is_empty() {
                self.tables.swap_remove(i_table);
            }

            let (removed, min, max) = (cell.coord(), self.min, self.max);
            // Only need to recompute the bounds if the removed cell was touching the boundary.
            if min.x == removed.x || min.y == removed.y || min.z == removed.z {
                self.compute_min();
            }
            // Note this is not an `else if`. The cell might be on the max bound in one axis, and the
            // min bound in another.
            if max.x == removed.x || max.y == removed.y || max.z == removed.z {
                self.compute_max();
            }
            true
        }

        /// Computes the minimum bounding coordinate. Requires linearly scanning over entries in the
        /// partition.
        #[inline]
        fn compute_min(&mut self) {
            if let Some(min) = self.iter().map(CellId::coord).reduce(|acc, e| acc.min(e)) {
                self.min = min;
            } else {
                self.min = CellCoord::ONE * 1e10f64 as GridPrecision;
            }
        }

        /// Computes the maximum bounding coordinate. Requires linearly scanning over entries in the
        /// partition.
        #[inline]
        fn compute_max(&mut self) {
            if let Some(max) = self.iter().map(CellId::coord).reduce(|acc, e| acc.max(e)) {
                self.max = max;
            } else {
                self.min = CellCoord::ONE * -1e10 as GridPrecision;
            }
        }
    }
}
