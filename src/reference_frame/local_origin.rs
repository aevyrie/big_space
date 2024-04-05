//! Describes how the floating origin's position is propagated through the hierarchy of reference
//! frames, and used to compute the floating origin's position relative to each reference frame.

use bevy::{
    ecs::{
        prelude::*,
        system::{
            lifetimeless::{Read, Write},
            SystemParam,
        },
    },
    hierarchy::prelude::*,
    log::prelude::*,
    math::{prelude::*, DAffine3, DQuat},
    transform::prelude::*,
};

use super::{ReferenceFrame, RootReferenceFrame};
use crate::{FloatingOrigin, GridCell, GridPrecision};

pub use inner::LocalFloatingOrigin;

/// A module kept private to enforce use of setters and getters within the parent module.
mod inner {
    use bevy::{
        math::{prelude::*, DAffine3, DMat3, DQuat},
        reflect::prelude::*,
    };

    use crate::{GridCell, GridPrecision};

    /// An isometry that describes the location of the floating origin's grid cell, relative to the
    /// local reference frame. Additionally, this contains the
    ///
    /// Used to compute the [`GlobalTransform`](bevy::transform::components::GlobalTransform) of
    /// every entity within a reference frame. We can compute it once, then use it to transform
    /// every entity relative to the floating origin.
    ///
    /// More precisely, this tells us the position of the origin of the *grid cell* the floating
    /// origin is located in, relative to the local reference frame.
    ///
    /// If the floating origin is in this local reference frame, the `float` fields will be
    /// identity. The `float` fields` will be non-identity when the floating origin is in a
    /// different reference frame that does not perfectly align with this one. Different reference
    /// frames can be rotated and offset from each other - consider the reference frame of a planet,
    /// spinning about its axis and orbiting about a star, it will not align with the inertial
    /// reference frame of the star system!
    #[derive(Default, Debug, Clone, PartialEq, Reflect)]
    pub struct LocalFloatingOrigin<P: GridPrecision> {
        /// The cell that the origin of the floating origin's grid cell falls into.
        cell: GridCell<P>,
        /// The translation of floating origin's grid cell relative the specified cell.
        translation: Vec3,
        /// The rotation of the floating origin's grid cell relative to the specified cell.
        rotation: DQuat,
        /// Transform from the floating origin's grid cell's reference frame to the local
        /// [`LocalFloatingOrigin::cell`]. This is used to compute the [`GlobalTransform`] of all
        /// entities in this reference frame.
        ///
        /// This is like a camera's "view transform", but instead of transforming an object into a
        /// camera's view space, this will transform an object into the floating origin's reference
        /// frame.
        ///   - That object must be positioned in the same [`ReferenceFrame`] that this
        ///     [`LocalFloatingOrigin`] is part of.
        ///   - That object's position must be relative to the same grid cell as defined by
        ///     [`LocalFloatingOrigin::translation`].
        ///
        /// The above requirements help to ensure this transform has a small magnitude, maximizing
        /// precision, and minimizing floating point error.
        origin_transform: DAffine3,
    }

    impl<P: GridPrecision> LocalFloatingOrigin<P> {
        /// The "view" transform of the reference frame's transform within its grid cell.
        pub fn transform(&self) -> DAffine3 {
            self.origin_transform
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

            self.origin_transform = DAffine3 {
                matrix3: DMat3::from_quat(self.rotation),
                translation: self.translation.as_dvec3(),
            }
            .inverse()
        }

        /// Create a new [`LocalFloatingOrigin`].
        pub fn new(cell: GridCell<P>, translation: Vec3, rotation: DQuat) -> Self {
            let origin_transform = DAffine3 {
                matrix3: DMat3::from_quat(rotation),
                translation: translation.as_dvec3(),
            }
            .inverse();

            Self {
                cell,
                translation,
                rotation,
                origin_transform,
            }
        }
    }
}

