//! Logic for propagating transforms through the hierarchy of reference frames.

use bevy_ecs::{batching::BatchingStrategy, prelude::*};
use bevy_hierarchy::prelude::*;
use bevy_transform::prelude::*;

use crate::{precision::GridPrecision, reference_frame::ReferenceFrame, BigSpace, GridCell};

use super::PropagationStats;

impl<P: GridPrecision> ReferenceFrame<P> {
    /// Update the `GlobalTransform` of entities with a [`GridCell`], using the [`ReferenceFrame`]
    /// the entity belongs to.
    pub fn propagate_high_precision(
        mut stats: ResMut<PropagationStats>,
        reference_frames: Query<&ReferenceFrame<P>>,
        mut entities: ParamSet<(
            Query<(
                Ref<GridCell<P>>,
                Ref<Transform>,
                Ref<Parent>,
                &mut GlobalTransform,
            )>,
            Query<(&ReferenceFrame<P>, &mut GlobalTransform), With<BigSpace>>,
        )>,
    ) {
        let start = bevy_utils::Instant::now();

        entities
            .p0()
            .par_iter_mut()
            .batching_strategy(BatchingStrategy::fixed(10_000)) // Better scaling than default
            .for_each(|(grid, transform, parent, mut global_transform)| {
                if let Ok(frame) = reference_frames.get(parent.get()) {
                    // Optimization: we don't need to recompute the transforms if the entity hasn't
                    // moved and the floating origin's local origin in that reference frame hasn't
                    // changed.
                    if frame.local_floating_origin().is_local_origin_unchanged()
                        && !transform.is_changed()
                        && !grid.is_changed()
                        && !parent.is_changed()
                    {
                        return;
                    }
                    *global_transform = frame.global_transform(&grid, &transform);
                }
            });

        // Root reference frames
        //
        // These are handled separately because the root reference frame doesn't have a
        // Transform or GridCell - it wouldn't make sense because it is the root, and these
        // components are relative to their parent. Due to floating origins, it *is* possible
        // for the root reference frame to have a GlobalTransform - this is what makes it
        // possible to place a low precision (Transform only) entity in a root transform - it is
        // relative to the origin of the root frame.
        entities
            .p1()
            .iter_mut()
            .for_each(|(frame, mut global_transform)| {
                if frame.local_floating_origin().is_local_origin_unchanged() {
                    return; // By definition, this means the frame has not moved
                }
                // The global transform of the root frame is the same as the transform of an entity
                // at the origin - it is determined entirely by the local origin position:
                *global_transform =
                    frame.global_transform(&GridCell::default(), &Transform::IDENTITY);
            });

        stats.high_precision_propagation = start.elapsed();
    }

    /// Update the [`GlobalTransform`] of entities with a [`Transform`], without a [`GridCell`], and
    /// that are children of an entity with a [`GlobalTransform`]. This will recursively propagate
    /// entities that only have low-precision [`Transform`]s, just like bevy's built in systems.
    pub fn propagate_low_precision(
        mut stats: ResMut<PropagationStats>,
        roots: Query<
            (Entity, &Children, Ref<GlobalTransform>),
            (
                // A root big space does not have a grid cell, and not all high precision entities
                // have a reference frame
                Or<(With<ReferenceFrame<P>>, With<GridCell<P>>)>,
            ),
        >,
        transform_query: Query<
            (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
            (
                With<Parent>,
                Without<GridCell<P>>,
                Without<ReferenceFrame<P>>,
            ),
        >,
        parent_query: Query<
            (Entity, Ref<Parent>),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<GridCell<P>>,
                Without<ReferenceFrame<P>>,
            ),
        >,
    ) {
        let start = bevy_utils::Instant::now();
        let update_transforms = |entity, children, global_transform: Ref<GlobalTransform>| {
            for (child, actual_parent) in parent_query.iter_many(children) {
                assert_eq!(
                    actual_parent.get(), entity,
                    "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
                );

                // High precision global transforms are change-detected, and are only updated if
                // that entity has moved relative to the floating origin's grid cell.
                let changed = global_transform.is_changed();

                // SAFETY:
                // - `child` must have consistent parentage, or the above assertion would panic.
                // Since `child` is parented to a root entity, the entire hierarchy leading to it is
                // consistent.
                // - We may operate as if all descendants are consistent, since
                //   `propagate_recursive` will panic before continuing to propagate if it
                //   encounters an entity with inconsistent parentage.
                // - Since each root entity is unique and the hierarchy is consistent and
                //   forest-like, other root entities' `propagate_recursive` calls will not conflict
                //   with this one.
                // - Since this is the only place where `transform_query` gets used, there will be
                //   no conflicting fetches elsewhere.
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

        roots
            .par_iter()
            .for_each(|(e, c, g)| update_transforms(e, c, g));

        stats.low_precision_propagation = start.elapsed();
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
        parent_query: &Query<
            (Entity, Ref<Parent>),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<GridCell<P>>,
                Without<ReferenceFrame<P>>,
            ),
        >,
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
            (global_transform, children)
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
                    global_matrix.as_ref(),
                    transform_query,
                    parent_query,
                    child,
                    changed || actual_parent.is_changed(),
                );
            }
        }
    }
}
