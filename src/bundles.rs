//! Component bundles for big_space.

use crate::{precision::GridPrecision, reference_frame::ReferenceFrame, BigSpace, GridCell};

use bevy_ecs::prelude::*;
use bevy_transform::prelude::*;

/// Minimal bundle needed to position an entity in floating origin space.
///
/// This is the floating origin equivalent of the `bevy` `SpatialBundle`.
#[derive(Bundle, Default)]
pub struct BigSpatialBundle<P: GridPrecision> {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub visibility: bevy_render::view::Visibility,
    /// The inherited visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub inherited: bevy_render::view::InheritedVisibility,
    /// The view visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub view: bevy_render::view::ViewVisibility,
    /// The transform of the entity.
    pub transform: Transform,
    /// The global transform of the entity.
    pub global_transform: GlobalTransform,
    /// The grid position of the entity
    pub cell: GridCell<P>,
}

/// A `SpatialBundle` that also has a reference frame, allowing other high precision spatial bundles
/// to be nested within that reference frame.
///
/// This is the floating origin equivalent of the `bevy` `SpatialBundle`.
#[derive(Bundle, Default)]
pub struct BigReferenceFrameBundle<P: GridPrecision> {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub visibility: bevy_render::view::Visibility,
    /// The inherited visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub inherited: bevy_render::view::InheritedVisibility,
    /// The view visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub view: bevy_render::view::ViewVisibility,
    /// The transform of the entity.
    pub transform: Transform,
    /// The global transform of the entity for rendering, computed relative to the floating origin.
    pub global_transform: GlobalTransform,
    /// The grid position of the entity within
    pub cell: GridCell<P>,
    /// The reference frame
    pub reference_frame: ReferenceFrame<P>,
}

/// The root of any [`BigSpace`] needs these components to function.
#[derive(Bundle, Default)]
pub struct BigSpaceRootBundle<P: GridPrecision> {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub visibility: bevy_render::view::Visibility,
    /// The inherited visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub inherited: bevy_render::view::InheritedVisibility,
    /// The view visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub view: bevy_render::view::ViewVisibility,
    /// The root reference frame
    pub reference_frame: ReferenceFrame<P>,
    /// Tracks the current floating origin
    pub root: BigSpace,
}
