//! Spatial hashing acceleration structure. See [`SpatialHashPlugin`].

use std::{
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, query::QueryFilter};
use bevy_hierarchy::Parent;
use bevy_math::IVec3;
use bevy_reflect::{Reflect, TypePath};
use bevy_utils::{
    hashbrown::{HashMap, HashSet},
    AHasher, Duration, Instant, PassHash,
};

use crate::{precision::GridPrecision, GridCell};

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

impl<P: GridPrecision + TypePath, F: QueryFilter + Send + Sync + 'static> Plugin
    for SpatialHashPlugin<P, F>
{
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialHashMap<P, F>>()
            .register_type::<SpatialHash<P>>()
            .init_resource::<SpatialHashStats>()
            .register_type::<SpatialHashStats>()
            .add_systems(
                PostUpdate,
                (
                    SpatialHashStats::reset.in_set(SpatialHashSet::Init),
                    Self::update_spatial_hashes
                        .in_set(SpatialHashSet::UpdateHash)
                        .after(SpatialHashSet::Init)
                        .after(crate::FloatingOriginSet::RecenterLargeTransforms),
                    SpatialHashMap::<P, F>::update
                        .in_set(SpatialHashSet::UpdateMap)
                        .after(SpatialHashSet::UpdateHash),
                )
                    .in_set(bevy_transform::TransformSystem::TransformPropagate),
            );
    }
}

impl<P: GridPrecision, F: QueryFilter> SpatialHashPlugin<P, F> {
    /// Update or insert the [`SpatialHash`] of all changed entities that match the optional
    /// `QueryFilter`.
    fn update_spatial_hashes(
        mut commands: Commands,
        mut spatial_entities: ParamSet<(
            Query<
                (&Parent, &GridCell<P>, &mut SpatialHash<P>),
                (F, Or<(Changed<Parent>, Changed<GridCell<P>>)>),
            >,
            Query<(Entity, &Parent, &GridCell<P>), Without<SpatialHash<P>>>,
        )>,
        mut stats: ResMut<SpatialHashStats>,
    ) {
        let start = Instant::now();

        // Create new
        for (entity, parent, cell) in spatial_entities.p1().iter() {
            let spatial_hash = SpatialHash::new(parent, cell);
            commands.entity(entity).insert(spatial_hash);
        }

        // Update existing
        spatial_entities
            .p0()
            .par_iter_mut()
            .for_each(|(parent, cell, mut old_hash)| {
                let spatial_hash = SpatialHash::new(parent, cell);
                // This check has a 40% savings in cases where the grid cell is mutated (change
                // detection triggered), but it has not actually changed, this also helps if
                // multiple plugins are updating the spatial hash, and it is already correct.
                if old_hash.ne(&spatial_hash) {
                    *old_hash = spatial_hash;
                }
            });

        stats.hash_update_duration += start.elapsed();
    }
}

/// Aggregate runtime statistics across all [`SpatialHashPlugin`]s.
#[derive(Resource, Debug, Clone, Default, Reflect)]
pub struct SpatialHashStats {
    hash_update_duration: Duration,
    map_update_duration: Duration,
}

impl SpatialHashStats {
    fn reset(mut stats: ResMut<SpatialHashStats>) {
        *stats = Self::default();
    }

    /// Time to update all entity hashes.
    pub fn hash_update_duration(&self) -> Duration {
        self.hash_update_duration
    }

    /// Time to update all spatial hash maps.
    pub fn map_update_duration(&self) -> Duration {
        self.map_update_duration
    }
}

/// System sets for [`SpatialHashPlugin`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum SpatialHashSet {
    /// Setup.
    Init,
    /// [`SpatialHash`] updated.
    UpdateHash,
    /// [`SpatialHashMap`] updated.
    UpdateMap,
}

