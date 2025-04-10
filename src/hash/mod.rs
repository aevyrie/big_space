//! Spatial hashing acceleration structure. See [`GridHashPlugin`].

use core::marker::PhantomData;

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, query::QueryFilter};
use bevy_platform_support::prelude::*;

pub mod component;
pub mod map;
pub mod partition;

/// Add spatial hashing acceleration to `big_space`, accessible through the [`GridHashMap`] resource,
/// and [`GridHash`] components.
///
/// You can optionally add a [`GridHashMapFilter`] to this plugin, to only run the spatial hashing on
/// entities that match the query filter. This is useful if you only want to, say, compute hashes
/// and insert in the [`GridHashMap`] for `Player` entities.
///
/// If you are adding multiple copies of this plugin with different filters, there are optimizations
/// in place to avoid duplicating work. However, you should still take care to avoid excessively
/// overlapping filters.
pub struct GridHashPlugin<F = ()>(PhantomData<F>)
where
    F: GridHashMapFilter;

impl<F> Plugin for GridHashPlugin<F>
where
    F: GridHashMapFilter,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<GridHashMap<F>>()
            .init_resource::<ChangedGridHashes<F>>()
            .register_type::<GridHash>()
            .add_systems(
                PostUpdate,
                (
                    GridHash::update::<F>
                        .in_set(GridHashMapSystem::UpdateHash)
                        .after(FloatingOriginSystem::RecenterLargeTransforms),
                    GridHashMap::<F>::update
                        .in_set(GridHashMapSystem::UpdateMap)
                        .after(GridHashMapSystem::UpdateHash),
                ),
            );
    }
}

impl<F: GridHashMapFilter> Default for GridHashPlugin<F> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

/// System sets for [`GridHashPlugin`].
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum GridHashMapSystem {
    /// [`GridHash`] updated.
    UpdateHash,
    /// [`GridHashMap`] updated.
    UpdateMap,
    /// [`GridPartitionMap`] updated.
    UpdatePartition,
}

/// Used as a [`QueryFilter`] to include or exclude certain types of entities from spatial
/// hashing.The trait is automatically implemented for all compatible types, like [`With`] or
/// [`Without`].
///
/// By default, this is `()`, but it can be overridden when adding the [`GridHashPlugin`] and
/// [`GridHashMap`]. For example, if you use `With<Players>` as your filter, only `Player`s would be
/// considered when building spatial hash maps. This is useful when you only care about querying
/// certain entities, and want to avoid the plugin doing bookkeeping work for entities you don't
/// care about.
pub trait GridHashMapFilter: QueryFilter + Send + Sync + 'static {}
impl<T: QueryFilter + Send + Sync + 'static> GridHashMapFilter for T {}

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
struct ChangedGridHashes<F: GridHashMapFilter> {
    updated: Vec<Entity>,
    spooky: PhantomData<F>,
}

impl<F: GridHashMapFilter> Default for ChangedGridHashes<F> {
    fn default() -> Self {
        Self {
            updated: Vec::new(),
            spooky: PhantomData,
        }
    }
}

// TODO:
//
// - When an entity is re-parented, is is removed/updated in the spatial map?
// - Entities are hashed with their parent - what happens if an entity is moved to the root? Is the
//   hash ever recomputed? Is it removed? Is the spatial map updated?
#[cfg(test)]
mod tests {
    use crate::{hash::map::SpatialEntryToEntities, prelude::*};
    use bevy_platform_support::{collections::HashSet, sync::OnceLock};

    #[test]
    fn entity_despawn() {
        use bevy::prelude::*;

        static ENTITY: OnceLock<Entity> = OnceLock::new();

        let setup = |mut commands: Commands| {
            commands.spawn_big_space_default(|root| {
                let entity = root.spawn_spatial(GridCell::ZERO).id();
                ENTITY.set(entity).ok();
            });
        };

        let mut app = App::new();
        app.add_plugins(GridHashPlugin::<()>::default())
            .add_systems(Update, setup)
            .update();

        let hash = *app
            .world()
            .entity(*ENTITY.get().unwrap())
            .get::<GridHash>()
            .unwrap();

        assert!(app.world().resource::<GridHashMap>().get(&hash).is_some());

        app.world_mut().despawn(*ENTITY.get().unwrap());

        app.update();

        assert!(app.world().resource::<GridHashMap>().get(&hash).is_none());
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
            commands.spawn_big_space_default(|root| {
                let a = root.spawn_spatial(GridCell::new(0, 1, 2)).id();
                let b = root.spawn_spatial(GridCell::new(0, 1, 2)).id();
                let c = root.spawn_spatial(GridCell::new(5, 5, 5)).id();

                root.commands().insert_resource(ParentSet { a, b, c });

                root.with_grid_default(|grid| {
                    let x = grid.spawn_spatial(GridCell::new(0, 1, 2)).id();
                    let y = grid.spawn_spatial(GridCell::new(0, 1, 2)).id();
                    let z = grid.spawn_spatial(GridCell::new(5, 5, 5)).id();
                    grid.commands().insert_resource(ChildSet { x, y, z });
                });
            });
        };

        let mut app = App::new();
        app.add_plugins(GridHashPlugin::<()>::default())
            .add_systems(Update, setup);

        app.update();

