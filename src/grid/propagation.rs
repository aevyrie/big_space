//! Logic for propagating transforms through the hierarchy of grids.

use crate::{prelude::*, stationary::GridDirtyTick};
use alloc::vec::Vec;
use bevy_ecs::{prelude::*, system::SystemChangeTick};
use bevy_reflect::Reflect;
use bevy_tasks::ComputeTaskPool;
use bevy_transform::prelude::*;

/// Wraps a type-erased pointer to the cells [`Query`] for sending across
/// [`ComputeTaskPool`] scope tasks.
///
/// Stores the address as `usize` (which is always [`Send`]) to prevent Rust's partial-capture
/// optimization from capturing the inner `*const T` field (which is [`Send`]) individually
/// in `async move` blocks — that would incorrectly make the block non-[`Send`].
///
/// # Safety
///
/// Callers must ensure that concurrent tasks access only disjoint entity sets, preventing
/// aliased mutable references from [`Query::get_unchecked`]. This is upheld by the tree/forest
/// structure of the big space hierarchy: each entity has exactly one parent grid, so sibling
/// subtrees never share entities.
#[derive(Clone, Copy)]
struct SendCellsQuery(usize);

// SAFETY: See struct documentation above — safety is delegated to callers who must guarantee
// disjoint entity access across concurrent tasks.
#[expect(
    unsafe_code,
    reason = "SendCellsQuery wraps a raw query pointer for cross-task sharing; \
              safety is upheld by the big-space forest structure."
)]
unsafe impl Send for SendCellsQuery {}