/// A`Component` storing an automatically-updated hash of this entity's high-precision position,
/// derived from its [`GridCell`] and [`Parent`].
///
/// Once computed, a spatial hash can be used to rapidly check if any two entities are in the same
/// cell, by comparing their spatial hashes. You can also get a list of all entities within a cell
/// using the [`SpatialHashMap`] resource.
///
/// Due to reference frames and multiple big spaces in a single world, this must use both the
/// [`GridCell`] and the [`Parent`] of the entity to uniquely identify its position. These two
/// values are then hashed and stored in this spatial hash component.
///
/// # WARNING
///
/// Like all hashes, it is possible to encounter collisions. If two spatial hashes are identical,
/// this does ***not*** guarantee that these two entities are located in the same cell. If the
/// hashes are *not* equal, however, this ***does*** guarantee that the entities are in different
/// cells.
///
/// This means you should only use spatial hashes to accelerate checks by filtering out entities
/// that could not possibly overlap: if the spatial hashes do not match, you can be certain they are
/// not in the same cell.
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct SpatialHash<P: GridPrecision>(u64, #[reflect(ignore)] PhantomData<P>);

impl<P: GridPrecision> PartialEq for SpatialHash<P> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<P: GridPrecision> Eq for SpatialHash<P> {}

impl<P: GridPrecision> Hash for SpatialHash<P> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

impl<P: GridPrecision> SpatialHash<P> {
    /// Generate a new hash from parts.
    #[inline]
    pub fn new(parent: &Parent, cell: &GridCell<P>) -> Self {
        PartialSpatialHash::new(parent).generate(cell)
    }

    /// Effectively the same as [`Self::new`], but uses `Entity` instead of `Parent` as the input.
    ///
    /// We use `Parent` on the "happy path" to help prevent errors when passing in the wrong entity.
    /// Using `Parent` also makes it obvious that when you are querying the `GridCell` of the
    /// entity, you should add `Parent` to the query, *not* the `Entity`.
    #[inline]
    pub fn from_parent(parent: Entity, cell: &GridCell<P>) -> Self {
        PartialSpatialHash::from_parent(parent).generate(cell)
    }
}

/// An entry in a [`SpatialHashMap`].
#[derive(Clone, Debug)]
pub struct SpatialHashEntry<P: GridPrecision> {
    /// The reference frame entity that this grid cell and entities are a child of.
    pub reference_frame: Entity,
    /// The grid cell coordinate of this spatial hash.
    pub cell: GridCell<P>,
    /// All the entities located in this grid cell.
    pub entities: HashSet<Entity, PassHash>,
}

/// A global spatial hash map for quickly finding entities in a grid cell.
#[derive(Resource, Clone)]
pub struct SpatialHashMap<P, F = ()>
where
    P: GridPrecision,
    F: QueryFilter + Send + Sync + 'static,
{
    map: HashMap<SpatialHash<P>, SpatialHashEntry<P>, PassHash>,
    reverse_map: HashMap<Entity, SpatialHash<P>, PassHash>,
    /// Creating and freeing hash sets is expensive. To reduce time spent allocating and running
    /// destructors, we save any hash sets that would otherwise be thrown away. The next time we
    /// need to construct a new hash set of entities, we can grab one here.
    ///
    /// <https://en.wikipedia.org/wiki/Object_pool_pattern>.
    hash_set_pool: Vec<HashSet<Entity, PassHash>>,
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
            hash_set_pool: Default::default(),
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
        changed_entities: Query<
            (Entity, &SpatialHash<P>, &Parent, &GridCell<P>),
            (F, Changed<SpatialHash<P>>),
        >,
        mut removed: RemovedComponents<SpatialHash<P>>,
        mut stats: ResMut<SpatialHashStats>,
        mut destroy_list: Local<Vec<SpatialHash<P>>>,
    ) {
        let start = Instant::now();

        for entity in removed.read() {
            spatial_map.remove(entity)
        }

        for (entity, spatial_hash, parent, cell) in &changed_entities {
            spatial_map.insert_or_update(entity, *spatial_hash, parent, cell);
        }

        spatial_map.clean_up_empty_sets(&mut destroy_list);

        stats.map_update_duration += start.elapsed();
    }

    #[inline]
    fn insert_or_update(
        &mut self,
        entity: Entity,
        hash: SpatialHash<P>,
        parent: &Parent,
        cell: &GridCell<P>,
    ) {
        // If this entity is already in the maps, we need to remove and update it.
        if let Some(old_hash) = self.reverse_map.get_mut(&entity) {
            if hash.eq(old_hash) {
                return; // If the spatial hash is unchanged, early exit.
            }
            Self::remove_from_map(entity, *old_hash, &mut self.map);
            *old_hash = hash;
        } else {
            self.reverse_map.insert(entity, hash);
        }

        self.map
            .entry(hash)
            .and_modify(|entry| {
                entry.entities.insert(entity);
            })
            .or_insert_with(|| {
                let mut entities = self.hash_set_pool.pop().unwrap_or_default();
                entities.insert(entity);
                SpatialHashEntry {
                    reference_frame: parent.get(),
                    cell: *cell,
                    entities,
                }
            });
    }

    /// Remove an entity from the [`SpatialHashMap`].
    #[inline]
    fn remove(&mut self, entity: Entity) {
        if let Some(old_hash) = self.reverse_map.remove(&entity) {
            Self::remove_from_map(entity, old_hash, &mut self.map)
        }
    }

    #[inline]
    fn remove_from_map(
        entity: Entity,
        old_hash: SpatialHash<P>,
        map: &mut HashMap<SpatialHash<P>, SpatialHashEntry<P>, PassHash>,
    ) {
        if let Some(entry) = map.get_mut(&old_hash) {
            entry.entities.remove(&entity);
        }
    }

    fn clean_up_empty_sets(&mut self, destroy_list: &mut Local<Vec<SpatialHash<P>>>) {
        **destroy_list = self
            .map
            .iter()
            .filter(|(_k, v)| v.entities.is_empty())
            .map(|(k, _v)| *k)
            .collect();
        for empty_key in destroy_list.iter() {
            if let Some(old_entry) = self.map.remove(empty_key) {
                self.hash_set_pool.push(old_entry.entities);
            }
        }
        destroy_list.clear();
    }

    /// Get a list of all entities in the same [`GridCell`] using a [`SpatialHash`].
    #[inline]
    pub fn get(&self, hash: &SpatialHash<P>) -> Option<&SpatialHashEntry<P>> {
        self.map.get(hash)
    }

    /// An iterator visiting all spatial hash cells and their contents in arbitrary order.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&SpatialHash<P>, &SpatialHashEntry<P>)> {
        self.map.iter()
    }

    /// Find entities in this and neighboring cells, within `cell_radius`.
    ///
    /// A radius of `1` will search all cells within a Chebyshev distance of `1`, or a total of 9
    /// cells. You can also think of this as a cube centered on the specified cell, expanded in each
    /// direction by `radius`.
    ///
    /// Returns an iterator over all non-empty neighboring cells, including the cell, and the set of
    /// entities in that cell.
    ///
    /// This is a lazy query, if you don't consume the iterator, it won't do any work!
    pub fn neighbors(
        &self,
        cell_radius: u8,
        parent: &Parent,
        cell: GridCell<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, &SpatialHashEntry<P>)> + '_ {
        self.neighbor_hashes(cell_radius, parent, cell).filter_map(
            |(neighbor_hash, _neighbor_cell)| {
                self.get(&neighbor_hash)
                    .map(|neighbor_entry| (neighbor_hash, neighbor_entry))
            },
        )
    }

    /// Returns an iterator over all neighboring grid cells and their hashes, within the
    /// `cell_radius`. This iterator will also visit `cell`.
    pub fn neighbor_hashes<'a>(
        &'a self,
        cell_radius: u8,
        parent: &'a Parent,
        cell: GridCell<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, GridCell<P>)> {
        let radius = cell_radius as i32;
        let search_width = 1 + 2 * radius;
        let search_volume = search_width.pow(3);
        let center = -radius;
        let partial_hash = PartialSpatialHash::new(parent);
        (0..search_volume).map(move |i| {
            let x = center + (i/* / search_width.pow(0) */) % search_width;
            let y = center + (i / search_width/*.pow(1) */) % search_width;
            let z = center + (i / search_width.pow(2)) % search_width;
            let offset = IVec3::new(x, y, z);
            let neighbor_cell = cell + offset;
            (partial_hash.generate(&neighbor_cell), neighbor_cell)
        })
    }

    /// Like [`Self::neighbors`], but flattens the included set of entities into a flat list.
    pub fn neighbors_flat(
        &self,
        cell_radius: u8,
        parent: &Parent,
        cell: GridCell<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, GridCell<P>, Entity)> + '_ {
        self.neighbors(cell_radius, parent, cell)
            .flat_map(|(hash, entry)| {
                entry
                    .entities
                    .iter()
                    .map(move |entity| (hash, entry.cell, *entity))
            })
    }

    /// Recursively searches for all connected neighboring cells within the given `cell_radius` at
    /// every point. The result is a set of all grid cells connected by a cell distance of
    /// `max_distance` or less.
    pub fn neighbors_contiguous<'a>(
        &'a self,
        max_distance: u8,
        parent: &'a Parent,
        cell: GridCell<P>,
    ) -> impl Iterator<Item = (SpatialHash<P>, &'a SpatialHashEntry<P>)> {
        let mut pushed_cells = HashSet::with_capacity_and_hasher(self.map.len() / 2, PassHash);
        pushed_cells.insert(SpatialHash::new(parent, &cell));
        ContiguousNeighborsIter {
            spatial_map: self,
            max_distance,
            parent,
            stack: vec![cell],
            pushed_cells,
        }
    }
}

