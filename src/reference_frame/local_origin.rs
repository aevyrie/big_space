//! Describes how the floating origin's position is propagated through the hierarchy of reference
//! frames, and used to compute the floating origin's position relative to each reference frame. See
//! [`LocalFloatingOrigin`].

use bevy_ecs::{
    prelude::*,
    system::{
        lifetimeless::{Read, Write},
        SystemParam,
    },
};
use bevy_hierarchy::prelude::*;
use bevy_log::prelude::*;
use bevy_math::{prelude::*, DAffine3, DQuat};
use bevy_transform::prelude::*;

pub use inner::LocalFloatingOrigin;

use crate::{precision::GridPrecision, BigSpace, GridCell};

use super::ReferenceFrame;

/// A module kept private to enforce use of setters and getters within the parent module.
mod inner {
    use bevy_math::{prelude::*, DAffine3, DMat3, DQuat};
    use bevy_reflect::prelude::*;

    use crate::{precision::GridPrecision, GridCell};

    /// An isometry that describes the location of the floating origin's grid cell's origin, in the
    /// local reference frame.
    ///
    /// Used to compute the [`GlobalTransform`](bevy_transform::components::GlobalTransform) of
    /// every entity within a reference frame. Because this tells us where the floating origin cell
    /// is located in the local frame, we can compute the inverse transform once, then use it to
    /// transform every entity relative to the floating origin.
    ///
    /// If the floating origin is in this local reference frame, the `float` fields will be
    /// identity. The `float` fields will be non-identity when the floating origin is in a different
    /// reference frame that does not perfectly align with this one. Different reference frames can
    /// be rotated and offset from each other - consider the reference frame of a planet, spinning
    /// about its axis and orbiting about a star, it will not align with the reference frame of the
    /// star system!
    #[derive(Default, Debug, Clone, PartialEq, Reflect)]
    pub struct LocalFloatingOrigin<P: GridPrecision> {
        /// The local cell that the floating origin's grid cell origin falls into.
        cell: GridCell<P>,
        /// The translation of floating origin's grid cell relative to the origin of
        /// [`LocalFloatingOrigin::cell`].
        translation: Vec3,
        /// The rotation of the floating origin's grid cell relative to the origin of
        /// [`LocalFloatingOrigin::cell`].
        rotation: DQuat,
        /// Transform from the local reference frame to the floating origin's grid cell. This is
        /// used to compute the `GlobalTransform` of all entities in this reference frame.
        ///
        /// Imagine you have the local reference frame and the floating origin's reference frame
        /// overlapping in space, misaligned. This transform is the smallest possible that will
        /// align the two reference frame grids, going from the local frame, to the floating
        /// origin's frame.
        ///
        /// This is like a camera's "view transform", but instead of transforming an object into a
        /// camera's view space, this will transform an object into the floating origin's reference
        /// frame.
        ///   - That object must be positioned in the same [`super::ReferenceFrame`] that this
        ///     [`LocalFloatingOrigin`] is part of.
        ///   - That object's position must be relative to the same grid cell as defined by
        ///     [`Self::cell`].
        ///
        /// The above requirements help to ensure this transform has a small magnitude, maximizing
        /// precision, and minimizing floating point error.
        reference_frame_transform: DAffine3,
    }

    impl<P: GridPrecision> LocalFloatingOrigin<P> {
        /// The reference frame transform from the local reference frame, to the floating origin's
        /// reference frame. See [Self::reference_frame_transform].
        pub fn reference_frame_transform(&self) -> DAffine3 {
            self.reference_frame_transform
        }

        /// Gets [`Self::cell`].
        pub fn cell(&self) -> GridCell<P> {
            self.cell
        }

        /// Gets [`Self::translation`].
        pub fn translation(&self) -> Vec3 {
            self.translation
        }

        /// Gets [`Self::rotation`].
        pub fn rotation(&self) -> DQuat {
            self.rotation
        }

