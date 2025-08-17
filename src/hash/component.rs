//! Components for spatial hashing.

use alloc::vec::Vec;
use core::hash::{BuildHasher, Hash, Hasher};

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::IVec3;
use bevy_platform::{
    collections::{HashMap, HashSet},
    hash::{FixedHasher, PassHash},
    time::Instant,
};
use bevy_reflect::Reflect;

use super::{ChangedCells, SpatialHashFilter};

use crate::portable_par::PortableParallel;

/// A fast but lossy version of [`CellId`]. Use this component when you don't care about false
/// positives (hash collisions). See the docs on [`CellId::fast_eq`] for more details on fast but
/// lossy equality checks.
///
/// ### Hashing
///
/// Use this in `HashMap`s and `HashSet`s with `PassHash` to avoid re-hashing the stored precomputed
/// hash. Remember, hash collisions cannot be resolved for this type!
#[derive(Component, Clone, Copy, Debug, Reflect, PartialEq, Eq)]
pub struct CellHash(u64);

impl Hash for CellHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

impl PartialEq<CellId> for CellHash {
    fn eq(&self, other: &CellId) -> bool {
        self.0 == other.pre_hash
    }
}

impl From<CellId> for CellHash {
    fn from(value: CellId) -> Self {
        Self(value.pre_hash)
    }
}

/// A [`HashSet`] type you can use to describe a set of globally unique grid cells.
///
/// Keys are prehashed to make set construction and lookups faster.
///
/// Cells with the same [`CellCoord`] index but different parent [`Grid`]s are *not* equivalent.
pub type CellHashSet = HashSet<CellId, PassHash>;

/// A [`HashMap`] type you can use to map any grid cell in the world to a value.
///
/// Keys are prehashed to make map construction and lookups faster.
///
/// Cells with the same [`CellCoord`] index but different parent [`Grid`]s are *not* equivalent.
pub type CellHashMap<T> = HashMap<CellId, T, PassHash>;

/// Uniquely identifies a grid cell across all [`Grid`]s in a [`World`], caching the hash for fast
/// lookups in hashmaps that use this as a key. This component is automatically added to entities
/// with a [`CellCoord`].
///
/// This unique ID can be used to rapidly check if any two entities are in the same cell by
/// comparing the hashes. Unlike [`CellHash`], [`CellId`] will not result in false positives when
/// checking equality. However, it is larger and theoretically slower.
///
/// You can get a list of all entities within a cell using the [`CellLookup`] resource.
///
/// Due to grids and multiple big spaces in a single world, this must use both the [`CellCoord`] and
/// the [`ChildOf`] of the entity to uniquely identify its position. These two values are then hashed
/// and stored in this spatial hash component.
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct CellId {
    // Needed for equality checks
    coord: CellCoord,
    // Needed for equality checks
    grid: Entity,
    // The hashed value of the `cell` and `grid` fields. Hash collisions are possible, especially
    // for grids with very large `GridPrecision`s, because a single u64 can only represent the
    // fraction of possible states compared to an `Entity` (2x u32) and `GridCell` (3x i128)
    // combined.
    pre_hash: u64,
}

impl PartialEq for CellId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Short circuit the fast path by comparing the prehashed value.
        self.pre_hash == other.pre_hash && self.coord == other.coord && self.grid == other.grid
    }
}

impl Eq for CellId {}

impl Hash for CellId {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.pre_hash);
    }
}

impl CellId {
    /// Generate a new hash from parts.
    ///
    /// Intentionally left private, so we can ensure the only place these are constructed/mutated is
    /// this module. This allows us to optimize change detection using [`ChangedCells`].
    #[inline]
    pub(super) fn new(parent: &ChildOf, cell: &CellCoord) -> Self {
        Self::from_parent(parent.parent(), cell)
    }

    #[inline]
    pub(super) fn from_parent(parent: Entity, cell: &CellCoord) -> Self {
        let mut hasher = FixedHasher.build_hasher();
        hasher.write_u64(parent.to_bits());
        cell.hash(&mut hasher);

        CellId {
            coord: *cell,
            grid: parent,
            pre_hash: hasher.finish(),
        }
    }

    /// Do not use this to manually construct this component. You've been warned.
    #[doc(hidden)]
    pub fn __new_manual(parent: Entity, cell: &CellCoord) -> Self {
        Self::from_parent(parent, cell)
    }