        let mut spatial_hashes = app.world_mut().query::<&GridHash>();

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
            .resource::<GridHashMap>()
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
            commands.spawn_big_space_default(|root| {
                let a = root.spawn_spatial(GridCell::new(0, 0, 0)).id();
                let b = root.spawn_spatial(GridCell::new(1, 1, 1)).id();
                let c = root.spawn_spatial(GridCell::new(2, 2, 2)).id();

                root.commands().insert_resource(Entities { a, b, c });
            });
        };

        let mut app = App::new();
        app.add_plugins(GridHashPlugin::<()>::default())
            .add_systems(Startup, setup);

        app.update();

        let entities = app.world().resource::<Entities>().clone();
        let parent = app
            .world_mut()
            .query::<&ChildOf>()
            .get(app.world(), entities.a)
            .unwrap();

        let map = app.world().resource::<GridHashMap>();
        let entry = map.get(&GridHash::new(parent, &GridCell::ZERO)).unwrap();
        let neighbors: HashSet<Entity> = map.nearby(entry).entities().collect();

        assert!(neighbors.contains(&entities.a));
        assert!(neighbors.contains(&entities.b));
        assert!(!neighbors.contains(&entities.c));

        let flooded: HashSet<Entity> = map
            .flood(&GridHash::new(parent, &GridCell::ZERO), None)
            .entities()
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
            commands.spawn_big_space_default(|root| {
                root.spawn_spatial((GridCell::ZERO, Player));
                root.spawn_spatial(GridCell::ZERO);
                root.spawn_spatial(GridCell::ZERO);
                ROOT.set(root.id()).ok();
            });
        };

        let mut app = App::new();
        app.add_plugins((
            GridHashPlugin::<()>::default(),
            GridHashPlugin::<With<Player>>::default(),
            GridHashPlugin::<Without<Player>>::default(),
        ))
        .add_systems(Startup, setup)
        .update();

        let zero_hash = GridHash::from_parent(*ROOT.get().unwrap(), &GridCell::ZERO);

        let map = app.world().resource::<GridHashMap>();
        assert_eq!(
            map.get(&zero_hash).unwrap().entities.iter().count(),
            3,
            "There are a total of 3 spatial entities"
        );

        let map = app.world().resource::<GridHashMap<With<Player>>>();
        assert_eq!(
            map.get(&zero_hash).unwrap().entities.iter().count(),
            1,
            "There is only one entity with the Player component"
        );

        let map = app.world().resource::<GridHashMap<Without<Player>>>();
        assert_eq!(
            map.get(&zero_hash).unwrap().entities.iter().count(),
            2,
            "There are two entities without the player component"
        );
    }

    /// Verify that [`GridHashMap::just_removed`] and [`GridHashMap::just_inserted`] work correctly when
    /// entities are spawned and move between cells.
    #[test]
    fn spatial_map_changed_cell_tracking() {
        use bevy::prelude::*;

        #[derive(Resource, Clone)]
        struct Entities {
            a: Entity,
            b: Entity,
            c: Entity,
        }

        let setup = |mut commands: Commands| {
            commands.spawn_big_space_default(|root| {
                let a = root.spawn_spatial(GridCell::new(0, 0, 0)).id();
                let b = root.spawn_spatial(GridCell::new(1, 1, 1)).id();
                let c = root.spawn_spatial(GridCell::new(2, 2, 2)).id();

                root.commands().insert_resource(Entities { a, b, c });
            });
        };

        let mut app = App::new();
        app.add_plugins((BigSpacePlugin::default(), GridHashPlugin::<()>::default()))
            .add_systems(Startup, setup);

        app.update();

        let entities = app.world().resource::<Entities>().clone();
        let get_hash = |app: &mut App, entity| {
            *app.world_mut()
                .query::<&GridHash>()
                .get(app.world(), entity)
                .unwrap()
        };

        let a_hash_t0 = get_hash(&mut app, entities.a);
        let b_hash_t0 = get_hash(&mut app, entities.b);
        let c_hash_t0 = get_hash(&mut app, entities.c);
        let map = app.world().resource::<GridHashMap>();
        assert!(map.just_inserted().contains(&a_hash_t0));
        assert!(map.just_inserted().contains(&b_hash_t0));
        assert!(map.just_inserted().contains(&c_hash_t0));

        // Move entities and run an update
        app.world_mut()
            .entity_mut(entities.a)
            .get_mut::<GridCell>()
            .unwrap()
            .z += 1;
        app.world_mut()
            .entity_mut(entities.b)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .z += 1e10;
        app.update();

        let a_hash_t1 = get_hash(&mut app, entities.a);
        let b_hash_t1 = get_hash(&mut app, entities.b);
        let c_hash_t1 = get_hash(&mut app, entities.c);
        let map = app.world().resource::<GridHashMap>();

        // Last grid
        assert!(map.just_removed().contains(&a_hash_t0)); // Moved cell
        assert!(map.just_removed().contains(&b_hash_t0)); // Moved cell via transform
        assert!(!map.just_removed().contains(&c_hash_t0)); // Did not move

        // Current grid
        assert!(map.just_inserted().contains(&a_hash_t1)); // Moved cell
        assert!(map.just_inserted().contains(&b_hash_t1)); // Moved cell via transform
        assert!(!map.just_inserted().contains(&c_hash_t1)); // Did not move
    }
}