/// An iterator over the neighbors of a cell.
pub struct ContiguousNeighborsIter<'a, P, F>
where
    P: GridPrecision,
    F: QueryFilter + Send + Sync + 'static,
{
    spatial_map: &'a SpatialHashMap<P, F>,
    max_distance: u8,
    parent: &'a Parent,
    stack: Vec<GridCell<P>>,
    pushed_cells: HashSet<SpatialHash<P>, PassHash>,
}

impl<'a, P, F> Iterator for ContiguousNeighborsIter<'a, P, F>
where
    P: GridPrecision,
    F: QueryFilter + Send + Sync + 'static,
{
    type Item = (SpatialHash<P>, &'a SpatialHashEntry<P>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cell = self.stack.pop()?;

            // We know the current cell contains some entities, so we should push all neighbors with
            // entities onto the stack.
            self.spatial_map
                .neighbor_hashes(self.max_distance, self.parent, cell)
                .for_each(|(neighbor_hash, neighbor_cell)| {
                    if self.pushed_cells.insert(neighbor_hash)
                        && self.spatial_map.map.contains_key(&neighbor_hash)
                    {
                        self.stack.push(neighbor_cell);
                    }
                });

            let hash = SpatialHash::new(self.parent, &cell);
            if let Some(neighbor) = self.spatial_map.get(&hash).map(|entry| (hash, entry)) {
                return Some(neighbor);
            }
        }
    }
}