impl SendCellsQuery {
    /// # Safety
    ///
    /// The pointer must originate from a valid `&Q`, and the caller must ensure no other
    /// mutable access to the same entities occurs concurrently.
    #[expect(
        unsafe_code,
        reason = "Reconstructs a typed reference from the stored address; \
                  safety guaranteed by the forest structure."
    )]
    unsafe fn as_ref<Q>(&self) -> &Q {
        // SAFETY: See method documentation.
        unsafe { &*(self.0 as *const Q) }
    }
}

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
            // p0: sub-grid entities only (Grid + CellCoord). Leaf entities (CellCoord, no Grid)
            // are handled by the separate `propagate_leaf_entities` system.
            Query<
                (
                    Ref<CellCoord>,
                    Ref<Transform>,
                    Ref<ChildOf>,
                    &mut GlobalTransform,
                    Option<&Stationary>,
                    Option<&StationaryComputed>,
                    &Grid,
                    Option<&GridDirtyTick>,
                    Option<&Children>,
                ),
                (With<CellCoord>, With<Grid>),
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
        mut root_tasks: Local<Vec<(Grid, Vec<Entity>)>>,
    ) {
        let start = bevy_platform::time::Instant::now();

        // Phase 1: Process root BigSpace grids.
        // Collect traversal tasks (cloned grid data + child entity lists) so we can release p1
        // before accessing p0. Uses Local to reuse allocations across frames.
        root_tasks.clear();

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
        //   - Root subtrees are independent; we process them in parallel when > 1 exist.
        //   - Within each root, traverse_grid visits each entity at most once.
        //   - All Mut<GlobalTransform> borrows from one loop iteration are released before
        //     recursing into child grids, so no aliasing occurs.
        let cells_query = params.p0();
        #[expect(
            unsafe_code,
            reason = "Tree walk requires get_unchecked to avoid per-entity HashMap lookups; \
                      safety guaranteed by the forest structure of the hierarchy."
        )]
        if root_tasks.len() <= 1 {
            for (grid, children) in &root_tasks {
                // SAFETY: See contract above.
                unsafe { Self::traverse_grid(grid, children, &cells_query, system_ticks) }
            }
        } else {
            // Multiple independent BigSpace roots: process in parallel.
            // SAFETY: Root subtrees are disjoint by the forest structure; each task accesses
            // a different entity set. SendCellsQuery safely transmits the raw pointer.
            let sendable = SendCellsQuery(&cells_query as *const _ as usize);
            ComputeTaskPool::get().scope(|scope| {
                for (grid, children) in &root_tasks {
                    let q = sendable;
                    scope.spawn(async move {
                        // SAFETY: See above. `as_ref` takes self by value, forcing the compiler
                        // to capture the whole `SendCellsQuery` (usize, Send) rather than its
                        // individual field (which would otherwise be a non-Send raw pointer).
                        unsafe { Self::traverse_grid(grid, children, q.as_ref(), system_ticks) }
                    });
                }
            });
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
                &Grid,
                Option<&GridDirtyTick>,
                Option<&Children>,
            ),
            (With<CellCoord>, With<Grid>),
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
            //
            // Leaf entities (CellCoord, no Grid) do not match the query and return Err here;
            // they are handled by the parallel `propagate_leaf_entities` system instead.
            let Ok((
                cell,
                transform,
                parent_rel,
                mut gt,
                stationary,
                computed,
                sub_grid,
                dirty_tick,
                grandchildren,
            )) = (unsafe { cells_query.get_unchecked(child) })
            else {
                continue;
            };

            let is_stationary = stationary.is_some();
            let is_computed = computed.is_some();

            // Update GT for this sub-grid entity.
            if !grid.local_floating_origin().is_local_origin_unchanged()
                || (transform.is_changed() && !is_stationary)
                || cell.is_changed()
                || parent_rel.is_changed()
                || (is_stationary && !is_computed)
            {
                *gt = grid.global_transform(&cell, &transform);
            }

            // Check whether this sub-grid's subtree needs processing.
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
            // All Mut<> borrows (gt, cell, transform, parent_rel) are dropped here.
        }

        // All Mut<> borrows from the children loop are now released; safe to recurse.
        if child_grids.len() <= 1 {
            for (sub_grid, gc) in &child_grids {
                // SAFETY: sub-grids are distinct children; their subtrees are disjoint by the
                // tree property. No Mut<> borrows from the outer loop remain.
                unsafe { Self::traverse_grid(sub_grid, gc, cells_query, system_ticks) }
            }
        } else {
            // Multiple sibling sub-grids: process in parallel.
            // SAFETY: Sibling subtrees are disjoint; each task accesses a different entity set.
            // SendCellsQuery safely transmits the raw pointer; the get_unchecked safety is
            // upheld by the forest structure (no aliased Mut<> borrows).
            let sendable = SendCellsQuery(cells_query as *const _ as usize);
            ComputeTaskPool::get().scope(|scope| {
                for (sub_grid, gc) in &child_grids {
                    let q = sendable;
                    scope.spawn(async move {
                        // SAFETY: See above. `as_ref` takes self by value, forcing the compiler
                        // to capture the whole `SendCellsQuery` (usize, Send) rather than its
                        // individual field (which would otherwise be a non-Send raw pointer).
                        unsafe { Self::traverse_grid(sub_grid, gc, q.as_ref(), system_ticks) }
                    });
                }
            });
        }
    }

    /// Update the [`GlobalTransform`] of leaf entities — those with [`CellCoord`] but without
    /// [`Grid`].
    ///
    /// Runs as a flat [`Query::par_iter_mut`] over all leaf entities, looking up each entity's
    /// parent [`Grid`] by entity ID to retrieve the [`LocalFloatingOrigin`] that was already
    /// propagated by [`LocalFloatingOrigin::compute_all`]. This is safe to parallelize because
    /// every entity's GT depends only on its own components (owned) and its parent grid's
    /// [`LocalFloatingOrigin`] (shared read).
    ///
    /// Runs after [`Grid::propagate_high_precision`], which handles the sub-grid hierarchy and
    /// updates [`GlobalTransform`] for sub-grid entities.
    pub fn propagate_leaf_entities(
        system_ticks: SystemChangeTick,
        mut stats: Option<ResMut<crate::timing::PropagationStats>>,
        grids: Query<(&Grid, Option<&GridDirtyTick>)>,
        mut entities: Query<
            (
                Ref<CellCoord>,
                Ref<Transform>,
                Ref<ChildOf>,
                &mut GlobalTransform,
                Option<&Stationary>,
                Option<&StationaryComputed>,
            ),
            (With<CellCoord>, Without<Grid>),
        >,
    ) {
        let start = bevy_platform::time::Instant::now();

        entities.par_iter_mut().for_each(
            |(cell, transform, parent_rel, mut gt, stationary, computed)| {
                let Ok((grid, dirty_tick)) = grids.get(parent_rel.parent()) else {
                    return;
                };

                let is_stationary = stationary.is_some();
                let is_computed = computed.is_some();

                // Grid-level early exit: if the grid's origin is unchanged and its subtree is
                // clean (no non-stationary entity changed this frame), skip without touching the
                // per-entity change-detection fields. This is semantically equivalent to the
                // subtree pruning in `traverse_grid`, but applied per-entity so it works with the
                // flat parallel iteration.
                let subtree_clean = dirty_tick.is_some_and(|dt| !dt.is_dirty(system_ticks));
                if grid.local_floating_origin().is_local_origin_unchanged() && subtree_clean {
                    return;
                }

                if !grid.local_floating_origin().is_local_origin_unchanged()
                    || (transform.is_changed() && !is_stationary)
                    || cell.is_changed()
                    || parent_rel.is_changed()
                    || (is_stationary && !is_computed)
                {
                    *gt = grid.global_transform(&cell, &transform);
                }
            },
        );

        if let Some(stats) = stats.as_mut() {
            stats.high_precision_propagation += start.elapsed();
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
                Without<CellCoord>,
                Without<Grid>,
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

    /// Verifies that `traverse_grid` correctly recurses into sub-grids.
    ///
    /// Hierarchy: Root BigSpace → SubGrid (CellCoord + Grid + Transform(100,0,0))
    ///                                  → Entity (CellCoord + Transform(50,0,0))
    ///
    /// Entity's GT should be 100 + 50 = 150 from the root FO.
    #[test]
    fn sub_grid_gt_is_correct() {
        #[derive(Component)]
        struct TestEntity;

        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn_big_space_default(|root| {
                    root.spawn_spatial(FloatingOrigin);
                    // Sub-grid at (100, 0, 0) in root grid containing an entity at (50, 0, 0).
                    root.with_grid_default(|sub_grid| {
                        sub_grid.insert(Transform::from_xyz(100.0, 0.0, 0.0));
                        sub_grid.spawn_spatial((Transform::from_xyz(50.0, 0.0, 0.0), TestEntity));
                    });
                });
            });

        app.update();

        let mut q = app
            .world_mut()
            .query_filtered::<&GlobalTransform, With<TestEntity>>();
        let gt = *q.single(app.world()).unwrap();
        assert_eq!(
            gt.translation(),
            Vec3::new(150.0, 0.0, 0.0),
            "Entity in sub-grid should have GT = sub-grid pos + entity pos = 150"
        );
    }

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