        /// Update this local floating origin, and compute the new inverse transform.
        pub fn set(
            &mut self,
            translation_grid: GridCell<P>,
            translation_float: Vec3,
            rotation_float: DQuat,
        ) {
            self.cell = translation_grid;
            self.translation = translation_float;
            self.rotation = rotation_float;

            self.reference_frame_transform = DAffine3 {
                matrix3: DMat3::from_quat(self.rotation),
                translation: self.translation.as_dvec3(),
            }
            .inverse()
        }

        /// Create a new [`LocalFloatingOrigin`].
        pub fn new(cell: GridCell<P>, translation: Vec3, rotation: DQuat) -> Self {
            let reference_frame_transform = DAffine3 {
                matrix3: DMat3::from_quat(rotation),
                translation: translation.as_dvec3(),
            }
            .inverse();

            Self {
                cell,
                translation,
                rotation,
                reference_frame_transform,
            }
        }
    }
}

fn propagate_origin_to_parent<P: GridPrecision>(
    this_frame_entity: Entity,
    reference_frames: &mut ReferenceFramesMut<P>,
    parent_frame_entity: Entity,
) {
    let (this_frame, this_cell, this_transform) = reference_frames.get(this_frame_entity);
    let (parent_frame, _parent_cell, _parent_transform) = reference_frames.get(parent_frame_entity);

    // Get this frame's double precision transform, relative to its cell. We ignore the grid
    // cell here because we don't want to lose precision - we can do these calcs relative to
    // this cell, then add the grid cell offset at the end.
    let this_transform = DAffine3::from_rotation_translation(
        this_transform.rotation.as_dquat(),
        this_transform.translation.as_dvec3(),
    );

    // Get the origin's double position in this reference frame
    let origin_translation = this_frame.grid_position_double(
        &this_frame.local_floating_origin.cell(),
        &Transform::from_translation(this_frame.local_floating_origin.translation()),
    );
    let this_local_origin_transform = DAffine3::from_rotation_translation(
        this_frame.local_floating_origin.rotation(),
        origin_translation,
    );

    // Multiply to move the origin into the parent's reference frame
    let origin_affine = this_transform * this_local_origin_transform;

    let (_, origin_rot, origin_trans) = origin_affine.to_scale_rotation_translation();
    let (origin_cell_relative_to_this_cell, origin_translation_remainder) =
        parent_frame.translation_to_grid(origin_trans);

    // Up until now we have been computing as if this cell is located at the origin, to maximize
    // precision. Now that we are done with floats, we can add the cell offset.
    let parent_origin_cell = origin_cell_relative_to_this_cell + this_cell;

    reference_frames.update_reference_frame(parent_frame_entity, |parent_frame, _, _| {
        parent_frame.local_floating_origin.set(
            parent_origin_cell,
            origin_translation_remainder,
            origin_rot,
        );
    });
}

fn propagate_origin_to_child<P: GridPrecision>(
    this_frame_entity: Entity,
    reference_frames: &mut ReferenceFramesMut<P>,
    child_frame_entity: Entity,
) {
    let (this_frame, _this_cell, _this_transform) = reference_frames.get(this_frame_entity);
    let (child_frame, child_cell, child_transform) = reference_frames.get(child_frame_entity);

    // compute double precision translation of origin treating child as the origin grid cell. Add this to the origin's float translation in double,
    let origin_cell_relative_to_child = this_frame.local_floating_origin.cell() - child_cell;
    let origin_translation = this_frame.grid_position_double(
        &origin_cell_relative_to_child,
        &Transform::from_translation(this_frame.local_floating_origin.translation()),
    );

    // then combine with rotation to get a double transform from the child's cell origin to the origin.
    let origin_rotation = this_frame.local_floating_origin.rotation();
    let origin_transform_child_cell_local =
        DAffine3::from_rotation_translation(origin_rotation, origin_translation);

    // Take the inverse of the child's transform as double (this is the "view" transform of the child reference frame)
    let child_view_child_cell_local = DAffine3::from_rotation_translation(
        child_transform.rotation.as_dquat(),
        child_transform.translation.as_dvec3(),
    )
    .inverse();

    // then multiply this by the double transform we got of the origin. This is now a transform64 of the origin, wrt to the child.
    let origin_child_affine = child_view_child_cell_local * origin_transform_child_cell_local;

    //  We can decompose into translation (high precision) and rotation.
    let (_, origin_child_rotation, origin_child_translation) =
        origin_child_affine.to_scale_rotation_translation();
    let (child_origin_cell, child_origin_translation_float) =
        child_frame.translation_to_grid(origin_child_translation);

    reference_frames.update_reference_frame(child_frame_entity, |child_frame, _, _| {
        child_frame.local_floating_origin.set(
            child_origin_cell,
            child_origin_translation_float,
            origin_child_rotation,
        );
    })
}

