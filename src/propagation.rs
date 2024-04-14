//! Propagates transforms through the entity hierarchy.
//!
//! This is a modified version of Bevy's own transform propagation system.

use crate::{
    precision::GridPrecision,
    reference_frame::{ReferenceFrame, RootReferenceFrame},
    GridCell,
};
use bevy::prelude::*;

/// Entities with this component will ignore the floating origin, and will instead propagate
/// transforms normally.
#[derive(Component, Debug, Reflect)]
pub struct IgnoreFloatingOrigin;

/// Update [`GlobalTransform`] component of entities that aren't in the hierarchy.
pub fn sync_simple_transforms<P: GridPrecision>(
    root: Res<RootReferenceFrame<P>>,
    mut query: ParamSet<(
        Query<
            (&Transform, &mut GlobalTransform, Has<IgnoreFloatingOrigin>),
            (
                Or<(Changed<Transform>, Added<GlobalTransform>)>,
                Without<Parent>,
                Without<Children>,
                Without<GridCell<P>>,
            ),
        >,
        Query<
            (
                Ref<Transform>,
                &mut GlobalTransform,
                Has<IgnoreFloatingOrigin>,
            ),
            (Without<Parent>, Without<Children>, Without<GridCell<P>>),
        >,
    )>,
    mut orphaned: RemovedComponents<Parent>,
) {
    // Update changed entities.
    query.p0().par_iter_mut().for_each(
        |(transform, mut global_transform, ignore_floating_origin)| {
            if ignore_floating_origin {
                *global_transform = GlobalTransform::from(*transform);
            } else {
                *global_transform = root.global_transform(&GridCell::ZERO, transform);
            }
        },
    );
    // Update orphaned entities.
    let mut query = query.p1();
    let mut iter = query.iter_many_mut(orphaned.read());
    while let Some((transform, mut global_transform, ignore_floating_origin)) = iter.fetch_next() {
        if !transform.is_changed() && !global_transform.is_added() {
            if ignore_floating_origin {
                *global_transform = GlobalTransform::from(*transform);
            } else {
                *global_transform = root.global_transform(&GridCell::ZERO, &transform);
            }
        }
    }
}

/// Update the [`GlobalTransform`] of entities with a [`Transform`] that are children of a
/// [`ReferenceFrame`] and do not have a [`GridCell`] component, or that are children of
/// [`GridCell`]s.
pub fn propagate_transforms<P: GridPrecision>(
    frames: Query<&Children, With<ReferenceFrame<P>>>,
    frame_child_query: Query<(Entity, &Children, &GlobalTransform), With<GridCell<P>>>,
    root_frame_query: Query<
        (Entity, &Children, &GlobalTransform),
        (With<GridCell<P>>, Without<Parent>),
    >,
    root_frame: Res<RootReferenceFrame<P>>,
    mut root_frame_gridless_query: Query<
        (
            Entity,
            &Children,
            &Transform,
            &mut GlobalTransform,
            Has<IgnoreFloatingOrigin>,
        ),
        (Without<GridCell<P>>, Without<Parent>),
    >,
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
                propagate_recursive(&global_transform, &transform_query, &parent_query, child);
            }
        }
    };

    frames.par_iter().for_each(|children| {
        children
            .iter()
            .filter_map(|child| frame_child_query.get(*child).ok())
            .for_each(|(e, c, g)| update_transforms((e, c, *g)))
    });
    root_frame_query
        .par_iter()
        .for_each(|(e, c, g)| update_transforms((e, c, *g)));
    root_frame_gridless_query.par_iter_mut().for_each(
        |(entity, children, local, mut global, ignore_floating_origin)| {
            if ignore_floating_origin {
                *global = GlobalTransform::from(*local);
            } else {
                *global = root_frame.global_transform(&GridCell::ZERO, local);
            }
            update_transforms((entity, children, *global))
        },
    );
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
unsafe fn propagate_recursive<P: GridPrecision>(
    parent: &GlobalTransform,
    transform_query: &Query<
        (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
        (
            With<Parent>,
            Without<GridCell<P>>,
            Without<ReferenceFrame<P>>,
        ),
    >,
    parent_query: &Query<(Entity, Ref<Parent>)>,
    entity: Entity,
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

        *global_transform = parent.mul_transform(*transform);

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
            propagate_recursive(&global_matrix, transform_query, parent_query, child);
        }
    }
}
