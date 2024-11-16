//! Spatial hashing acceleration structure. See [`SpatialHashPlugin`].

use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, query::QueryFilter};
use bevy_hierarchy::Parent;
use bevy_math::IVec3;
use bevy_reflect::Reflect;
use bevy_utils::{
    hashbrown::{HashMap, HashSet},
    AHasher, Instant, Parallel, PassHash,
};

/// Add spatial hashing acceleration to `big_space`, accessible through the [`SpatialHashMap`]
/// resource, and [`SpatialHash`] components.
///
/// You can optionally add a filter to this plugin, to only run the spatial hashing on entities that
/// match the supplied query filter. This is useful if you only want to, say, compute hashes and
/// insert in the [`SpatialHashMap`] for `Player` entities.
///
/// If you are adding multiple copies of this plugin with different filters, there are optimizations
/// in place to avoid duplicating work. However, you should still take care to avoid excessively
/// overlapping filters.
pub struct SpatialHashPlugin<P: GridPrecision, F: QueryFilter = ()>(PhantomData<(P, F)>);

impl<P: GridPrecision, F: QueryFilter> Default for SpatialHashPlugin<P, F> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<P: GridPrecision, F: QueryFilter + Send + Sync + 'static> Plugin for SpatialHashPlugin<P, F> {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialHashMap<P, F>>()
            .register_type::<SpatialHash<P>>()
            .init_resource::<ChangedSpatialHashes<P, F>>()
            .add_systems(
                PostUpdate,
                (
                    Self::update_spatial_hashes
                        .in_set(SpatialHashSystem::UpdateHash)
                        .after(FloatingOriginSystem::RecenterLargeTransforms),
                    SpatialHashMap::<P, F>::update
                        .in_set(SpatialHashSystem::UpdateMap)
                        .after(SpatialHashSystem::UpdateHash),
                )
                    .in_set(bevy_transform::TransformSystem::TransformPropagate),
            );
    }
}

/// Used to manually track spatial hashes that have changed, for optimization purposes.
///
/// We use a manual collection instead of a `Changed` query because a query that uses `Changed`
/// still has to iterate over every single entity. By making a shortlist of changed entities
/// ourselves, we can make this 1000x faster.
///
/// Note that this is optimized for *sparse* updates, this may perform worse if you are updating
/// every entity. The observation here is that usually entities are not moving between grid cells,
/// and thus their spatial hash is not changing. On top of that, many entities are completely
/// static.
///
/// It may be possible to remove this if bevy gets archetype change detection, or observers that can
/// react to a component being mutated. For now, this performs well enough.
#[derive(Resource)]
struct ChangedSpatialHashes<P: GridPrecision, F: QueryFilter> {
    list: Vec<Entity>,
    spooky: PhantomData<(P, fn() -> F)>, // fn makes this send and sync
}

impl<P: GridPrecision, F: QueryFilter> Default for ChangedSpatialHashes<P, F> {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            spooky: PhantomData,
        }
    }
}

