//! The bevy plugin for big_space.

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{ScheduleLabel, SystemConfigs};
use bevy_transform::prelude::*;

/// Add this plugin to your [`App`] for floating origin functionality.
pub struct BigSpacePlugin {
    pub validate_hierarchies: bool,
    pub fixed_timestep: bool,
}

impl Default for BigSpacePlugin {
    fn default() -> Self {
        Self {
            validate_hierarchies: cfg!(debug_assertions),
            fixed_timestep: false,
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
            BigSpaceMinimalPlugin {
                fixed_timestep: self.fixed_timestep,
            },
            BigSpaceValidationPlugin {
                validate_hierarchies: self.validate_hierarchies,
                fixed_timestep: self.fixed_timestep,
            },
            BigSpacePropagationPlugin {
                fixed_timestep: self.fixed_timestep,
            },
            crate::timing::TimingStatsPlugin,
        ));
    }
}

/// Common setup needed for all uses of big space - reflection, timing, stock bevy-compatible
/// transform propagation.
pub struct BigSpaceMinimalPlugin {
    pub fixed_timestep: bool,
}
impl Plugin for BigSpaceMinimalPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<GridCell>()
            .register_type::<Grid>()
            .register_type::<BigSpace>()
            .register_type::<FloatingOrigin>();

        // Silence bevy's built-in error spam about GlobalTransforms in the hierarchy
        app.insert_resource(bevy_hierarchy::ReportHierarchyIssue::<GlobalTransform>::new(false));

        let recenter_grid_cells = GridCell::recenter_large_transforms
            .in_set(FloatingOriginSystem::RecenterLargeTransforms);

        if self.fixed_timestep {
            app.add_systems(FixedPostUpdate, recenter_grid_cells);
        } else {
            app.add_systems(PostUpdate, recenter_grid_cells);
        }
    }
}

pub struct BigSpaceValidationPlugin {
    validate_hierarchies: bool,
    fixed_timestep: bool,
}

impl Plugin for BigSpaceValidationPlugin {
    fn build(&self, app: &mut App) {
        let config =
            crate::validation::validate_hierarchy::<crate::validation::SpatialHierarchyRoot>
                .after(TransformSystem::TransformPropagate)
                .run_if({
                    let run = self.validate_hierarchies;
                    move || run
                });

        if self.fixed_timestep {
            app.add_systems(FixedPostUpdate, config);
        } else {
            app.add_systems(PostUpdate, config);
        }
    }
}

pub struct BigSpacePropagationPlugin {
    fixed_timestep: bool,
}

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

        app.add_systems(PostStartup, configs());

        if self.fixed_timestep {
            app.add_systems(FixedPostUpdate, configs());
        } else {
            app.add_systems(PostUpdate, configs());
        };

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
                bevy_transform::systems::sync_simple_transforms,
                bevy_transform::systems::propagate_transforms,
            )
                .in_set(TransformSystem::TransformPropagate),
        )
        .add_systems(
            PostUpdate,
            (
                bevy_transform::systems::sync_simple_transforms,
                bevy_transform::systems::propagate_transforms,
            )
                .in_set(TransformSystem::TransformPropagate),
        );
    }
}
