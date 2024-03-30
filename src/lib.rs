//! This [`bevy`] plugin makes it easy to build high-precision worlds that exceed the size of the
//! observable universe, with no added dependencies, while remaining largely compatible with the
//! rest of the Bevy ecosystem.
//!
//! ### Problem
//!
//! Objects far from the origin suffer from reduced precision, causing rendered meshes to jitter and
//! jiggle, and transformation calculations to encounter catastrophic cancellation.
//!
//! As the camera moves farther from the origin, the scale of floats needed to describe the position
//! of meshes and the camera get larger, which in turn means there is less precision available.
//! Consequently, when the matrix math is done to compute the position of objects in view space,
//! mesh vertices will be displaced due to this lost precision.
//!
//! ### Solution
//!
//! While using the [`FloatingOriginPlugin`], entities are placed into a [`GridCell`] in a large
//! fixed precision grid. Inside a `GridCell`, an entity's `Transform` is relative to the center of
//! that grid cell. If an entity moves into a neighboring cell, its transform will be recomputed
//! relative to the center of that new cell. This prevents `Transforms` from ever becoming larger
//! than a single grid cell, and thus prevents floating point precision artifacts.
//!
//! The same thing happens to the entity marked with the [`FloatingOrigin`] component. The only
//! difference is that the `GridCell` of the floating origin is used when computing the
//! `GlobalTransform` of all other entities. To an outside observer, as the floating origin camera
//! moves through space and reaches the limits of its `GridCell`, it would appear to teleport to the
//! opposite side of the cell, similar to the spaceship in the game *Asteroids*.
//!
//! The `GlobalTransform` of all entities is computed relative to the floating origin's grid cell.
//! Because of this, entities very far from the origin will have very large, imprecise positions.
//! However, this is always relative to the camera (floating origin), so these artifacts will always
//! be too far away to be seen, no matter where the camera moves. Because this only affects the
//! `GlobalTransform` and not the `Transform`, this also means that entities will never permanently
//! lose precision just because they were far from the origin at some point.
//!
//! # Getting Started
//!
//! All that's needed to start using this plugin:
//! 1. Disable Bevy's transform plugin: `DefaultPlugins.build().disable::<TransformPlugin>()`
//! 2. Add the [`FloatingOriginPlugin`] to your `App`
//! 3. Add the [`GridCell`] component to all spatial entities
//! 4. Add the [`FloatingOrigin`] component to the active camera
//!
//! Take a look at [`FloatingOriginSettings`] resource for some useful helper methods.
//!
//! # Moving Entities
//!
//! For the most part, you can update the position of entities normally while using this plugin, and
//! it will automatically handle the tricky bits. However, there is one big caveat:
//!
//! **Avoid setting position absolutely, instead prefer applying a relative delta**
//!
//! Instead of:
//!
//! ```ignore
//! transform.translation = a_huge_imprecise_position;
//! ```
//!
//! do:
//!
//! ```ignore
//! let delta = new_pos - old_pos;
//! transform.translation += delta;
//! ```
//!
//! ## Absolute Position
//!
//! If you are updating the position of an entity with absolute positions, and the position exceeds
//! the bounds of the entity's grid cell, the floating origin plugin will recenter that entity into
//! its new cell. Every time you update that entity, you will be fighting with the plugin as it
//! constantly recenters your entity. This can especially cause problems with camera controllers
//! which may not expect the large discontinuity in position as an entity moves between cells.
//!
//! The other reason to avoid this is you will likely run into precision issues! This plugin exists
//! because single precision is limited, and the larger the position coordinates get, the less
//! precision you have.
//!
//! However, if you have something that must not accumulate error, like the orbit of a planet, you
//! can instead do the orbital calculation (position as a function of time) to compute the absolute
//! position of the planet with high precision, then directly compute the [`GridCell`] and
//! [`Transform`] of that entity using [`FloatingOriginSettings::translation_to_grid`]. If the star
//! this planet is orbiting around is also moving through space, note that you can add/subtract grid
//! cells. This means you can do each calculation in the reference frame of the moving body, and sum
//! up the computed translations and grid cell offsets to get a more precise result.

