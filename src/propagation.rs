//! Propagates transforms through the entity hierarchy.
//!
//! This is a slightly modified version of Bevy's own transform propagation system.

use crate::{precision::GridPrecision, FloatingOrigin, FloatingOriginSettings, GridCell};
use bevy::prelude::*;

/// Update [`GlobalTransform`] component of entities based on entity hierarchy and
/// [`Transform`] component.
pub fn propagate_transforms<P: GridPrecision>(
    origin_moved: Query<(), (Changed<GridCell<P>>, With<FloatingOrigin>)>,
    mut root_query: Query<
        (
            Entity,
            &Children,
            Ref<Transform>,
            &mut GlobalTransform,
            Option<Ref<GridCell<P>>>,
        ),
        Without<Parent>,
    >,
    transform_query: Query<
        (
            Ref<Transform>,
            &mut GlobalTransform,
            Option<&Children>,
            Option<Ref<GridCell<P>>>,
        ),
        With<Parent>,
    >,
    parent_query: Query<(Entity, Ref<Parent>)>,
    settings: Res<FloatingOriginSettings>,
) {
    let origin_cell_changed = !origin_moved.is_empty();

    for (entity, children, transform, mut global_transform, cell) in root_query.iter_mut() {
        let cell_changed = cell.as_ref().is_some_and(|cell| cell.is_changed());
        let transform_changed = transform.is_changed();

        if transform_changed && cell.is_none() {
            *global_transform = GlobalTransform::from(*transform);
        }

        let changed = transform_changed || cell_changed || origin_cell_changed;

        for (child, actual_parent) in parent_query.iter_many(children) {
            assert_eq!(
                actual_parent.get(), entity,
                "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
            );
            // SAFETY:
            // - `child` must have consistent parentage, or the above assertion would panic.
            // Since `child` is parented to a root entity, the entire hierarchy leading to it is consistent.
            // - We may operate as if all descendants are consistent, since `propagate_recursive` will panic before
            //   continuing to propagate if it encounters an entity with inconsistent parentage.
            // - Since each root entity is unique and the hierarchy is consistent and forest-like,
            //   other root entities' `propagate_recursive` calls will not conflict with this one.
            // - Since this is the only place where `transform_query` gets used, there will be no conflicting fetches elsewhere.
            unsafe {
                propagate_recursive(
                    &global_transform,
                    &transform_query,
                    &parent_query,
                    child,
                    changed || actual_parent.is_changed(),
                    &settings,
                );
            }
        }
    }
}

/// COPIED EXACTLY FROM BEVY (and adjusted for accumulating GridCells through children)
///
/// Recursively propagates the transforms for `entity` and all of its descendants.
///
/// # Panics
///
/// If `entity`'s descendants have a malformed hierarchy, this function will panic occur before propagating
/// the transforms of any malformed entities and their descendants.
///
/// # Safety
///
/// - While this function is running, `transform_query` must not have any fetches for `entity`,
/// nor any of its descendants.
/// - The caller must ensure that the hierarchy leading to `entity`
/// is well-formed and must remain as a tree or a forest. Each entity must have at most one parent.
unsafe fn propagate_recursive<P: GridPrecision>(
    parent: &GlobalTransform,
    transform_query: &Query<
        (
            Ref<Transform>,
            &mut GlobalTransform,
            Option<&Children>,
            Option<Ref<GridCell<P>>>,
        ),
        With<Parent>,
    >,
    parent_query: &Query<(Entity, Ref<Parent>)>,
    entity: Entity,
    mut changed: bool,
    settings: &FloatingOriginSettings,
) {
    let (global_matrix, children) = {
        let Ok((transform, mut global_transform, children, cell)) =
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

        let cell_changed = cell.as_ref().is_some_and(|cell| cell.is_changed());

        changed |= transform.is_changed() | cell_changed;
        if changed {
            if let Some(cell) = &cell {
                let offset = settings.grid_position(cell, &transform);
                *global_transform = parent.mul_transform(transform.with_translation(offset));
            } else {
                *global_transform = parent.mul_transform(*transform);
            }
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
            propagate_recursive(
                &global_matrix,
                transform_query,
                parent_query,
                child,
                changed || actual_parent.is_changed(),
                settings,
            );
        }
    }
}
