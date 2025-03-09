//! The bevy plugin for big_space.

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_transform::prelude::*;

/// Add this plugin to your [`App`] for floating origin functionality.
pub struct BigSpacePlugin {
    validate_hierarchies: bool,
}

impl BigSpacePlugin {
    /// Create a big space plugin, and specify whether hierarchy validation should be enabled.
    pub fn new(validate_hierarchies: bool) -> Self {
        Self {
            validate_hierarchies,
        }
    }
}

impl Default for BigSpacePlugin {
    fn default() -> Self {
        Self {
            validate_hierarchies: cfg!(debug_assertions),
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
        // Silence bevy's built-in error spam about GlobalTransforms in the hierarchy
        // app.insert_resource(ReportHierarchyIssue::<GlobalTransform>::new(false));

        // Performance timings
        app.add_plugins(crate::timing::TimingStatsPlugin);

        let system_set_config = || {
            (
                Grid::tag_low_precision_roots // loose ordering on this set
                    .after(FloatingOriginSystem::Init)
                    .before(FloatingOriginSystem::PropagateLowPrecision),
                (
                    GridCell::recenter_large_transforms,
                    BigSpace::find_floating_origin,
                )
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

        app
            // Reflect
            .register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<GridCell>()
            .register_type::<Grid>()
            .register_type::<BigSpace>()
            .register_type::<FloatingOrigin>()
            // Meat of the plugin, once on startup, as well as every update
            .add_systems(PostStartup, system_set_config())
            .add_systems(PostUpdate, system_set_config())
            // Validation
            .add_systems(
                PostUpdate,
                crate::validation::validate_hierarchy::<crate::validation::SpatialHierarchyRoot>
                    .after(TransformSystem::TransformPropagate)
                    .run_if({
                        let run = self.validate_hierarchies;
                        move || run
                    }),
            )
            // These are the bevy transform propagation systems. Because these start from the root
            // of the hierarchy, and BigSpace bundles (at the root) do not contain a Transform,
            // these systems will not interact with any high precision entities in big space. These
            // systems are added for ecosystem compatibility with bevy, although the rendered
            // behavior might look strange if they share a camera with one using the floating
            // origin.
            //
            // This is most useful for bevy_ui, which relies on the transform systems to work, or if
            // you want to render a camera that only needs to render a low-precision scene.
            .add_systems(
                PostStartup,
                (
                    bevy_transform::systems::sync_simple_transforms,
                    bevy_transform::systems::propagate_parent_transforms,
                )
                    .in_set(TransformSystem::TransformPropagate),
            )
            .add_systems(
                PostUpdate,
                (
                    bevy_transform::systems::sync_simple_transforms,
                    bevy_transform::systems::propagate_parent_transforms,
                )
                    .in_set(TransformSystem::TransformPropagate),
            );
    }
}
