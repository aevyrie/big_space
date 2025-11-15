//! Detect and update groups of nearby occupied cells. See [`PartitionPlugin`].

use crate::hash::{SpatialHashFilter, SpatialHashSystems};
use crate::partition::map::PartitionLookup;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use core::{hash::Hash, marker::PhantomData};

pub mod change_tracking;
pub mod map;
mod private;
mod tests;

pub use private::Partition;

/// Adds support for spatial partitioning. Requires [`GridHashPlugin`](super::CellHashingPlugin).
pub struct PartitionPlugin<F = ()>(PhantomData<F>)
where
    F: SpatialHashFilter;

impl<F> PartitionPlugin<F>
where
    F: SpatialHashFilter,
{
    /// Create a new instance of [`PartitionPlugin`].
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl Default for PartitionPlugin<()> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F> Plugin for PartitionPlugin<F>
where
    F: SpatialHashFilter,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<PartitionLookup<F>>().add_systems(
            PostUpdate,
            PartitionLookup::<F>::update
                .in_set(SpatialHashSystems::UpdatePartitionLookup)
                .after(SpatialHashSystems::UpdateCellLookup),
        );
    }
}

/// Uniquely identifies a [`Partition`] in the [`PartitionLookup`] resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionId(u64);

impl PartitionId {
    /// The inner partition id.
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl Hash for PartitionId {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}