#![allow(clippy::type_complexity)]
#![deny(missing_docs)]

use bevy::{prelude::*, transform::TransformSystem};
use propagation::{propagate_transforms, sync_simple_transforms};
use std::marker::PhantomData;
use world_query::{GridTransformReadOnly, GridTransformReadOnlyItem};

pub mod grid_cell;
pub mod precision;
pub mod propagation;
pub mod reference_frame;
pub mod world_query;

pub use grid_cell::GridCell;

#[cfg(feature = "debug")]
pub mod debug;

#[cfg(feature = "camera")]
pub mod camera;

use precision::*;

use crate::reference_frame::{ReferenceFrame, RootReferenceFrame};

/// Add this plugin to your [`App`] for floating origin functionality.
pub struct FloatingOriginPlugin<P: GridPrecision> {
    /// The edge length of a single cell.
    pub grid_edge_length: f32,
    /// How far past the extents of a cell an entity must travel before a grid recentering occurs.
    /// This prevents entities from rapidly switching between cells when moving along a boundary.
    pub switching_threshold: f32,
    phantom: PhantomData<P>,
}

impl<P: GridPrecision> Default for FloatingOriginPlugin<P> {
    fn default() -> Self {
        Self::new(2_000f32, 100f32)
    }
}

impl<P: GridPrecision> FloatingOriginPlugin<P> {
    /// Construct a new plugin with the following settings.
    pub fn new(grid_edge_length: f32, switching_threshold: f32) -> Self {
        FloatingOriginPlugin {
            grid_edge_length,
            switching_threshold,
            phantom: PhantomData,
        }
    }
}

impl<P: GridPrecision + Reflect + FromReflect + TypePath> Plugin for FloatingOriginPlugin<P> {
    fn build(&self, app: &mut App) {
        #[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
        struct RootGlobalTransformUpdates;

        app.insert_resource(RootReferenceFrame::<P>(ReferenceFrame::new(
            self.grid_edge_length,
            self.switching_threshold,
        )))
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<GridCell<P>>()
        .add_plugins(ValidParentCheckPlugin::<GlobalTransform>::default())
        .add_systems(
            PostStartup,
            (
                recenter_transform_on_grid::<P>.before(RootGlobalTransformUpdates),
                (sync_simple_transforms::<P>, update_global_from_grid::<P>)
                    .in_set(RootGlobalTransformUpdates),
                propagate_transforms::<P>.after(RootGlobalTransformUpdates),
            )
                .in_set(TransformSystem::TransformPropagate),
        )
        .add_systems(
            PostUpdate,
            (
                recenter_transform_on_grid::<P>.before(RootGlobalTransformUpdates),
                (sync_simple_transforms::<P>, update_global_from_grid::<P>)
                    .in_set(RootGlobalTransformUpdates),
                propagate_transforms::<P>.after(RootGlobalTransformUpdates),
            )
                .in_set(TransformSystem::TransformPropagate),
        );
    }
}

/// Minimal bundle needed to position an entity in floating origin space.
///
/// This is the floating origin equivalent of the [`SpatialBundle`].
#[derive(Bundle, Default)]
pub struct FloatingSpatialBundle<P: GridPrecision> {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub visibility: Visibility,
    /// The inherited visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub inherited: InheritedVisibility,
    /// The view visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub view: ViewVisibility,
    /// The transform of the entity.
    pub transform: Transform,
    /// The global transform of the entity.
    pub global_transform: GlobalTransform,
    /// The grid position of the entity
    pub grid_position: GridCell<P>,
}

/// Marks the entity to use as the floating origin. All other entities will be positioned relative
/// to this entity's [`GridCell`].
#[derive(Component, Reflect)]
pub struct FloatingOrigin;

