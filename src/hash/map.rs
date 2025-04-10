//! The [`GridHashMap`] that contains mappings between entities and their spatial hash.

use alloc::collections::VecDeque;
use core::marker::PhantomData;

use super::GridHashMapFilter;
use crate::prelude::*;
use bevy_ecs::{entity::EntityHash, prelude::*};
use bevy_platform_support::{
    collections::{HashMap, HashSet},
    hash::PassHash,
    prelude::*,
    time::Instant,
};

/// An entry in a [`GridHashMap`], accessed with a [`GridHash`].
#[derive(Clone, Debug)]
pub struct GridHashEntry {
    /// All the entities located in this grid cell.
    pub entities: HashSet<Entity, EntityHash>,
    /// Precomputed hashes to direct neighbors.
    // TODO: computation cheap, heap slow. Can this be replaced with a u32 bitmask of occupied cells
    // (only need 26 bits), with the hashes computed based on the neighbor's relative position?
    pub occupied_neighbors: Vec<GridHash>,
}

impl GridHashEntry {
    /// Find an occupied neighbor's index in the list.
    fn neighbor_index(&self, hash: &GridHash) -> Option<usize> {
        self.occupied_neighbors
            .iter()
            .enumerate()
            .rev() // recently added cells are more likely to be removed
            .find_map(|(i, h)| (h == hash).then_some(i))
    }

    /// Iterate over this cell and its non-empty adjacent neighbors.
    ///
    /// See [`GridHashMap::nearby`].
    pub fn nearby<'a, F: GridHashMapFilter>(
        &'a self,
        map: &'a GridHashMap<F>,
    ) -> impl Iterator<Item = &'a GridHashEntry> + 'a {
        map.nearby(self)
    }
}

/// Trait extension that adds `.entities()` to any iterator of [`GridHashEntry`]s.
pub trait SpatialEntryToEntities<'a> {
    /// Flatten an iterator of [`GridHashEntry`]s into an iterator of [`Entity`]s.
    fn entities(self) -> impl Iterator<Item = Entity> + 'a;
}

impl<'a, T, I> SpatialEntryToEntities<'a> for T
where
    T: Iterator<Item = I> + 'a,
    I: SpatialEntryToEntities<'a> + 'a,
{
    fn entities(self) -> impl Iterator<Item = Entity> + 'a {
        self.flat_map(SpatialEntryToEntities::entities)
    }
}

impl<'a> SpatialEntryToEntities<'a> for &'a GridHashEntry {
    #[inline]
    fn entities(self) -> impl Iterator<Item = Entity> + 'a {
        self.entities.iter().copied()
    }
}

impl<'a> SpatialEntryToEntities<'a> for Neighbor<'a> {
    #[inline]
    fn entities(self) -> impl Iterator<Item = Entity> + 'a {
        self.1.entities.iter().copied()
    }
}

/// A global spatial hash map for quickly finding entities in a grid cell.
#[derive(Resource, Clone)]
pub struct GridHashMap<F = ()>
where
    F: GridHashMapFilter,
{
    /// The primary hash map for looking up entities by their [`GridHash`].
    map: InnerGridHashMap,
    /// A reverse lookup to find the latest spatial hash associated with an entity that this map is
    /// aware of. This is needed to remove or move an entity when its cell changes, because once it
    /// changes in the ECS, we need to know its *previous* value when it was inserted in this map.
    reverse_map: HashMap<Entity, GridHash, PassHash>,
    spooky: PhantomData<F>,
}

impl<F> core::fmt::Debug for GridHashMap<F>
where
    F: GridHashMapFilter,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GridHashMap")
            .field("map", &self.map)
            .field("reverse_map", &self.reverse_map)
            .finish()
    }
}

impl<F> Default for GridHashMap<F>
where
    F: GridHashMapFilter,
{
    fn default() -> Self {
        Self {
            map: Default::default(),
            reverse_map: Default::default(),
            spooky: PhantomData,
        }
    }
}

