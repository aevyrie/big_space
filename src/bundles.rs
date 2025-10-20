//! Component bundles for `big_space`.

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_transform::prelude::*;

/// Minimal bundle needed to position an entity in floating origin space.
///
/// This is the floating origin equivalent of the `bevy` `SpatialBundle`.
#[derive(Bundle, Default)]
pub struct BigSpatialBundle {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_camera")]
    pub visibility: bevy_camera::visibility::Visibility,
    /// The inherited visibility of the entity.
    #[cfg(feature = "bevy_camera")]
    pub inherited: bevy_camera::visibility::InheritedVisibility,
    /// The view visibility of the entity.
    #[cfg(feature = "bevy_camera")]
    pub view: bevy_camera::visibility::ViewVisibility,
    /// The transform of the entity.
    pub transform: Transform,
    /// The global transform of the entity.
    pub global_transform: GlobalTransform,
    /// The grid position of the entity
    pub cell: CellCoord,
}

/// A `SpatialBundle` that also has a grid, allowing other high precision spatial bundles to be
/// nested within that grid.
///
/// This is the floating origin equivalent of the `bevy` `SpatialBundle`.
#[derive(Bundle, Default)]
pub struct BigGridBundle {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_camera")]
    pub visibility: bevy_camera::visibility::Visibility,
    /// The transform of the entity.
    pub transform: Transform,
    /// The global transform of the entity for rendering, computed relative to the floating origin.
    pub global_transform: GlobalTransform,
    /// The grid position of the grid within its parent grid.
    pub cell: CellCoord,
    /// The grid.
    pub grid: Grid,
}

/// The root of any [`BigSpace`] needs these components to function.
#[derive(Bundle, Default)]
pub struct BigSpaceRootBundle {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_camera")]
    pub visibility: bevy_camera::visibility::Visibility,
    /// The root grid
    pub grid: Grid,
    /// The rendered position of the root grid relative to the floating origin.
    pub global_transform: GlobalTransform,
    /// Tracks the current floating origin.
    pub root: BigSpace,
}
