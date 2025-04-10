//! Describes how the floating origin's position is propagated through the hierarchy of reference
//! grids, and used to compute the floating origin's position relative to each grid. See
//! [`LocalFloatingOrigin`].

use crate::prelude::*;
use bevy_ecs::{
    prelude::*,
    relationship::Relationship,
    system::{
        lifetimeless::{Read, Write},
        SystemParam,
    },
};
use bevy_math::{prelude::*, DAffine3, DQuat};
use bevy_platform_support::prelude::*;
use bevy_transform::prelude::*;

pub use inner::LocalFloatingOrigin;

use super::Grid;

/// A module kept private to enforce use of setters and getters within the parent module.
mod inner {
    use crate::prelude::*;
    use bevy_math::{prelude::*, DAffine3, DMat3, DQuat};
    use bevy_reflect::prelude::*;

    /// An isometry that describes the location of the floating origin's grid cell's origin, in the
    /// local grid.
    ///
    /// Used to compute the [`GlobalTransform`](bevy_transform::components::GlobalTransform) of
    /// every entity within a grid. Because this tells us where the floating origin cell is located
    /// in the local grid, we can compute the inverse transform once, then use it to transform every
    /// entity relative to the floating origin.
    ///
    /// If the floating origin is in this local grid, the `float` fields will be identity. The
    /// `float` fields will be non-identity when the floating origin is in a different grid that
    /// does not perfectly align with this one. Different grids can be rotated and offset from each
    /// other - consider the grid of a planet, spinning about its axis and orbiting about a star, it
    /// will not align with the grid of the star system!
    #[derive(Default, Debug, Clone, PartialEq, Reflect)]
    pub struct LocalFloatingOrigin {
        /// The local cell that the floating origin's grid cell origin falls into.
        cell: GridCell,
        /// The translation of the floating origin's grid cell relative to the origin of
        /// [`LocalFloatingOrigin::cell`].
        translation: Vec3,
        /// The rotation of the floating origin's grid cell relative to the origin of
        /// [`LocalFloatingOrigin::cell`].
        rotation: DQuat,
        /// Transform from the local grid to the floating origin's grid cell. This is used to
        /// compute the `GlobalTransform` of all entities in this grid.
        ///
        /// Imagine you have the local grid and the floating origin's grid overlapping in space,
        /// misaligned. This transform is the smallest possible that will align the two grid grids,
        /// going from the local grid, to the floating origin's grid.
        ///
        /// This is like a camera's "view transform", but instead of transforming an object into a
        /// camera's view space, this will transform an object into the floating origin's reference
        /// grid.
        ///   - That object must be positioned in the same [`super::Grid`] that this
        ///     [`LocalFloatingOrigin`] is part of.
        ///   - That object's position must be relative to the same grid cell as defined by
        ///     [`Self::cell`].
        ///
        /// The above requirements help to ensure this transform has a small magnitude, maximizing
        /// precision, and minimizing floating point error.
        grid_transform: DAffine3,
        /// Returns `true` iff the position of the floating origin's grid origin has not moved
        /// relative to this grid.
        ///
        /// When true, this means that any entities in this grid that have not moved do not need to
        /// have their `GlobalTransform` recomputed.
        is_local_origin_unchanged: bool,
    }

    impl LocalFloatingOrigin {
        /// The grid transform from the local grid, to the floating origin's grid. See
        /// [`Self::grid_transform`].
        #[inline]
        pub fn grid_transform(&self) -> DAffine3 {
            self.grid_transform
        }

        /// Gets [`Self::cell`].
        #[inline]
        pub fn cell(&self) -> GridCell {
            self.cell
        }

        /// Gets [`Self::translation`].
        #[inline]
        pub fn translation(&self) -> Vec3 {
            self.translation
        }

        /// Gets [`Self::rotation`].
        #[inline]
        pub fn rotation(&self) -> DQuat {
            self.rotation
        }

