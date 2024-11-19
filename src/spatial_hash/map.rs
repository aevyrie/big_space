//! The [`SpatialHashMap`] that contains mappings between entities and their spatial hash.

use std::{collections::VecDeque, marker::PhantomData, time::Instant};

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_utils::{
    hashbrown::{HashMap, HashSet},
    PassHash,
};

use super::SpatialHashFilter;

/// An entry in a [`SpatialHashMap`].
#[derive(Clone, Debug)]
pub struct SpatialHashEntry<P: GridPrecision> {
    /// All the entities located in this grid cell.
    pub entities: HashSet<Entity, PassHash>,
    /// Precomputed hashes to direct neighbors.
    pub occupied_neighbors: Vec<SpatialHash<P>>,
}

impl<P: GridPrecision> SpatialHashEntry<P> {
    /// Find an occupied neighbor's index in the list.
    fn neighbor_index(&self, hash: &SpatialHash<P>) -> Option<usize> {
        self.occupied_neighbors
            .iter()
            .enumerate()
            .rev() // recently added cells are more likely to be removed
            .find_map(|(i, h)| (h == hash).then_some(i))
    }
}

/// A global spatial hash map for quickly finding entities in a grid cell.
#[derive(Resource, Clone)]
pub struct SpatialHashMap<P, F = ()>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    /// The primary hash map for looking up entities by their [`SpatialHash`].
    map: InnerSpatialHashMap<P>,
    /// A reverse lookup to find the latest spatial hash associated with an entity that this map is
    /// aware of. This is needed to remove or move an entity when its cell changes, because once it
    /// changes in the ECS, we need to know its *previous* value when it was inserted in this map.
    reverse_map: HashMap<Entity, SpatialHash<P>, PassHash>,
    spooky: PhantomData<F>,
}

impl<P, F> std::fmt::Debug for SpatialHashMap<P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpatialHashMap")
            .field("map", &self.map)
            .field("reverse_map", &self.reverse_map)
            .finish()
    }
}

impl<P, F> Default for SpatialHashMap<P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    fn default() -> Self {
        Self {
            map: Default::default(),
            reverse_map: Default::default(),
            spooky: PhantomData,
        }
    }
}

