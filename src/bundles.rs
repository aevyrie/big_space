//! Component bundles for big_space.

use crate::*;

/// Minimal bundle needed to position an entity in floating origin space.
///
/// This is the floating origin equivalent of the [`bevy::prelude::SpatialBundle`].
#[derive(Bundle, Default)]
pub struct BigSpatialBundle<P: GridPrecision> {
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
    pub cell: GridCell<P>,
}

/// A [`SpatialBundle`] that also has a reference frame, allowing other high precision spatial
/// bundles to be nested within that reference frame.
///
/// This is the floating origin equivalent of the [`SpatialBundle`].
#[derive(Bundle, Default)]
pub struct BigReferenceFrameBundle<P: GridPrecision> {
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
    pub cell: GridCell<P>,
    /// The reference frame
    pub reference_frame: ReferenceFrame<P>,
}

/// Bundled needed for root reference frames.
#[derive(Bundle, Default)]
pub struct BigSpaceBundle<P: GridPrecision> {
    /// The visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub visibility: Visibility,
    /// The inherited visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub inherited: InheritedVisibility,
    /// The view visibility of the entity.
    #[cfg(feature = "bevy_render")]
    pub view: ViewVisibility,
    /// The root reference frame
    pub reference_frame: ReferenceFrame<P>,
    /// Tracks the current floating origin
    pub root: BigSpace,
}