/// A system param for more easily navigating a hierarchy of reference frames.
#[derive(SystemParam)]
pub struct ReferenceFrames<'w, 's, P: GridPrecision> {
    parent: Query<'w, 's, Read<Parent>>,
    children: Query<'w, 's, Read<Children>>,
    // position: Query<'w, 's, (Read<GridCell<P>>, Read<Transform>), With<ReferenceFrame<P>>>,
    frame_query: Query<'w, 's, (Entity, Read<ReferenceFrame<P>>, Option<Read<Parent>>)>,
}

impl<'w, 's, P: GridPrecision> ReferenceFrames<'w, 's, P> {
    /// Get a [`ReferenceFrame`] from its `Entity`.
    pub fn get(&self, frame_entity: Entity) -> &ReferenceFrame<P> {
        self.frame_query
            .get(frame_entity)
            .map(|(_entity, frame, _parent)| frame)
            .unwrap_or_else(|e| {
                panic!("Reference frame entity missing ReferenceFrame component.\n\tError: {e}");
            })
    }

    /// Get the [`ReferenceFrame`] that `this` `Entity` is a child of, if it exists.
    pub fn parent_frame(&self, this: Entity) -> Option<&ReferenceFrame<P>> {
        self.parent_frame_entity(this)
            .map(|frame_entity| self.get(frame_entity))
    }

    /// Get the ID of the reference frame that `this` `Entity` is a child of, if it exists.
    #[inline]
    pub fn parent_frame_entity(&self, this: Entity) -> Option<Entity> {
        match self.parent.get(this).map(|parent| **parent) {
            Err(_) => None,
            Ok(parent) => match self.frame_query.contains(parent) {
                true => Some(parent),
                false => None,
            },
        }
    }

    /// Get handles to all reference frames that are children of this reference frame. Applies a
    /// filter to the returned children.
    fn child_frames_filtered(
        &mut self,
        this: Entity,
        mut filter: impl FnMut(Entity) -> bool,
    ) -> Vec<Entity> {
        self.children
            .get(this)
            .iter()
            .flat_map(|c| c.iter())
            .filter(|entity| filter(**entity))
            .filter(|child| self.frame_query.contains(**child))
            .copied()
            .collect()
    }

    /// Get IDs to all reference frames that are children of this reference frame.
    pub fn child_frames(&mut self, this: Entity) -> Vec<Entity> {
        self.child_frames_filtered(this, |_| true)
    }

    /// Get IDs to all reference frames that are siblings of this reference frame.
    pub fn sibling_frames(&mut self, this_entity: Entity) -> Vec<Entity> {
        if let Some(parent) = self.parent_frame_entity(this_entity) {
            self.child_frames_filtered(parent, |e| e != this_entity)
        } else {
            Vec::new()
        }
    }
}

/// Used to access a reference frame. Needed because the reference frame could either be a
/// component, or a resource if at the root of the hierarchy.
#[derive(SystemParam)]
pub struct ReferenceFramesMut<'w, 's, P: GridPrecision> {
    parent: Query<'w, 's, Read<Parent>>,
    children: Query<'w, 's, Read<Children>>,
    position: Query<'w, 's, (Read<GridCell<P>>, Read<Transform>)>,
    frame_query: Query<'w, 's, (Entity, Write<ReferenceFrame<P>>, Option<Read<Parent>>)>,
}

