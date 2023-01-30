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
//! Take a look at [`FloatingOriginSettings`] resource for configuration options, as well as some
//! useful helper methods.
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
//! ```no_run
//! transform.translation = a_huge_imprecise_position;
//! ```
//!
//! do:
//!
//! ```no_run
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

use bevy::{math::DVec3, prelude::*, transform::TransformSystem};
use std::marker::PhantomData;

pub mod camera;
pub mod precision;

#[cfg(feature = "debug")]
pub mod debug;

use precision::*;

/// Add this plugin to your [`App`] to for floating origin functionality.
#[derive(Default)]
pub struct FloatingOriginPlugin<P: GridPrecision> {
    /// Initial floating origin settings.
    pub settings: FloatingOriginSettings,
    phantom: PhantomData<P>,
}

impl<P: GridPrecision> FloatingOriginPlugin<P> {
    /// # `switching_threshold`:
    ///
    /// How far past the extents of a cell an entity must travel before a grid recentering occurs.
    /// This prevents entities from rapidly switching between cells when moving along a boundary.
    pub fn new(grid_edge_length: f32, switching_threshold: f32) -> Self {
        FloatingOriginPlugin {
            settings: FloatingOriginSettings::new(grid_edge_length, switching_threshold),
            ..Default::default()
        }
    }
}

