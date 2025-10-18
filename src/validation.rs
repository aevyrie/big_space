//! Tools for validating high-precision transform hierarchies

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::entity::EntityHashSet;
use bevy_ecs::prelude::*;
use bevy_platform::{collections::HashMap, prelude::*};
use bevy_transform::prelude::*;

use crate::{grid::Grid, BigSpace, CellCoord, FloatingOrigin};

struct ValidationStackEntry {
    parent_node: Box<dyn ValidHierarchyNode>,
    children: Vec<Entity>,
}

/// Adds hierarchy validation features.
pub struct BigSpaceValidationPlugin;
impl Plugin for BigSpaceValidationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            validate_hierarchy::<SpatialHierarchyRoot>.after(TransformSystems::Propagate),
        );
    }
}

#[derive(Default, Resource)]
struct ValidatorCaches {
    query_state_cache: HashMap<&'static str, QueryState<(Entity, Option<&'static Children>)>>,
    validator_cache: HashMap<&'static str, Vec<Box<dyn ValidHierarchyNode>>>,
    root_query: Option<QueryState<Entity, Without<ChildOf>>>,
    stack: Vec<ValidationStackEntry>,
    /// Only report errors for an entity one time.
    error_entities: EntityHashSet,
}

/// An exclusive system that validate the entity hierarchy and report errors.
pub fn validate_hierarchy<V: 'static + ValidHierarchyNode + Default>(world: &mut World) {
    world.init_resource::<ValidatorCaches>();
    let mut caches = world.remove_resource::<ValidatorCaches>().unwrap();

    let root_entities = caches
        .root_query
        .get_or_insert(world.query_filtered::<Entity, Without<ChildOf>>())
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
                    world
                        .inspect_entity(*entity)
                        .into_iter()
                        .flatten()
                        .for_each(|info| {
                            inspect.push_str("  - ");
                            inspect.push_str(&info.name());
                            inspect.push('\n');
                        });

                    bevy_log::error!("
-------------------------------------------
big_space hierarchy validation error report
-------------------------------------------

Entity {:#} is a child of a {:#?}, but the components on this entity do not match any of the allowed archetypes for children of this parent.
                    
Because it is a child of a {:#?}, the entity must be one of the following:
{}
However, the entity has the following components, which does not match any of the allowed archetypes listed above:
{}

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
    /// A unique identifier of this type
    fn name(&self) -> &'static str {
        core::any::type_name::<Self>()
    }
    /// Add filters to a query to check if entities match this type of node
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>);
    /// The types of nodes that can be children of this node.
    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>>;
}

mod sealed {
    use super::ValidHierarchyNode;
    use bevy_platform::prelude::*;

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
pub struct SpatialHierarchyRoot;

impl ValidHierarchyNode for SpatialHierarchyRoot {
    fn name(&self) -> &'static str {
        "Root"
    }

    fn match_self(&self, _: &mut QueryBuilder<(Entity, Option<&Children>)>) {}

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<RootFrame>::default(),
            Box::<RootSpatialLowPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct AnyNonSpatial;

impl ValidHierarchyNode for AnyNonSpatial {
    fn name(&self) -> &'static str {
        "Any non-spatial entity"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .without::<CellCoord>()
            .without::<Transform>()
            .without::<GlobalTransform>()
            .without::<BigSpace>()
            .without::<Grid>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![Box::<AnyNonSpatial>::default()]
    }
}

#[derive(Default, Clone)]
struct RootFrame;

impl ValidHierarchyNode for RootFrame {
    fn name(&self) -> &'static str {
        "Root of a BigSpace"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<BigSpace>()
            .with::<Grid>()
            .with::<GlobalTransform>()
            .without::<CellCoord>()
            .without::<Transform>()
            .without::<ChildOf>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildFrame>::default(),
            Box::<ChildSpatialLowPrecision>::default(),
            Box::<ChildSpatialHighPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct RootSpatialLowPrecision;

impl ValidHierarchyNode for RootSpatialLowPrecision {
    fn name(&self) -> &'static str {
        "Root of a Transform hierarchy at the root of the tree outside of any BigSpace"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .without::<CellCoord>()
            .without::<BigSpace>()
            .without::<Grid>()
            .without::<ChildOf>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildFrame;

impl ValidHierarchyNode for ChildFrame {
    fn name(&self) -> &'static str {
        "Non-root Grid"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Grid>()
            .with::<CellCoord>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<ChildOf>()
            .without::<BigSpace>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildFrame>::default(),
            Box::<ChildRootSpatialLowPrecision>::default(),
            Box::<ChildSpatialHighPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildRootSpatialLowPrecision;

impl ValidHierarchyNode for ChildRootSpatialLowPrecision {
    fn name(&self) -> &'static str {
        "Root of a low-precision Transform hierarchy, within a BigSpace"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<ChildOf>()
            .with::<crate::grid::propagation::LowPrecisionRoot>()
            .without::<CellCoord>()
            .without::<BigSpace>()
            .without::<Grid>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildSpatialLowPrecision;

impl ValidHierarchyNode for ChildSpatialLowPrecision {
    fn name(&self) -> &'static str {
        "Non-root low-precision spatial entity"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<ChildOf>()
            .without::<CellCoord>()
            .without::<BigSpace>()
            .without::<Grid>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}

#[derive(Default, Clone)]
struct ChildSpatialHighPrecision;

impl ValidHierarchyNode for ChildSpatialHighPrecision {
    fn name(&self) -> &'static str {
        "Non-root high precision spatial entity"
    }

    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<CellCoord>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<ChildOf>()
            .without::<BigSpace>()
            .without::<Grid>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildRootSpatialLowPrecision>::default(),
            Box::<AnyNonSpatial>::default(),
        ]
    }
}