/// If an entity's transform becomes larger than the specified limit, it is relocated to the nearest
/// grid cell to reduce the size of the transform.
pub fn recenter_transform_on_grid<P: GridPrecision>(
    settings: Res<RootReferenceFrame<P>>,
    mut query: Query<(&mut GridCell<P>, &mut Transform), (Changed<Transform>, Without<Parent>)>,
) {
    query
        .par_iter_mut()
        .for_each(|(mut grid_pos, mut transform)| {
            if transform.as_ref().translation.abs().max_element()
                > settings.maximum_distance_from_origin()
            {
                let (grid_cell_delta, translation) =
                    settings.imprecise_translation_to_grid(transform.as_ref().translation);
                *grid_pos += grid_cell_delta;
                transform.translation = translation;
            }
        });
}

/// Compute the `GlobalTransform` relative to the floating origin's cell.
pub fn update_global_from_grid<P: GridPrecision>(
    settings: Res<RootReferenceFrame<P>>,
    origin: Query<(Ref<GridCell<P>>, Ref<FloatingOrigin>)>,
    mut entities: ParamSet<(
        Query<
            (GridTransformReadOnly<P>, &mut GlobalTransform),
            Or<(Changed<GridCell<P>>, Changed<Transform>)>,
        >,
        Query<(GridTransformReadOnly<P>, &mut GlobalTransform)>,
    )>,
) {
    let (origin_cell, floating_origin) = origin.single();

    if origin_cell.is_changed() || floating_origin.is_changed() {
        let mut all_entities = entities.p1();
        all_entities.par_iter_mut().for_each(|(local, global)| {
            update_global_from_cell_local(&settings, &origin_cell, local, global);
        });
    } else {
        let mut moved_cell_entities = entities.p0();
        moved_cell_entities
            .par_iter_mut()
            .for_each(|(local, global)| {
                update_global_from_cell_local(&settings, &origin_cell, local, global);
            });
    }
}

fn update_global_from_cell_local<P: GridPrecision>(
    settings: &ReferenceFrame<P>,
    origin_cell: &GridCell<P>,
    local: GridTransformReadOnlyItem<P>,
    mut global: Mut<GlobalTransform>,
) {
    let grid_cell_delta = *local.cell - *origin_cell;
    *global = local
        .transform
        .with_translation(settings.grid_position(&grid_cell_delta, local.transform))
        .into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_floating_origin_updates_global_transform() {
        let mut app = App::new();
        app.add_plugins(FloatingOriginPlugin::<i32>::default());

        let first = app
            .world
            .spawn((
                TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                    150.0, 0.0, 0.0,
                ))),
                GridCell::<i32>::new(5, 0, 0),
                FloatingOrigin,
            ))
            .id();

        let second = app
            .world
            .spawn((
                TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                    0.0, 0.0, 300.0,
                ))),
                GridCell::<i32>::new(0, -15, 0),
            ))
            .id();

        app.update();

        app.world.entity_mut(first).remove::<FloatingOrigin>();
        app.world.entity_mut(second).insert(FloatingOrigin);

        app.update();

        let second_global_transform = app.world.get::<GlobalTransform>(second).unwrap();

        assert_eq!(
            second_global_transform.translation(),
            Vec3::new(0.0, 0.0, 300.0)
        );
    }

    #[test]
    fn child_global_transforms_are_updated_when_floating_origin_changes() {
        let mut app = App::new();
        app.add_plugins(FloatingOriginPlugin::<i32>::default());

        let first = app
            .world
            .spawn((
                TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                    150.0, 0.0, 0.0,
                ))),
                GridCell::<i32>::new(5, 0, 0),
                FloatingOrigin,
            ))
            .id();

        let second = app
            .world
            .spawn((
                TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                    0.0, 0.0, 300.0,
                ))),
                GridCell::<i32>::new(0, -15, 0),
            ))
            .with_children(|parent| {
                parent.spawn((TransformBundle::from_transform(
                    Transform::from_translation(Vec3::new(0.0, 0.0, 300.0)),
                ),));
            })
            .id();

        app.update();

        app.world.entity_mut(first).remove::<FloatingOrigin>();
        app.world.entity_mut(second).insert(FloatingOrigin);

        app.update();

        let child = app.world.get::<Children>(second).unwrap()[0];
        let child_transform = app.world.get::<GlobalTransform>(child).unwrap();

        assert_eq!(child_transform.translation(), Vec3::new(0.0, 0.0, 600.0));
    }
}