impl<F> GridHashMap<F>
where
    F: GridHashMapFilter,
{
    /// Get information about all entities located at this [`GridHash`], as well as its
    /// neighbors.
    #[inline]
    pub fn get(&self, hash: &GridHash) -> Option<&GridHashEntry> {
        self.map.inner.get(hash)
    }

    /// Returns `true` if this [`GridHash`] is occupied.
    #[inline]
    pub fn contains(&self, hash: &GridHash) -> bool {
        self.map.inner.contains_key(hash)
    }

    /// An iterator visiting all spatial hash cells and their contents in arbitrary order.
    #[inline]
    pub fn all_entries(&self) -> impl Iterator<Item = (&GridHash, &GridHashEntry)> {
        self.map.inner.iter()
    }

    /// Iterate over this cell and its non-empty adjacent neighbors.
    ///
    /// `GridHashEntry`s cache information about their neighbors as the spatial map is updated,
    /// making it faster to look up neighboring entries when compared to computing all neighbor
    /// hashes and checking if they exist.
    ///
    /// This function intentionally accepts [`GridHashEntry`] instead of [`GridHash`],
    /// because it is not a general radius test; it only works for occupied cells with a
    /// [`GridHashEntry`]. This API makes the above optimization possible, while preventing
    /// misuse and foot guns.
    #[inline]
    pub fn nearby<'a>(
        &'a self,
        entry: &'a GridHashEntry,
    ) -> impl Iterator<Item = &'a GridHashEntry> + 'a {
        // Use `core::iter::once` to avoid returning a function-local variable.
        Iterator::chain(
            core::iter::once(entry),
            entry.occupied_neighbors.iter().map(|neighbor_hash| {
                self.get(neighbor_hash)
                    .expect("occupied_neighbors should be occupied")
            }),
        )
    }

    /// Iterate over all [`GridHashEntry`]s within a cube with `center` and `radius`.
    ///
    /// ### Warning
    ///
    /// This can become expensive very quickly! The number of cells that need to be checked is
    /// exponential, a radius of 1 will access 26 cells, a radius of 2, will access 124 cells, and
    /// radius 5 will access 1,330 cells.
    ///
    /// Additionally, unlike `nearby`, this function cannot rely on cached information about
    /// neighbors. If you are using this function when `hash` is an occupied cell and `radius` is
    /// `1`, you should probably be using [`GridHashMap::nearby`] instead.
    #[inline]
    pub fn within_cube<'a>(
        &'a self,
        center: &'a GridHash,
        radius: u8,
    ) -> impl Iterator<Item = &'a GridHashEntry> + 'a {
        // Use `core::iter::once` to avoid returning a function-local variable.
        Iterator::chain(core::iter::once(*center), center.adjacent(radius))
            .filter_map(|hash| self.get(&hash))
    }

    /// Iterate over all connected neighboring cells with a breadth-first "flood fill" traversal
    /// starting at `seed`. Limits the extents of the breadth-first flood fill traversal with a
    /// `max_depth`.
    ///
    /// ## Depth Limit
    ///
    /// This will exit the breadth first traversal as soon as the depth is exceeded. While this
    /// measurement is the same as the radius, it will not necessarily visit all cells within the
    /// radius - it will only visit cells within this radius *and* search depth.
    ///
    /// Consider the case of a long thin U-shaped set of connected cells. While iterating from one
    /// end of the "U" to the other with this flood fill, if any of the cells near the base of the
    /// "U" exceed the `max_depth` (radius), iteration will stop. Even if the "U" loops back within
    /// the radius, those cells will never be visited.
    ///
    /// Also note that the `max_depth` (radius) is a Chebyshev distance, not a Euclidean distance.
    #[doc(alias = "bfs")]
    pub fn flood(
        &self,
        seed: &GridHash,
        max_depth: Option<GridPrecision>,
    ) -> impl Iterator<Item = Neighbor> {
        let starting_cell_cell = seed.cell();
        ContiguousNeighborsIter {
            initial_hash: Some(*seed),
            spatial_map: self,
            stack: Default::default(),
            visited_cells: Default::default(),
        }
        .take_while(move |Neighbor(hash, _)| {
            let Some(max_depth) = max_depth else {
                return true;
            };
            let dist = hash.cell() - starting_cell_cell;
            dist.x <= max_depth && dist.y <= max_depth && dist.z <= max_depth
        })
    }

    /// The set of cells that were inserted in the last update to the spatial hash map.
    ///
    /// These are cells that were previously empty, but now contain at least one entity.
    ///
    /// Useful for incrementally updating data structures that extend the functionality of
    /// [`GridHashMap`]. Updated in [`GridHashMapSystem::UpdateMap`].
    pub fn just_inserted(&self) -> &HashSet<GridHash, PassHash> {
        &self.map.just_inserted
    }

    /// The set of cells that were removed in the last update to the spatial hash map.
    ///
    /// These are cells that were previously occupied, but now contain no entities.
    ///
    /// Useful for incrementally updating data structures that extend the functionality of
    /// [`GridHashMap`]. Updated in [`GridHashMapSystem::UpdateMap`].
    pub fn just_removed(&self) -> &HashSet<GridHash, PassHash> {
        &self.map.just_removed
    }
}

