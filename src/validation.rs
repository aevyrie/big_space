//! Tools for validating high-precision transform hierarchies

use crate::*;

/// Validate the entity hierarchy and report errors.
pub fn validate_hierarchy<V: 'static + ValidHierarchyNode + Default>(world: &mut World) {
    let mut root_entities: Vec<Entity> = world
        .query_filtered::<Entity, Without<Parent>>()
        .iter(world)
        .collect();

    let mut query_state_cache =
        bevy::utils::HashMap::<&'static str, QueryState<(Entity, Option<&Children>)>>::default();

    struct ValidationStackEntry {
        parent_node: Box<dyn ValidHierarchyNode>,
        entity: Entity,
    }

    let mut validator_stack = Vec::<ValidationStackEntry>::with_capacity(root_entities.len());

    for entity in root_entities.drain(..) {
        validator_stack.push(ValidationStackEntry {
            parent_node: Box::<V>::default(),
            entity,
        })
    }

    while let Some(entry) = validator_stack.pop() {
        let mut allowed_nodes = entry.parent_node.allowed_child_nodes();
        let test_allowed_nodes = allowed_nodes
            .drain(..)
            .filter_map(|validator| {
                let validation_query =
                    query_state_cache
                        .entry(validator.name())
                        .or_insert_with(|| {
                            let mut query_builder = QueryBuilder::new(world);
                            validator.match_self(&mut query_builder);
                            query_builder.build()
                        });

                validation_query
                    .get(world, entry.entity)
                    .ok()
                    .map(|(_e, c)| c.map(|c| (validator, c.to_vec())))
            })
            .next();

        match test_allowed_nodes {
            None => {
                let mut possibilities = String::new();
                entry
                    .parent_node
                    .allowed_child_nodes()
                    .iter()
                    .for_each(|v| {
                        possibilities.push('\t');
                        possibilities.push('\t');
                        possibilities.push_str(v.name());
                        possibilities.push('\n');
                    });

                error!("big_space hierarchy validation error:\n\tEntity {:#?} is a child of the node {:#?}\n\tThis entity must be one of the following allowed nodes:\n{}\tSee {} for details.", entry.entity, entry.parent_node.name(), possibilities, file!());
                continue;
            }
            Some(None) => continue, // no children
            Some(Some((this_validator, children))) => {
                for child in children {
                    validator_stack.push(ValidationStackEntry {
                        parent_node: this_validator.clone(),
                        entity: child,
                    })
                }
            }
        }
    }
}

/// Defines a valid node in the hierarchy: what components it must have, must not have, and what
/// kinds of nodes its children can be. This can be used recursively to validate an entire entity
/// hierarchy by starting from the root.
pub trait ValidHierarchyNode: sealed::CloneHierarchy {
    /// Add filters to a query to check if entities match this type of node
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>);
    /// The types of nodes that can be children of this node.
    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>>;
    /// A unique identifier of this type
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

pub(super) mod sealed {
    use super::ValidHierarchyNode;

    pub trait CloneHierarchy {
        fn clone_box(&self) -> Box<dyn ValidHierarchyNode>;
    }

    impl<T: ?Sized> CloneHierarchy for T
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

/// The root hierarchy validation struct, used as a generic parameter in [`validation`].
#[derive(Default, Clone)]
pub struct SpatialHierarchyRoot<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for SpatialHierarchyRoot<P> {
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
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .without::<GridCell<P>>()
            .without::<Transform>()
            .without::<GlobalTransform>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>()
            .without::<FloatingOrigin>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![Box::<AnyNonSpatial<P>>::default()]
    }
}

#[derive(Default, Clone)]
struct RootFrame<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for RootFrame<P> {
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<BigSpace>()
            .with::<ReferenceFrame<P>>()
            .without::<GridCell<P>>()
            .without::<Transform>()
            .without::<GlobalTransform>()
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
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .without::<GridCell<P>>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>()
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
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<ReferenceFrame<P>>()
            .with::<GridCell<P>>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<BigSpace>();
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
struct ChildSpatialLowPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> ValidHierarchyNode for ChildSpatialLowPrecision<P> {
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<GridCell<P>>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>()
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
    fn match_self(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<GridCell<P>>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn ValidHierarchyNode>> {
        vec![
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<AnyNonSpatial<P>>::default(),
        ]
    }
}