impl<'w, 's, P: GridPrecision> ReferenceFramesMut<'w, 's, P> {
    /// Get mutable access to the [`ReferenceFrame`], and run the provided function or closure,
    /// optionally returning data.
    ///
    /// ## Panics
    ///
    /// This will panic if the entity passed in is invalid.
    ///
    /// ## Why a closure?
    ///
    /// This expects a closure because the reference frame could be stored as a component or a
    /// resource, making it difficult (impossible?) to return a mutable reference to the reference
    /// frame when the types involved are different. The main issue seems to be that the component
    /// is returned as a `Mut<T>`; getting a mutable reference to the internal value requires that
    /// this function return a reference to a value owned by the function.
    ///
    /// I tried returning an enum or a boxed trait object, but ran into issues expressing the
    /// lifetimes. Worth revisiting if this turns out to be annoying, but seems pretty insignificant
    /// at the time of writing.
    pub fn update_reference_frame<T>(
        &mut self,
        frame_entity: Entity,
        mut func: impl FnMut(&mut ReferenceFrame<P>, &GridCell<P>, &Transform) -> T,
    ) -> T {
        let (cell, transform) = self.position(frame_entity);
        self.frame_query
            .get_mut(frame_entity)
            .map(|(_entity, mut frame, _parent)| func(frame.as_mut(), &cell, &transform))
            .expect("The supplied reference frame handle to node is no longer valid.")
    }

    /// Get the reference frame and the position of the reference frame from its `Entity`.
    pub fn get(&self, frame_entity: Entity) -> (&ReferenceFrame<P>, GridCell<P>, Transform) {
        let (cell, transform) = self.position(frame_entity);
        self.frame_query
            .get(frame_entity)
            .map(|(_entity, frame, _parent)| (frame, cell, transform))
            .unwrap_or_else(|e| {
                panic!("Reference frame entity {frame_entity:?} missing ReferenceFrame component.\n\tError: {e}");
            })
    }

    /// Get the position of this reference frame, including its grid cell and transform, or return
    /// defaults if they are missing.
    ///
    /// Needed because the root reference frame should not have a grid cell or transform.
    pub fn position(&self, frame_entity: Entity) -> (GridCell<P>, Transform) {
        let (cell, transform) = (GridCell::default(), Transform::default());
        let (cell, transform) = self.position.get(frame_entity).unwrap_or_else(|_| {
        assert!(self.parent.get(frame_entity).is_err(), "Reference frame entity {frame_entity:?} is missing a GridCell and Transform. This is valid only if this is a root reference frame, but this is not.");
            (&cell, &transform)
        });
        (*cell, *transform)
    }

    /// Get the ID of the reference frame that `this` `Entity` is a child of, if it exists.
    #[inline]
    pub fn parent_frame(&self, this: Entity) -> Option<Entity> {
        match self.parent.get(this).map(|parent| **parent) {
            Err(_) => None,
            Ok(parent) => match self.frame_query.contains(parent) {
                true => Some(parent),
                false => None,
            },
        }
    }

    /// Get handles to all reference frames that are children of this reference frame. Applies a
    /// filter to the returned children.
    fn child_frames_filtered(
        &mut self,
        this: Entity,
        mut filter: impl FnMut(Entity) -> bool,
    ) -> Vec<Entity> {
        self.children
            .get(this)
            .iter()
            .flat_map(|c| c.iter())
            .filter(|entity| filter(**entity))
            .filter(|child| self.frame_query.contains(**child))
            .copied()
            .collect()
    }

    /// Get IDs to all reference frames that are children of this reference frame.
    pub fn child_frames(&mut self, this: Entity) -> Vec<Entity> {
        self.child_frames_filtered(this, |_| true)
    }

    /// Get IDs to all reference frames that are siblings of this reference frame.
    pub fn sibling_frames(&mut self, this_entity: Entity) -> Vec<Entity> {
        if let Some(parent) = self.parent_frame(this_entity) {
            self.child_frames_filtered(parent, |e| e != this_entity)
        } else {
            Vec::new()
        }
    }
}