    /// Fast comparison that can return false positives, but never false negatives.
    ///
    /// Consider using [`CellHash`] if you only need fast equality comparisons, as it is much
    /// more cache-friendly than this [`CellId`] component.
    ///
    /// Unlike the [`PartialEq`] implementation, this equality check will only compare the hash
    /// value instead of the cell and parent. This can result in collisions. You should only use
    /// this when you want to prove that two cells do not overlap.
    ///
    /// - If this returns `false`, it is guaranteed that the entities are in different cells.
    /// - If this returns `true`, it is probable (but not guaranteed) that the entities are in the
    ///   same cell
    ///
    /// If this returns true, you may either want to try the slightly slower `eq` method, or, ignore
    /// the chance of a false positive. This is common in collision detection - a false positive is
    /// rare and only results in doing some extra narrow-phase collision tests, but no logic errors.
    ///
    /// In other words, this should only be used for acceleration when you want to quickly cull
    /// non-overlapping cells, and you will be double-checking for false positives later.
    #[inline]
    pub fn fast_eq(&self, other: &Self) -> bool {
        self.pre_hash == other.pre_hash
    }

    /// Returns an iterator over all neighboring grid cells and their hashes, within the
    /// `cell_radius`. This iterator will not visit `cell`.
    pub fn adjacent(&self, cell_radius: u8) -> impl Iterator<Item = CellId> + '_ {
        let radius = cell_radius as i32;
        let search_width = 1 + 2 * radius;
        let search_volume = search_width.pow(3);
        let center = -radius;
        let stride = IVec3::new(1, search_width, search_width.pow(2));
        (0..search_volume)
            .map(move |i| center + i / stride % search_width)
            .filter(|offset| *offset != IVec3::ZERO) // Skip center cell
            .map(move |offset| {
                let neighbor_cell = self.coord + offset;
                CellId::from_parent(self.grid, &neighbor_cell)
            })
    }

    /// Update or insert the [`CellId`] of all changed entities that match the optional
    /// [`SpatialHashFilter`].
    pub fn update<F: SpatialHashFilter>(
        mut commands: Commands,
        mut changed_cells: ResMut<ChangedCells<F>>,
        mut spatial_entities: Query<
            (Entity, &ChildOf, &CellCoord, &mut CellId, &mut CellHash),
            (F, Or<(Changed<ChildOf>, Changed<CellCoord>)>),
        >,
        added_entities: Query<(Entity, &ChildOf, &CellCoord), (F, Without<CellId>)>,
        mut stats: Option<ResMut<crate::timing::GridHashStats>>,
        mut thread_updated_hashes: Local<PortableParallel<Vec<Entity>>>,
        mut thread_commands: Local<PortableParallel<Vec<(Entity, CellId, CellHash)>>>,
    ) {
        let start = Instant::now();
        changed_cells.updated.clear();

        // Create new
        added_entities
            .par_iter()
            .for_each(|(entity, parent, cell)| {
                let cell_guid = CellId::new(parent, cell);
                let fast_hash = cell_guid.into();
                thread_commands.scope(|tl| tl.push((entity, cell_guid, fast_hash)));
                thread_updated_hashes.scope(|tl| tl.push(entity));
            });
        for (entity, cell_guid, fast_hash) in thread_commands.drain() {
            commands.entity(entity).insert((cell_guid, fast_hash));
        }

        // Update existing
        spatial_entities.par_iter_mut().for_each(
            |(entity, parent, cell, mut cell_guid, mut fast_hash)| {
                let new_cell_guid = CellId::new(parent, cell);
                let new_fast_hash = new_cell_guid.pre_hash;
                if cell_guid.replace_if_neq(new_cell_guid).is_some() {
                    thread_updated_hashes.scope(|tl| tl.push(entity));
                }
                fast_hash.0 = new_fast_hash;
            },
        );
        changed_cells.updated.extend(thread_updated_hashes.drain());

        if let Some(ref mut stats) = stats {
            stats.hash_update_duration += start.elapsed();
        }
    }

    /// The [`CellCoord`] associated with this spatial hash.
    pub fn coord(&self) -> CellCoord {
        self.coord
    }

    /// The [`ChildOf`] [`Grid`] of this spatial hash.
    pub fn grid(&self) -> Entity {
        self.grid
    }
}