        /// Update this local floating origin, and compute the new inverse transform.
        pub fn set(
            &mut self,
            translation_grid: GridCell,
            translation_float: Vec3,
            rotation_float: DQuat,
        ) {
            let prev = self.clone();

            self.cell = translation_grid;
            self.translation = translation_float;
            self.rotation = rotation_float;
            self.grid_transform = DAffine3 {
                matrix3: DMat3::from_quat(self.rotation),
                translation: self.translation.as_dvec3(),
            }
            .inverse();
            self.is_local_origin_unchanged = prev.eq(self);
        }

        /// Create a new [`LocalFloatingOrigin`].
        pub fn new(cell: GridCell, translation: Vec3, rotation: DQuat) -> Self {
            let grid_transform = DAffine3 {
                matrix3: DMat3::from_quat(rotation),
                translation: translation.as_dvec3(),
            }
            .inverse();

            Self {
                cell,
                translation,
                rotation,
                grid_transform,
                is_local_origin_unchanged: false,
            }
        }

        /// Returns true iff the local origin has not changed relative to the floating origin.
        #[inline]
        pub fn is_local_origin_unchanged(&self) -> bool {
            self.is_local_origin_unchanged
        }
    }
}

fn propagate_origin_to_parent(
    this_grid_entity: Entity,
    grids: &mut GridsMut,
    parent_grid_entity: Entity,
) {
    let (this_grid, this_cell, this_transform) = grids.get(this_grid_entity);
    let (parent_grid, _parent_cell, _parent_transform) = grids.get(parent_grid_entity);

    // Get this grid's double precision transform, relative to its cell. We ignore the grid cell
    // here because we don't want to lose precision - we can do these calcs relative to this cell,
    // then add the grid cell offset at the end.
    let this_transform = DAffine3::from_rotation_translation(
        this_transform.rotation.as_dquat(),
        this_transform.translation.as_dvec3(),
    );

    // Get the origin's double position in this grid
    let origin_translation = this_grid.grid_position_double(
        &this_grid.local_floating_origin.cell(),
        &Transform::from_translation(this_grid.local_floating_origin.translation()),
    );
    let this_local_origin_transform = DAffine3::from_rotation_translation(
        this_grid.local_floating_origin.rotation(),
        origin_translation,
    );

    // Multiply to move the origin into the parent's grid
    let origin_affine = this_transform * this_local_origin_transform;

    let (_, origin_rot, origin_trans) = origin_affine.to_scale_rotation_translation();
    let (origin_cell_relative_to_this_cell, origin_translation_remainder) =
        parent_grid.translation_to_grid(origin_trans);

    // Up until now we have been computing as if this cell is located at the origin, to maximize
    // precision. Now that we are done with floats, we can add the cell offset.
    let parent_origin_cell = origin_cell_relative_to_this_cell + this_cell;

    grids.update(parent_grid_entity, |parent_grid, _, _| {
        parent_grid.local_floating_origin.set(
            parent_origin_cell,
            origin_translation_remainder,
            origin_rot,
        );
    });
}

fn propagate_origin_to_child(
    this_grid_entity: Entity,
    grids: &mut GridsMut,
    child_grid_entity: Entity,
) {
    let (this_grid, _this_cell, _this_transform) = grids.get(this_grid_entity);
    let (child_grid, child_cell, child_transform) = grids.get(child_grid_entity);

    // compute double precision translation of origin treating child as the origin grid cell. Add
    // this to the origin's float translation in double,
    let origin_cell_relative_to_child = this_grid.local_floating_origin.cell() - child_cell;
    let origin_translation = this_grid.grid_position_double(
        &origin_cell_relative_to_child,
        &Transform::from_translation(this_grid.local_floating_origin.translation()),
    );

    // then combine with rotation to get a double transform from the child's cell origin to the
    // origin.
    let origin_rotation = this_grid.local_floating_origin.rotation();
    let origin_transform_child_cell_local =
        DAffine3::from_rotation_translation(origin_rotation, origin_translation);

    // Take the inverse of the child's transform as double (this is the "view" transform of the
    // child grid)
    let child_view_child_cell_local = DAffine3::from_rotation_translation(
        child_transform.rotation.as_dquat(),
        child_transform.translation.as_dvec3(),
    )
    .inverse();

    // then multiply this by the double transform we got of the origin. This is now a transform64 of
    // the origin, wrt to the child.
    let origin_child_affine = child_view_child_cell_local * origin_transform_child_cell_local;

    //  We can decompose into translation (high precision) and rotation.
    let (_, origin_child_rotation, origin_child_translation) =
        origin_child_affine.to_scale_rotation_translation();
    let (child_origin_cell, child_origin_translation_float) =
        child_grid.translation_to_grid(origin_child_translation);

    grids.update(child_grid_entity, |child_grid, _, _| {
        child_grid.local_floating_origin.set(
            child_origin_cell,
            child_origin_translation_float,
            origin_child_rotation,
        );
    });
}

