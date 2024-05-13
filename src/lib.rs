//! This [`bevy`] plugin makes it possible to build high-precision worlds that exceed the size of
//! the observable universe, with no added dependencies, while remaining largely compatible with the
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
//! While using the [`FloatingOriginPlugin`], the position of entities is now defined with the
//! [`ReferenceFrame`], [`GridCell`], and [`Transform`] components. The `ReferenceFrame` is a large
//! integer grid of cells; entities are located within this grid using the `GridCell` component.
//! Finally, the `Transform` is used to position the entity relative to the center of its
//! `GridCell`. If an entity moves into a neighboring cell, its transform will be automatically
//! recomputed relative to the center of that new cell. This prevents `Transforms` from ever
//! becoming larger than a single grid cell, and thus prevents floating point precision artifacts.
//!
//! The grid adds precision to your transforms. If you are using (32-bit) `Transform`s on an `i32`
//! grid, you will have 64 bits of precision: 32 bits to address into a large integer grid, and 32
//! bits of floating point precision within a grid cell. This plugin is generic up to `i128` grids,
//! giving you up tp 160 bits of precision of translation.
//!
//! `ReferenceFrame`s - grids - can be nested. This allows you to define moving reference frames,
//! which can make certain use cases much simpler. For example, if you have a planet rotating, and
//! orbiting around its star, it would be very annoying if you had to compute this orbit and
//! rotation for all objects on the surface in high precision. Instead, you can place the planet and
//! all objects on its surface in the same reference frame. The motion of the planet will be
//! inherited by all children in that reference frame, in high precision.
//!
//! Entities at the root of bevy's entity hierarchy are not in any reference frame. This allows
//! plugins from the rest of the ecosystem to operate normally, such as bevy_ui, which relies on the
//! built in transform propagation system. This also means that if you don't need to place entities
//! in a high-precision reference frame, you don't have to, as the process is opt-in. The
//! high-precision hierarchical reference frames are explicit. Each high-precision tree must have a
//! [`BigSpaceBundle`] at the root, and each `BigSpace` is independent. This means that each
//! `BigSpace` has its own floating origin, which allows you to do things like rendering two players
//! on opposite ends of the universe simultaneously.
//!
//! All of the above applies to the entity marked with the [`FloatingOrigin`] component. The
//! floating origin can be any high-precision entity in a `BigSpace`. The only thing special about
//! the entity marked as the floating origin, is that it used to compute the[`GlobalTransform`] of
//! all other entities in that `BigSpace`. To an outside observer, every high-precision entity
//! within a `BigSpace` is confined to a box the size of a grid cell - like a game of *Asteroids*.
//! Only once you render the `BigSpace` from the point of view of the floating origin, by
//! calculating their `GlobalTransform`s, do entities appear very distant from the floating origin.
//!
//! As described above. the `GlobalTransform` of all entities is computed relative to the floating
//! origin's grid cell. Because of this, entities very far from the origin will have very large,
//! imprecise positions. However, this is always relative to the camera (floating origin), so these
//! artifacts will always be too far away to be seen, no matter where the camera moves. Because this
//! only affects the `GlobalTransform` and not the `Transform`, this also means that entities will
//! never permanently lose precision just because they were far from the origin at some point. The
//! lossy calculation only occurs when computing the `GlobalTransform` of entities, the high
//! precision `GridCell` and `Transform` are never touched.
//!
//! # Getting Started
//!
//! To start using this plugin:
//! 0. Choose how big your world should be! Do you need an i32, or an i128?
//! 1. Disable Bevy's transform plugin: `DefaultPlugins.build().disable::<TransformPlugin>()`
//! 2. Add the [`FloatingOriginPlugin`] to your `App`
//! 3. Create a new `BigSpace` tree using a [`BigSpaceBundle`].
//! 4. Spawn entities as children of the `BigSpace`, using the [`BigSpatialBundle`].
//! 5. Add the [`FloatingOrigin`] component to the active camera
//! 6. To add more levels to the hierarchy, add a [`ReferenceFrame`] to an existing
//!    [`BigSpatialBundle`], or use the [`BigReferenceFrameBundle`] instead.
//!
//! Take a look at [`ReferenceFrame`] component for some useful helper methods.
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
//! [`Transform`] of that entity using [`ReferenceFrame::translation_to_grid`]. If the star this
//! planet is orbiting around is also moving through space, note that you can add/subtract grid
//! cells. This means you can do each calculation in the reference frame of the moving body, and sum
//! up the computed translations and grid cell offsets to get a more precise result.

#![allow(clippy::type_complexity)]
#![deny(missing_docs)]

use bevy::{prelude::*, transform::TransformSystem};
use propagation::propagate_reference_frame_transforms;
use std::marker::PhantomData;
use world_query::GridTransformReadOnly;

pub mod bundles;
pub mod grid_cell;
pub mod precision;
pub mod propagation;
pub mod reference_frame;
pub mod validation;
pub mod world_query;

pub use bundles::*;
pub use grid_cell::GridCell;

#[cfg(feature = "camera")]
pub mod camera;
#[cfg(feature = "debug")]
pub mod debug;

use crate::precision::*;
use crate::reference_frame::{local_origin::LocalFloatingOrigin, BigSpace, ReferenceFrame};

