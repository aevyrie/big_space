//! Logic for propagating transforms through the hierarchy of grids.

use crate::prelude::*;
use bevy_ecs::{prelude::*, relationship::Relationship};
use bevy_reflect::Reflect;
use bevy_transform::prelude::*;

/// Marks entities in the big space hierarchy that are themselves roots of a low-precision subtree.
/// While finding these entities is slow, we only have to do it during hierarchy or archetype
/// changes. Once the entity is marked (updating its archetype), querying it is now very fast.
///
/// - This entity's parent must be a high precision entity (with a [`GridCell`]).
/// - This entity must not have a [`GridCell`].
/// - This entity may or may not have children.
#[derive(Component, Default, Reflect)]
pub struct LowPrecisionRoot;

impl Grid {
    /// Update the `GlobalTransform` of entities with a [`GridCell`], using the [`Grid`] the entity
    /// belongs to.
    pub fn propagate_high_precision(
        mut stats: ResMut<crate::timing::PropagationStats>,
        grids: Query<&Grid>,
        mut entities: ParamSet<(
            Query<(
                Ref<GridCell>,
                Ref<Transform>,
                Ref<ChildOf>,
                &mut GlobalTransform,
            )>,
            Query<(&Grid, &mut GlobalTransform), With<BigSpace>>,
        )>,
    ) {
        let start = bevy_platform_support::time::Instant::now();

        // Performance note: I've also tried to iterate over each grid's children at once, to avoid
        // the grid and parent lookup, but that made things worse because it prevented dumb
        // parallelism. The only thing I can see to make this faster is archetype change detection.
        // Change filters are not archetype filters, so they scale with the total number of entities
        // that match the query, regardless of change.
        entities
            .p0()
            .par_iter_mut()
            .for_each(|(cell, transform, parent, mut global_transform)| {
                if let Ok(grid) = grids.get(parent.get()) {
                    // Optimization: we don't need to recompute the transforms if the entity hasn't
                    // moved and the floating origin's local origin in that grid hasn't changed.
                    //
                    // This also ensures we don't trigger change detection on GlobalTransforms when
                    // they haven't changed.
                    //
                    // This check can have a big impact on reducing computations for entities in the
                    // same grid as the floating origin, i.e. the main camera. It also means that as
                    // the floating origin moves between cells, that could suddenly cause a spike in
                    // the amount of computation needed that grid. In the future, we might be able
                    // to spread that work across grids, entities far away can maybe be delayed for
                    // a grid or two without being noticeable.
                    if !grid.local_floating_origin().is_local_origin_unchanged()
                        || transform.is_changed()
                        || cell.is_changed()
                        || parent.is_changed()
                    {
                        *global_transform = grid.global_transform(&cell, &transform);
                    }
                }
            });

        // Root grids
        //
        // These are handled separately because the root grid doesn't have a Transform or GridCell -
        // it wouldn't make sense because it is the root, and these components are relative to their
        // parent. Due to floating origins, it *is* possible for the root grid to have a
        // GlobalTransform - this is what makes it possible to place a low precision (Transform
        // only) entity in a root transform - it is relative to the origin of the root grid.
        entities
            .p1()
            .iter_mut()
            .for_each(|(grid, mut global_transform)| {
                if grid.local_floating_origin().is_local_origin_unchanged() {
                    return; // By definition, this means the grid has not moved
                }
                // The global transform of the root grid is the same as the transform of an entity
                // at the origin - it is determined entirely by the local origin position:
                *global_transform =
                    grid.global_transform(&GridCell::default(), &Transform::IDENTITY);
            });

        stats.high_precision_propagation += start.elapsed();
    }

