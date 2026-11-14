//! Logic for propagating transforms through the hierarchy of grids.

use crate::{prelude::*, stationary::GridDirtyTick};
use bevy_ecs::{prelude::*, system::SystemChangeTick};
#[cfg(feature = "std")]
use bevy_log::tracing::Instrument;
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

#[cfg(feature = "std")]
type HighPrecisionEntity = (
    Ref<'static, CellCoord>,
    Ref<'static, Transform>,
    Ref<'static, ChildOf>,
    &'static mut GlobalTransform,
    Has<Stationary>,
    Has<StationaryInitialized>,
);

#[cfg(feature = "std")]
type HighPrecisionChanged = Or<(
    Changed<CellCoord>,
    Changed<Transform>,
    Changed<ChildOf>,
    (With<Stationary>, Without<StationaryInitialized>),
)>;

impl Grid {
    /// Update the [`GlobalTransform`] of root [`BigSpace`] grids.
    ///
    /// Root grids don't have a [`CellCoord`], so they aren't covered by
    /// [`Self::propagate_high_precision`]. Their GT is determined entirely by the
    /// [`LocalFloatingOrigin`].
    pub fn propagate_root_grids(
        mut root_grids: Query<(&Grid, &mut GlobalTransform), With<BigSpace>>,
    ) {
        root_grids.par_iter_mut().for_each(|(grid, mut gt)| {
            if !grid.local_floating_origin().is_local_origin_unchanged() {
                *gt = grid.global_transform(&CellCoord::default(), &Transform::IDENTITY);
            }
        });
    }

    /// Update the [`GlobalTransform`] of all entities with a [`CellCoord`], including both sub-grid
    /// entities and leaf entities.
    ///
    /// Runs as a flat [`Query::par_iter_mut`], looking up each entity's parent [`Grid`] to retrieve
    /// the [`LocalFloatingOrigin`] already propagated by [`LocalFloatingOrigin::compute_all`].
    /// Every entity's GT depends only on its own components (owned) and its parent grid's
    /// [`LocalFloatingOrigin`] (shared read), so this is trivially parallelizable with no unsafe
    /// code.
    ///
    /// If [`GridDirtyTick`] is present on the parent grid (inserted by
    /// [`BigSpaceStationaryPlugin`]), entities in clean subtrees are skipped entirely.
    pub fn propagate_high_precision(
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
                Option<&StationaryInitialized>,
            ),
            With<CellCoord>,
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

                // Grid-level early exit: we can only skip when BOTH conditions hold:
                // 1. The grid's local floating origin hasn't moved (no cell change by the FO), AND
                // 2. The subtree is clean (no non-stationary entity changed this frame).
                // If the FO moved to a new cell, every entity in the grid needs a new GT
                // regardless of whether the entity itself changed.
                let subtree_clean = dirty_tick.is_some_and(|dt| !dt.is_dirty(system_ticks));
                if grid.local_floating_origin().is_local_origin_unchanged() && subtree_clean {
                    return;
                }

                // Recompute GT when:
                // - The grid's local origin moved (FO changed cells), forcing all entities to
                //   update even if they haven't moved themselves, OR
                // - The entity's own transform/cell/parent changed, OR
                // - The entity is stationary but hasn't had its initial GT computed yet.
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

    /// Update the [`GlobalTransform`] of all entities with a [`CellCoord`].
    ///
    /// Entities whose parent grid's local floating origin did not move are visited by a single
    /// change-filtered [`Query::par_iter_mut`], so only entities whose cell, transform or parent
    /// changed (or stationary entities that were not initialized yet) are touched. The cost of a
    /// mostly static world no longer depends on its size when a few entities move.
    ///
    /// Grids whose local floating origin moved need every child recomputed. Their children are
    /// sent through a [`BufferedChannel`] by producer tasks, and consumer workers update their
    /// [`GlobalTransform`] concurrently via [`Query::get_unchecked`].
    ///
    /// This is the default high-precision propagation system on `std` targets. The flat
    /// [`Self::propagate_high_precision`] variant is used on `no_std`.
    ///
    /// [`BufferedChannel`]: crate::buffered_channel::BufferedChannel
    #[allow(rustdoc::private_intra_doc_links)]
    #[cfg(feature = "std")]
    pub fn propagate_high_precision_channeled(
        mut stats: Option<ResMut<crate::timing::PropagationStats>>,
        grids: Query<(Entity, &Grid, Option<&Children>)>,
        mut entities: ParamSet<(
            Query<HighPrecisionEntity, With<CellCoord>>,
            Query<HighPrecisionEntity, (With<CellCoord>, HighPrecisionChanged)>,
        )>,
        mut channel: Local<crate::buffered_channel::BufferedChannel<Entity>>,
    ) {
        let start = bevy_platform::time::Instant::now();
        let any_grid_moved = grids
            .iter()
            .any(|(_, grid, _)| !grid.local_floating_origin().is_local_origin_unchanged());

        // Changed entities in grids whose local floating origin did not move. Entities in moved
        // grids are skipped here, they are all recomputed by `propagate_moved_grids`.
        entities.p1().par_iter_mut().for_each(
            |(cell, transform, parent, mut gt, is_stationary, is_computed)| {
                let Ok((_, grid, _)) = grids.get(parent.parent()) else {
                    return;
                };
                if !grid.local_floating_origin().is_local_origin_unchanged() {
                    return;
                }
                // Recompute GT when:
                // - The entity's own transform/cell/parent changed, OR
                // - The entity is stationary but hasn't had its initial GT computed yet.
                if (transform.is_changed() && !is_stationary)
                    || cell.is_changed()
                    || parent.is_changed()
                    || (is_stationary && !is_computed)
                {
                    *gt = grid.global_transform(&cell, &transform);
                }
            },
        );

        if any_grid_moved {
            Self::propagate_moved_grids(&grids, &entities.p0(), &mut channel);
        }

        if let Some(stats) = stats.as_mut() {
            stats.high_precision_propagation += start.elapsed();
        }
    }