impl<P: GridPrecision> Plugin for FloatingOriginPlugin<P> {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.settings.clone())
            .register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<GridCell<P>>()
            .add_plugin(ValidParentCheckPlugin::<GlobalTransform>::default())
            // add transform systems to startup so the first update is "correct"
            .add_startup_system_to_stage(
                StartupStage::PostStartup,
                recenter_transform_on_grid::<P>
                    .label(TransformSystem::TransformPropagate)
                    .before(update_global_from_grid::<P>),
            )
            .add_startup_system_to_stage(
                StartupStage::PostStartup,
                update_global_from_grid::<P>
                    .label(TransformSystem::TransformPropagate)
                    .before(transform_propagate_system::<P>),
            )
            .add_startup_system_to_stage(
                StartupStage::PostStartup,
                transform_propagate_system::<P>.label(TransformSystem::TransformPropagate),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                recenter_transform_on_grid::<P>
                    .label(TransformSystem::TransformPropagate)
                    .before(update_global_from_grid::<P>),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                update_global_from_grid::<P>
                    .label(TransformSystem::TransformPropagate)
                    .before(transform_propagate_system::<P>),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                transform_propagate_system::<P>.label(TransformSystem::TransformPropagate),
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
    fn new(grid_edge_length: f32, switching_threshold: f32) -> Self {
        Self {
            grid_edge_length,
            maximum_distance_from_origin: grid_edge_length / 2.0 + switching_threshold,
        }
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

impl Default for FloatingOriginSettings {
    fn default() -> Self {
        Self::new(10_000f32, 100f32)
    }
}

/// Minimal bundle needed to position an entity in floating origin space.
///
/// This is the floating origin equivalent of the [`SpatialBundle`].
#[derive(Bundle, Default)]
pub struct FloatingSpatialBundle<P: GridPrecision> {
    /// The visibility of the entity.
    pub visibility: Visibility,
    /// The computed visibility of the entity.
    pub computed: ComputedVisibility,
    /// The transform of the entity.
    pub transform: Transform,
    /// The global transform of the entity.
    pub global_transform: GlobalTransform,
    /// The grid position of the entity
    pub grid_position: GridCell<P>,
}

/// Defines the grid cell this entity's `Transform` is relative to.
///
/// This component is generic over a few integer types to allow you to select the grid size you
/// need. These correspond to a total usable volume of a cube with the following edge lengths:
///
/// **Assuming you are using a grid cell edge length of 10,000 meters, and `1.0` == 1 meter**
///
/// - i8: 2,560 km = 74% of the diameter of the Moon
/// - i16: 655,350 km = 85% of the diameter of the Moon's orbit around Earth
/// - i32: 0.0045 light years = ~4 times the width of the solar system
/// - i64: 19.5 million light years = ~100 times the width of the milky way galaxy
/// - i128: 3.6e+26 light years = ~3.9e+15 times the width of the observable universe
///
/// where
///
/// `usable_edge_length = 2^(integer_bits) * grid_cell_edge_length`
///
/// # Note
///
/// Be sure you are using the same grid index precision everywhere. It might be a good idea to
/// define a type alias!
///
/// ```
/// # use crate::GridCell;
/// type GalacticGrid = GridCell<i64>;
/// ```
///
#[derive(Component, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Reflect)]
#[reflect(Component, Default, PartialEq)]
pub struct GridCell<P: GridPrecision> {
    /// The x-index of the cell.
    pub x: P,
    /// The y-index of the cell.
    pub y: P,
    /// The z-index of the cell.
    pub z: P,
}

impl<P: GridPrecision> GridCell<P> {
    /// Construct a new [`GridCell`].
    pub fn new(x: P, y: P, z: P) -> Self {
        Self { x, y, z }
    }

    /// The origin [`GridCell`].
    pub const ZERO: Self = GridCell {
        x: P::ZERO,
        y: P::ZERO,
        z: P::ZERO,
    };

    /// A unit value [`GridCell`]. Useful for offsets.
    pub const ONE: Self = GridCell {
        x: P::ONE,
        y: P::ONE,
        z: P::ONE,
    };
}
impl<P: GridPrecision> std::ops::Add for GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add(rhs.x),
            y: self.y.wrapping_add(rhs.y),
            z: self.z.wrapping_add(rhs.z),
        }
    }
}
impl<P: GridPrecision> std::ops::Sub for GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_sub(rhs.x),
            y: self.y.wrapping_sub(rhs.y),
            z: self.z.wrapping_sub(rhs.z),
        }
    }
}
impl<P: GridPrecision> std::ops::Add for &GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        (*self).add(*rhs)
    }
}
impl<P: GridPrecision> std::ops::Sub for &GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl<P: GridPrecision> std::ops::AddAssign for GridCell<P> {
    fn add_assign(&mut self, rhs: Self) {
        use std::ops::Add;
        *self = self.add(rhs);
    }
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
    query.par_for_each_mut(1024, |(mut grid_pos, mut transform)| {
        if transform.as_ref().translation.abs().max_element()
            > settings.maximum_distance_from_origin
        {
            let (grid_cell_delta, translation) =
                settings.imprecise_translation_to_grid(transform.as_ref().translation);
            *grid_pos = *grid_pos + grid_cell_delta;
            transform.translation = translation;
        }
    });
}

/// Compute the `GlobalTransform` relative to the floating origin.
pub fn update_global_from_grid<P: GridPrecision>(
    settings: Res<FloatingOriginSettings>,
    origin: Query<(&GridCell<P>, Changed<GridCell<P>>), With<FloatingOrigin>>,
    mut entities: ParamSet<(
        Query<
            (&Transform, &mut GlobalTransform, &GridCell<P>),
            Or<(Changed<GridCell<P>>, Changed<Transform>)>,
        >,
        Query<(&Transform, &mut GlobalTransform, &GridCell<P>)>,
    )>,
) {
    let (origin_cell, origin_grid_pos_changed) = origin.single();

    if origin_grid_pos_changed {
        let mut all_entities = entities.p1();
        all_entities.par_for_each_mut(1024, |(local, global, entity_cell)| {
            update_global_from_cell_local(&settings, entity_cell, origin_cell, local, global);
        });
    } else {
        let mut moved_cell_entities = entities.p0();
        moved_cell_entities.par_for_each_mut(1024, |(local, global, entity_cell)| {
            update_global_from_cell_local(&settings, entity_cell, origin_cell, local, global);
        });
    }
}