/// A system param for more easily navigating a hierarchy of [`Grid`]s.
#[derive(SystemParam)]
pub struct Grids<'w, 's> {
    parent: Query<'w, 's, Read<ChildOf>>,
    grid_query: Query<'w, 's, (Entity, Read<Grid>, Option<Read<ChildOf>>)>,
}

impl Grids<'_, '_> {
    /// Get a [`Grid`] from its `Entity`.
    pub fn get(&self, grid_entity: Entity) -> &Grid {
        self.grid_query
            .get(grid_entity)
            .map(|(_entity, grid, _parent)| grid)
            .unwrap_or_else(|e| {
                panic!("Grid entity {grid_entity:?} missing Grid component.\n\tError: {e}");
            })
    }

    /// Get the [`Grid`] that `this` `Entity` is a child of, if it exists.
    pub fn parent_grid(&self, this: Entity) -> Option<&Grid> {
        self.parent_grid_entity(this)
            .map(|grid_entity| self.get(grid_entity))
    }

    /// Get the ID of the grid that `this` `Entity` is a child of, if it exists.
    #[inline]
    pub fn parent_grid_entity(&self, this: Entity) -> Option<Entity> {
        match self.parent.get(this).map(Relationship::get) {
            Err(_) => None,
            Ok(parent) => match self.grid_query.contains(parent) {
                true => Some(parent),
                false => None,
            },
        }
    }

    /// Get all grid entities that are children of this grid. Applies a filter to the returned
    /// children.
    fn child_grids_filtered<'a>(
        &'a mut self,
        this: Entity,
        mut filter: impl FnMut(Entity) -> bool + 'a,
    ) -> impl Iterator<Item = Entity> + 'a {
        // This is intentionally formulated to query grids, and filter those, as opposed to
        // iterating through the children of the current grid. The latter is extremely inefficient
        // with wide hierarchies (many entities in a grid, which is a common case), and it is
        // generally better to be querying fewer entities by using a more restrictive query - e.g.
        // only querying grids.
        self.grid_query
            .iter()
            .filter_map(move |(entity, _, parent)| {
                parent
                    .map(Relationship::get)
                    .filter(|parent| *parent == this)
                    .map(|_| entity)
            })
            .filter(move |entity| filter(*entity))
    }

    /// Get all grid entities that are children of this grid.
    pub fn child_grids(&mut self, this: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.child_grids_filtered(this, |_| true)
    }

    /// Get all grid entities that are siblings of this grid. Returns `None` if there are no
    /// siblings.
    pub fn sibling_grids(
        &mut self,
        this_entity: Entity,
    ) -> Option<impl Iterator<Item = Entity> + '_> {
        self.parent_grid_entity(this_entity)
            .map(|parent| self.child_grids_filtered(parent, move |e| e != this_entity))
    }
}

/// A system param for more easily navigating a hierarchy of grids mutably.
#[derive(SystemParam)]
pub struct GridsMut<'w, 's> {
    parent: Query<'w, 's, Read<ChildOf>>,
    position: Query<'w, 's, (Read<GridCell>, Read<Transform>), With<Grid>>,
    grid_query: Query<'w, 's, (Entity, Write<Grid>, Option<Read<ChildOf>>)>,
}