    /// Recompute the [`GlobalTransform`] of every child of the grids whose local floating origin
    /// moved, using a producer-consumer architecture with a [`BufferedChannel`].
    ///
    /// [`BufferedChannel`]: crate::buffered_channel::BufferedChannel
    #[cfg(feature = "std")]
    #[expect(
        unsafe_code,
        reason = "Uses get_unchecked and a Send wrapper for sharing queries across threads."
    )]
    fn propagate_moved_grids(
        grids: &Query<(Entity, &Grid, Option<&Children>)>,
        entities: &Query<HighPrecisionEntity, With<CellCoord>>,
        channel: &mut crate::buffered_channel::BufferedChannel<Entity>,
    ) {
        let task_pool = bevy_tasks::ComputeTaskPool::get();
        let n_threads = task_pool.thread_num().max(1);

        task_pool.scope(|scope| {
            // Tuned via benchmarking
            channel.chunk_size = 4096;
            let (rx, tx) = channel.unbounded();

            // Spawn consumer workers upfront so they start pulling work immediately.
            for _ in 0..n_threads {
                let rx = rx.clone();
                scope.spawn(
                    async move {
                        while let Ok(chunk) = rx.recv().await {
                            for &entity in chunk.iter() {
                                // SAFETY: Each entity is sent through the channel at most
                                // once (each entity has exactly one parent grid, and each
                                // grid's children are sent exactly once by the producers).
                                // Therefore, no two consumer tasks will call get_unchecked
                                // on the same entity.
                                let Ok((cell, transform, parent, mut gt, _, _)) =
                                    (unsafe { entities.get_unchecked(entity) })
                                else {
                                    continue;
                                };

                                // Read-only lookup of the parent grid.
                                let Ok((_, grid, _)) = grids.get(parent.parent()) else {
                                    continue;
                                };
                                *gt = grid.global_transform(&cell, &transform);
                            }
                        }
                    }
                    .instrument(bevy_log::info_span!("hp_propagation_worker")),
                );
            }
            drop(rx);

            // Producers: par_iter over grids for wide-hierarchy parallelism. Only grids whose
            // local floating origin moved are sent, since every child needs a new GT. Children
            // of unmoved grids were already handled by the change-filtered pass.
            // Small grids send children directly from the par_iter thread.
            // Large grids are collected via thread-local storage (no
            // contention) and chunked into separate scope tasks after par_iter.
            let min_chunk = n_threads * 10;
            let mut large_grids = bevy_utils::Parallel::<Vec<Entity>>::default();
            grids.par_iter().for_each_init(
                || tx.clone(),
                |sender, (grid_entity, grid, children)| {
                    if grid.local_floating_origin().is_local_origin_unchanged() {
                        return;
                    }
                    let Some(children) = children else { return };
                    if children.len() < min_chunk {
                        for child in children.iter() {
                            sender.send_blocking(child).ok();
                        }
                    } else {
                        large_grids.borrow_local_mut().push(grid_entity);
                    }
                },
            );

            // Spawn chunked producer tasks for large grids.
            for grid_entity in large_grids.drain() {
                let Ok((_, _, Some(children))) = grids.get(grid_entity) else {
                    continue;
                };
                let chunk_size = (children.len() / n_threads / 10).max(1);
                for child_chunk in children.chunks(chunk_size) {
                    let mut chunk_sender = tx.clone();
                    scope.spawn(
                        async move {
                            for &child in child_chunk {
                                chunk_sender.send_blocking(child).ok();
                            }
                        }
                        .instrument(bevy_log::info_span!("hp_propagation_producer")),
                    );
                }
            }
            drop(tx);
        });
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
            (With<ChildOf>, Without<CellCoord>, Without<Grid>),
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

    /// Verifies that entities in sub-grids get the correct `GlobalTransform`.
    ///
    /// Hierarchy: Root `BigSpace` -> `SubGrid` (`CellCoord` + Grid + Transform(100,0,0))
    ///                                  -> Entity (`CellCoord` + Transform(50,0,0))
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

    /// Verifies that the root `BigSpace` grid's `GlobalTransform` updates when the floating
    /// origin moves to a new cell. The root grid has no `CellCoord`, so it must be handled
    /// separately from the flat `par_iter` over `CellCoord` entities.
    #[test]
    fn root_grid_gt_updates_when_fo_moves() {
        let mut app = App::new();
        app.add_plugins(BigSpaceMinimalPlugins)
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn_big_space_default(|root| {
                    root.spawn_spatial(FloatingOrigin);
                });
            });

        app.update();

        // Find the FO and root grid
        let fo = app
            .world_mut()
            .query_filtered::<Entity, With<FloatingOrigin>>()
            .single(app.world())
            .unwrap();
        let root = app
            .world_mut()
            .query_filtered::<Entity, With<BigSpace>>()
            .single(app.world())
            .unwrap();

        let root_gt_before = app
            .world()
            .get::<GlobalTransform>(root)
            .unwrap()
            .translation();
        assert_eq!(root_gt_before, Vec3::ZERO);

        // Move FO to cell (1, 0, 0) - root GT should shift by -cell_size
        app.world_mut()
            .entity_mut(fo)
            .get_mut::<CellCoord>()
            .unwrap()
            .x = 1;
        app.update();

        let root_gt_after = app
            .world()
            .get::<GlobalTransform>(root)
            .unwrap()
            .translation();
        assert_ne!(
            root_gt_after,
            Vec3::ZERO,
            "Root grid GT must update when the floating origin moves to a new cell"
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

    /// Spawns a high precision entity at (3, 0, 0) in the root grid with a low precision child at
    /// (1, 0, 0), then runs two updates so that nothing is marked as changed anymore.
    ///
    /// Returns the high precision entity and its low precision child.
    fn spawn_low_precision_child(app: &mut App) -> (Entity, Entity) {
        #[derive(Component)]
        struct Spatial;
        #[derive(Component)]
        struct Child;

        app.add_plugins(BigSpaceMinimalPlugins)
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn_big_space_default(|root| {
                    root.spawn_spatial(FloatingOrigin);
                    root.spawn_spatial((Transform::from_xyz(3.0, 0.0, 0.0), Spatial))
                        .with_children(|spatial| {
                            spatial.spawn((Transform::from_xyz(1.0, 0.0, 0.0), Child));
                        });
                });
            });
        app.update();
        app.update();
        let spatial = app
            .world_mut()
            .query_filtered::<Entity, With<Spatial>>()
            .single(app.world())
            .unwrap();
        let child = app
            .world_mut()
            .query_filtered::<Entity, With<Child>>()
            .single(app.world())
            .unwrap();
        (spatial, child)
    }

    /// Verifies that moving a high precision entity in a grid whose floating origin did not move
    /// updates the `GlobalTransform` of its low precision child.
    #[test]
    fn low_precision_child_follows_a_moved_parent() {
        let mut app = App::new();
        let (spatial, child) = spawn_low_precision_child(&mut app);
        app.world_mut()
            .get_mut::<Transform>(spatial)
            .unwrap()
            .translation
            .x = 10.0;
        app.update();
        let gt = app.world().get::<GlobalTransform>(child).unwrap();
        assert_eq!(gt.translation(), Vec3::new(11.0, 0.0, 0.0));
    }

    /// Verifies that a low precision child whose own `Transform` changed is updated even though
    /// its high precision parent did not change this frame.
    #[test]
    fn low_precision_child_follows_its_own_change_under_a_still_parent() {
        let mut app = App::new();
        let (_, child) = spawn_low_precision_child(&mut app);
        app.world_mut()
            .get_mut::<Transform>(child)
            .unwrap()
            .translation
            .x = 5.0;
        app.update();
        let gt = app.world().get::<GlobalTransform>(child).unwrap();
        assert_eq!(gt.translation(), Vec3::new(8.0, 0.0, 0.0));
    }
}