impl<P: GridPrecision> LocalFloatingOrigin<P> {
    /// Update the [`LocalFloatingOrigin`] of every [`ReferenceFrame`] in the world. This does not
    /// update any entity transforms, instead this is a preceding step that updates every reference
    /// frame, so it knows where the floating origin is located with respect to that reference
    /// frame. This is all done in high precision if possible, however any loss in precision will
    /// only affect the rendering precision. The high precision coordinates ([`GridCell`] and
    /// [`Transform`]) are the source of truth and never mutated.
    pub fn compute_all(
        mut reference_frames: ReferenceFramesMut<P>,
        mut frame_stack: Local<Vec<Entity>>,
        cells: Query<(Entity, &GridCell<P>)>,
        roots: Query<(Entity, &BigSpace)>,
        parents: Query<&Parent>,
    ) {
        /// The maximum reference frame tree depth, defensively prevents infinite looping in case
        /// there is a degenerate hierarchy. It might take a while, but at least it's not forever?
        const MAX_REFERENCE_FRAME_DEPTH: usize = 255;

        // TODO: because each tree under a root is disjoint, these updates can be done in parallel
        // without aliasing. This will require unsafe, just like bevy's own transform propagation.
        'outer: for (origin_entity, origin_cell) in roots
            .iter() // TODO: If any of these checks fail, log to some diagnostic
            .filter_map(|(root_entity, root)| root.validate_floating_origin(root_entity, &parents))
            .filter_map(|origin| cells.get(origin).ok())
        {
            let Some(mut this_frame) = reference_frames.parent_frame(origin_entity) else {
                error!("The floating origin is not in a valid reference frame. The floating origin entity must be a child of an entity with the `ReferenceFrame` component.");
                continue;
            };

            // Prepare by resetting the `origin_transform` of the floating origin's reference frame.
            // Because the floating origin is within this reference frame, there is no grid
            // misalignment and thus no need for any floating offsets.
            reference_frames.update_reference_frame(this_frame, |frame, _, _| {
                frame
                    .local_floating_origin
                    .set(*origin_cell, Vec3::ZERO, DQuat::IDENTITY);
            });

            // Seed the frame stack with the floating origin's reference frame. From this point out,
            // we will only look at siblings and parents, which will allow us to visit the entire
            // tree.
            frame_stack.clear();
            frame_stack.push(this_frame);

            // Recurse up and across the tree, updating siblings and their children.
            for _ in 0..MAX_REFERENCE_FRAME_DEPTH {
                // We start by propagating up to the parent of this frame, then propagating down to
                // the siblings of this frame (children of the parent that are not this frame).
                if let Some(parent_frame) = reference_frames.parent_frame(this_frame) {
                    propagate_origin_to_parent(this_frame, &mut reference_frames, parent_frame);
                    for sibling_frame in reference_frames.sibling_frames(this_frame) {
                        // The siblings of this frame are also the children of the parent frame.
                        propagate_origin_to_child(
                            parent_frame,
                            &mut reference_frames,
                            sibling_frame,
                        );
                        frame_stack.push(sibling_frame); // We'll recurse through children next
                    }
                }

                // All of the reference frames pushed on the stack have been processed. We can now
                // pop those off the stack and recursively process their children all the way out to
                // the leaves of the tree.
                while let Some(this_frame) = frame_stack.pop() {
                    for child_frame in reference_frames.child_frames(this_frame) {
                        propagate_origin_to_child(this_frame, &mut reference_frames, child_frame);
                        frame_stack.push(child_frame) // Push processed child onto the stack
                    }
                }

                // Finally, now that the siblings of this frame have been recursively processed, we
                // process the parent and set it as the current reference frame. Note that every
                // time we step to a parent, "this frame" and all descendants have already been
                // processed, so we only need to process the siblings.
                match reference_frames.parent_frame(this_frame) {
                    Some(parent_frame) => this_frame = parent_frame,
                    None => continue 'outer, // We have reached the root of the tree, and can exit.
                }
            }

            error!("Reached the maximum reference frame depth ({MAX_REFERENCE_FRAME_DEPTH}), and exited early to prevent an infinite loop. This might be caused by a degenerate hierarchy.")
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::system::SystemState, math::DVec3, prelude::*};

    use super::*;
    use crate::*;

    /// Test that the reference frame getters do what they say they do.
    #[test]
    fn frame_hierarchy_getters() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::<i32>::default());

