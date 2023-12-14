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

use bevy::{math::DVec3, prelude::*, reflect::TypePath, transform::TransformSystem};
use propagation::propagate_transforms;
use std::marker::PhantomData;
use world_query::{GridTransformReadOnly, GridTransformReadOnlyItem};

pub mod grid_cell;
pub mod precision;
pub mod propagation;
pub mod world_query;

pub use grid_cell::GridCell;

#[cfg(feature = "debug")]
pub mod debug;

#[cfg(feature = "camera")]
pub mod camera;

use precision::*;

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

        app.insert_resource(FloatingOriginSettings::new(
            self.grid_edge_length,
            self.switching_threshold,
        ))
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

/// Configuration settings for the floating origin plugin.
#[derive(Reflect, Clone, Resource)]
pub struct FloatingOriginSettings {
    grid_edge_length: f32,
    maximum_distance_from_origin: f32,
}

impl FloatingOriginSettings {
    /// Construct a new [`FloatingOriginSettings`] struct. This cannot be updated after the plugin
    /// is built.
    pub fn new(grid_edge_length: f32, switching_threshold: f32) -> Self {
        Self {
            grid_edge_length,
            maximum_distance_from_origin: grid_edge_length / 2.0 + switching_threshold,
        }
    }

    /// Get the plugin's `grid_edge_length`.
    pub fn grid_edge_length(&self) -> f32 {
        self.grid_edge_length
    }

    /// Get the plugin's `maximum_distance_from_origin`.
    pub fn maximum_distance_from_origin(&self) -> f32 {
        self.maximum_distance_from_origin
    }

    /// Compute the double precision position of an entity's [`Transform`] with respect to the given
    /// [`GridCell`].
    pub fn grid_position_double<P: GridPrecision>(
        &self,
        pos: &GridCell<P>,
        transform: &Transform,
    ) -> DVec3 {
        DVec3 {
            x: pos.x.as_f64() * self.grid_edge_length as f64 + transform.translation.x as f64,
            y: pos.y.as_f64() * self.grid_edge_length as f64 + transform.translation.y as f64,
            z: pos.z.as_f64() * self.grid_edge_length as f64 + transform.translation.z as f64,
        }
    }

    /// Compute the single precision position of an entity's [`Transform`] with respect to the given
    /// [`GridCell`].
    pub fn grid_position<P: GridPrecision>(
        &self,
        pos: &GridCell<P>,
        transform: &Transform,
    ) -> Vec3 {
        Vec3 {
            x: pos.x.as_f64() as f32 * self.grid_edge_length + transform.translation.x,
            y: pos.y.as_f64() as f32 * self.grid_edge_length + transform.translation.y,
            z: pos.z.as_f64() as f32 * self.grid_edge_length + transform.translation.z,
        }
    }

    /// Convert a large translation into a small translation relative to a grid cell.
    pub fn translation_to_grid<P: GridPrecision>(
        &self,
        input: impl Into<DVec3>,
    ) -> (GridCell<P>, Vec3) {
        let l = self.grid_edge_length as f64;
        let input = input.into();
        let DVec3 { x, y, z } = input;

        if input.abs().max_element() < self.maximum_distance_from_origin as f64 {
            return (GridCell::default(), input.as_vec3());
        }

        let x_r = (x / l).round();
        let y_r = (y / l).round();
        let z_r = (z / l).round();
        let t_x = x - x_r * l;
        let t_y = y - y_r * l;
        let t_z = z - z_r * l;

        (
            GridCell {
                x: P::from_f32(x_r as f32),
                y: P::from_f32(y_r as f32),
                z: P::from_f32(z_r as f32),
            },
            Vec3::new(t_x as f32, t_y as f32, t_z as f32),
        )
    }

    /// Convert a large translation into a small translation relative to a grid cell.
    pub fn imprecise_translation_to_grid<P: GridPrecision>(
        &self,
        input: Vec3,
    ) -> (GridCell<P>, Vec3) {
        self.translation_to_grid(input.as_dvec3())
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
    settings: Res<FloatingOriginSettings>,
    mut query: Query<(&mut GridCell<P>, &mut Transform), (Changed<Transform>, Without<Parent>)>,
) {
    query
        .par_iter_mut()
        .for_each(|(mut grid_pos, mut transform)| {
            if transform.as_ref().translation.abs().max_element()
                > settings.maximum_distance_from_origin
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
    settings: Res<FloatingOriginSettings>,
    origin: Query<Ref<GridCell<P>>, With<FloatingOrigin>>,
    mut entities: ParamSet<(
        Query<
            (GridTransformReadOnly<P>, &mut GlobalTransform),
            (
                Or<(Changed<GridCell<P>>, Changed<Transform>)>,
                Without<Parent>,
            ),
        >,
        Query<(GridTransformReadOnly<P>, &mut GlobalTransform), Without<Parent>>,
    )>,
) {
    let origin_cell = origin.single();

    if origin_cell.is_changed() {
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
    settings: &FloatingOriginSettings,
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

/// Update [`GlobalTransform`] component of entities that aren't in the hierarchy
///
/// Third party plugins should ensure that this is used in concert with [`propagate_transforms`].
pub fn sync_simple_transforms<P: GridPrecision>(
    mut query: Query<
        (&Transform, &mut GlobalTransform),
        (
            Changed<Transform>,
            Without<Parent>,
            Without<Children>,
            Without<GridCell<P>>,
        ),
    >,
) {
    query
        .par_iter_mut()
        .for_each(|(transform, mut global_transform)| {
            *global_transform = GlobalTransform::from(*transform);
        });
}