fn update_global_from_cell_local<P: GridPrecision>(
    settings: &FloatingOriginSettings,
    entity_cell: &GridCell<P>,
    origin_cell: &GridCell<P>,
    local: &Transform,
    mut global: Mut<GlobalTransform>,
) {
    let grid_cell_delta = entity_cell - origin_cell;
    *global = local
        .with_translation(settings.grid_position(&grid_cell_delta, local))
        .into();
}

/// Update [`GlobalTransform`] component of entities based on entity hierarchy and
/// [`Transform`] component.
pub fn transform_propagate_system<P: GridPrecision>(
    origin_moved: Query<(), (Changed<GridCell<P>>, With<FloatingOrigin>)>,
    mut root_query_no_grid: Query<
        (
            Option<(&Children, Changed<Children>)>,
            &Transform,
            Changed<Transform>,
            &mut GlobalTransform,
            Entity,
        ),
        (Without<GridCell<P>>, Without<Parent>),
    >,
    mut root_query_grid: Query<
        (
            Option<(&Children, Changed<Children>)>,
            Changed<Transform>,
            Changed<GridCell<P>>,
            &GlobalTransform,
            Entity,
        ),
        (With<GridCell<P>>, Without<Parent>),
    >,
    mut transform_query: Query<(
        &Transform,
        Changed<Transform>,
        &mut GlobalTransform,
        &Parent,
    )>,
    children_query: Query<(&Children, Changed<Children>), (With<Parent>, With<GlobalTransform>)>,
) {
    let origin_cell_changed = !origin_moved.is_empty();

    for (children, transform, transform_changed, mut global_transform, entity) in
        root_query_no_grid.iter_mut()
    {
        let mut changed = transform_changed || origin_cell_changed;

        if transform_changed {
            *global_transform = GlobalTransform::from(*transform);
        }

        if let Some((children, changed_children)) = children {
            // If our `Children` has changed, we need to recalculate everything below us
            changed |= changed_children;
            for child in children {
                let _ = propagate_recursive(
                    &global_transform,
                    &mut transform_query,
                    &children_query,
                    *child,
                    entity,
                    changed,
                );
            }
        }
    }

    for (children, cell_changed, transform_changed, global_transform, entity) in
        root_query_grid.iter_mut()
    {
        let mut changed = transform_changed || cell_changed || origin_cell_changed;

        if let Some((children, changed_children)) = children {
            // If our `Children` has changed, we need to recalculate everything below us
            changed |= changed_children;
            for child in children {
                let _ = propagate_recursive(
                    global_transform,
                    &mut transform_query,
                    &children_query,
                    *child,
                    entity,
                    changed,
                );
            }
        }
    }
}

fn propagate_recursive(
    parent: &GlobalTransform,
    transform_query: &mut Query<(
        &Transform,
        Changed<Transform>,
        &mut GlobalTransform,
        &Parent,
    )>,
    children_query: &Query<(&Children, Changed<Children>), (With<Parent>, With<GlobalTransform>)>,
    entity: Entity,
    expected_parent: Entity,
    mut changed: bool,
    // We use a result here to use the `?` operator. Ideally we'd use a try block instead
) -> Result<(), ()> {
    let global_matrix = {
        let (transform, transform_changed, mut global_transform, child_parent) =
            transform_query.get_mut(entity).map_err(drop)?;
        // Note that for parallelising, this check cannot occur here, since there is an `&mut GlobalTransform` (in global_transform)
        assert_eq!(
            child_parent.get(), expected_parent,
            "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
        );
        changed |= transform_changed;
        if changed {
            *global_transform = parent.mul_transform(*transform);
        }
        *global_transform
    };

    let (children, changed_children) = children_query.get(entity).map_err(drop)?;
    // If our `Children` has changed, we need to recalculate everything below us
    changed |= changed_children;
    for child in children {
        let _ = propagate_recursive(
            &global_matrix,
            transform_query,
            children_query,
            *child,
            entity,
            changed,
        );
    }
    Ok(())
}