/// Used to access a reference frame. Needed because the reference frame could either be a
/// component, or a resource if at the root of the hierarchy.
#[derive(SystemParam)]
pub struct ReferenceFrameParam<'w, 's, P: GridPrecision> {
    parent: Query<'w, 's, Read<Parent>>,
    children: Query<'w, 's, Read<Children>>,
    frame_root: ResMut<'w, RootReferenceFrame<P>>,
    frame_query: Query<
        'w,
        's,
        (
            Entity,
            Read<GridCell<P>>,
            Read<Transform>,
            Write<ReferenceFrame<P>>,
            Option<Read<Parent>>,
        ),
    >,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
/// Use the [`ReferenceFrameParam`] [`SystemParam`] to do useful things with this handle.
///
/// A reference frame can either be a node in the entity hierarchy stored as a component, or will be
/// the root reference frame, which is tracked with a resource. This handle is used to unify access
/// to reference frames with a single lightweight type.
pub enum ReferenceFrameHandle {
    /// The reference frame is a node in the hierarchy, stored in a [`ReferenceFrame`] component.
    Node(Entity),
    /// The root reference frame, defined in the [`RootReferenceFrame`] resource.
    Root,
}

impl ReferenceFrameHandle {
    /// Propagate the local origin position from `self` to `child`.
    ///
    /// This is not a method on `ReferenceFrameParam` to help prevent misuse when accidentally
    /// swapping the position of arguments.
    fn propagate_origin_to_child<P: GridPrecision>(
        self,
        reference_frames: &mut ReferenceFrameParam<P>,
        child: ReferenceFrameHandle,
    ) {
        let (this_frame, _this_cell, _this_transform) = reference_frames.get(self);
        let (child_frame, child_cell, child_transform) = reference_frames.get(child);

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

        reference_frames.update(child, |child_frame, _, _| {
            child_frame.local_floating_origin.set(
                child_origin_cell,
                child_origin_translation_float,
                origin_child_rotation,
            );
        })
    }

    fn propagate_origin_to_parent<P: GridPrecision>(
        self,
        reference_frames: &mut ReferenceFrameParam<P>,
        parent: ReferenceFrameHandle,
    ) {
        let (this_frame, this_cell, this_transform) = reference_frames.get(self);
        let (parent_frame, _parent_cell, _parent_transform) = reference_frames.get(parent);

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

        reference_frames.update(parent, |parent_frame, _, _| {
            parent_frame.local_floating_origin.set(
                parent_origin_cell,
                origin_translation_remainder,
                origin_rot,
            );
        });
    }
}

impl<'w, 's, P: GridPrecision> ReferenceFrameParam<'w, 's, P> {
    /// Get mutable access to the [`ReferenceFrame`], and run the provided function or closure,
    /// optionally returning data.
    ///
    /// ## Panics
    ///
    /// This will panic if the handle passed in is invalid.
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
    #[inline]
    pub fn update<T>(
        &mut self,
        handle: ReferenceFrameHandle,
        mut func: impl FnMut(&mut ReferenceFrame<P>, &GridCell<P>, &Transform) -> T,
    ) -> T {
        match handle {
            ReferenceFrameHandle::Node(frame_entity) => self
                .frame_query
                .get_mut(frame_entity)
                .map(|(_entity, cell, transform, mut frame, _parent)| {
                    func(frame.as_mut(), cell, transform)
                })
                .expect("The supplied reference frame handle to node is no longer valid."),
            ReferenceFrameHandle::Root => func(
                &mut self.frame_root,
                &GridCell::default(), // the reference frame itself is not within another.
                &Transform::default(), // the reference frame itself is not within another.
            ),
        }
    }

