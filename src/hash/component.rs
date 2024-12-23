//! Components for spatial hashing.

use std::hash::{Hash, Hasher};

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::Parent;
use bevy_math::IVec3;
use bevy_reflect::Reflect;
use bevy_utils::{AHasher, Instant, Parallel};

use super::{ChangedGridCells, HashGridFilter};

/// A fast but lossy version of [`GridCellHash`]. Use this component when you don't care about false
/// positives (hash collisions). See the docs on [`GridCellHash::fast_eq`] for more details on fast
/// but lossy equality checks.
#[derive(Component, Clone, Copy, Debug, Reflect, PartialEq, Eq)]
pub struct FastGridCellHash(u64);

impl Hash for FastGridCellHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// A unique spatial hash shared by all entities in the same [`GridCell`] and [`Grid`].
///
/// Once computed, a spatial hash can be used to rapidly check if any two entities are in the same
/// cell, by comparing the hashes. You can also get a list of all entities within a cell using the
/// [`HashGrid`] resource.
///
/// Due to grids and multiple big spaces in a single world, this must use both the [`GridCell`] and
/// the [`Parent`] of the entity to uniquely identify its position. These two values are then hashed
/// and stored in this spatial hash component.
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct GridCellHash<P: GridPrecision> {
    cell: GridCell<P>,
    grid: Entity,
    pre_hash: u64,
}

impl<P: GridPrecision> PartialEq for GridCellHash<P> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Comparing the hash is redundant.
        //
        // TODO benchmark adding a hash comparison at the front, may help early out for most
        // comparisons? It might not be a win, because many of the comparisons could be coming from
        // hashmaps, in which case we already know the hashes are the same.
        self.cell == other.cell && self.grid == other.grid
    }
}

impl<P: GridPrecision> Eq for GridCellHash<P> {}

impl<P: GridPrecision> Hash for GridCellHash<P> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.pre_hash);
    }
}

impl<P: GridPrecision> GridCellHash<P> {
    /// Generate a new hash from parts.
    ///
    /// Intentionally left private, so we can ensure the only place these are constructed/mutated is
    /// this module. This allows us to optimize change detection using [`ChangedGridCellHashes`].
    #[inline]
    pub(super) fn new(parent: &Parent, cell: &GridCell<P>) -> Self {
        Self::from_parent(parent.get(), cell)
    }

    #[inline]
    pub(super) fn from_parent(parent: Entity, cell: &GridCell<P>) -> Self {
        let hasher = &mut AHasher::default();
        hasher.write_u64(parent.to_bits());
        cell.hash(hasher);

        GridCellHash {
            cell: *cell,
            grid: parent,
            pre_hash: hasher.finish(),
        }
    }

    /// Do not use this to manually construct this component. You've been warned.
    #[doc(hidden)]
    pub fn __new_manual(parent: Entity, cell: &GridCell<P>) -> Self {
        Self::from_parent(parent, cell)
    }

    /// Fast comparison that can return false positives, but never false negatives.
    ///
    /// Consider using [`FastGridCellHash`] if you only need fast equality comparisons, as it is
    /// much more cache friendly than this [`GridCellHash`] component.
    ///
    /// Unlike the [`PartialEq`] implementation, this equality check will only compare the hash
    /// value instead of the cell and parent. This can result in collisions. You should only use
    /// this when you want to prove that two cells do not overlap.
    ///
    /// - If this returns `false`, it is guaranteed that the entities are in different cells.
    /// - if this returns `true`, it is probable (but not guaranteed) that the entities are in the
    ///   same cell
    ///
    /// If this returns true, you may either want to try the slightly slower `eq` method, or, ignore
    /// the chance of a false positive. This is common in collision detection - a false positive is
    /// rare, and only results in doing some extra narrow-phase collision tests, but no logic
    /// errors.
    ///
    /// In other words, this should only be used for acceleration, when you want to quickly cull
    /// non-overlapping cells, and you will be double checking for false positives later.
    #[inline]
    pub fn fast_eq(&self, other: &Self) -> bool {
        self.pre_hash == other.pre_hash
    }

    /// Returns an iterator over all neighboring grid cells and their hashes, within the
    /// `cell_radius`. This iterator will not visit `cell`.
    pub fn adjacent(&self, cell_radius: u8) -> impl Iterator<Item = GridCellHash<P>> + '_ {
        let radius = cell_radius as i32;
        let search_width = 1 + 2 * radius;
        let search_volume = search_width.pow(3);
        let center = -radius;
        let stride = IVec3::new(1, search_width, search_width.pow(2));
        (0..search_volume)
            .map(move |i| center + i / stride % search_width)
            .filter(|offset| *offset != IVec3::ZERO) // Skip center cell
            .map(move |offset| {
                let neighbor_cell = self.cell + offset;
                GridCellHash::from_parent(self.grid, &neighbor_cell)
            })
    }

    /// Update or insert the [`GridCellHash`] of all changed entities that match the optional
    /// [`HashGridFilter`].
    pub(super) fn update<F: HashGridFilter>(
        mut commands: Commands,
        mut changed_hashes: ResMut<ChangedGridCells<P, F>>,
        mut spatial_entities: ParamSet<(
            Query<
                (
                    Entity,
                    &Parent,
                    &GridCell<P>,
                    &mut GridCellHash<P>,
                    &mut FastGridCellHash,
                ),
                (F, Or<(Changed<Parent>, Changed<GridCell<P>>)>),
            >,
            Query<(Entity, &Parent, &GridCell<P>), (F, Without<GridCellHash<P>>)>,
        )>,
        mut stats: Option<ResMut<crate::timing::GridCellHashStats>>,
        mut thread_changed_hashes: Local<Parallel<Vec<Entity>>>,
        mut thread_commands: Local<Parallel<Vec<(Entity, GridCellHash<P>, FastGridCellHash)>>>,
    ) {
        let start = Instant::now();

        // Create new
        spatial_entities
            .p1()
            .par_iter()
            .for_each(|(entity, parent, cell)| {
                let spatial_hash = GridCellHash::new(parent, cell);
                let fast_hash = FastGridCellHash(spatial_hash.pre_hash);
                thread_commands.scope(|tl| tl.push((entity, spatial_hash, fast_hash)));
                thread_changed_hashes.scope(|tl| tl.push(entity));
            });
        for (entity, spatial_hash, fast_hash) in thread_commands.drain() {
            commands.entity(entity).insert((spatial_hash, fast_hash));
        }

        // Update existing
        spatial_entities.p0().par_iter_mut().for_each(
            |(entity, parent, cell, mut hash, mut fast_hash)| {
                let new_hash = GridCellHash::new(parent, cell);
                let new_fast_hash = new_hash.pre_hash;
                if hash.replace_if_neq(new_hash).is_some() {
                    thread_changed_hashes.scope(|tl| tl.push(entity));
                }
                fast_hash.0 = new_fast_hash;
            },
        );

        changed_hashes.list.extend(thread_changed_hashes.drain());

        if let Some(ref mut stats) = stats {
            stats.hash_update_duration += start.elapsed();
        }
    }

    /// The [`GridCell`] associated with this spatial hash.
    pub fn cell(&self) -> GridCell<P> {
        self.cell
    }

    /// The [`Parent`] [`Grid`] of this spatial hash.
    pub fn grid(&self) -> Entity {
        self.grid
    }
}
