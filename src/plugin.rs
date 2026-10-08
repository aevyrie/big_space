//! The bevy plugin for `big_space`.

use crate::*;
use bevy_app::{prelude::*, PluginGroupBuilder};
use bevy_ecs::prelude::*;
#[cfg(feature = "debug")]
use core::marker::PhantomData;

pub use crate::{timing::BigSpaceTimingStatsPlugin, validation::BigSpaceValidationPlugin};
#[cfg(feature = "camera")]
pub use camera::BigSpaceCameraControllerPlugin;
#[cfg(feature = "debug")]
pub use debug::BigSpaceDebugPlugin;

/// Set of plugins needed for bare-bones floating origin functionality.
pub struct BigSpaceMinimalPlugins;

impl PluginGroup for BigSpaceMinimalPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(BigSpaceCorePlugin)
            .add(BigSpacePropagationPlugin)
    }
}

/// All plugins needed for the core functionality of [`big_space`](crate).
///
/// By default,
/// - `BigSpaceValidationPlugin` is enabled in debug, and disabled in release.
/// - `BigSpaceCameraControllerPlugin` is enabled if the `camera` feature is enabled.
///
/// Hierarchy validation is not behind a feature flag because it does not add dependencies.
///
/// Debug visualization is not included here. It requires spatial hashing, which has runtime
/// cost that should be an explicit opt-in; add [`BigSpaceDebugPlugins`] alongside this group
/// when you want debug gizmos.
pub struct BigSpaceDefaultPlugins;

impl PluginGroup for BigSpaceDefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group
            .add_group(BigSpaceMinimalPlugins)
            .add(BigSpaceStationaryPlugin)
            .add(BigSpaceTimingStatsPlugin);

        #[cfg(any(debug_assertions, feature = "debug"))]
        {
            group = group.add(BigSpaceValidationPlugin);
        }
        #[cfg(feature = "camera")]
        {
            group = group.add(BigSpaceCameraControllerPlugin::default());
        }
        group
    }
}

/// Bundles [`BigSpaceDebugPlugin<F>`] with its spatial-hashing dependencies so cell gizmos
/// work out of the box. Spatial hashing has per-frame cost, so this group is kept separate
/// from [`BigSpaceDefaultPlugins`] and must be added explicitly.
///
/// Defaults to the unfiltered case (`F = ()`); use the turbofish form
/// (`BigSpaceDebugPlugins::<With<Foo>>::default()`) to bundle filtered variants of all three
/// plugins so their [`SpatialHashFilter`](crate::hash::SpatialHashFilter) parameters match.
///
/// If the app already installs [`CellHashingPlugin<F>`] or [`PartitionPlugin<F>`] with the
/// same filter, add [`BigSpaceDebugPlugin<F>`] directly instead of this group to avoid adding
/// the same plugin twice (which Bevy rejects).
#[cfg(feature = "debug")]
pub struct BigSpaceDebugPlugins<F: hash::SpatialHashFilter = ()>(PhantomData<F>);

#[cfg(feature = "debug")]
impl<F: hash::SpatialHashFilter> BigSpaceDebugPlugins<F> {
    /// Construct the group with an explicit [`SpatialHashFilter`](crate::hash::SpatialHashFilter).
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

// Matches the `Default` shape on `CellHashingPlugin`/`PartitionPlugin`: only the unfiltered
// case is `Default`, so a bare `BigSpaceDebugPlugins::default()` resolves to `<()>` without
// needing a turbofish. Filtered variants use `BigSpaceDebugPlugins::<With<Foo>>::new()`.
#[cfg(feature = "debug")]
impl Default for BigSpaceDebugPlugins<()> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[cfg(feature = "debug")]
impl<F: hash::SpatialHashFilter> PluginGroup for BigSpaceDebugPlugins<F> {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(CellHashingPlugin::<F>::new())
            .add(PartitionPlugin::<F>::new())
            .add(BigSpaceDebugPlugin::<F>::new())
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

/// Core setup needed for all uses of big space - reflection and grid cell recentering.
///
/// This plugin does not handle any transform propagation it only maintains the local transforms and
/// grid cells.
pub struct BigSpaceCorePlugin;
impl Plugin for BigSpaceCorePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<TransformTreeChanged>()
            .register_type::<CellCoord>()
            .register_type::<Grid>()
            .register_type::<BigSpace>()
            .register_type::<FloatingOrigin>()
            .add_systems(
                PostUpdate,
                CellCoord::recenter_large_transforms
                    .in_set(BigSpaceSystems::RecenterLargeTransforms),
            );
    }

    fn cleanup(&self, app: &mut App) {
        if app.is_plugin_added::<TransformPlugin>() {
            panic!("\nERROR: Bevy's default transformation plugin must be disabled while using `big_space`: \n\tDefaultPlugins.build().disable::<TransformPlugin>();\n");
        }
    }
}

/// Adds transform propagation, computing `GlobalTransforms` from hierarchies of [`Transform`],
/// [`CellCoord`], [`Grid`], and [`BigSpace`]s.
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
                    .after(BigSpaceSystems::Init)
                    .before(BigSpaceSystems::PropagateLowPrecision),
                BigSpace::find_floating_origin.in_set(BigSpaceSystems::RecenterLargeTransforms),
                LocalFloatingOrigin::compute_all
                    .in_set(BigSpaceSystems::LocalFloatingOrigins)
                    .after(BigSpaceSystems::RecenterLargeTransforms),
                Grid::propagate_root_grids
                    .in_set(BigSpaceSystems::PropagateHighPrecision)
                    .after(BigSpaceSystems::LocalFloatingOrigins),
                Grid::propagate_low_precision
                    .in_set(BigSpaceSystems::PropagateLowPrecision)
                    .after(BigSpaceSystems::PropagateHighPrecision),
            )
                .in_set(TransformSystems::Propagate)
        };

        #[cfg(feature = "std")]
        let hp_system = || {
            Grid::propagate_high_precision_channeled
                .in_set(BigSpaceSystems::PropagateHighPrecision)
                .after(BigSpaceSystems::LocalFloatingOrigins)
                .in_set(TransformSystems::Propagate)
        };
        #[cfg(not(feature = "std"))]
        let hp_system = || {
            Grid::propagate_high_precision
                .in_set(BigSpaceSystems::PropagateHighPrecision)
                .after(BigSpaceSystems::LocalFloatingOrigins)
                .in_set(TransformSystems::Propagate)
        };

        app.add_systems(PostStartup, (configs(), hp_system()))
            .add_systems(PostUpdate, (configs(), hp_system()));

        // These are the bevy transform propagation systems. Because these start from the root
        // of the hierarchy, and BigSpace bundles (at the root) do not contain a Transform,
        // these systems will not interact with any high-precision entities in `big_space`. These
        // systems are added for ecosystem compatibility with bevy, although the rendered
        // behavior might look strange if they share a camera with one using the floating
        // origin.
        //
        // This is most useful for bevy_ui, which relies on the transform systems to work, or if
        // you want to render a camera that only needs to render a low-precision scene.
        app.add_systems(
            PostStartup,
            (
                bevy_compat::propagate_parent_transforms,
                bevy_transform::systems::sync_simple_transforms,
            )
                .in_set(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            (
                bevy_compat::propagate_parent_transforms,
                bevy_transform::systems::sync_simple_transforms,
            )
                .in_set(TransformSystems::Propagate),
        );
    }
}