impl GridsMut<'_, '_> {
    /// Get mutable access to the [`Grid`], and run the provided function or closure, optionally
    /// returning data.
    ///
    /// ## Panics
    ///
    /// This will panic if the entity passed in is invalid.
    pub fn update<T>(
        &mut self,
        grid_entity: Entity,
        mut func: impl FnMut(&mut Grid, &GridCell, &Transform) -> T,
    ) -> T {
        let (cell, transform) = self.position(grid_entity);
        self.grid_query
            .get_mut(grid_entity)
            .map(|(_entity, mut grid, _parent)| func(grid.as_mut(), &cell, &transform))
            .expect("The supplied grid entity is no longer valid.")
    }

    /// Get the grid and the position of the grid from its `Entity`.
    pub fn get(&self, grid_entity: Entity) -> (&Grid, GridCell, Transform) {
        let (cell, transform) = self.position(grid_entity);
        self.grid_query
            .get(grid_entity)
            .map(|(_entity, grid, _parent)| (grid, cell, transform))
            .unwrap_or_else(|e| {
                panic!("Grid entity {grid_entity:?} missing Grid component.\n\tError: {e}");
            })
    }

    /// Get the position of this grid, including its grid cell and transform, or return defaults if
    /// they are missing.
    ///
    /// Needed because the root grid should not have a grid cell or transform.
    pub fn position(&self, grid_entity: Entity) -> (GridCell, Transform) {
        let (cell, transform) = (GridCell::default(), Transform::default());
        let (cell, transform) = self.position.get(grid_entity).unwrap_or_else(|_| {
        assert!(self.parent.get(grid_entity).is_err(), "Grid entity {grid_entity:?} is missing a GridCell and Transform. This is valid only if this is a root grid, but this is not.");
            (&cell, &transform)
        });
        (*cell, *transform)
    }

    /// Get the [`Grid`] that `this` `Entity` is a child of, if it exists.
    pub fn parent_grid(&self, this: Entity) -> Option<(&Grid, GridCell, Transform)> {
        self.parent_grid_entity(this)
            .map(|grid_entity| self.get(grid_entity))
    }

    /// Get the ID of the grid that `this` `Entity` is a child of, if it exists.
    #[inline]
    pub fn parent_grid_entity(&self, this: Entity) -> Option<Entity> {
        match self.parent.get(this).map(Relationship::get) {
            Err(_) => None,
            Ok(parent) => match self.grid_query.contains(parent) {
                true => Some(parent),
                false => None,
            },
        }
    }

    /// Get all grid entities that are children of this grid. Applies a filter to the returned
    /// children.
    fn child_grids_filtered<'a>(
        &'a mut self,
        this: Entity,
        mut filter: impl FnMut(Entity) -> bool + 'a,
    ) -> impl Iterator<Item = Entity> + 'a {
        // This is intentionally formulated to query grids, and filter those, as opposed to
        // iterating through the children of the current grid. The latter is extremely inefficient
        // with wide hierarchies (many entities in a grid, which is a common case), and it is
        // generally better to be querying fewer entities by using a more restrictive query - e.g.
        // only querying grids.
        self.grid_query
            .iter()
            .filter_map(move |(entity, _, parent)| {
                parent
                    .map(Relationship::get)
                    .filter(|parent| *parent == this)
                    .map(|_| entity)
            })
            .filter(move |entity| filter(*entity))
    }

    /// Get all grid entities that are children of this grid.
    pub fn child_grids(&mut self, this: Entity) -> impl Iterator<Item = Entity> + '_ {
        self.child_grids_filtered(this, |_| true)
    }

    /// Get all grid entities that are siblings of this grid.
    pub fn sibling_grids(
        &mut self,
        this_entity: Entity,
    ) -> Option<impl Iterator<Item = Entity> + '_> {
        self.parent_grid_entity(this_entity)
            .map(|parent| self.child_grids_filtered(parent, move |e| e != this_entity))
    }
}