impl<P: GridPrecision, F: QueryFilter> SpatialHashPlugin<P, F> {
    /// Update or insert the [`SpatialHash`] of all changed entities that match the optional
    /// `QueryFilter`.
    fn update_spatial_hashes(
        mut commands: Commands,
        mut changed_hashes: ResMut<ChangedSpatialHashes<P, F>>,
        mut spatial_entities: ParamSet<(
            Query<
                (
                    Entity,
                    &Parent,
                    &GridCell<P>,
                    &mut SpatialHash<P>,
                    &mut FastSpatialHash,
                ),
                (F, Or<(Changed<Parent>, Changed<GridCell<P>>)>),
            >,
            Query<(Entity, &Parent, &GridCell<P>), (F, Without<SpatialHash<P>>)>,
        )>,
        mut stats: Option<ResMut<crate::timing::SpatialHashStats>>,
        mut thread_changed_hashes: Local<Parallel<Vec<Entity>>>,
        mut thread_commands: Local<Parallel<Vec<(Entity, SpatialHash<P>, FastSpatialHash)>>>,
    ) {
        let start = Instant::now();

        // Create new
        spatial_entities
            .p1()
            .par_iter()
            .for_each(|(entity, parent, cell)| {
                let spatial_hash = SpatialHash::new(parent, cell);
                let fast_hash = FastSpatialHash(spatial_hash.pre_hash);
                thread_commands.scope(|tl| tl.push((entity, spatial_hash, fast_hash)));
                thread_changed_hashes.scope(|tl| tl.push(entity));
            });
        for (entity, spatial_hash, fast_hash) in thread_commands.drain::<Vec<_>>() {
            commands.entity(entity).insert((spatial_hash, fast_hash));
        }

        // Update existing
        spatial_entities.p0().par_iter_mut().for_each(
            |(entity, parent, cell, mut hash, mut fast_hash)| {
                let new_hash = SpatialHash::new(parent, cell);
                let new_fast_hash = new_hash.pre_hash;
                if hash.replace_if_neq(new_hash).is_some() {
                    thread_changed_hashes.scope(|tl| tl.push(entity));
                }
                fast_hash.0 = new_fast_hash;
            },
        );

        changed_hashes
            .list
            .extend(thread_changed_hashes.drain::<Vec<Entity>>());

        if let Some(ref mut stats) = stats {
            stats.hash_update_duration += start.elapsed();
        }
    }
}

/// System sets for [`SpatialHashPlugin`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum SpatialHashSystem {
    /// [`SpatialHash`] updated.
    UpdateHash,
    /// [`SpatialHashMap`] updated.
    UpdateMap,
}

/// A fast but lossy version of [`SpatialHash`]. Use this component when you don't care about
/// testing for false positives (hash collisions). See the docs on [`SpatialHash::fast_eq`] for more
/// details on fast but lossy equality checks.
#[derive(Component, Clone, Copy, Debug, Reflect, PartialEq, Eq)]
pub struct FastSpatialHash(u64);

impl Hash for FastSpatialHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// A`Component` used to create a unique spatial hash of any entity within this [`GridCell`].
///
/// Once computed, a spatial hash can be used to rapidly check if any two entities are in the same
/// cell, by comparing the hashes. You can also get a list of all entities within a cell
/// using the [`SpatialHashMap`] resource.
///
/// Due to reference frames and multiple big spaces in a single world, this must use both the
/// [`GridCell`] and the [`Parent`] of the entity to uniquely identify its position. These two
/// values are then hashed and stored in this spatial hash component.
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct SpatialHash<P: GridPrecision> {
    cell: GridCell<P>,
    parent: Entity,
    pre_hash: u64,
}

impl<P: GridPrecision> PartialEq for SpatialHash<P> {
    fn eq(&self, other: &Self) -> bool {
        // Comparing the hash is redundant.
        //
        // TODO benchmark adding a hash comparison at the front, may help early out for most
        // comparisons? It might not be a win, because many of the comparisons could be coming from
        // hashmaps, in which case we already know the hashes are the same.
        self.cell == other.cell && self.parent == other.parent
    }
}

impl<P: GridPrecision> Eq for SpatialHash<P> {}

impl<P: GridPrecision> Hash for SpatialHash<P> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.pre_hash);
    }
}

impl<P: GridPrecision> SpatialHash<P> {
    /// Generate a new hash from parts.
    ///
    /// Intentionally left private, so we can ensure the only place these are constructed/mutated is
    /// this module. This allows us to optimize change detection using [`ChangedSpatialHashes`].
    #[inline]
    pub fn new(parent: &Parent, cell: &GridCell<P>) -> Self {
        Self::from_parent(parent.get(), cell)
    }

