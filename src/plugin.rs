//! The bevy plugins for `big_space`. Most use cases should use [`BigSpaceDefaultPlugins`].

use crate::*;
use bevy_app::{prelude::*, PluginGroupBuilder};
use bevy_ecs::prelude::*;

/// Core setup needed for all uses of big space.
///
/// This plugin does not handle transform propagation, it only maintains the local transforms and
/// grid cells. Features like hashing and partitioning do not rely on propagation, that is only
/// needed when rendering the world.
pub struct BigSpaceCorePlugin;

impl Plugin for BigSpaceCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            CellCoord::recenter_large_transforms.in_set(BigSpaceSystems::RecenterLargeTransforms),
        );
    }

    fn cleanup(&self, app: &mut App) {
        if app.is_plugin_added::<TransformPlugin>() {
            panic!("\nERROR: Bevy's default transformation plugin must be disabled while using `big_space`: \n\tDefaultPlugins.build().disable::<TransformPlugin>();\n");
        }
    }
}

/// Set of plugins needed for bare-bones floating origin functionality.
pub struct BigSpaceMinimalPlugins;

impl PluginGroup for BigSpaceMinimalPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(BigSpaceCorePlugin)
            .add(propagation::BigSpacePropagationPlugin)
    }
}

/// All plugins needed for the core functionality of [`big_space`](crate).
///
/// By default,
/// - `BigSpaceValidationPlugin` is enabled in `debug` (feature or profile).
/// - `BigSpaceDebugPlugin` is enabled if the `debug` feature is enabled.
/// - `BigSpaceCameraControllerPlugin` is enabled if the `camera` feature is enabled.
///
/// Hierarchy validation is not behind a feature flag because it does not add dependencies.
pub struct BigSpaceDefaultPlugins;

impl PluginGroup for BigSpaceDefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group
            .add_group(BigSpaceMinimalPlugins)
            .add(timing::BigSpaceTimingStatsPlugin);

        #[cfg(any(debug_assertions, feature = "debug"))]
        {
            group = group.add(validation::BigSpaceValidationPlugin);
        }
        #[cfg(feature = "debug")]
        {
            group = group.add(debug::BigSpaceDebugPlugin);
        }
        #[cfg(feature = "camera")]
        {
            group = group.add(camera::BigSpaceCameraControllerPlugin);
        }
        group
    }
}

#[allow(missing_docs)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum BigSpaceSystems {
    Init,
    RecenterLargeTransforms,
    LocalFloatingOrigins,
    PropagateHighPrecision,
    PropagateLowPrecision,
}
