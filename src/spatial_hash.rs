//! Spatial hashing acceleration structure.

use std::{
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::Parent;
use bevy_reflect::Reflect;
use bevy_utils::{
    hashbrown::{hash_map::Iter, HashMap, HashSet},
    PassHash,
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<P: GridPrecision> SpatialHash<P> {
    /// Generate a new hash from parts.
    pub fn new(parent: &Parent, cell: &GridCell<P>) -> Self {
        let hasher = &mut bevy_utils::AHasher::default();
        parent.hash(hasher);
        cell.hash(hasher);
        Self(hasher.finish(), PhantomData)
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
    pub fn get(&self, hash: &SpatialHash<P>) -> Option<&HashSet<Entity, PassHash>> {
        self.map.get(hash)
    }

    /// An iterator visiting all spatial hash cells in arbitrary order.
    pub fn iter(&self) -> Iter<'_, SpatialHash<P>, HashSet<Entity, PassHash>> {
        self.map.iter()
    }

    fn update_spatial_hash(
        mut commands: Commands,
        mut spatial: ResMut<SpatialHashMap<P>>,
        changed_entities: Query<
            (Entity, &Parent, &GridCell<P>),
            Or<(Changed<Parent>, Changed<GridCell<P>>)>,
        >,
    ) {
        for (entity, parent, cell) in &changed_entities {
            let spatial_hash = SpatialHash::new(parent, cell);
            commands.entity(entity).insert(spatial_hash);
            spatial.insert(entity, spatial_hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        spatial_hash::{SpatialHash, SpatialHashMap, SpatialHashPlugin},
        BigSpaceCommands, GridCell, ReferenceFrame,
    };

    #[test]
    fn comprehensive() {
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

        let mut spatial_hashes = app.world.query::<&SpatialHash<i32>>();

        let parent = app.world.resource::<ParentSet>().clone();
        let child = app.world.resource::<ChildSet>().clone();

        assert_eq!(
            spatial_hashes.get(&app.world, parent.a).unwrap(),
            spatial_hashes.get(&app.world, parent.b).unwrap(),
            "Same parent, same cell"
        );

        assert_ne!(
            spatial_hashes.get(&app.world, parent.a).unwrap(),
            spatial_hashes.get(&app.world, parent.c).unwrap(),
            "Same parent, different cell"
        );

        assert_eq!(
            spatial_hashes.get(&app.world, child.x).unwrap(),
            spatial_hashes.get(&app.world, child.y).unwrap(),
            "Same parent, same cell"
        );

        assert_ne!(
            spatial_hashes.get(&app.world, child.x).unwrap(),
            spatial_hashes.get(&app.world, child.z).unwrap(),
            "Same parent, different cell"
        );

        assert_ne!(
            spatial_hashes.get(&app.world, parent.a).unwrap(),
            spatial_hashes.get(&app.world, child.x).unwrap(),
            "Same cell, different parent"
        );

        let entities = app
            .world
            .resource::<SpatialHashMap<i32>>()
            .get(spatial_hashes.get(&app.world, parent.a).unwrap())
            .unwrap();

        assert!(entities.contains(&parent.a));
        assert!(entities.contains(&parent.b));
        assert!(!entities.contains(&parent.c));
        assert!(!entities.contains(&child.x));
        assert!(!entities.contains(&child.y));
        assert!(!entities.contains(&child.z));
    }
}