    #[inline]
    pub(super) fn from_parent(parent: Entity, cell: &GridCell<P>) -> Self {
        let hasher = &mut AHasher::default();
        hasher.write_u64(parent.to_bits());
        cell.hash(hasher);

        SpatialHash {
            cell: *cell,
            parent,
            pre_hash: hasher.finish(),
        }
    }

    /// Do not use this as a component. You've been warned.
    #[doc(hidden)]
    pub fn __new_manual(parent: Entity, cell: &GridCell<P>) -> Self {
        Self::from_parent(parent, cell)
    }

    /// Fast comparison that can return false positives, but never false negatives.
    ///
    /// Consider using [`FastSpatialHash`] if you only need fast equality comparisons, as it is much
    /// more cache friendly than this [`SpatialHash`] component.
    ///
    /// Unlike the [`PartialEq`] implementation, this equality check will only compare the hash
    /// value instead of the cell and parent. This can result in collisions. You should only use
    /// this when you want to prove that two cells do not overlap.
    ///
    /// - If this returns `false`, it is guaranteed that the two cells are in different positions
    /// - if this returns `true`, it is probable (but not guaranteed) that the two cells are in the
    ///   same position.
    ///
    /// If this returns true, you may either want to try the slightly slower `eq` method, or, ignore
    /// the chance of a false positive. This is common in collision detection - a false positive is
    /// rare, and only results in doing some extra narrow-phase collision tests, but no logic
    /// errors.
    ///
    /// In other words, this should only be used for acceleration, when you want to quickly cull
    /// non-overlapping cells, and you will be double checking for false positives later.
    pub fn fast_eq(&self, other: &Self) -> bool {
        self.pre_hash == other.pre_hash
    }