        let frame_bundle = (
            Transform::default(),
            GridCell::<i32>::default(),
            ReferenceFrame::<i32>::default(),
        );

        let child_1 = app.world_mut().spawn(frame_bundle.clone()).id();
        let child_2 = app.world_mut().spawn(frame_bundle.clone()).id();
        let parent = app.world_mut().spawn(frame_bundle.clone()).id();
        let root = app.world_mut().spawn(frame_bundle.clone()).id();

        app.world_mut().entity_mut(root).push_children(&[parent]);
        app.world_mut()
            .entity_mut(parent)
            .push_children(&[child_1, child_2]);

        let mut state = SystemState::<ReferenceFramesMut<i32>>::new(app.world_mut());
        let mut ref_frames = state.get_mut(app.world_mut());

        // Children
        let result = ref_frames.child_frames(root);
        assert_eq!(result, vec![parent]);
        let result = ref_frames.child_frames(parent);
        assert!(result.contains(&child_1));
        assert!(result.contains(&child_2));
        let result = ref_frames.child_frames(child_1);
        assert_eq!(result, Vec::new());

        // Parent
        let result = ref_frames.parent_frame(root);
        assert_eq!(result, None);
        let result = ref_frames.parent_frame(parent);
        assert_eq!(result, Some(root));
        let result = ref_frames.parent_frame(child_1);
        assert_eq!(result, Some(parent));