/// Private Systems
impl<F> GridHashMap<F>
where
    F: GridHashMapFilter,
{
    /// Update the [`GridHashMap`] with entities that have changed [`GridHash`]es, and meet the
    /// optional [`GridHashMapFilter`].
    pub(super) fn update(
        mut spatial_map: ResMut<Self>,
        mut changed_hashes: ResMut<super::ChangedGridHashes<F>>,
        all_hashes: Query<(Entity, &GridHash), F>,
        mut removed: RemovedComponents<GridHash>,
        mut stats: Option<ResMut<crate::timing::GridHashStats>>,
    ) {
        let start = Instant::now();

        spatial_map.map.just_inserted.clear();
        spatial_map.map.just_removed.clear();

        for entity in removed.read() {
            spatial_map.remove(entity);
        }

        if let Some(ref mut stats) = stats {
            stats.moved_entities = changed_hashes.updated.len();
        }

        // See the docs on ChangedGridHash understand why we don't use query change detection.
        for (entity, spatial_hash) in changed_hashes
            .updated
            .drain(..)
            .filter_map(|entity| all_hashes.get(entity).ok())
        {
            spatial_map.insert(entity, *spatial_hash);
        }

        if let Some(ref mut stats) = stats {
            stats.map_update_duration += start.elapsed();
        }
    }
}

