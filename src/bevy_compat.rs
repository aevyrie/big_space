//! Systems for [`Transform`] propagation compatibility with entities outside a
//! [`BigSpace`](crate::BigSpace), needed when bevy's built in transform propagation is disabled.

use alloc::vec::Vec;
use bevy_ecs::{change_detection::Ref, prelude::*};
use bevy_transform::prelude::*;

/// Copied from bevy. This is the simpler propagation implementation that doesn't use dirty tree
/// marking. This is needed because dirty tree marking doesn't start from the root, and will end up
/// doing the work for big space hierarchies, which it cannot affect anyway.
pub fn propagate_parent_transforms(
    mut root_query: Query<
        (Entity, &Children, Ref<Transform>, &mut GlobalTransform),
        Without<ChildOf>,
    >,
    mut orphaned: RemovedComponents<ChildOf>,
    transform_query: Query<
        (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
        With<ChildOf>,
    >,
    child_query: Query<(Entity, Ref<ChildOf>), With<GlobalTransform>>,
    mut orphaned_entities: Local<Vec<Entity>>,
) {
    orphaned_entities.clear();
    orphaned_entities.extend(orphaned.read());
    orphaned_entities.sort_unstable();
    root_query.par_iter_mut().for_each(
        |(entity, children, transform, mut global_transform)| {
            let changed = transform.is_changed() || global_transform.is_added() || orphaned_entities.binary_search(&entity).is_ok();
            if changed {
                *global_transform = GlobalTransform::from(*transform);
            }

            for (child, child_of) in child_query.iter_many(children) {
                assert_eq!(
                    child_of.parent(), entity,
                    "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
                );
                // SAFETY:
                // - `child` must have consistent parentage, or the above assertion would panic.
                //   Since `child` is parented to a root entity, the entire hierarchy leading to it
                //   is consistent.
                // - We may operate as if all descendants are consistent, since
                //   `propagate_recursive` will panic before continuing to propagate if it
                //   encounters an entity with inconsistent parentage.
                // - Since each root entity is unique and the hierarchy is consistent and
                //   forest-like, other root entities' `propagate_recursive` calls will not conflict
                //   with this one.
                // - Since this is the only place where `transform_query` gets used, there will be
                //   no conflicting fetches elsewhere.
                #[expect(unsafe_code, reason = "`propagate_recursive()` is unsafe due to its use of `Query::get_unchecked()`.")]
                unsafe {
                    propagate_recursive(
                        &global_transform,
                        &transform_query,
                        &child_query,
                        child,
                        changed || child_of.is_changed(),
                    );
                }
            }
        },
    );
}

/// Recursively propagates the transforms for `entity` and all of its descendants.
///
/// # Panics
///
/// If `entity`'s descendants have a malformed hierarchy, this function will panic occur before
/// propagating the transforms of any malformed entities and their descendants.
///
/// # Safety
///
/// - While this function is running, `transform_query` must not have any fetches for `entity`,
///   nor any of its descendants.
/// - The caller must ensure that the hierarchy leading to `entity` is well-formed and must
///   remain as a tree or a forest. Each entity must have at most one parent.
#[expect(
    unsafe_code,
    reason = "This function uses `Query::get_unchecked()`, which can result in multiple mutable references if the preconditions are not met."
)]
unsafe fn propagate_recursive(
    parent: &GlobalTransform,
    transform_query: &Query<
        (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
        With<ChildOf>,
    >,
    child_query: &Query<(Entity, Ref<ChildOf>), With<GlobalTransform>>,
    entity: Entity,
    mut changed: bool,
) {
    let (global_matrix, children) = {
        let Ok((transform, mut global_transform, children)) =
            // SAFETY: This call cannot create aliased mutable references.
            //   - The top level iteration parallelizes on the roots of the hierarchy.
            //   - The caller ensures that each child has one and only one unique parent throughout
            //     the entire hierarchy.
            //
            // For example, consider the following malformed hierarchy:
            //
            //     A
            //   /   \
            //  B     C
            //   \   /
            //     D
            //
            // D has two parents, B and C. If the propagation passes through C, but the ChildOf
            // component on D points to B, the above check will panic as the origin parent does
            // match the recorded parent.
            //
            // Also consider the following case, where A and B are roots:
            //
            //  A       B
            //   \     /
            //    C   D
            //     \ /
            //      E
            //
            // Even if these A and B start two separate tasks running in parallel, one of them will
            // panic before attempting to mutably access E.
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
    for (child, child_of) in child_query.iter_many(children) {
        assert_eq!(
            child_of.parent(), entity,
            "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
        );
        // SAFETY: The caller guarantees that `transform_query` will not be fetched for any
        // descendants of `entity`, so it is safe to call `propagate_recursive` for each child.
        //
        // The above assertion ensures that each child has one and only one unique parent
        // throughout the entire hierarchy.
        unsafe {
            propagate_recursive(
                global_matrix.as_ref(),
                transform_query,
                child_query,
                child,
                changed || child_of.is_changed(),
            );
        }
    }
}