    /// Get the reference frame and the position of the reference frame from a handle.
    pub fn get(
        &self,
        handle: ReferenceFrameHandle,
    ) -> (&ReferenceFrame<P>, GridCell<P>, Transform) {
        match handle {
            ReferenceFrameHandle::Node(frame_entity) => self
                .frame_query
                .get(frame_entity)
                .map(|(_entity, cell, transform, frame, _parent)| (frame, *cell, *transform))
                .unwrap_or_else(|e| {
                    panic!("{} {handle:?} failed to resolve.\n\tEnsure all GridPrecision components are using the <{}> generic, and all required components are present.\n\tQuery Error: {e}", std::any::type_name::<ReferenceFrameHandle>(), std::any::type_name::<P>())
                }),
            ReferenceFrameHandle::Root => {
                (&self.frame_root, GridCell::default(), Transform::default())
            }
        }
    }

    /// Get a handle to this entity's reference frame, if it exists.
    #[inline]
    pub fn reference_frame(&mut self, this: Entity) -> Option<ReferenceFrameHandle> {
        match self.parent.get(this).map(|parent| **parent) {
            Err(_) => Some(ReferenceFrameHandle::Root),
            Ok(parent) => match self.frame_query.contains(parent) {
                true => Some(ReferenceFrameHandle::Node(parent)),
                false => None,
            },
        }
    }

    /// Get a handle to the parent reference frame of this reference frame, if it exists.
    #[inline]
    pub fn parent(&mut self, this: ReferenceFrameHandle) -> Option<ReferenceFrameHandle> {
        match this {
            ReferenceFrameHandle::Node(this) => self.reference_frame(this),
            ReferenceFrameHandle::Root => None,
        }
    }

    /// Get handles to all reference frames that are children of this reference frame. Applies a
    /// filter to the returned children.
    #[inline]
    pub fn children_filtered(
        &mut self,
        this: ReferenceFrameHandle,
        mut filter: impl FnMut(Entity) -> bool,
    ) -> Vec<ReferenceFrameHandle> {
        match this {
            ReferenceFrameHandle::Node(this) => self
                .children
                .get(this)
                .iter()
                .flat_map(|c| c.iter())
                .filter(|entity| filter(**entity))
                .filter(|child| self.frame_query.contains(**child))
                .map(|child| ReferenceFrameHandle::Node(*child))
                .collect(),
            ReferenceFrameHandle::Root => self
                .frame_query
                .iter()
                .filter(|(entity, ..)| filter(*entity))
                .filter(|(.., parent)| parent.is_none())
                .map(|(entity, ..)| ReferenceFrameHandle::Node(entity))
                .collect(),
        }
    }

    /// Get handles to all reference frames that are children of this reference frame.
    #[inline]
    pub fn children(&mut self, this: ReferenceFrameHandle) -> Vec<ReferenceFrameHandle> {
        self.children_filtered(this, |_| true)
    }

