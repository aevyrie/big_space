//! The bevy plugin for `big_space`.

use crate::{prelude::*, timing::BigSpaceTimingStatsPlugin, validation::BigSpaceValidationPlugin};
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_transform::prelude::*;

/// All plugins needed for the core functionality of [`big_space`](crate).
///
/// By default,
/// - Hierarchy validation is enabled in debug, and disabled in release.
/// - Debug gizmos are enabled if the feature is enabled.
///
/// This functionality can be disabled manually in this struct.
///
/// The debug plugin is behind a feature flag because, unlike hierarchy validation, it pulls in new
/// dependencies.
pub struct BigSpacePlugin {
    /// Enables runtime validation of `big_space` hierarchies, generating a detailed report on error
    /// using [`BigSpaceValidationPlugin`].
    pub validation: bool,
    /// Enables debug gizmos to illustrate grid axes and occupied cells using
    /// [`BigSpaceDebugPlugin`].
    #[cfg(feature = "debug")]
    pub debug: bool,
}

impl BigSpacePlugin {
    /// Enable runtime hierarchy validation. See [`BigSpaceValidationPlugin`].
    pub fn with_validation(mut self) -> Self {
        self.validation = true;
        self
    }

    /// Enable debug gizmos. See [`BigSpaceDebugPlugin`].
    #[cfg(feature = "debug")]
    pub fn with_debug(mut self) -> Self {
        self.debug = true;
        self
    }
}

impl Default for BigSpacePlugin {
    fn default() -> Self {
        Self {
            validation: cfg!(debug_assertions),
            #[cfg(feature = "debug")]
            debug: true,
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum FloatingOriginSystem {
    Init,
    RecenterLargeTransforms,
    LocalFloatingOrigins,
    PropagateHighPrecision,
    PropagateLowPrecision,
}

impl Plugin for BigSpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BigSpaceMinimalPlugin,
            BigSpacePropagationPlugin,
            BigSpaceTimingStatsPlugin,
        ));
        if self.validation {
            app.add_plugins(BigSpaceValidationPlugin);
        }
        #[cfg(feature = "debug")]
        if self.debug {
            app.add_plugins(BigSpaceDebugPlugin);
        }
    }
}

/// Core setup needed for all uses of big space - reflection and grid cell recentering.
pub struct BigSpaceMinimalPlugin;
impl Plugin for BigSpaceMinimalPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<TransformTreeChanged>()
            .register_type::<GridCell>()
            .register_type::<Grid>()
            .register_type::<BigSpace>()
            .register_type::<FloatingOrigin>()
            .add_systems(
                PostUpdate,
                GridCell::recenter_large_transforms
                    .in_set(FloatingOriginSystem::RecenterLargeTransforms),
            );
    }
}

/// Adds transform propagation, computing `GlobalTransforms` from hierarchies of [`Transform`],
/// [`GridCell`], [`Grid`], and [`BigSpace`]s.
///
/// Disable Bevy's [`TransformPlugin`] while using this plugin.
///
/// This also adds support for Bevy's low-precision [`Transform`] hierarchies.
pub struct BigSpacePropagationPlugin;
impl Plugin for BigSpacePropagationPlugin {
    fn build(&self, app: &mut App) {
        let configs = || {
            (
                Grid::tag_low_precision_roots // loose ordering on this set
                    .after(FloatingOriginSystem::Init)
                    .before(FloatingOriginSystem::PropagateLowPrecision),
                BigSpace::find_floating_origin
                    .in_set(FloatingOriginSystem::RecenterLargeTransforms),
                LocalFloatingOrigin::compute_all
                    .in_set(FloatingOriginSystem::LocalFloatingOrigins)
                    .after(FloatingOriginSystem::RecenterLargeTransforms),
                Grid::propagate_high_precision
                    .in_set(FloatingOriginSystem::PropagateHighPrecision)
                    .after(FloatingOriginSystem::LocalFloatingOrigins),
                Grid::propagate_low_precision
                    .in_set(FloatingOriginSystem::PropagateLowPrecision)
                    .after(FloatingOriginSystem::PropagateHighPrecision),
            )
                .in_set(TransformSystem::TransformPropagate)
        };

        app.add_systems(PostStartup, configs())
            .add_systems(PostUpdate, configs());

        // These are the bevy transform propagation systems. Because these start from the root
        // of the hierarchy, and BigSpace bundles (at the root) do not contain a Transform,
        // these systems will not interact with any high precision entities in big space. These
        // systems are added for ecosystem compatibility with bevy, although the rendered
        // behavior might look strange if they share a camera with one using the floating
        // origin.
        //
        // This is most useful for bevy_ui, which relies on the transform systems to work, or if
        // you want to render a camera that only needs to render a low-precision scene.
        app.add_systems(
            PostStartup,
            (
                crate::bevy_compat::propagate_parent_transforms,
                bevy_transform::systems::sync_simple_transforms,
            )
                .in_set(TransformSystem::TransformPropagate),
        )
        .add_systems(
            PostUpdate,
            (
                crate::bevy_compat::propagate_parent_transforms,
                bevy_transform::systems::sync_simple_transforms,
            )
                .in_set(TransformSystem::TransformPropagate),
        );
    }
}
