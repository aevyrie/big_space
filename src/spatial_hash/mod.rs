//! Spatial hashing acceleration structure. See [`SpatialHashPlugin`].

use std::marker::PhantomData;

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::{prelude::*, query::QueryFilter};

pub mod component;
pub mod map;

/// Add spatial hashing acceleration to `big_space`, accessible through the [`SpatialHashMap`]
/// resource, and [`SpatialHash`] components.
///
/// You can optionally add a [`SpatialHashFilter`] to this plugin, to only run the spatial hashing
/// on entities that match the query filter. This is useful if you only want to, say, compute hashes
/// and insert in the [`SpatialHashMap`] for `Player` entities.
///
/// If you are adding multiple copies of this plugin with different filters, there are optimizations
/// in place to avoid duplicating work. However, you should still take care to avoid excessively
/// overlapping filters.
pub struct SpatialHashPlugin<P: GridPrecision, F: SpatialHashFilter = ()>(PhantomData<(P, F)>);

impl<P: GridPrecision, F: SpatialHashFilter> Plugin for SpatialHashPlugin<P, F> {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialHashMap<P, F>>()
            .init_resource::<ChangedSpatialHashes<P, F>>()
            .register_type::<SpatialHash<P>>()
            .add_systems(
                PostUpdate,
                (
                    SpatialHash::<P>::update::<F>
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

impl<P: GridPrecision, F: SpatialHashFilter> Default for SpatialHashPlugin<P, F> {
    fn default() -> Self {
        Self(PhantomData)
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

/// Used as a [`QueryFilter`] to include or exclude certain types of entities from spatial
/// hashing.The trait is automatically implemented for all compatible types, like [`With`] or
/// [`Without`].
///
/// By default, this is `()`, but it can be overidden when adding the [`SpatialHashPlugin`] and
/// [`SpatialHashMap`]. For example, if you use `With<Players>` as your filter, only `Player`s would
/// be considered when building spatial hash maps. This is useful when you only care about querying
/// certain entities, and want to avoid the plugin doing bookkeeping work for entities you don't
/// care about.
pub trait SpatialHashFilter: QueryFilter + Send + Sync + 'static {}
impl<T: QueryFilter + Send + Sync + 'static> SpatialHashFilter for T {}

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
struct ChangedSpatialHashes<P: GridPrecision, F: SpatialHashFilter> {
    list: Vec<Entity>,
    spooky: PhantomData<(P, F)>,
}

impl<P: GridPrecision, F: SpatialHashFilter> Default for ChangedSpatialHashes<P, F> {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            spooky: PhantomData,
        }
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
