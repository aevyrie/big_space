//! Logic for propagating transforms through the hierarchy of reference frames.

use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_transform::prelude::*;

use crate::{precision::GridPrecision, reference_frame::ReferenceFrame, GridCell};

impl<P: GridPrecision> ReferenceFrame<P> {
    /// Update the `GlobalTransform` of entities with a [`GridCell`], using the [`ReferenceFrame`]
    /// the entity belongs to.
    pub fn propagate_high_precision(
        reference_frames: Query<&ReferenceFrame<P>>,
        mut entities: Query<(&GridCell<P>, &Transform, &Parent, &mut GlobalTransform)>,
    ) {
        // Update the GlobalTransform of GridCell entities that are children of a ReferenceFrame
        entities
            .par_iter_mut()
            .for_each(|(grid, transform, parent, mut global_transform)| {
                if let Ok(frame) = reference_frames.get(parent.get()) {
                    *global_transform = frame.global_transform(grid, transform);
                }
            });
    }

    /// Update the [`GlobalTransform`] of entities with a [`Transform`] that are children of a
    /// [`ReferenceFrame`] and do not have a [`GridCell`] component, or that are children of
    /// [`GridCell`]s. This will recursively propagate entities that only have low-precision
    /// [`Transform`]s, just like bevy's built in systems.
    pub fn propagate_low_precision(
        frames: Query<&Children, With<ReferenceFrame<P>>>,
        frame_child_query: Query<(Entity, &Children, &GlobalTransform), With<GridCell<P>>>,
        transform_query: Query<
            (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
            (
                With<Parent>,
                Without<GridCell<P>>,
                Without<ReferenceFrame<P>>,
            ),
        >,
        parent_query: Query<(Entity, Ref<Parent>)>,
    ) {
        let update_transforms = |(entity, children, global_transform)| {
            for (child, actual_parent) in parent_query.iter_many(children) {
                assert_eq!(
                actual_parent.get(), entity,
                "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
            );

                // Unlike bevy's transform propagation, change detection is much more complex, because
                // it is relative to the floating origin, *and* whether entities are moving.
                // - If the floating origin changes grid cells, everything needs to update
                // - If the floating origin's reference frame moves (translation, rotation), every
                //   entity outside of the reference frame subtree that the floating origin is in must
                //   update.
                // - All entities or reference frame subtrees that move within the same frame as the
                //   floating origin must be updated.
                //
                // Instead of adding this complexity and computation, is it much simpler to update
                // everything every frame.
                let changed = true;

                // SAFETY:
                // - `child` must have consistent parentage, or the above assertion would panic. Since
                // `child` is parented to a root entity, the entire hierarchy leading to it is
                // consistent.
                // - We may operate as if all descendants are consistent, since `propagate_recursive`
                //   will panic before continuing to propagate if it encounters an entity with
                //   inconsistent parentage.
                // - Since each root entity is unique and the hierarchy is consistent and forest-like,
                //   other root entities' `propagate_recursive` calls will not conflict with this one.
                // - Since this is the only place where `transform_query` gets used, there will be no
                //   conflicting fetches elsewhere.
                unsafe {
                    Self::propagate_recursive(
                        &global_transform,
                        &transform_query,
                        &parent_query,
                        child,
                        changed,
                    );
                }
            }
        };

        frames.par_iter().for_each(|children| {
            children
                .iter()
                .filter_map(|child| frame_child_query.get(*child).ok())
                .for_each(|(e, c, g)| update_transforms((e, c, *g)))
        });
    }

    /// COPIED FROM BEVY
    ///
    /// Recursively propagates the transforms for `entity` and all of its descendants.
    ///
    /// # Panics
    ///
    /// If `entity`'s descendants have a malformed hierarchy, this function will panic occur before
    /// propagating the transforms of any malformed entities and their descendants.
    ///
    /// # Safety
    ///
    /// - While this function is running, `transform_query` must not have any fetches for `entity`, nor
    /// any of its descendants.
    /// - The caller must ensure that the hierarchy leading to `entity` is well-formed and must remain
    /// as a tree or a forest. Each entity must have at most one parent.
    unsafe fn propagate_recursive(
        parent: &GlobalTransform,
        transform_query: &Query<
            (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
            (
                With<Parent>,
                Without<GridCell<P>>, // ***ADDED*** Only recurse low-precision entities
                Without<ReferenceFrame<P>>, // ***ADDED*** Only recurse low-precision entities
            ),
        >,
        parent_query: &Query<(Entity, Ref<Parent>)>,
        entity: Entity,
        mut changed: bool,
    ) {
        let (global_matrix, children) = {
            let Ok((transform, mut global_transform, children)) =
            // SAFETY: This call cannot create aliased mutable references.
            //   - The top level iteration parallelizes on the roots of the hierarchy.
            //   - The caller ensures that each child has one and only one unique parent throughout the entire
            //     hierarchy.
            //
            // For example, consider the following malformed hierarchy:
            //
            //     A
            //   /   \
            //  B     C
            //   \   /
            //     D
            //
            // D has two parents, B and C. If the propagation passes through C, but the Parent component on D points to B,
            // the above check will panic as the origin parent does match the recorded parent.
            //
            // Also consider the following case, where A and B are roots:
            //
            //  A       B
            //   \     /
            //    C   D
            //     \ /
            //      E
            //
            // Even if these A and B start two separate tasks running in parallel, one of them will panic before attempting
            // to mutably access E.
            (unsafe { transform_query.get_unchecked(entity) }) else {
                return;
            };

            changed |= transform.is_changed() || global_transform.is_added();
            if changed {
                *global_transform = parent.mul_transform(*transform);
            }
            (*global_transform, children)
        };

        let Some(children) = children else { return };
        for (child, actual_parent) in parent_query.iter_many(children) {
            assert_eq!(
            actual_parent.get(), entity,
            "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
        );
            // SAFETY: The caller guarantees that `transform_query` will not be fetched
            // for any descendants of `entity`, so it is safe to call `propagate_recursive` for each child.
            //
            // The above assertion ensures that each child has one and only one unique parent throughout the
            // entire hierarchy.
            unsafe {
                Self::propagate_recursive(
                    &global_matrix,
                    transform_query,
                    parent_query,
                    child,
                    changed || actual_parent.is_changed(),
                );
            }
        }
    }
}