    /// Get handles to all reference frames that are siblings of this reference frame.
    #[inline]
    pub fn siblings(&mut self, this: ReferenceFrameHandle) -> Vec<ReferenceFrameHandle> {
        match this {
            ReferenceFrameHandle::Node(this_entity) => {
                if let Some(parent) = self.parent(this) {
                    self.children_filtered(parent, |e| e != this_entity)
                } else {
                    Vec::new()
                }
            }
            ReferenceFrameHandle::Root => Vec::new(),
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
    pub fn update(
        origin: Query<(Entity, &GridCell<P>), With<FloatingOrigin>>,
        mut reference_frames: ReferenceFrameParam<P>,
        mut frame_stack: Local<Vec<ReferenceFrameHandle>>,
    ) {
        /// The maximum reference frame tree depth, defensively prevents infinite looping in case
        /// there is a degenerate hierarchy. It might take a while, but at least it's not forever?
        const MAX_REFERENCE_FRAME_DEPTH: usize = usize::MAX;

        let (origin_entity, origin_cell) = origin
            .get_single()
            .expect("There can only be one entity with the `FloatingOrigin` component.");

        let Some(mut this_frame) = reference_frames.reference_frame(origin_entity) else {
            error!("The floating origin is not in a valid reference frame. The floating origin entity must be a child of an entity with the `ReferenceFrame`, `GridCell`, and `Transform` components, or be at the root of the parent-child hierarchy.");
            return;
        };

        // Prepare by resetting the `origin_transform` of the floating origin's reference frame.
        // Because the floating origin is within this reference frame, there is no grid misalignment
        // and thus no need for any floating offsets.
        reference_frames.update(this_frame, |frame, _, _| {
            frame
                .local_floating_origin
                .set(*origin_cell, Vec3::ZERO, DQuat::IDENTITY);
        });

        // Seed the frame stack with the floating origin's reference frame. From this point out, we
        // will only look at siblings and parents, which will allow us to visit the entire tree.
        frame_stack.clear();
        frame_stack.push(this_frame);

        // Recurse up and across the tree, updating siblings and their children.
        for _ in 0..MAX_REFERENCE_FRAME_DEPTH {
            // We start by propagating up to the parent of this frame, then propagating down to the
            // siblings of this frame (children of the parent that are not this frame).
            if let Some(parent_frame) = reference_frames.parent(this_frame) {
                this_frame.propagate_origin_to_parent(&mut reference_frames, parent_frame);
                for sibling_frame in reference_frames.siblings(this_frame) {
                    // The siblings of this frame are also the children of the parent frame.
                    parent_frame.propagate_origin_to_child(&mut reference_frames, sibling_frame);
                    frame_stack.push(sibling_frame); // We'll recurse through children next
                }
            }

            // All of the reference frames pushed on the stack have been processed. We can now pop
            // those off the stack and recursively process their children all the way out to the
            // leaves of the tree.
            while let Some(this_frame) = frame_stack.pop() {
                for child_frame in reference_frames.children(this_frame) {
                    this_frame.propagate_origin_to_child(&mut reference_frames, child_frame);
                    frame_stack.push(child_frame) // Push processed child onto the stack. Recursion, baby!
                }
            }

            // Finally, now that the siblings of this frame have been recursively processed, we
            // process the parent and set it as the current reference frame. Note that every time we
            // step to a parent, "this frame" and all descendants have already been processed, so we
            // only need to process the siblings.
            match reference_frames.parent(this_frame) {
                Some(parent_frame) => this_frame = parent_frame,
                None => return, // We have reached the root of the tree, and can exit.
            }
        }

        error!("Reached the maximum reference frame depth ({MAX_REFERENCE_FRAME_DEPTH}), and exited early to prevent an infinite loop. This might be caused by a degenerate hierarchy.")
    }
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::system::SystemState, math::DVec3};

    use super::*;
    use crate::*;

    /// Test that the reference frame getters do what they say they do.
    #[test]
    fn frame_hierarchy_getters() {
        let mut app = App::new();
        app.add_plugins(FloatingOriginPlugin::<i32>::default());

        let frame_bundle = (
            Transform::default(),
            GridCell::<i32>::default(),
            ReferenceFrame::<i32>::default(),
        );

        let child_1 = app.world.spawn(frame_bundle.clone()).id();
        let child_2 = app.world.spawn(frame_bundle.clone()).id();
        let parent = app.world.spawn(frame_bundle.clone()).id();
        app.world
            .entity_mut(parent)
            .push_children(&[child_1, child_2]);

        let mut state = SystemState::<ReferenceFrameParam<i32>>::new(&mut app.world);
        let mut ref_frame = state.get_mut(&mut app.world);

        // Children
        let result = ref_frame.children(ReferenceFrameHandle::Root);
        assert_eq!(result, vec![ReferenceFrameHandle::Node(parent)]);
        let result = ref_frame.children(ReferenceFrameHandle::Node(parent));
        assert!(result.contains(&ReferenceFrameHandle::Node(child_1)));
        assert!(result.contains(&ReferenceFrameHandle::Node(child_2)));
        let result = ref_frame.children(ReferenceFrameHandle::Node(child_1));
        assert_eq!(result, Vec::new());

        // Parent
        let result = ref_frame.parent(ReferenceFrameHandle::Root);
        assert_eq!(result, None);
        let result = ref_frame.parent(ReferenceFrameHandle::Node(parent));
        assert_eq!(result, Some(ReferenceFrameHandle::Root));
        let result = ref_frame.parent(ReferenceFrameHandle::Node(child_1));
        assert_eq!(result, Some(ReferenceFrameHandle::Node(parent)));

        // Siblings
        let result = ref_frame.siblings(ReferenceFrameHandle::Root);
        assert_eq!(result, vec![]);
        let result = ref_frame.siblings(ReferenceFrameHandle::Node(parent));
        assert_eq!(result, vec![]);
        let result = ref_frame.siblings(ReferenceFrameHandle::Node(child_1));
        assert_eq!(result, vec![ReferenceFrameHandle::Node(child_2)]);
    }

    #[test]
    fn child_propagation() {
        let mut app = App::new();
        app.add_plugins(FloatingOriginPlugin::<i32>::default());

        let root = ReferenceFrameHandle::Root;

        app.insert_resource(RootReferenceFrame(ReferenceFrame {
            local_floating_origin: LocalFloatingOrigin::new(
                GridCell::<i32>::new(1_000_000, -1, -1),
                Vec3::ZERO,
                DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2),
            ),
            ..default()
        }));

        let child = app
            .world
            .spawn((
                Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(1.0, 1.0, 0.0)),
                GridCell::<i32>::new(1_000_000, 0, 0),
                ReferenceFrame::<i32>::default(),
            ))
            .id();
        let child = ReferenceFrameHandle::Node(child);