/// Private Methods
impl<F> GridHashMap<F>
where
    F: GridHashMapFilter,
{
    /// Insert an entity into the [`GridHashMap`], updating any existing entries.
    #[inline]
    fn insert(&mut self, entity: Entity, hash: GridHash) {
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

    /// Remove an entity from the [`GridHashMap`].
    #[inline]
    fn remove(&mut self, entity: Entity) {
        if let Some(old_hash) = self.reverse_map.remove(&entity) {
            self.map.remove(entity, old_hash);
        }
    }
}

/// The primary spatial hash extracted into its own type to help uphold invariants around insertions
/// and removals.
//
// TODO: Performance
//
// Improve the data locality of neighbors. Completely random access in a hot loop is probably
// unlikely, we should instead optimize for the case of wanting to look up neighbors of the current
// cell. We know neighbor lookups are a common need, and are a bottleneck currently.
//
//  - To do this, we could store neighboring entities together in the same entry, so they fill the
//    cache line during a lookup. Getting a neighbor in the current entry should then be super fast,
//    as it is already loaded on the cache.
//  - Not sure what the group size would be, probably depends on a bunch of factors, though will be
//    limited by common cache line sizes in practice, the decision is probably between whether to
//    group 2x2x2 or 3x3x3 blocks of cells into the same entry.
//  - Considering the entity hash set is stored on the heap, it might also make sense to group all
//    of these into a single collection. Iterating over all neighbors would then only need to access
//    this single hash set, and scan through it linear, instead of grabbing 8 (2x2x2) or 27 (3x3x3)
//    independent sets each at a different memory location.
//      - Not sure how you would efficiently partition this for each cell however. It could be a
//        hashmap whose value is the cell? Iterating over the entities in a single cell would then
//        require filtering out other cells. This might not be a big deal because iteration go brrr.
//        Unique insertion would be an issue though, e.g. the hash set for each cell ensures the
//        entity is unique.
//
//  - Another wild idea is to not change the hashmap structure at all, but store all entries in
//    Z-order in *another* collection (BTreeMap?) to improve locality for sequential lookups of
//    spatial neighbors. Would ordering cause hitches with insertions?
#[derive(Debug, Clone, Default)]
struct InnerGridHashMap {
    inner: HashMap<GridHash, GridHashEntry, PassHash>,
    /// Creating and freeing hash sets is expensive. To reduce time spent allocating and running
    /// destructors, we save any hash sets that would otherwise be thrown away. The next time we
    /// need to construct a new hash set of entities, we can grab one here.
    ///
    /// <https://en.wikipedia.org/wiki/Object_pool_pattern>.
    hash_set_pool: Vec<HashSet<Entity, EntityHash>>,
    neighbor_pool: Vec<Vec<GridHash>>,
    /// Cells that were added because they were empty but now contain entities.
    just_inserted: HashSet<GridHash, PassHash>,
    /// Cells that were removed because all entities vacated the cell.
    just_removed: HashSet<GridHash, PassHash>,
}

impl InnerGridHashMap {
    #[inline]
    fn insert(&mut self, entity: Entity, hash: GridHash) {
        if let Some(entry) = self.inner.get_mut(&hash) {
            entry.entities.insert(entity);
        } else {
            let mut entities = self.hash_set_pool.pop().unwrap_or_default();
            entities.insert(entity);
            self.insert_entry(hash, entities);
        }
    }

    #[inline]
    fn insert_entry(&mut self, hash: GridHash, entities: HashSet<Entity, EntityHash>) {
        let mut occupied_neighbors = self.neighbor_pool.pop().unwrap_or_default();
        occupied_neighbors.extend(hash.adjacent(1).filter(|neighbor| {
            self.inner
                .get_mut(neighbor)
                .map(|entry| {
                    entry.occupied_neighbors.push(hash);
                    true
                })
                .unwrap_or_default()
        }));

        self.inner.insert(
            hash,
            GridHashEntry {
                entities,
                occupied_neighbors,
            },
        );

        if !self.just_removed.remove(&hash) {
            // If a cell is removed then added within the same update, it can't be considered
            // "just added" because it *already existed* at the start of the update.
            self.just_inserted.insert(hash);
        }
    }

    #[inline]
    fn remove(&mut self, entity: Entity, old_hash: GridHash) {
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
            self.neighbor_pool.push(removed_entry.occupied_neighbors);

            if !self.just_inserted.remove(&old_hash) {
                // If a cell is added then removed within the same update, it can't be considered
                // "just removed" because it *already didn't exist* at the start of the update.
                self.just_removed.insert(old_hash);
            }
        }
    }
}

/// An iterator over the neighbors of a cell, breadth-first.
pub struct ContiguousNeighborsIter<'a, F>
where
    F: GridHashMapFilter,
{
    initial_hash: Option<GridHash>,
    spatial_map: &'a GridHashMap<F>,
    stack: VecDeque<Neighbor<'a>>,
    visited_cells: HashSet<GridHash>,
}

/// Newtype used for adding useful extensions like `.entities()`.
pub struct Neighbor<'a>(pub GridHash, pub &'a GridHashEntry);

impl<'a, F> Iterator for ContiguousNeighborsIter<'a, F>
where
    F: GridHashMapFilter,
{
    type Item = Neighbor<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(hash) = self.initial_hash.take() {
            self.stack
                .push_front(Neighbor(hash, self.spatial_map.get(&hash)?));
            self.visited_cells.insert(hash);
        }
        let Neighbor(hash, entry) = self.stack.pop_back()?;
        for (neighbor_hash, neighbor_entry) in entry
            .occupied_neighbors
            .iter()
            .filter(|neighbor_hash| self.visited_cells.insert(**neighbor_hash))
            .map(|neighbor_hash| {
                let entry = self
                    .spatial_map
                    .get(neighbor_hash)
                    .expect("Neighbor hashes in GridHashEntry are guaranteed to exist.");
                (neighbor_hash, entry)
            })
        {
            self.stack
                .push_front(Neighbor(*neighbor_hash, neighbor_entry));
        }
        Some(Neighbor(hash, entry))
    }
}
