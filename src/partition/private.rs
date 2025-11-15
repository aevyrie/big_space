//! A private module to ensure the internal fields of the partition are not accessed directly.
//! Needed to ensure invariants are upheld.

use crate::grid::cell::CellCoord;
use crate::hash::component::CellHashSet;
use crate::hash::component::CellId;
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
