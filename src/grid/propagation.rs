//! Logic for propagating transforms through the hierarchy of grids.

use crate::{prelude::*, stationary::GridDirtyTick};
use alloc::vec::Vec;
use bevy_ecs::{prelude::*, system::SystemChangeTick};
use bevy_reflect::Reflect;
use bevy_transform::prelude::*;

/// Marks entities in the big space hierarchy that are themselves roots of a low-precision subtree.
/// While finding these entities is slow, we only have to do it during hierarchy or archetype
/// changes. Once the entity is marked (updating its archetype), querying it is now very fast.
///
/// - This entity's parent must be a high precision entity (with a [`CellCoord`]).
/// - This entity must not have a [`CellCoord`].
/// - This entity may or may not have children.
#[derive(Component, Default, Reflect)]
pub struct LowPrecisionRoot;

impl Grid {
    /// Update the `GlobalTransform` of entities with a [`CellCoord`], using the [`Grid`] the entity
    /// belongs to.
    ///
    /// Uses a grid-tree walk to propagate transforms. If [`GridDirtyTick`] is present on a grid
    /// (inserted by [`crate::stationary::BigSpaceStationaryPlugin`]), entire subtrees are pruned
    /// when the grid's local floating origin is unchanged and no non-[`Stationary`] entity in the
    /// subtree changed this frame.
    pub fn propagate_high_precision(
        system_ticks: SystemChangeTick,
        mut stats: Option<ResMut<crate::timing::PropagationStats>>,
        mut params: ParamSet<(
            // p0: all CellCoord entities — includes sub-grids (Grid + CellCoord)
            Query<
                (
                    Ref<CellCoord>,
                    Ref<Transform>,
                    Ref<ChildOf>,
                    &mut GlobalTransform,
                    Option<&Stationary>,
                    Option<&StationaryComputed>,
                    Option<&Grid>,
                    Option<&GridDirtyTick>,
                    Option<&Children>,
                ),
                With<CellCoord>,
            >,
            // p1: root BigSpace grids — no CellCoord, disjoint from p0
            Query<
                (
                    &Grid,
                    Option<&GridDirtyTick>,
                    Option<&Children>,
                    &mut GlobalTransform,
                ),
                With<BigSpace>,
            >,
        )>,
    ) {
        let start = bevy_platform::time::Instant::now();

        // Phase 1: Process root BigSpace grids.
        // Collect traversal tasks (cloned grid data + child entity lists) so we can release p1
        // before accessing p0.
        let mut root_tasks: Vec<(Grid, Vec<Entity>)> = Vec::new();

        for (grid, dirty_tick, children, mut gt) in params.p1().iter_mut() {
            // Update root GT when the floating origin's position in this grid changed.
            // The root grid's GT is the same as an entity at the grid origin.
            if !grid.local_floating_origin().is_local_origin_unchanged() {
                *gt = grid.global_transform(&CellCoord::default(), &Transform::IDENTITY);
            }

            // Check subtree skip condition.
            // Absent GridDirtyTick (plugin not added) → always process.
            let subtree_clean = dirty_tick.is_some_and(|d| !d.is_dirty(system_ticks));
            if grid.local_floating_origin().is_local_origin_unchanged() && subtree_clean {
                continue;
            }

            let child_entities: Vec<Entity> =
                children.map(|c| c.iter().collect()).unwrap_or_default();
            root_tasks.push((grid.clone(), child_entities));
        }
        // p1 borrow ends here; safe to access p0.

        // Phase 2: Tree-walk child entities using p0 with get_unchecked.
        // Safety contract for all traverse_grid calls below:
        //   - The big-space hierarchy is a forest: each entity has at most one parent.
        //   - Root subtrees are independent; we process them sequentially.
        //   - Within each root, traverse_grid visits each entity at most once.
        //   - All Mut<GlobalTransform> borrows from one loop iteration are released before
        //     recursing into child grids, so no aliasing occurs.
        let cells_query = params.p0();
        for (grid, children) in &root_tasks {
            #[expect(
                unsafe_code,
                reason = "Tree walk requires get_unchecked to avoid per-entity HashMap lookups; \
                          safety guaranteed by the forest structure of the hierarchy."
            )]
            // SAFETY: See contract above.
            unsafe {
                Self::traverse_grid(grid, children, &cells_query, system_ticks);
            }
        }

        if let Some(stats) = stats.as_mut() {
            stats.high_precision_propagation += start.elapsed();
        }
    }

    /// Recursively traverse a single grid's children, updating their [`GlobalTransform`]s and
    /// collecting sub-grids for further recursion.
    ///
    /// # Safety
    ///
    /// - The caller must guarantee that the hierarchy rooted at this grid forms a tree (forest):
    ///   each entity appears as a child of at most one grid entity.
    /// - No two concurrent calls to this function (or [`Self::propagate_high_precision`]) may
    ///   access the same entity through `cells_query`.
    /// - All [`Mut`] borrows from a prior call to `cells_query.get_unchecked` must have been
    ///   dropped before this function is called again for a different entity.
    #[expect(
        unsafe_code,
        reason = "Uses Query::get_unchecked to avoid per-entity HashMap lookups while walking \
                  the grid tree; safety enforced by the tree/forest structure of the hierarchy."
    )]
    unsafe fn traverse_grid(
        grid: &Grid,
        children: &[Entity],
        cells_query: &Query<
            (
                Ref<CellCoord>,
                Ref<Transform>,
                Ref<ChildOf>,
                &mut GlobalTransform,
                Option<&Stationary>,
                Option<&StationaryComputed>,
                Option<&Grid>,
                Option<&GridDirtyTick>,
                Option<&Children>,
            ),
            With<CellCoord>,
        >,
        system_ticks: SystemChangeTick,
    ) {
        // Collect sub-grids to recurse into after processing all direct children.
        // We finish the children loop (dropping all Mut<> borrows) before recursing.
        let mut child_grids: Vec<(Grid, Vec<Entity>)> = Vec::new();

        for &child in children {
            // SAFETY: The tree structure guarantees each entity appears as a child of at most one
            // grid, so this entity is visited exactly once across the entire traversal. All
            // previously returned Mut<> borrows from prior iterations are dropped at loop body end.
            let Ok((
                cell,
                transform,
                parent_rel,
                mut gt,
                stationary,
                computed,
                child_grid,
                dirty_tick,
                grandchildren,
            )) = (unsafe { cells_query.get_unchecked(child) })
            else {
                continue;
            };

            let is_stationary = stationary.is_some();
            let is_computed = computed.is_some();

            // Same update condition as the original flat par_iter_mut.
            if !grid.local_floating_origin().is_local_origin_unchanged()
                || (transform.is_changed() && !is_stationary)
                || cell.is_changed()
                || parent_rel.is_changed()
                || (is_stationary && !is_computed)
            {
                *gt = grid.global_transform(&cell, &transform);
            }

            // If this child is also a sub-grid, check whether its subtree needs processing.
            if let Some(sub_grid) = child_grid {
                let subtree_clean = dirty_tick.is_some_and(|d| !d.is_dirty(system_ticks));
                if sub_grid.local_floating_origin().is_local_origin_unchanged() && subtree_clean {
                    // The sub-grid's origin is unchanged and no entity in its subtree changed;
                    // skip this entire subtree.
                    continue;
                }

                let gc: Vec<Entity> = grandchildren
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                // Clone the sub-grid data so all Mut<> borrows from get_unchecked are released
                // before we recurse.
                child_grids.push((sub_grid.clone(), gc));
            }
            // All Mut<> borrows (gt, cell, transform, parent_rel) are dropped here.
        }

        // All Mut<> borrows from the children loop are now released; safe to recurse.
        for (sub_grid, gc) in &child_grids {
            // SAFETY: sub-grids are distinct children; their subtrees are disjoint by the tree
            // property. No Mut<> borrows from the outer loop remain.
            unsafe {
                Self::traverse_grid(sub_grid, gc, cells_query, system_ticks);
            }
        }
    }

    /// Marks entities with [`LowPrecisionRoot`]. Handles adding and removing the component.
    pub fn tag_low_precision_roots(
        mut stats: Option<ResMut<crate::timing::PropagationStats>>,
        mut commands: Commands,
        valid_parent: Query<(), (With<CellCoord>, With<GlobalTransform>, With<Children>)>,
        unmarked: Query<
            (Entity, &ChildOf),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<CellCoord>,
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
                    With<CellCoord>,
                    Without<ChildOf>,
                )>,
            ),
        >,
        has_possibly_invalid_parent: Query<(Entity, &ChildOf), With<LowPrecisionRoot>>,
    ) {
        let start = bevy_platform::time::Instant::now();
        for (entity, parent) in unmarked.iter() {
            if valid_parent.contains(parent.parent()) {
                commands.entity(entity).insert(LowPrecisionRoot);
            }
        }

        for entity in invalidated.iter() {
            commands.entity(entity).remove::<LowPrecisionRoot>();
        }

        for (entity, parent) in has_possibly_invalid_parent.iter() {
            if !valid_parent.contains(parent.parent()) {
                commands.entity(entity).remove::<LowPrecisionRoot>();
            }
        }
        if let Some(stats) = stats.as_mut() {
            stats.low_precision_root_tagging += start.elapsed();
        }
    }

    /// Update the [`GlobalTransform`] of entities with a [`Transform`], without a [`CellCoord`], and
    /// that are children of an entity with a [`GlobalTransform`]. This will recursively propagate
    /// entities that only have low-precision [`Transform`]s, just like bevy's built in systems.
    pub fn propagate_low_precision(
        mut stats: Option<ResMut<crate::timing::PropagationStats>>,
        root_parents: Query<
            Ref<GlobalTransform>,
            (
                // A root big space does not have a grid cell, and not all high precision entities
                // have a grid
                Or<(With<Grid>, With<CellCoord>)>,
            ),
        >,
        roots: Query<(Entity, &ChildOf), With<LowPrecisionRoot>>,
        transform_query: Query<
            (Ref<Transform>, &mut GlobalTransform, Option<&Children>),
            (
                With<ChildOf>,
                Without<CellCoord>, // Used to prove access to GlobalTransform is disjoint
                Without<Grid>,
            ),
        >,
        parent_query: Query<
            (Entity, Ref<ChildOf>),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<CellCoord>,
                Without<Grid>,
            ),
        >,
    ) {
        let start = bevy_platform::time::Instant::now();
        let update_transforms = |low_precision_root, parent_transform: Ref<GlobalTransform>| {
            // High precision global transforms are change-detected and are only updated if that
            // entity has moved relative to the floating origin's grid cell.
            let changed = parent_transform.is_changed();

            #[expect(
                unsafe_code,
                reason = "`propagate_recursive()` is unsafe due to its use of `Query::get_unchecked()`."
            )]
            // SAFETY:
            // - Unlike the bevy version of this, we do not iterate over all children of the root
            //   and manually verify each child has a parent component that points back to the same
            //   entity. Instead, we query the roots directly, so we know they are unique.
            // - We may operate as if all descendants are consistent, since `propagate_recursive`
            //   will panic before continuing to propagate if it encounters an entity with
            //   inconsistent parentage.
            // - Since each root entity is unique and the hierarchy is consistent and forest-like,
            //   other root entities' `propagate_recursive` calls will not conflict with this one.
            // - Since this is the only place where `transform_query` gets used, there will be no
            //   conflicting fetches elsewhere.
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
            if let Ok(parent_transform) = root_parents.get(parent.parent()) {
                update_transforms(low_precision_root, parent_transform);
            }
        });

        if let Some(stats) = stats.as_mut() {
            stats.low_precision_propagation += start.elapsed();
        }
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
                Without<CellCoord>, // ***ADDED*** Only recurse low-precision entities
                Without<Grid>,      // ***ADDED*** Only recurse low-precision entities
            ),
        >,
        parent_query: &Query<
            (Entity, Ref<ChildOf>),
            (
                With<Transform>,
                With<GlobalTransform>,
                Without<CellCoord>,
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
    use crate::plugin::BigSpaceMinimalPlugins;
    use crate::prelude::*;
    use bevy::prelude::*;

    #[test]
    fn low_precision_in_big_space() {
        #[derive(Component)]
        struct Test;

        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn_big_space_default(|root| {
                    root.spawn_spatial(FloatingOrigin);
                    root.spawn_spatial((
                        Transform::from_xyz(3.0, 3.0, 3.0),
                        CellCoord::new(1, 1, 1), // Default cell size is 2000
                    ))
                    .with_children(|spatial| {
                        spatial.spawn((
                            Transform::from_xyz(1.0, 2.0, 3.0),
                            Visibility::default(),
                            Test,
                        ));
                    });
                });
            });

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
