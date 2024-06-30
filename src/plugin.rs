//! The bevy plugin for big_space.

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_reflect::{prelude::*, GetTypeRegistration};
use bevy_transform::{prelude::*, TransformSystem};
use std::marker::PhantomData;

use crate::{
    precision::GridPrecision,
    reference_frame::{local_origin::LocalFloatingOrigin, ReferenceFrame},
    validation, BigSpace, FloatingOrigin, GridCell,
};

/// Add this plugin to your [`App`] for floating origin functionality.
pub struct BigSpacePlugin<P: GridPrecision> {
    phantom: PhantomData<P>,
    validate_hierarchies: bool,
}

impl<P: GridPrecision> BigSpacePlugin<P> {
    /// Create a big space plugin, and specify whether hierarchy validation should be enabled.
    pub fn new(validate_hierarchies: bool) -> Self {
        Self {
            phantom: PhantomData::<P>,
            validate_hierarchies,
        }
    }
}

impl<P: GridPrecision> Default for BigSpacePlugin<P> {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        let validate_hierarchies = true;

        #[cfg(not(debug_assertions))]
        let validate_hierarchies = false;

        Self {
            phantom: Default::default(),
            validate_hierarchies,
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum FloatingOriginSet {
    RecenterLargeTransforms,
    LocalFloatingOrigins,
    PropagateHighPrecision,
    PropagateLowPrecision,
}

impl<P: GridPrecision + Reflect + FromReflect + TypePath + GetTypeRegistration> Plugin
    for BigSpacePlugin<P>
{
    fn build(&self, app: &mut App) {
        let system_set_config = || {
            (
                (
                    GridCell::<P>::recenter_large_transforms,
                    BigSpace::find_floating_origin,
                )
                    .in_set(FloatingOriginSet::RecenterLargeTransforms),
                LocalFloatingOrigin::<P>::compute_all
                    .in_set(FloatingOriginSet::LocalFloatingOrigins)
                    .after(FloatingOriginSet::RecenterLargeTransforms),
                ReferenceFrame::<P>::propagate_high_precision
                    .in_set(FloatingOriginSet::PropagateHighPrecision)
                    .after(FloatingOriginSet::LocalFloatingOrigins),
                ReferenceFrame::<P>::propagate_low_precision
                    .in_set(FloatingOriginSet::PropagateLowPrecision)
                    .after(FloatingOriginSet::PropagateHighPrecision),
            )
                .in_set(TransformSystem::TransformPropagate)
        };

        app.register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<GridCell<P>>()
            .register_type::<ReferenceFrame<P>>()
            .register_type::<BigSpace>()
            .register_type::<FloatingOrigin>()
            .add_systems(PostStartup, system_set_config())
            .add_systems(PostUpdate, system_set_config())
            .add_systems(
                PostUpdate,
                validation::validate_hierarchy::<validation::SpatialHierarchyRoot<P>>
                    .before(TransformSystem::TransformPropagate)
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
