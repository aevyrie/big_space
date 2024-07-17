//! Spatial hashing acceleration structure.

use std::{
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::Parent;
use bevy_math::IVec3;
use bevy_reflect::Reflect;
use bevy_utils::{
    hashbrown::{HashMap, HashSet},
    AHasher, PassHash,
};

use crate::{precision::GridPrecision, GridCell};

/// Add spatial hashing acceleration to `big_space`, accessible through the [`SpatialHashMap`]
/// resource, and [`SpatialHash`] components.
#[derive(Default)]
pub struct SpatialHashPlugin<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> Plugin for SpatialHashPlugin<P> {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialHashMap<P>>().add_systems(
            PostUpdate,
            SpatialHashMap::<P>::update_spatial_hash
                .after(crate::FloatingOriginSet::RecenterLargeTransforms)
                .in_set(bevy_transform::TransformSystem::TransformPropagate),
        );
    }
}

/// A global spatial hash map for quickly finding entities in a grid cell.
#[derive(Resource, Default)]
pub struct SpatialHashMap<P: GridPrecision> {
    map: HashMap<SpatialHash<P>, HashSet<Entity, PassHash>, PassHash>,
    reverse_map: HashMap<Entity, SpatialHash<P>, PassHash>,
}

/// An automatically updated `Component` that uniquely identifies an entity's cell.
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
/// that could not possibly overlap; if the spatial hashes do not match, you can be certain they are
/// not in the same cell.
#[derive(Component, Clone, Copy, Eq, Debug, Reflect)]
pub struct SpatialHash<P: GridPrecision>(u64, PhantomData<P>);

impl<P: GridPrecision> PartialEq for SpatialHash<P> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

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
}

impl<P: GridPrecision> SpatialHashMap<P> {
    fn insert(&mut self, entity: Entity, hash: SpatialHash<P>) {
        // If this entity is already in the maps, we need to remove and update it.
        if let Some(old_hash) = self.reverse_map.get_mut(&entity) {
            if hash.eq(old_hash) {
                return; // If the spatial hash is unchanged, early exit.
            }
            self.map
                .get_mut(old_hash)
                .map(|entities| entities.remove(&entity));
            *old_hash = hash;
        }

        self.map
            .entry(hash)
            .and_modify(|list| {
                list.insert(entity);
            })
            .or_insert_with(|| {
                let mut hm = HashSet::with_hasher(PassHash);
                hm.insert(entity);
                hm
            });
    }

    /// Get a list of all entities in the same [`GridCell`] using a [`SpatialHash`].
    #[inline]
    pub fn get(&self, hash: &SpatialHash<P>) -> Option<&HashSet<Entity, PassHash>> {
        self.map.get(hash)
    }

    /// An iterator visiting all spatial hash cells and their contents in arbitrary order.
    #[inline]
    pub fn iter(
        &self,
    ) -> bevy_utils::hashbrown::hash_map::Iter<'_, SpatialHash<P>, HashSet<Entity, PassHash>> {
        self.map.iter()
    }

    /// Find entities in this and neighboring cells, within `cell_radius`.
    ///
    /// A radius of `1` will search all cells within a Chebyshev distance of `1`, or a total of 9
    /// cells. You can also think of this as a cube centered on the specified cell, expanded in each
    /// direction by `radius`.
    pub fn neighbors<'a>(
        &'a self,
        cell_radius: u8,
        parent: &'a Parent,
        cell: GridCell<P>,
    ) -> impl Iterator<Item = &Entity> + 'a {
        let radius = cell_radius as i32;
        let search_width = 1 + 2 * radius;
        let search_volume = search_width.pow(3);
        let center = -radius;
        let hash = PartialSpatialHash::new(parent);
        (0..search_volume)
            .filter_map(move |i| {
                let x = center + i; //  % search_width.pow(0)
                let y = center + i % search_width; // .pow(1)
                let z = center + i % search_width.pow(2);
                let offset = IVec3::new(x, y, z);
                let hash = hash.generate(&(cell + offset));
                self.get(&hash).map(|set| set.iter())
            })
            .flatten()
    }

    fn update_spatial_hash(
        mut commands: Commands,
        mut spatial: ResMut<SpatialHashMap<P>>,
        changed_entities: Query<
            (Entity, &Parent, &GridCell<P>),
            Or<(Changed<Parent>, Changed<GridCell<P>>)>,
        >,
    ) {
        // This simple sequential impl is faster than the parallel versions I've tried.
        for (entity, parent, cell) in &changed_entities {
            let spatial_hash = SpatialHash::new(parent, cell);
            commands.entity(entity).insert(spatial_hash);
            spatial.insert(entity, spatial_hash);
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
        let mut hasher = AHasher::default();
        hasher.write_u64(parent.to_bits());
        PartialSpatialHash {
            hasher,
            spooky: PhantomData,
        }
    }

    /// Generate a mew, fully complete [`SpatialHash`] by providing the other required half of the
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
    use bevy_utils::hashbrown::HashSet;

    use crate::{
        spatial_hash::{SpatialHash, SpatialHashMap, SpatialHashPlugin},
        BigSpaceCommands, GridCell, ReferenceFrame,
    };

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

        let entities = app
            .world()
            .resource::<SpatialHashMap<i32>>()
            .get(spatial_hashes.get(app.world(), parent.a).unwrap())
            .unwrap();

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
                let c = root.spawn_spatial(GridCell::new(-2, -2, -2)).id();

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

        let neighbors: HashSet<Entity> = app
            .world()
            .resource::<SpatialHashMap<i32>>()
            .neighbors(1, parent, GridCell::new(0, 0, 0))
            .copied()
            .collect();

        assert!(neighbors.contains(&entities.a));
        assert!(neighbors.contains(&entities.b));
        assert!(!neighbors.contains(&entities.c));
    }
}