    /// Returns an iterator over all neighboring grid cells and their hashes, within the
    /// `cell_radius`. This iterator will not visit `cell`.
    pub fn neighbors(
        &self,
        cell_radius: u8,
    ) -> impl Iterator<Item = (SpatialHash<P>, GridCell<P>)> + use<'_, P> {
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
                (
                    SpatialHash::from_parent(self.parent, &neighbor_cell),
                    neighbor_cell,
                )
            })
    }
}

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
    F: QueryFilter + Send + Sync + 'static,
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
    F: QueryFilter + Send + Sync + 'static,
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
    F: QueryFilter + Send + Sync + 'static,
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
    F: QueryFilter + Send + Sync + 'static,
{
    /// Update the [`SpatialHashMap`] with entities that have changed [`SpatialHash`]es, and meet
    /// the optional `QueryFilter`.
    fn update(
        mut spatial_map: ResMut<SpatialHashMap<P, F>>,
        mut changed_hashes: ResMut<ChangedSpatialHashes<P, F>>,
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
    F: QueryFilter + Send + Sync + 'static,
{
    initial_hash: Option<SpatialHash<P>>,
    spatial_map: &'a SpatialHashMap<P, F>,
    stack: VecDeque<(SpatialHash<P>, &'a SpatialHashEntry<P>)>,
    visited_cells: HashSet<SpatialHash<P>>,
}

impl<'a, P, F> Iterator for ContiguousNeighborsIter<'a, P, F>
where
    P: GridPrecision,
    F: QueryFilter + Send + Sync + 'static,
{
    type Item = (SpatialHash<P>, &'a SpatialHashEntry<P>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(hash) = self.initial_hash.take() {
            self.stack.push_front((hash, self.spatial_map.get(&hash)?));
            self.visited_cells.insert(hash);
        }
        while let Some((hash, entry)) = self.stack.pop_back() {
            for (neighbor_hash, neighbor_entry) in entry
                .occupied_neighbors
                .iter()
                .filter(|neighbor_hash| self.visited_cells.insert(**neighbor_hash))
                .map(|neighbor_hash| {
                    let entry = self
                        .spatial_map
                        .get(&neighbor_hash)
                        .expect("Neighbor hashes in SpatialHashEntry are guaranteed to exist.");
                    (neighbor_hash, entry)
                })
            {
                self.stack.push_front((*neighbor_hash, neighbor_entry));
            }
            return Some((hash, entry));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use crate::prelude::*;
    use bevy_utils::hashbrown::HashSet;

    #[test]
    fn entity_despawn() {
        use bevy::prelude::*;

        static ENTITY: OnceLock<Entity> = OnceLock::new();

        let setup = |mut commands: Commands| {
            commands.spawn_big_space(ReferenceFrame::<i32>::default(), |root| {
                let entity = root.spawn_spatial(GridCell::<i32>::ZERO).id();
                ENTITY.set(entity).ok();
            });
        };

        let mut app = App::new();
        app.add_plugins(SpatialHashPlugin::<i32>::default())
            .add_systems(Update, setup)
            .update();

        let hash = *app
            .world()
            .entity(*ENTITY.get().unwrap())
            .get::<SpatialHash<i32>>()
            .unwrap();

        assert!(app
            .world()
            .resource::<SpatialHashMap<i32>>()
            .get(&hash)
            .is_some());

        app.world_mut().despawn(*ENTITY.get().unwrap());

        app.update();

        assert!(app
            .world()
            .resource::<SpatialHashMap<i32>>()
            .get(&hash)
            .is_none());
    }

    #[test]
    fn get_hash() {
        use bevy::prelude::*;

        #[derive(Resource, Clone)]
        struct ParentSet {
            a: Entity,
            b: Entity,
            c: Entity,
        }

        #[derive(Resource, Clone)]
        struct ChildSet {
            x: Entity,
            y: Entity,
            z: Entity,
        }

        let setup = |mut commands: Commands| {
            commands.spawn_big_space(ReferenceFrame::<i32>::default(), |root| {
                let a = root.spawn_spatial(GridCell::new(0, 1, 2)).id();
                let b = root.spawn_spatial(GridCell::new(0, 1, 2)).id();
                let c = root.spawn_spatial(GridCell::new(5, 5, 5)).id();

                root.commands().insert_resource(ParentSet { a, b, c });

                root.with_frame_default(|frame| {
                    let x = frame.spawn_spatial(GridCell::new(0, 1, 2)).id();
                    let y = frame.spawn_spatial(GridCell::new(0, 1, 2)).id();
                    let z = frame.spawn_spatial(GridCell::new(5, 5, 5)).id();
                    frame.commands().insert_resource(ChildSet { x, y, z });
                });
            });
        };

        let mut app = App::new();
        app.add_plugins(SpatialHashPlugin::<i32>::default())
            .add_systems(Update, setup);

        app.update();

        let mut spatial_hashes = app.world_mut().query::<&SpatialHash<i32>>();

        let parent = app.world().resource::<ParentSet>().clone();
        let child = app.world().resource::<ChildSet>().clone();

        assert_eq!(
            spatial_hashes.get(app.world(), parent.a).unwrap(),
            spatial_hashes.get(app.world(), parent.b).unwrap(),
            "Same parent, same cell"
        );

        assert_ne!(
            spatial_hashes.get(app.world(), parent.a).unwrap(),
            spatial_hashes.get(app.world(), parent.c).unwrap(),
            "Same parent, different cell"
        );

        assert_eq!(
            spatial_hashes.get(app.world(), child.x).unwrap(),
            spatial_hashes.get(app.world(), child.y).unwrap(),
            "Same parent, same cell"
        );

        assert_ne!(
            spatial_hashes.get(app.world(), child.x).unwrap(),
            spatial_hashes.get(app.world(), child.z).unwrap(),
            "Same parent, different cell"
        );

        assert_ne!(
            spatial_hashes.get(app.world(), parent.a).unwrap(),
            spatial_hashes.get(app.world(), child.x).unwrap(),
            "Same cell, different parent"
        );

        let entities = &app
            .world()
            .resource::<SpatialHashMap<i32>>()
            .get(spatial_hashes.get(app.world(), parent.a).unwrap())
            .unwrap()
            .entities;

        assert!(entities.contains(&parent.a));
        assert!(entities.contains(&parent.b));
        assert!(!entities.contains(&parent.c));
        assert!(!entities.contains(&child.x));
        assert!(!entities.contains(&child.y));
        assert!(!entities.contains(&child.z));
    }

    #[test]
    fn neighbors() {
        use bevy::prelude::*;

        #[derive(Resource, Clone)]
        struct Entities {
            a: Entity,
            b: Entity,
            c: Entity,
        }

        let setup = |mut commands: Commands| {
            commands.spawn_big_space(ReferenceFrame::<i32>::default(), |root| {
                let a = root.spawn_spatial(GridCell::new(0, 0, 0)).id();
                let b = root.spawn_spatial(GridCell::new(1, 1, 1)).id();
                let c = root.spawn_spatial(GridCell::new(2, 2, 2)).id();

                root.commands().insert_resource(Entities { a, b, c });
            });
        };

        let mut app = App::new();
        app.add_plugins(SpatialHashPlugin::<i32>::default())
            .add_systems(Update, setup);

        app.update();

        let entities = app.world().resource::<Entities>().clone();
        let parent = app
            .world_mut()
            .query::<&Parent>()
            .get(app.world(), entities.a)
            .unwrap();

        let map = app.world().resource::<SpatialHashMap<i32>>();
        let entry = map.get(&SpatialHash::new(parent, &GridCell::ZERO)).unwrap();
        let neighbors: HashSet<Entity> =
            map.nearby_flat(entry).map(|(.., entity)| entity).collect();

        assert!(!neighbors.contains(&entities.a));
        assert!(neighbors.contains(&entities.b));
        assert!(!neighbors.contains(&entities.c));

        let flooded: HashSet<Entity> = map
            .nearby_flood(&SpatialHash::new(parent, &GridCell::ZERO))
            .flat_map(|(_hash, entry)| entry.entities.iter().copied())
            .collect();

        assert!(flooded.contains(&entities.a));
        assert!(flooded.contains(&entities.b));
        assert!(flooded.contains(&entities.c));
    }

    #[test]
    fn query_filters() {
        use bevy::prelude::*;

        #[derive(Component)]
        struct Player;

        static ROOT: OnceLock<Entity> = OnceLock::new();

        let setup = |mut commands: Commands| {
            commands.spawn_big_space(ReferenceFrame::<i32>::default(), |root| {
                root.spawn_spatial((GridCell::<i32>::ZERO, Player));
                root.spawn_spatial(GridCell::<i32>::ZERO);
                root.spawn_spatial(GridCell::<i32>::ZERO);
                ROOT.set(root.id()).ok();
            });
        };

        let mut app = App::new();
        app.add_plugins((
            SpatialHashPlugin::<i32>::default(),
            SpatialHashPlugin::<i32, With<Player>>::default(),
            SpatialHashPlugin::<i32, Without<Player>>::default(),
        ))
        .add_systems(Update, setup)
        .update();

        let zero_hash = SpatialHash::from_parent(*ROOT.get().unwrap(), &GridCell::ZERO);

        let map = app.world().resource::<SpatialHashMap<i32>>();
        assert_eq!(
            map.get(&zero_hash).unwrap().entities.iter().count(),
            3,
            "There are a total of 3 spatial entities"
        );

        let map = app.world().resource::<SpatialHashMap<i32, With<Player>>>();
        assert_eq!(
            map.get(&zero_hash).unwrap().entities.iter().count(),
            1,
            "There is only one entity with the Player component"
        );

        let map = app
            .world()
            .resource::<SpatialHashMap<i32, Without<Player>>>();
        assert_eq!(
            map.get(&zero_hash).unwrap().entities.iter().count(),
            2,
            "There are two entities without the player component"
        );
    }
}
