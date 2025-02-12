//! Tools for validating high-precision transform hierarchies

use std::marker::PhantomData;

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_transform::prelude::*;
use bevy_utils::{HashMap, HashSet};

use crate::{grid::Grid, precision::GridPrecision, BigSpace, FloatingOrigin, GridCell};

struct ValidationStackEntry {
    parent_node: Box<dyn ValidHierarchyNode>,
    children: Vec<Entity>,
}

#[derive(Default, Resource)]
struct ValidatorCaches {
    query_state_cache: HashMap<&'static str, QueryState<(Entity, Option<&'static Children>)>>,
    validator_cache: HashMap<&'static str, Vec<Box<dyn ValidHierarchyNode>>>,
    root_query: Option<QueryState<Entity, Without<Parent>>>,
    stack: Vec<ValidationStackEntry>,
    /// Only report errors for an entity one time.
    error_entities: HashSet<Entity>,
}

/// An exclusive system that validate the entity hierarchy and report errors.
pub fn validate_hierarchy<V: 'static + ValidHierarchyNode + Default>(world: &mut World) {
    world.init_resource::<ValidatorCaches>();
    let mut caches = world.remove_resource::<ValidatorCaches>().unwrap();

    let root_entities = caches
        .root_query
        .get_or_insert(world.query_filtered::<Entity, Without<Parent>>())
        .iter(world)
        .collect();

    caches.stack.push(ValidationStackEntry {
        parent_node: Box::<V>::default(),
        children: root_entities,
    });

    while let Some(stack_entry) = caches.stack.pop() {
        let mut validators_and_queries = caches
            .validator_cache
            .entry(stack_entry.parent_node.name())
            .or_insert_with(|| stack_entry.parent_node.allowed_child_nodes())
            .iter()
            .map(|validator| {
                let query = caches
                    .query_state_cache
                    .remove(validator.name())
                    .unwrap_or_else(|| {
                        let mut query_builder = QueryBuilder::new(world);
                        validator.match_self(&mut query_builder);
                        query_builder.build()
                    });
                (validator, query)
            })
            .collect::<Vec<_>>();

        for entity in stack_entry.children.iter() {
            let query_result = validators_and_queries
                .iter_mut()
                .find_map(|(validator, query)| {
                    query.get(world, *entity).ok().map(|res| (validator, res.1))
                });

            match query_result {
                Some((validator, Some(children))) => {
                    caches.stack.push(ValidationStackEntry {
                        parent_node: validator.clone(),
                        children: children.to_vec(),
                    });
                }
                Some(_) => (), // Matched, but no children to push on the stack
                None => {
                    if caches.error_entities.contains(entity) {
                        continue; // Don't repeat error messages for the same entity
                    }

                    let mut possibilities = String::new();
                    stack_entry
                        .parent_node
                        .allowed_child_nodes()
                        .iter()
                        .for_each(|v| {
                            possibilities.push_str("  - ");
                            possibilities.push_str(v.name());
                            possibilities.push('\n');
                        });

                    let mut inspect = String::new();
                    world.inspect_entity(*entity).for_each(|info| {
                        inspect.push_str("  - ");
                        inspect.push_str(info.name());
                        inspect.push('\n');
                    });

                    tracing::error!("
-------------------------------------------
big_space hierarchy validation error report
-------------------------------------------

Entity {:#} is a child of a {:#?}, but the components on this entity do not match any of the allowed archetypes for children of this parent.
                    
Because it is a child of a {:#?}, the entity must be one of the following:
{}
However, the entity has the following components, which does not match any of the allowed archetypes listed above:
{}
Common errors include:
  - Using mismatched GridPrecisions, like GridCell<i32> and GridCell<i64>
  - Spawning an entity with a GridCell as a child of an entity without a Grid.

If possible, use commands.spawn_big_space(), which prevents these errors, instead of manually assembling a hierarchy. See {} for details.", entity, stack_entry.parent_node.name(), stack_entry.parent_node.name(), possibilities, inspect, file!());
                    caches.error_entities.insert(*entity);
                }
            }
        }

        for (validator, query) in validators_and_queries.drain(..) {
            caches.query_state_cache.insert(validator.name(), query);
        }
    }

    world.insert_resource(caches);
}

/// Defines a valid node in the hierarchy: what components it must have, must not have, and what
/// kinds of nodes its children can be. This can be used recursively to validate an entire entity
/// hierarchy by starting from the root.
pub trait ValidHierarchyNode: sealed::CloneHierarchy + Send + Sync {
    /// Add filters to a query to check if entities match this type of node
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>);
    /// The types of nodes that can be children of this node.
    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>>;
    /// A unique identifier of this type
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

mod sealed {
    use super::ValidHierarchyNode;

    pub trait CloneHierarchy {
        fn clone_box(&self) -> Box<dyn ValidHierarchyNode>;
    }

    impl<T> CloneHierarchy for T
    where
        T: 'static + ValidHierarchyNode + Clone,
    {
        fn clone_box(&self) -> Box<dyn ValidHierarchyNode> {
            Box::new(self.clone())
        }
    }

    impl Clone for Box<dyn ValidHierarchyNode> {
        fn clone(&self) -> Self {
            self.clone_box()
        }
    }
}

/// The root hierarchy validation struct, used as a generic parameter in [`crate::validation`].
#[derive(Default, Clone)]
pub struct SpatialHierarchyRoot<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for SpatialHierarchyRoot<P> {
    fn name(&self) -> &'static str {
        "Root"
    }

    fn match_self(&self, _: &mut QueryBuilder<(Entity, Option<&Children>)>) {}

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<RootFrame<P>>::default(),
            Box::<RootSpatialLowPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct AnyNonSpatial<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for AnyNonSpatial<P> {
    fn name(&self) -> &'static str {
        "Any non-spatial entity"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .without::<GridCellAny>()
            .without::<Transform>()
            .without::<GlobalTransform>()
            .without::<BigSpace>()
            .without::<Grid<P>>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![Box::<AnyNonSpatial<P>>::default()]
    }
}

#[derive(Default, Clone)]
struct RootFrame<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for RootFrame<P> {
    fn name(&self) -> &'static str {
        "Root of a BigSpace"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<BigSpace>()
            .with::<Grid<P>>()
            .with::<GlobalTransform>()
            .without::<GridCellAny>()
            .without::<Transform>()
            .without::<Parent>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildFrame<P>>::default(),
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<ChildSpatialHighPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct RootSpatialLowPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for RootSpatialLowPrecision<P> {
    fn name(&self) -> &'static str {
        "Root of a Transform hierarchy at the root of the tree outside of any BigSpace"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .without::<GridCellAny>()
            .without::<BigSpace>()
            .without::<Grid<P>>()
            .without::<Parent>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildFrame<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for ChildFrame<P> {
    fn name(&self) -> &'static str {
        "Non-root Grid"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Grid<P>>()
            .with::<GridCell<P>>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<BigSpace>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildFrame<P>>::default(),
            Box::<ChildRootSpatialLowPrecision<P>>::default(),
            Box::<ChildSpatialHighPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildRootSpatialLowPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for ChildRootSpatialLowPrecision<P> {
    fn name(&self) -> &'static str {
        "Root of a low-precision Transform hierarchy, within a BigSpace"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .with::<crate::grid::propagation::LowPrecisionRoot>()
            .without::<GridCellAny>()
            .without::<BigSpace>()
            .without::<Grid<P>>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildSpatialLowPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for ChildSpatialLowPrecision<P> {
    fn name(&self) -> &'static str {
        "Non-root low-precision spatial entity"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<GridCellAny>()
            .without::<BigSpace>()
            .without::<Grid<P>>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildSpatialHighPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for ChildSpatialHighPrecision<P> {
    fn name(&self) -> &'static str {
        "Non-root high precision spatial entity"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<GridCell<P>>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<BigSpace>()
            .without::<Grid<P>>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildRootSpatialLowPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}
