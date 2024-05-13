//! Tools for validating high-precision transform hierarchies

/// TODO:
///
/// MAKE ReferenceFrame optional!
///
/// root reference frame
/// - should NOT have a Transform, GlobalTransform, or Parent
/// - MUST have a ReferenceFrame, the local_origin field should be Some
///
/// entities with GridCell
/// - MUST have a parent with ReferenceFrame
/// - MUST have a Transform and GlobalTransform
///
/// Normal, low precision bevy transforms
/// entities with a Transform and WITHOUT a GridCell
/// - MUST have a chain of parents with Transforms that ends at the root OR end at a
///   RootReferenceFrame
/// - MUST have a GlobalTransform
///
/// Maybe a faster way to do this would be to do a depth first search of the tree to make sure every branch is valid? If you see an entity that is a child that doesn't match, that should trigger an error as that entity
/// ROOT
/// - root_frame:
///     - MUST(RootReferenceFrame, ReferenceFrame),
///     - NOT(GridCell, Transform, GlobalTransform, Parent)
///     - CHILDREN: child_frame, high_precision_spatial, low_precision_spatial
///
/// - root_spatial_low_precision:
///     - MUST(Transform, GlobalTransform),
///     - NOT(GridCell, RootReferenceFrame, ReferenceFrame, Parent)
///     - CHILDREN: low_precision_spatial
///
/// CHILDREN:
///- child_frame:
///     - MUST(GridCell, Transform, GlobalTransform, ReferenceFrame, Parent)
///     - NOT(RootReferenceFrame)
///     - CHILDREN: child_frame, high_precision_spatial, low_precision_spatial
///
/// - child_spatial_low_precision:
///     - MUST(Transform, GlobalTransform, Parent),
///     - NOT(GridCell, RootReferenceFrame, ReferenceFrame)
///     - CHILDREN: low_precision_spatial
///
///- child_spatial_high_precision:
///     - MUST(GridCell, Transform, GlobalTransform, Parent),
///     - NOT(RootReferenceFrame, ReferenceFrame)
///     - CHILDREN: low_precision_spatial
///
use crate::*;

/// Validate the entity hierarchy and report errors.
pub fn validate_hierarchy<V: 'static + HierarchyValidation + Default>(world: &mut World) {
    let mut root_entities: Vec<Entity> = world
        .query_filtered::<Entity, Without<Parent>>()
        .iter(world)
        .collect();

    let mut query_state_cache =
        bevy::utils::HashMap::<&'static str, QueryState<(Entity, Option<&Children>)>>::default();

    struct ValidationStackEntry {
        parent_node: Box<dyn HierarchyValidation>,
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
                let validation_query = query_state_cache
                    .entry(validator.name())
                    .or_insert_with(|| validator.build_query(world));

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

                error!("Entity {:#?} is a child of the node {:#?} and failed its validation criteria.\n\tThis entity must be one of the following allowed nodes:\n{}", entry.entity, entry.parent_node.name(),  possibilities);
                continue;
            }
            Some(None) => continue, // no children
            Some(Some((this_validator, children))) => {
                for child in children {
                    validator_stack.push(ValidationStackEntry {
                        parent_node: this_validator.dyn_boxed(),
                        entity: child,
                    })
                }
            }
        }
    }
}

/// Defines a valid node in the hierarchy: what components it must have, must not have, and what
/// kinds of nodes its children can be.
pub trait HierarchyValidation {
    /// Add filters to validate this node.
    fn validate(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>);
    /// The types of nodes that can be children of this node.
    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>>;
    /// Builds a query from the validation method. Automatically implemented.
    fn build_query(&self, world: &mut World) -> QueryState<(Entity, Option<&'static Children>)> {
        let mut query_builder = QueryBuilder::new(world);
        self.validate(&mut query_builder);
        query_builder.build()
    }
    /// Create a boxed trait object.
    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation>;
    /// A unique identifier of this type
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// The root hierarchy validation struct, used as a generic parameter in [`validation`].
#[derive(Default, Clone, Copy)]
pub struct BigSpaceRoot<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> HierarchyValidation for BigSpaceRoot<P> {
    fn validate(&self, _: &mut QueryBuilder<(Entity, Option<&Children>)>) {}

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![
            Box::<RootFrame<P>>::default(),
            Box::<RootSpatialLowPrecision<P>>::default(),
            Box::<AnyRoot>::default(),
        ]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}

#[derive(Default, Clone, Copy)]
struct AnyRoot;

impl HierarchyValidation for AnyRoot {
    fn validate(&self, _: &mut QueryBuilder<(Entity, Option<&Children>)>) {}

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![Box::<AnyRoot>::default()]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}

#[derive(Default, Clone, Copy)]
struct RootFrame<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> HierarchyValidation for RootFrame<P> {
    fn validate(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<BigSpace>()
            .with::<ReferenceFrame<P>>()
            .without::<GridCell<P>>()
            .without::<Transform>()
            .without::<GlobalTransform>()
            .without::<Parent>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![
            Box::<ChildFrame<P>>::default(),
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<ChildSpatialHighPrecision<P>>::default(),
        ]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}

#[derive(Default, Clone, Copy)]
struct RootSpatialLowPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> HierarchyValidation for RootSpatialLowPrecision<P> {
    fn validate(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .without::<GridCell<P>>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>()
            .without::<Parent>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![Box::<ChildSpatialLowPrecision<P>>::default()]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}

#[derive(Default, Clone, Copy)]
struct ChildFrame<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> HierarchyValidation for ChildFrame<P> {
    fn validate(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<ReferenceFrame<P>>()
            .with::<GridCell<P>>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<BigSpace>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![
            Box::<ChildFrame<P>>::default(),
            Box::<ChildSpatialLowPrecision<P>>::default(),
            Box::<ChildSpatialHighPrecision<P>>::default(),
        ]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}

#[derive(Default, Clone, Copy)]
struct ChildSpatialLowPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> HierarchyValidation for ChildSpatialLowPrecision<P> {
    fn validate(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<GridCell<P>>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![Box::<ChildSpatialLowPrecision<P>>::default()]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}

#[derive(Default, Clone, Copy)]
struct ChildSpatialHighPrecision<P: GridPrecision>(PhantomData<P>);

impl<P: GridPrecision> HierarchyValidation for ChildSpatialHighPrecision<P> {
    fn validate(&self, query: &mut QueryBuilder<(Entity, Option<&Children>)>) {
        query
            .with::<GridCell<P>>()
            .with::<Transform>()
            .with::<GlobalTransform>()
            .with::<Parent>()
            .without::<BigSpace>()
            .without::<ReferenceFrame<P>>();
    }

    fn allowed_child_nodes(&self) -> Vec<Box<dyn HierarchyValidation>> {
        vec![Box::<ChildSpatialLowPrecision<P>>::default()]
    }

    fn dyn_boxed(&self) -> Box<dyn HierarchyValidation> {
        Box::new(*self)
    }
}