/// Add this plugin to your [`App`] for floating origin functionality.
#[derive(Default)]
pub struct FloatingOriginPlugin<P: GridPrecision> {
    phantom: PhantomData<P>,
}

impl<P: GridPrecision + Reflect + FromReflect + TypePath> Plugin for FloatingOriginPlugin<P> {
    fn build(&self, app: &mut App) {
        #[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
        enum FloatingOriginSet {
            RecenterLargeTransforms,
            LocalFloatingOrigins,
            RootGlobalTransforms,
            PropagateTransforms,
        }

        let system_set_config = || {
            (
                (
                    recenter_transform_on_grid::<P>,
                    BigSpace::update_floating_origin,
                )
                    .in_set(FloatingOriginSet::RecenterLargeTransforms),
                LocalFloatingOrigin::<P>::update
                    .in_set(FloatingOriginSet::LocalFloatingOrigins)
                    .after(FloatingOriginSet::RecenterLargeTransforms),
                update_grid_cell_global_transforms::<P>
                    .in_set(FloatingOriginSet::RootGlobalTransforms)
                    .after(FloatingOriginSet::LocalFloatingOrigins),
                propagate_reference_frame_transforms::<P>
                    .in_set(FloatingOriginSet::PropagateTransforms)
                    .after(FloatingOriginSet::RootGlobalTransforms),
            )
                .in_set(TransformSystem::TransformPropagate)
        };

        app.register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<GridCell<P>>()
            .register_type::<ReferenceFrame<P>>()
            .register_type::<BigSpace>()
            .register_type::<FloatingOrigin>()
            .add_systems(PostStartup, system_set_config())
            .add_systems(PostUpdate, system_set_config())
            .add_systems(
                PostUpdate,
                validation::validate_hierarchy::<validation::BigSpaceRoot<P>>
                    .before(TransformSystem::TransformPropagate),
            )
            .add_systems(
                PostStartup,
                (
                    // These are the bevy transform propagation systems. Because these start from
                    // the root of the hierarchy, and BigSpace bundles (at the root) do not contain
                    // a Transform, these systems will not interact with any high precision entities
                    // in big space. These systems are added for ecosystem compatibility with bevy,
                    // although the rendered behavior might look strange if they share a camera with
                    // one using the floating origin.
                    //
                    // This is most useful for bevy_ui, which relies on the transform systems to
                    // work, or if you want to render a camera that only needs to render a
                    // low-precision scene.
                    bevy::transform::systems::sync_simple_transforms,
                    bevy::transform::systems::propagate_transforms,
                )
                    .in_set(TransformSystem::TransformPropagate),
            );
    }
}

/// Marks the entity to use as the floating origin. The [`GlobalTransform`] of all entities within
/// this [`BigSpace`] will be computed relative to this floating origin. There should always be
/// exactly one entity marked with this component within a [`BigSpace`].
#[derive(Component, Reflect)]
pub struct FloatingOrigin;

/// If an entity's transform translation becomes larger than the limit specified in its
/// [`ReferenceFrame`], it will be relocated to the nearest grid cell to reduce the size of the
/// transform.
pub fn recenter_transform_on_grid<P: GridPrecision>(
    reference_frames: Query<&ReferenceFrame<P>>,
    mut changed_transform: Query<(&mut GridCell<P>, &mut Transform, &Parent), Changed<Transform>>,
) {
    changed_transform
        .par_iter_mut()
        .for_each(|(mut grid_pos, mut transform, parent)| {
            let Ok(reference_frame) = reference_frames.get(parent.get()) else {
                return;
            };
            if transform.as_ref().translation.abs().max_element()
                > reference_frame.maximum_distance_from_origin()
            {
                let (grid_cell_delta, translation) =
                    reference_frame.imprecise_translation_to_grid(transform.as_ref().translation);
                *grid_pos += grid_cell_delta;
                transform.translation = translation;
            }
        });
}

/// Update the `GlobalTransform` of entities with a [`GridCell`], using the [`ReferenceFrame`] the
/// entity belongs to.
pub fn update_grid_cell_global_transforms<P: GridPrecision>(
    reference_frames: Query<(&ReferenceFrame<P>, &Children)>,
    mut entities: Query<(GridTransformReadOnly<P>, &mut GlobalTransform), With<Parent>>,
) {
    // Update the GlobalTransform of GridCell entities that are children of a ReferenceFrame
    for (frame, children) in &reference_frames {
        let mut frame_children = entities.iter_many_mut(children);
        while let Some((grid_transform, mut global_transform)) = frame_children.fetch_next() {
            *global_transform =
                frame.global_transform(grid_transform.cell, grid_transform.transform);
        }
    }
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

        app.world
            .spawn(bundles::BigSpaceBundle::<i32>::default())
            .push_children(&[first, second]);

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

        app.world
            .spawn(bundles::BigSpaceBundle::<i32>::default())
            .push_children(&[first, second]);

        app.update();

        app.world.entity_mut(first).remove::<FloatingOrigin>();
        app.world.entity_mut(second).insert(FloatingOrigin);

        app.update();

        let child = app.world.get::<Children>(second).unwrap()[0];
        let child_transform = app.world.get::<GlobalTransform>(child).unwrap();

        assert_eq!(child_transform.translation(), Vec3::new(0.0, 0.0, 600.0));
    }
}