impl<P, F> SpatialHashMap<P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    /// Update the [`SpatialHashMap`] with entities that have changed [`SpatialHash`]es, and meet
    /// the optional [`SpatialHashFilter`].
    pub(super) fn update(
        mut spatial_map: ResMut<SpatialHashMap<P, F>>,
        mut changed_hashes: ResMut<super::ChangedSpatialHashes<P, F>>,
        all_hashes: Query<(Entity, &SpatialHash<P>), F>,
        mut removed: RemovedComponents<SpatialHash<P>>,
        mut stats: Option<ResMut<crate::timing::SpatialHashStats>>,
    ) {
        let start = Instant::now();

        for entity in removed.read() {
            spatial_map.remove(entity)
        }

        if let Some(ref mut stats) = stats {
            stats.moved_entities = changed_hashes.list.len();
        }

        // See the docs on ChangedSpatialHash understand why we don't use query change detection.
        for (entity, spatial_hash) in changed_hashes
            .list
            .drain(..)
            .filter_map(|entity| all_hashes.get(entity).ok())
        {
            spatial_map.insert(entity, *spatial_hash);
        }

        if let Some(ref mut stats) = stats {
            stats.map_update_duration += start.elapsed();
        }
    }

    #[inline]
    fn insert(&mut self, entity: Entity, hash: SpatialHash<P>) {
        // If this entity is already in the maps, we need to remove and update it.
        if let Some(old_hash) = self.reverse_map.get_mut(&entity) {
            if hash.eq(old_hash) {
                return; // If the spatial hash is unchanged, early exit.
            }
            self.map.remove(entity, *old_hash);
            *old_hash = hash;
        } else {
            self.reverse_map.insert(entity, hash);
        }

        self.map.insert(entity, hash);
    }

    /// Remove an entity from the [`SpatialHashMap`].
    #[inline]
    fn remove(&mut self, entity: Entity) {
        if let Some(old_hash) = self.reverse_map.remove(&entity) {
            self.map.remove(entity, old_hash)
        }
    }

    /// Get a list of all entities in the same [`GridCell`] using a [`SpatialHash`].
    #[inline]
    pub fn get(&self, hash: &SpatialHash<P>) -> Option<&SpatialHashEntry<P>> {
        self.map.inner.get(hash)
    }

    /// An iterator visiting all spatial hash cells and their contents in arbitrary order.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&SpatialHash<P>, &SpatialHashEntry<P>)> {
        self.map.inner.iter()
    }

    /// Find entities in this and neighboring cells, within `cell_radius`.
    ///
    /// A radius of `1` will search all cells within a Chebyshev distance of `1`, or a total of 9
    /// cells. You can also think of this as a cube centered on the specified cell, expanded in each
    /// direction by `radius`.
    ///
    /// Returns an iterator over all non-empty neighboring cells and the set of entities in those
    /// cells.
    ///
    /// This is a lazy query, if you don't consume the iterator, it won't do any work!
    pub fn nearby<'a>(
        &'a self,
        entry: &'a SpatialHashEntry<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, &SpatialHashEntry<P>)> + '_ {
        entry.occupied_neighbors.iter().map(|neighbor_hash| {
            // We can unwrap here because occupied_neighbors are guaranteed to be occupied
            let neighbor_entry = self.get(neighbor_hash).unwrap();
            (*neighbor_hash, neighbor_entry)
        })
    }

    /// Like [`Self::nearby`], but flattens the included set of entities into a flat list.
    pub fn nearby_flat<'a>(
        &'a self,
        entry: &'a SpatialHashEntry<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, Entity)> + '_ {
        self.nearby(entry)
            .flat_map(|(hash, entry)| entry.entities.iter().map(move |entity| (hash, *entity)))
    }

    /// Iterates over all contiguous neighboring cells. Worst case, this could iterate over every
    /// cell in the map once.
    pub fn nearby_flood<'a>(
        &'a self,
        starting_cell: &SpatialHash<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, &'a SpatialHashEntry<P>)> {
        ContiguousNeighborsIter {
            initial_hash: Some(*starting_cell),
            spatial_map: self,
            stack: Default::default(),
            visited_cells: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct InnerSpatialHashMap<P: GridPrecision> {
    inner: HashMap<SpatialHash<P>, SpatialHashEntry<P>, PassHash>,
    /// Creating and freeing hash sets is expensive. To reduce time spent allocating and running
    /// destructors, we save any hash sets that would otherwise be thrown away. The next time we
    /// need to construct a new hash set of entities, we can grab one here.
    ///
    /// <https://en.wikipedia.org/wiki/Object_pool_pattern>.
    hash_set_pool: Vec<HashSet<Entity, PassHash>>,
    neighbor_pool: Vec<Vec<SpatialHash<P>>>,
}

impl<P: GridPrecision> InnerSpatialHashMap<P> {
    #[inline]
    fn insert(&mut self, entity: Entity, hash: SpatialHash<P>) {
        if let Some(entry) = self.inner.get_mut(&hash) {
            entry.entities.insert(entity);
        } else {
            let mut entities = self.hash_set_pool.pop().unwrap_or_default();
            entities.insert(entity);

            let mut occupied_neighbors = self.neighbor_pool.pop().unwrap_or_default();
            occupied_neighbors.extend(
                hash.neighbors(1)
                    .filter(|(neighbor, _)| {
                        self.inner
                            .get_mut(neighbor)
                            .map(|entry| {
                                entry.occupied_neighbors.push(hash);
                                true
                            })
                            .unwrap_or_default()
                    })
                    .map(|(neighbor, _)| neighbor),
            );

            self.inner.insert(
                hash,
                SpatialHashEntry {
                    entities,
                    occupied_neighbors,
                },
            );
        }
    }

    #[inline]
    fn remove(&mut self, entity: Entity, old_hash: SpatialHash<P>) {
        if let Some(entry) = self.inner.get_mut(&old_hash) {
            entry.entities.remove(&entity);
            if !entry.entities.is_empty() {
                return; // Early exit if the cell still has other entities in it
            }
        }

        // The entry is empty, so we need to do some cleanup
        if let Some(mut removed_entry) = self.inner.remove(&old_hash) {
            // Remove this entry from its neighbors' occupied neighbor list
            removed_entry
                .occupied_neighbors
                .drain(..)
                .for_each(|neighbor_hash| {
                    let neighbor = self
                        .inner
                        .get_mut(&neighbor_hash)
                        .expect("occupied neighbors is guaranteed to be up to date");
                    let index = neighbor.neighbor_index(&old_hash).unwrap();
                    neighbor.occupied_neighbors.remove(index);
                });

            // Add the allocated structs to their object pools, to reuse the allocations.
            self.hash_set_pool.push(removed_entry.entities);
            self.neighbor_pool.push(removed_entry.occupied_neighbors)
        }
    }
}

/// An iterator over the neighbors of a cell.
pub struct ContiguousNeighborsIter<'a, P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    initial_hash: Option<SpatialHash<P>>,
    spatial_map: &'a SpatialHashMap<P, F>,
    stack: VecDeque<(SpatialHash<P>, &'a SpatialHashEntry<P>)>,
    visited_cells: HashSet<SpatialHash<P>>,
}

impl<'a, P, F> Iterator for ContiguousNeighborsIter<'a, P, F>
where
    P: GridPrecision,
    F: SpatialHashFilter,
{
    type Item = (SpatialHash<P>, &'a SpatialHashEntry<P>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(hash) = self.initial_hash.take() {
            self.stack.push_front((hash, self.spatial_map.get(&hash)?));
            self.visited_cells.insert(hash);
        }
        let (hash, entry) = self.stack.pop_back()?;
        for (neighbor_hash, neighbor_entry) in entry
            .occupied_neighbors
            .iter()
            .filter(|neighbor_hash| self.visited_cells.insert(**neighbor_hash))
            .map(|neighbor_hash| {
                let entry = self
                    .spatial_map
                    .get(neighbor_hash)
                    .expect("Neighbor hashes in SpatialHashEntry are guaranteed to exist.");
                (neighbor_hash, entry)
            })
        {
            self.stack.push_front((*neighbor_hash, neighbor_entry));
        }
        Some((hash, entry))
    }
}