impl LocalFloatingOrigin {
    /// Update the [`LocalFloatingOrigin`] of every [`Grid`] in the world. This does not update any
    /// entity transforms, instead this is a preceding step that updates every reference grid, so it
    /// knows where the floating origin is located with respect to that reference grid. This is all
    /// done in high precision if possible, however any loss in precision will only affect the
    /// rendering precision. The high precision coordinates ([`GridCell`] and [`Transform`]) are the
    /// source of truth and never mutated.
    pub fn compute_all(
        mut stats: ResMut<crate::timing::PropagationStats>,
        mut grids: GridsMut,
        mut grid_stack: Local<Vec<Entity>>,
        mut scratch_buffer: Local<Vec<Entity>>,
        cells: Query<(Entity, Ref<GridCell>)>,
        roots: Query<(Entity, &BigSpace)>,
        parents: Query<&ChildOf>,
    ) {
        let start = bevy_platform_support::time::Instant::now();

        /// The maximum grid tree depth, defensively prevents infinite looping in case there is a
        /// degenerate hierarchy. It might take a while, but at least it's not forever?
        const MAX_REFERENCE_FRAME_DEPTH: usize = 1_000;

        // TODO: because each tree under a root is disjoint, these updates can be done in parallel
        // without aliasing. This will require unsafe, just like bevy's own transform propagation.
        'outer: for (origin_entity, origin_cell) in roots
            .iter() // TODO: If any of these checks fail, log to some diagnostic
            .filter_map(|(root_entity, root)| root.validate_floating_origin(root_entity, &parents))
            .filter_map(|origin| cells.get(origin).ok())
        {
            let Some(mut this_grid) = grids.parent_grid_entity(origin_entity) else {
                tracing::error!("The floating origin is not in a valid grid. The floating origin entity must be a child of an entity with the `Grid` component.");
                continue;
            };

            // Prepare by resetting the `origin_transform` of the floating origin's grid. Because
            // the floating origin is within this grid, there is no grid misalignment and thus no
            // need for any floating offsets.
            grids.update(this_grid, |grid, _, _| {
                grid.local_floating_origin
                    .set(*origin_cell, Vec3::ZERO, DQuat::IDENTITY);
            });

            // Seed the grid stack with the floating origin's grid. From this point out, we will
            // only look at siblings and parents, which will allow us to visit the entire tree.
            grid_stack.clear();
            grid_stack.push(this_grid);

            // Recurse up and across the tree, updating siblings and their children.
            for _ in 0..MAX_REFERENCE_FRAME_DEPTH {
                // We start by propagating up to the parent of this grid, then propagating down to
                // the siblings of this grid (children of the parent that are not this grid).
                if let Some(parent_grid) = grids.parent_grid_entity(this_grid) {
                    propagate_origin_to_parent(this_grid, &mut grids, parent_grid);
                    if let Some(siblings) = grids.sibling_grids(this_grid) {
                        scratch_buffer.extend(siblings);
                    }
                    for sibling_grid in scratch_buffer.drain(..) {
                        // The siblings of this grid are also the children of the parent grid.
                        propagate_origin_to_child(parent_grid, &mut grids, sibling_grid);
                        grid_stack.push(sibling_grid); // We'll recurse through children next
                    }
                }

                // All the grids pushed on the stack have been processed. We can now pop those off
                // the stack and recursively process their children all the way out to the leaves of
                // the tree.
                while let Some(this_grid) = grid_stack.pop() {
                    scratch_buffer.extend(grids.child_grids(this_grid));
                    // TODO: This loop could be run in parallel, because we are mutating each unique
                    // child, these do no alias.
                    for child_grid in scratch_buffer.drain(..) {
                        propagate_origin_to_child(this_grid, &mut grids, child_grid);
                        grid_stack.push(child_grid); // Push processed child onto the stack
                    }
                }

                // Finally, now that this grid and its siblings have been recursively processed, we
                // process the parent and set it as the current grid. Note that every time we step
                // to a parent, "this grid" and all descendants have already been processed, so we
                // only need to process the siblings.
                match grids.parent_grid_entity(this_grid) {
                    Some(parent_grid) => this_grid = parent_grid,
                    None => continue 'outer, // We have reached the root of the tree, and can exit.
                }
            }

            tracing::error!("Reached the maximum grid depth ({MAX_REFERENCE_FRAME_DEPTH}), and exited early to prevent an infinite loop. This might be caused by a degenerate hierarchy.");
        }

        stats.local_origin_propagation += start.elapsed();
    }
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::system::SystemState, math::DVec3, prelude::*};

    use super::*;

    /// Test that the grid getters do what they say they do.
    #[test]
    fn grid_hierarchy_getters() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::default());

        let grid_bundle = (Transform::default(), GridCell::default(), Grid::default());

        let child_1 = app.world_mut().spawn(grid_bundle.clone()).id();
        let child_2 = app.world_mut().spawn(grid_bundle.clone()).id();
        let parent = app.world_mut().spawn(grid_bundle.clone()).id();
        let root = app.world_mut().spawn(grid_bundle.clone()).id();

        app.world_mut().entity_mut(root).add_child(parent);
        app.world_mut()
            .entity_mut(parent)
            .add_children(&[child_1, child_2]);

        let mut state = SystemState::<GridsMut>::new(app.world_mut());
        let mut grids = state.get_mut(app.world_mut());

        // Children
        let result = grids.child_grids(root).collect::<Vec<_>>();
        assert_eq!(result, vec![parent]);
        let result = grids.child_grids(parent).collect::<Vec<_>>();
        assert!(result.contains(&child_1));
        assert!(result.contains(&child_2));
        let result = grids.child_grids(child_1).collect::<Vec<_>>();
        assert_eq!(result, Vec::new());

        // ChildOf
        let result = grids.parent_grid_entity(root);
        assert_eq!(result, None);
        let result = grids.parent_grid_entity(parent);
        assert_eq!(result, Some(root));
        let result = grids.parent_grid_entity(child_1);
        assert_eq!(result, Some(parent));

        // Siblings
        assert!(grids.sibling_grids(root).is_none());
        let result = grids.sibling_grids(parent).unwrap().collect::<Vec<_>>();
        assert_eq!(result, vec![]);
        let result = grids.sibling_grids(child_1).unwrap().collect::<Vec<_>>();
        assert_eq!(result, vec![child_2]);
    }

    #[test]
    fn child_propagation() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::default());

        let root_grid = Grid {
            local_floating_origin: LocalFloatingOrigin::new(
                GridCell::new(1_000_000, -1, -1),
                Vec3::ZERO,
                DQuat::from_rotation_z(-core::f64::consts::FRAC_PI_2),
            ),
            ..default()
        };
        let root = app
            .world_mut()
            .spawn((Transform::default(), GridCell::default(), root_grid))
            .id();

        let child = app
            .world_mut()
            .spawn((
                Transform::from_rotation(Quat::from_rotation_z(core::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(1.0, 1.0, 0.0)),
                GridCell::new(1_000_000, 0, 0),
                Grid::default(),
            ))
            .id();

        app.world_mut().entity_mut(root).add_child(child);

        let mut state = SystemState::<GridsMut>::new(app.world_mut());
        let mut grids = state.get_mut(app.world_mut());

        // The function we are testing
        propagate_origin_to_child(root, &mut grids, child);

        let (child_grid, ..) = grids.get(child);

        let computed_grid = child_grid.local_floating_origin.cell();
        let correct_grid = GridCell::new(-1, 0, -1);
        assert_eq!(computed_grid, correct_grid);

        let computed_rot = child_grid.local_floating_origin.rotation();
        let correct_rot = DQuat::from_rotation_z(core::f64::consts::PI);
        let rot_error = computed_rot.angle_between(correct_rot);
        assert!(rot_error < 1e-10);

        // Even though we are 2 billion units from the origin, our precision is still pretty good.
        // The loss of precision is coming from the affine multiplication that moves the origin into
        // the child's grid. The good news is that precision loss only scales with the distance of
        // the origin to the child (in the child's grid). In this test we are saying that the
        // floating origin is - with respect to the root - pretty near the child. Even though the
        // child and floating origin are very far from the origin, we only lose precision based on
        // how for the origin is from the child.
        let computed_trans = child_grid.local_floating_origin.translation();
        let correct_trans = Vec3::new(-1.0, 1.0, 0.0);
        let trans_error = computed_trans.distance(correct_trans);
        assert!(trans_error < 1e-4);
    }

    #[test]
    fn parent_propagation() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::default());

        let grid_bundle = (Transform::default(), GridCell::default(), Grid::default());
        let root = app.world_mut().spawn(grid_bundle.clone()).id();

        let child = app
            .world_mut()
            .spawn((
                Transform::from_rotation(Quat::from_rotation_z(core::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(1.0, 1.0, 0.0)),
                GridCell::new(150_000_003_000, 0, 0), // roughly radius of earth orbit
                Grid {
                    local_floating_origin: LocalFloatingOrigin::new(
                        GridCell::new(0, 3_000, 0),
                        Vec3::new(5.0, 5.0, 0.0),
                        DQuat::from_rotation_z(-core::f64::consts::FRAC_PI_2),
                    ),
                    ..Default::default()
                },
            ))
            .id();

        app.world_mut().entity_mut(root).add_child(child);

        let mut state = SystemState::<GridsMut>::new(app.world_mut());
        let mut grids = state.get_mut(app.world_mut());

        // The function we are testing
        propagate_origin_to_parent(child, &mut grids, root);

        let (root_grid, ..) = grids.get(root);

        let computed_grid = root_grid.local_floating_origin.cell();
        let correct_grid = GridCell::new(150_000_000_000, 0, 0);
        assert_eq!(computed_grid, correct_grid);

        let computed_rot = root_grid.local_floating_origin.rotation();
        let correct_rot = DQuat::IDENTITY;
        let rot_error = computed_rot.angle_between(correct_rot);
        assert!(rot_error < 1e-7);

        // This is the error of the position of the floating origin if the origin was a person
        // standing on earth, and their position was resampled with respect to the sun. This is 0.3
        // meters, but recall that this will be the error when positioning the other planets in the
        // solar system when rendering.
        //
        // This error scales with the distance of the floating origin from the origin of its grid,
        // in this case the radius of the earth, not the radius of the orbit.
        let computed_trans = root_grid.local_floating_origin.translation();
        let correct_trans = Vec3::new(-4.0, 6.0, 0.0);
        let trans_error = computed_trans.distance(correct_trans);
        assert!(trans_error < 0.3);
    }

    #[test]
    fn origin_transform() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::default());

        let root = app
            .world_mut()
            .spawn((
                Transform::default(),
                GridCell::default(),
                Grid {
                    local_floating_origin: LocalFloatingOrigin::new(
                        GridCell::new(0, 0, 0),
                        Vec3::new(1.0, 1.0, 0.0),
                        DQuat::from_rotation_z(0.0),
                    ),
                    ..default()
                },
            ))
            .id();

        let child = app
            .world_mut()
            .spawn((
                Transform::default()
                    .with_rotation(Quat::from_rotation_z(-core::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(3.0, 3.0, 0.0)),
                GridCell::new(0, 0, 0),
                Grid::default(),
            ))
            .id();

        app.world_mut().entity_mut(root).add_child(child);

        let mut state = SystemState::<GridsMut>::new(app.world_mut());
        let mut grids = state.get_mut(app.world_mut());

        propagate_origin_to_child(root, &mut grids, child);

        let (child_grid, ..) = grids.get(child);
        let child_local_point = DVec3::new(5.0, 5.0, 0.0);

        let computed_transform = child_grid.local_floating_origin.grid_transform();
        let computed_pos = computed_transform.transform_point3(child_local_point);

        let correct_transform = DAffine3::from_rotation_translation(
            DQuat::from_rotation_z(-core::f64::consts::FRAC_PI_2),
            DVec3::new(2.0, 2.0, 0.0),
        );
        let correct_pos = correct_transform.transform_point3(child_local_point);

        assert!((computed_pos - correct_pos).length() < 1e-6);
        assert!((computed_pos - DVec3::new(7.0, -3.0, 0.0)).length() < 1e-6);
    }
}