        // Siblings
        let result = ref_frames.sibling_frames(root);
        assert_eq!(result, vec![]);
        let result = ref_frames.sibling_frames(parent);
        assert_eq!(result, vec![]);
        let result = ref_frames.sibling_frames(child_1);
        assert_eq!(result, vec![child_2]);
    }

    #[test]
    fn child_propagation() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::<i32>::default());

        let root_frame = ReferenceFrame {
            local_floating_origin: LocalFloatingOrigin::new(
                GridCell::<i32>::new(1_000_000, -1, -1),
                Vec3::ZERO,
                DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2),
            ),
            ..default()
        };
        let root = app
            .world_mut()
            .spawn((Transform::default(), GridCell::<i32>::default(), root_frame))
            .id();

        let child = app
            .world_mut()
            .spawn((
                Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(1.0, 1.0, 0.0)),
                GridCell::<i32>::new(1_000_000, 0, 0),
                ReferenceFrame::<i32>::default(),
            ))
            .id();

        app.world_mut().entity_mut(root).push_children(&[child]);

        let mut state = SystemState::<ReferenceFramesMut<i32>>::new(app.world_mut());
        let mut reference_frames = state.get_mut(app.world_mut());

        // The function we are testing
        propagate_origin_to_child(root, &mut reference_frames, child);

        let (child_frame, ..) = reference_frames.get(child);

        let computed_grid = child_frame.local_floating_origin.cell();
        let correct_grid = GridCell::new(-1, 0, -1);
        assert_eq!(computed_grid, correct_grid);

        let computed_rot = child_frame.local_floating_origin.rotation();
        let correct_rot = DQuat::from_rotation_z(std::f64::consts::PI);
        let rot_error = computed_rot.angle_between(correct_rot);
        assert!(rot_error < 1e-10);

        // Even though we are 2 billion units from the origin, our precision is still pretty good.
        // The loss of precision is coming from the affine multiplication that moves the origin into
        // the child's reference frame. The good news is that precision loss only scales with the
        // distance of the origin to the child (in the child's reference frame). In this test we are
        // saying that the floating origin is - with respect to the root - pretty near the child.
        // Even though the child and floating origin are very far from the origin, we only lose
        // precision based on how for the origin is from the child.
        let computed_trans = child_frame.local_floating_origin.translation();
        let correct_trans = Vec3::new(-1.0, 1.0, 0.0);
        let trans_error = computed_trans.distance(correct_trans);
        assert!(trans_error < 1e-4);
    }

    #[test]
    fn parent_propagation() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::<i64>::default());

        let frame_bundle = (
            Transform::default(),
            GridCell::<i64>::default(),
            ReferenceFrame::<i64>::default(),
        );
        let root = app.world_mut().spawn(frame_bundle.clone()).id();

        let child = app
            .world_mut()
            .spawn((
                Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(1.0, 1.0, 0.0)),
                GridCell::<i64>::new(150_000_003_000, 0, 0), // roughly radius of earth orbit
                ReferenceFrame {
                    local_floating_origin: LocalFloatingOrigin::new(
                        GridCell::<i64>::new(0, 3_000, 0),
                        Vec3::new(5.0, 5.0, 0.0),
                        DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2),
                    ),
                    ..Default::default()
                },
            ))
            .id();

        app.world_mut().entity_mut(root).push_children(&[child]);

        let mut state = SystemState::<ReferenceFramesMut<i64>>::new(app.world_mut());
        let mut reference_frames = state.get_mut(app.world_mut());

        // The function we are testing
        propagate_origin_to_parent(child, &mut reference_frames, root);

        let (root_frame, ..) = reference_frames.get(root);

        let computed_grid = root_frame.local_floating_origin.cell();
        let correct_grid = GridCell::new(150_000_000_000, 0, 0);
        assert_eq!(computed_grid, correct_grid);

        let computed_rot = root_frame.local_floating_origin.rotation();
        let correct_rot = DQuat::IDENTITY;
        let rot_error = computed_rot.angle_between(correct_rot);
        assert!(rot_error < 1e-7);

        // This is the error of the position of the floating origin if the origin was a person
        // standing on earth, and their position was resampled with respect to the sun. This is 0.3
        // meters, but recall that this will be the error when positioning the other planets in the
        // solar system when rendering.
        //
        // This error scales with the distance of the floating origin from the origin of its
        // reference frame, in this case the radius of the earth, not the radius of the orbit.
        let computed_trans = root_frame.local_floating_origin.translation();
        let correct_trans = Vec3::new(-4.0, 6.0, 0.0);
        let trans_error = computed_trans.distance(correct_trans);
        assert!(trans_error < 0.3);
    }

    #[test]
    fn origin_transform() {
        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::<i32>::default());

        let root = app
            .world_mut()
            .spawn((
                Transform::default(),
                GridCell::<i32>::default(),
                ReferenceFrame {
                    local_floating_origin: LocalFloatingOrigin::new(
                        GridCell::<i32>::new(0, 0, 0),
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
                    .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(3.0, 3.0, 0.0)),
                GridCell::<i32>::new(0, 0, 0),
                ReferenceFrame::<i32>::default(),
            ))
            .id();

        app.world_mut().entity_mut(root).push_children(&[child]);

        let mut state = SystemState::<ReferenceFramesMut<i32>>::new(app.world_mut());
        let mut reference_frames = state.get_mut(app.world_mut());

        propagate_origin_to_child(root, &mut reference_frames, child);

        let (child_frame, ..) = reference_frames.get(child);
        let child_local_point = DVec3::new(5.0, 5.0, 0.0);

        let computed_transform = child_frame
            .local_floating_origin
            .reference_frame_transform();
        let computed_pos = computed_transform.transform_point3(child_local_point);

        let correct_transform = DAffine3::from_rotation_translation(
            DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2),
            DVec3::new(2.0, 2.0, 0.0),
        );
        let correct_pos = correct_transform.transform_point3(child_local_point);

        assert!((computed_pos - correct_pos).length() < 1e-6);
        assert!((computed_pos - DVec3::new(7.0, -3.0, 0.0)).length() < 1e-6);
    }
}