/// A halfway-hashed [`SpatialHash`], only taking into account the parent, and not the cell. This
/// allows for reusing the first half of the hash when computing spatial hashes of many cells in the
/// same reference frame. Reducing the amount of hashing can help performance in those cases.
pub struct PartialSpatialHash<P: GridPrecision> {
    hasher: AHasher,
    spooky: PhantomData<P>,
}

impl<P: GridPrecision> PartialSpatialHash<P> {
    /// Create a partial spatial hash from the parent of the hashed entity.
    pub fn new(parent: &Parent) -> Self {
        Self::from_parent(**parent)
    }

    /// When you don't have access to the `Parent`, but you do have the `Entity`. Careful not to use
    /// the wrong `Entity`!
    pub fn from_parent(parent: Entity) -> Self {
        let mut hasher = AHasher::default();
        hasher.write_u64(parent.to_bits());
        PartialSpatialHash {
            hasher,
            spooky: PhantomData,
        }
    }

    /// Generate a new, fully complete [`SpatialHash`] by providing the other required half of the
    /// hash - the grid cell. This function can be called many times.
    #[inline]
    pub fn generate(&self, cell: &GridCell<P>) -> SpatialHash<P> {
        let mut hasher_clone = self.hasher.clone();
        cell.hash(&mut hasher_clone);
        SpatialHash(hasher_clone.finish(), PhantomData)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use bevy_utils::hashbrown::HashSet;

    use crate::{
        spatial_hash::{SpatialHash, SpatialHashMap, SpatialHashPlugin},
        BigSpaceCommands, GridCell, ReferenceFrame,
    };

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
        let neighbors: HashSet<Entity> = map
            .neighbors_flat(1, parent, GridCell::ZERO)
            .map(|(.., entity)| entity)
            .collect();

        assert!(neighbors.contains(&entities.a));
        assert!(neighbors.contains(&entities.b));
        assert!(!neighbors.contains(&entities.c));

        let flooded: HashSet<Entity> = map
            .neighbors_contiguous(1, parent, GridCell::ZERO)
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