        let mut state = SystemState::<ReferenceFrameParam<i32>>::new(&mut app.world);
        let mut reference_frames = state.get_mut(&mut app.world);

        // The function we are testing
        root.propagate_origin_to_child(&mut reference_frames, child);

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
        app.add_plugins(FloatingOriginPlugin::<i64>::default());

        let root = ReferenceFrameHandle::Root;

        let child = app
            .world
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
                    ..default()
                },
            ))
            .id();
        let child = ReferenceFrameHandle::Node(child);

        let mut state = SystemState::<ReferenceFrameParam<i64>>::new(&mut app.world);
        let mut reference_frames = state.get_mut(&mut app.world);

        // The function we are testing
        child.propagate_origin_to_parent(&mut reference_frames, root);

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
        app.add_plugins(FloatingOriginPlugin::<i32>::default());

        let root = ReferenceFrameHandle::Root;

        app.insert_resource(RootReferenceFrame(ReferenceFrame {
            local_floating_origin: LocalFloatingOrigin::new(
                GridCell::<i32>::new(0, 0, 0),
                Vec3::new(1.0, 1.0, 0.0),
                DQuat::from_rotation_z(0.0),
            ),
            ..default()
        }));

        let child = app
            .world
            .spawn((
                Transform::default()
                    .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(3.0, 3.0, 0.0)),
                GridCell::<i32>::new(0, 0, 0),
                ReferenceFrame::<i32>::default(),
            ))
            .id();
        let child = ReferenceFrameHandle::Node(child);

        let mut state = SystemState::<ReferenceFrameParam<i32>>::new(&mut app.world);
        let mut reference_frames = state.get_mut(&mut app.world);

        root.propagate_origin_to_child(&mut reference_frames, child);

        let (child_frame, ..) = reference_frames.get(child);
        let child_local_point = DVec3::new(5.0, 5.0, 0.0);

        let computed_transform = child_frame.local_floating_origin.transform();
        let computed_pos = computed_transform.transform_point3(child_local_point);

        let correct_transform = DAffine3::from_rotation_translation(
            DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2),
            DVec3::new(2.0, 2.0, 0.0),
        );
        let correct_pos = correct_transform.transform_point3(child_local_point);

        // assert_eq!(computed_transform, correct_transform);
        assert!((computed_pos - correct_pos).length() < 1e-6);
        assert!((computed_pos - DVec3::new(7.0, -3.0, 0.0)).length() < 1e-6);
    }
}