    /// Marks entities with [`LowPrecisionRoot`]. Handles adding and removing the component.
    pub fn tag_low_precision_roots(
        mut stats: ResMut<crate::timing::PropagationStats>,
        mut commands: Commands,
        valid_parent: Query<(), (With<GridCell>, With<GlobalTransform>, With<Children>)>,
        unmarked: Query<
            (Entity, &ChildOf),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<GridCell>,
                Without<LowPrecisionRoot>,
                Or<(Changed<ChildOf>, Added<Transform>)>,
            ),
        >,
        invalidated: Query<
            Entity,
            (
                With<LowPrecisionRoot>,
                Or<(
                    Without<Transform>,
                    Without<GlobalTransform>,
                    With<GridCell>,
                    Without<ChildOf>,
                )>,
            ),
        >,
        has_possibly_invalid_parent: Query<(Entity, &ChildOf), With<LowPrecisionRoot>>,
    ) {
        let start = bevy_platform_support::time::Instant::now();
        for (entity, parent) in unmarked.iter() {
            if valid_parent.contains(parent.get()) {
                commands.entity(entity).insert(LowPrecisionRoot);
            }
        }

        for entity in invalidated.iter() {
            commands.entity(entity).remove::<LowPrecisionRoot>();
        }

        for (entity, parent) in has_possibly_invalid_parent.iter() {
            if !valid_parent.contains(parent.get()) {
                commands.entity(entity).remove::<LowPrecisionRoot>();
            }
        }
        stats.low_precision_root_tagging += start.elapsed();
    }

    /// Update the [`GlobalTransform`] of entities with a [`Transform`], without a [`GridCell`], and
    /// that are children of an entity with a [`GlobalTransform`]. This will recursively propagate
    /// entities that only have low-precision [`Transform`]s, just like bevy's built in systems.
    pub fn propagate_low_precision(
        mut stats: ResMut<crate::timing::PropagationStats>,
        root_parents: Query<
            Ref<GlobalTransform>,
            (
                // A root big space does not have a grid cell, and not all high precision entities
                // have a grid
                Or<(With<Grid>, With<GridCell>)>,
            ),
        >,
        roots: Query<(Entity, &ChildOf), With<LowPrecisionRoot>>,
        transform_query: Query<
            (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
            (
                With<ChildOf>,
                Without<GridCell>, // Used to prove access to GlobalTransform is disjoint
                Without<Grid>,
            ),
        >,
        parent_query: Query<
            (Entity, Ref<ChildOf>),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<GridCell>,
                Without<Grid>,
            ),
        >,
    ) {
        let start = bevy_platform_support::time::Instant::now();
        let update_transforms = |low_precision_root, parent_transform: Ref<GlobalTransform>| {
            // High precision global transforms are change-detected, and are only updated if that
            // entity has moved relative to the floating origin's grid cell.
            let changed = parent_transform.is_changed();

            // SAFETY:
            // - Unlike the bevy version of this, we do not iterate over all children of the root,
            //   and manually verify each child has a parent component that points back to the same
            //   entity. Instead, we query the roots directly, so we know they are unique.
            // - We may operate as if all descendants are consistent, since `propagate_recursive`
            //   will panic before continuing to propagate if it encounters an entity with
            //   inconsistent parentage.
            // - Since each root entity is unique and the hierarchy is consistent and forest-like,
            //   other root entities' `propagate_recursive` calls will not conflict with this one.
            // - Since this is the only place where `transform_query` gets used, there will be no
            //   conflicting fetches elsewhere.
            #[expect(
                unsafe_code,
                reason = "`propagate_recursive()` is unsafe due to its use of `Query::get_unchecked()`."
            )]
            unsafe {
                Self::propagate_recursive(
                    &parent_transform,
                    &transform_query,
                    &parent_query,
                    low_precision_root,
                    changed,
                );
            }
        };

        roots.par_iter().for_each(|(low_precision_root, parent)| {
            if let Ok(parent_transform) = root_parents.get(parent.get()) {
                update_transforms(low_precision_root, parent_transform);
            }
        });

        stats.low_precision_propagation += start.elapsed();
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
            (
                With<ChildOf>,
                Without<GridCell>, // ***ADDED*** Only recurse low-precision entities
                Without<Grid>,     // ***ADDED*** Only recurse low-precision entities
            ),
        >,
        parent_query: &Query<
            (Entity, Ref<ChildOf>),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<GridCell>,
                Without<Grid>,
            ),
        >,
        entity: Entity,
        mut changed: bool,
    ) {
        let (global_matrix, children) = {
            let Ok((transform, mut global_transform, children)) =
                // SAFETY: This call cannot create aliased mutable references.
                //   - The top level iteration parallelizes on the roots of the hierarchy.
                //   - The caller ensures that each child has one and only one unique parent
                //     throughout the entire hierarchy.
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
                // Even if these A and B start two separate tasks running in parallel, one of them
                // will panic before attempting to mutably access E.
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
        for (child, child_of) in parent_query.iter_many(children) {
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
                Self::propagate_recursive(
                    global_matrix.as_ref(),
                    transform_query,
                    parent_query,
                    child,
                    changed || child_of.is_changed(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use bevy::prelude::*;

    #[test]
    fn low_precision_in_big_space() {
        #[derive(Component)]
        struct Test;

        let mut app = App::new();
        app.add_plugins(BigSpacePlugin::default()).add_systems(
            Startup,
            |mut commands: Commands| {
                commands.spawn_big_space_default(|root| {
                    root.spawn_spatial(FloatingOrigin);
                    root.spawn_spatial((
                        Transform::from_xyz(3.0, 3.0, 3.0),
                        GridCell::new(1, 1, 1), // Default cell size is 2000
                    ))
                    .with_children(|spatial| {
                        spatial.spawn((
                            Transform::from_xyz(1.0, 2.0, 3.0),
                            Visibility::default(),
                            Test,
                        ));
                    });
                });
            },
        );

        app.update();

        let mut q = app
            .world_mut()
            .query_filtered::<&GlobalTransform, With<Test>>();
        let actual_transform = *q.single(app.world()).unwrap();
        assert_eq!(
            actual_transform,
            GlobalTransform::from_xyz(2004.0, 2005.0, 2006.0)
        );
    }
}
