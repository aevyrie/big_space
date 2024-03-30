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
    reflect::prelude::*,
    transform::prelude::*,
};

use crate::{GridCell, GridPrecision};

use super::{ReferenceFrame, RootReferenceFrame};

/// An isometry that describes the location of the floating origin's grid cell, relative to this
/// reference frame.
///
/// This is used to easily compute the rendering transform [`GlobalTransform`] of every entity
/// within a reference frame. Because this tells us where the floating origin is located within the
/// local reference frame, we can compute it once, then use it to transform every entity relative to
/// the floating origin.
///
/// If the floating origin is in this local reference frame, the `float` fields will be identity.
/// The `float` fields` will be non-identity when the floating origin is in a different reference
/// frame that does not perfectly align with this one. Different reference frames can be rotated and
/// offset from each other - consider the reference frame of a planet, spinning about its axis and
/// orbiting about a star, it will not align with the inertial reference frame of the star system!
#[derive(Default, Debug, Clone, PartialEq, Reflect)]
pub struct LocalFloatingOrigin<P: GridPrecision> {
    /// The cell that the origin of the floating origin's grid cell falls into.
    translation_grid: GridCell<P>,
    /// The translation of floating origin's grid cell relative the specified cell.
    translation_float: Vec3,
    /// The rotation of the floating origin's grid cell relative to the specified cell.
    rotation_float: DQuat,
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
        let origin_cell_relative_to_child =
            this_frame.origin_transform.translation_grid - child_cell;
        let origin_translation = this_frame.grid_position_double(
            &origin_cell_relative_to_child,
            &Transform::from_translation(this_frame.origin_transform.translation_float),
        );

        // then combine with rotation to get a double transform from the child's cell origin to the origin.
        let origin_rotation = this_frame.origin_transform.rotation_float;
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
            child_frame.origin_transform.translation_grid = child_origin_cell;
            child_frame.origin_transform.translation_float = child_origin_translation_float;
            child_frame.origin_transform.rotation_float = origin_child_rotation;
        })
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
                .expect("The supplied reference frame handle to node is no longer valid."),
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
    /// The maximum reference frame tree depth, defensively prevents infinite looping in case there
    /// is a degenerate hierarchy. It might take a long time, but at least it's not forever?
    const MAX_REFERENCE_FRAME_DEPTH: usize = usize::MAX;

    /// Update the [`LocalFloatingOrigin`] of every [`ReferenceFrame`] in the world. This does not
    /// update any entity transforms, instead this is a preceding step that updates every reference
    /// frame, so it knows where the floating origin is located with respect to that reference
    /// frame. This is all done in high precision if possible, however any loss in precision will
    /// only affect the rendering precision. The high precision coordinates ([`GridCell`] and
    /// [`Transform`]) are the source of truth and never mutated.
    pub fn update(
        origin: Query<(Entity, &GridCell<P>)>,
        mut reference_frames: ReferenceFrameParam<P>,
        mut frame_stack: Local<Vec<ReferenceFrameHandle>>,
    ) {
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
            frame.origin_transform.rotation_float = DQuat::IDENTITY;
            frame.origin_transform.translation_float = Vec3::ZERO;
            frame.origin_transform.translation_grid = *origin_cell;
        });

        // Seed the frame stack with the floating origin's reference frame. From this point out, we
        // will only look at siblings and parents, which will allow us to visit the entire tree.
        frame_stack.clear();
        frame_stack.push(this_frame);

        // Recurse up and across the tree, updating siblings and their children.
        for _ in 0..Self::MAX_REFERENCE_FRAME_DEPTH {
            // Compute sibling origins in higher precision by using their relative distance to their
            // sibling that contains the floating origin. This saves some precision that could be
            // lost if you instead used their position relative to their parent, which is generally
            // much larger scale that two sibling reference frames are near each other.
            for sibling_frame in reference_frames.siblings(this_frame) {
                let sibling_origin = todo!("update sibling reference frame origin transforms");
                frame_stack.push(sibling_frame);
            }

            // Pop all reference frames off the stack, adding children to the stack in the process.
            // This should result in all sibling subtrees of the current frame being processed
            // recursively.
            while let Some(this_frame) = frame_stack.pop() {
                for child_frame in reference_frames.children(this_frame) {
                    this_frame.propagate_origin_to_child(&mut reference_frames, child_frame);
                    frame_stack.push(child_frame)
                }
            }

            // Finally, now that the siblings of this frame have been recursively processed, we
            // process the parent and set it as the current reference frame. Note that every time we
            // step to a parent, "this frame" and all descendants have already been processed, so we
            // only need to process the siblings.
            match reference_frames.parent(this_frame) {
                Some(parent_frame) => {
                    let parent_origin = todo!("update parent origin_transform using this_frame");
                    this_frame = parent_frame;
                }
                None => return, // We have reached the root of the tree, and can exit.
            }
        }

        error!("Reached the maximum reference frame depth of {}, and exited early to prevent an infinite loop. This might be caused by a degenerate hierarchy.", Self::MAX_REFERENCE_FRAME_DEPTH)
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::SystemState;

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
    fn test_child_propagation() {
        let mut app = App::new();
        app.add_plugins(FloatingOriginPlugin::<i32>::default());

        let root = ReferenceFrameHandle::Root;
        app.insert_resource(RootReferenceFrame(ReferenceFrame {
            origin_transform: LocalFloatingOrigin {
                translation_grid: GridCell::<i32>::new(999, -1, -1),
                translation_float: Vec3::ZERO,
                rotation_float: DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2),
            },
            ..default()
        }));

        let child = app
            .world
            .spawn((
                Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                    .with_translation(Vec3::new(1.0, 1.0, 0.0)),
                GridCell::<i32>::new(1000, 0, 0),
                ReferenceFrame::<i32>::default(),
            ))
            .id();
        let child = ReferenceFrameHandle::Node(child);

        let mut state = SystemState::<ReferenceFrameParam<i32>>::new(&mut app.world);
        let mut reference_frames = state.get_mut(&mut app.world);

        // The function we are testing
        root.propagate_origin_to_child(&mut reference_frames, child);

        let (child_frame, ..) = reference_frames.get(child);

        let computed_grid = child_frame.origin_transform.translation_grid;
        let correct_grid = GridCell::new(-1, 1, -1);
        assert_eq!(computed_grid, correct_grid);

        let computed_rot = child_frame.origin_transform.rotation_float;
        let correct_rot = DQuat::from_rotation_z(std::f64::consts::PI);
        let rot_error = computed_rot.angle_between(correct_rot);
        assert!(rot_error < 0.001);

        let computed_trans = child_frame.origin_transform.translation_float;
        let correct_trans = Vec3::new(-1.0, 1.0, 0.0);
        let trans_error = computed_trans.distance(correct_trans);
        assert!(dbg!(trans_error) < 0.001);
    }
}
