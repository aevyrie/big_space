//! Timing statistics for transform propagation

use std::{collections::VecDeque, iter::Sum, ops::Div, time::Duration};

use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_reflect::prelude::*;
use bevy_transform::TransformSystem;

/// Summarizes plugin performance timings
pub struct TimingStatsPlugin;

impl Plugin for TimingStatsPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.init_resource::<PropagationStats>()
            .register_type::<PropagationStats>()
            .init_resource::<SpatialHashStats>()
            .register_type::<SpatialHashStats>()
            .init_resource::<SmoothedStat<PropagationStats>>()
            .register_type::<SmoothedStat<PropagationStats>>()
            .init_resource::<SmoothedStat<SpatialHashStats>>()
            .register_type::<SmoothedStat<SpatialHashStats>>()
            .add_systems(
                PostUpdate,
                (SpatialHashStats::reset, PropagationStats::reset)
                    .in_set(FloatingOriginSystem::Init),
            )
            .add_systems(
                PostUpdate,
                (update_totals, update_averages)
                    .chain()
                    .after(TransformSystem::TransformPropagate),
            );
    }
}

fn update_totals(
    mut prop_stats: ResMut<PropagationStats>,
    mut hash_stats: ResMut<SpatialHashStats>,
) {
    prop_stats.total = prop_stats.grid_recentering
        + prop_stats.high_precision_propagation
        + prop_stats.local_origin_propagation
        + prop_stats.low_precision_propagation
        + prop_stats.low_precision_root_tagging;

    hash_stats.total = hash_stats.hash_update_duration + hash_stats.map_update_duration;
}

fn update_averages(
    hash_stats: Res<SpatialHashStats>,
    mut avg_hash_stats: ResMut<SmoothedStat<SpatialHashStats>>,
    prop_stats: Res<PropagationStats>,
    mut avg_prop_stats: ResMut<SmoothedStat<PropagationStats>>,
) {
    avg_hash_stats.push(hash_stats.clone()).compute_avg();
    avg_prop_stats.push(prop_stats.clone()).compute_avg();
}

/// Aggregate runtime statistics for transform propagation.
#[derive(Resource, Debug, Clone, Default, Reflect)]
pub struct PropagationStats {
    pub(crate) grid_recentering: Duration,
    pub(crate) local_origin_propagation: Duration,
    pub(crate) high_precision_propagation: Duration,
    pub(crate) low_precision_root_tagging: Duration,
    pub(crate) low_precision_propagation: Duration,
    pub(crate) total: Duration,
}

impl PropagationStats {
    pub(crate) fn reset(mut stats: ResMut<Self>) {
        *stats = Self::default();
    }

    /// How long it took to run
    /// [`recenter_large_transforms`](crate::grid_cell::GridCell::recenter_large_transforms)
    /// propagation this update.
    pub fn grid_recentering(&self) -> Duration {
        self.grid_recentering
    }

    /// How long it took to run
    /// [`LocalFloatingOrigin`](crate::reference_frame::local_origin::LocalFloatingOrigin)
    /// propagation this update.
    pub fn local_origin_propagation(&self) -> Duration {
        self.local_origin_propagation
    }

    /// How long it took to run high precision
    /// [`Transform`](bevy_transform::prelude::Transform)+[`GridCell`](crate::grid_cell::GridCell)
    /// propagation this update.
    pub fn high_precision_propagation(&self) -> Duration {
        self.high_precision_propagation
    }

    /// How long it took to run low precision [`Transform`](bevy_transform::prelude::Transform)
    /// propagation this update.
    pub fn low_precision_propagation(&self) -> Duration {
        self.low_precision_propagation
    }

    /// How long it took to tag entities with
    /// [`LowPrecisionRoot`](crate::reference_frame::propagation::LowPrecisionRoot).
    pub fn low_precision_root_tagging(&self) -> Duration {
        self.low_precision_root_tagging
    }

    /// Total propagation time.
    pub fn total(&self) -> Duration {
        self.total
    }
}

impl<'a> std::iter::Sum<&'a PropagationStats> for PropagationStats {
    fn sum<I: Iterator<Item = &'a PropagationStats>>(iter: I) -> Self {
        iter.fold(PropagationStats::default(), |mut acc, e| {
            acc.grid_recentering += e.grid_recentering;
            acc.local_origin_propagation += e.local_origin_propagation;
            acc.high_precision_propagation += e.high_precision_propagation;
            acc.low_precision_propagation += e.low_precision_propagation;
            acc.low_precision_root_tagging += e.low_precision_root_tagging;
            acc.total += e.total;
            acc
        })
    }
}

impl std::ops::Div<u32> for PropagationStats {
    type Output = Self;

    fn div(self, rhs: u32) -> Self::Output {
        Self {
            grid_recentering: self.grid_recentering.div(rhs),
            local_origin_propagation: self.local_origin_propagation.div(rhs),
            high_precision_propagation: self.high_precision_propagation.div(rhs),
            low_precision_root_tagging: self.low_precision_root_tagging.div(rhs),
            low_precision_propagation: self.low_precision_propagation.div(rhs),
            total: self.total.div(rhs),
        }
    }
}

/// Aggregate runtime statistics across all [`crate::spatial_hash::SpatialHashPlugin`]s.
#[derive(Resource, Debug, Clone, Default, Reflect)]
pub struct SpatialHashStats {
    pub(crate) hash_update_duration: Duration,
    pub(crate) map_update_duration: Duration,
    pub(crate) moved_entities: usize,
    pub(crate) total: Duration,
}

impl SpatialHashStats {
    fn reset(mut stats: ResMut<SpatialHashStats>) {
        *stats = Self::default();
    }

    /// Time to update all entity hashes.
    pub fn hash_update_duration(&self) -> Duration {
        self.hash_update_duration
    }

    /// Time to update all spatial hash maps.
    pub fn map_update_duration(&self) -> Duration {
        self.map_update_duration
    }

    /// Number of entities with a changed spatial hash (moved to a new grid cell).
    pub fn moved_cell_entities(&self) -> usize {
        self.moved_entities
    }

    /// Total runtime cost of spatial hashing.
    pub fn total(&self) -> Duration {
        self.total
    }
}

impl<'a> std::iter::Sum<&'a SpatialHashStats> for SpatialHashStats {
    fn sum<I: Iterator<Item = &'a SpatialHashStats>>(iter: I) -> Self {
        iter.fold(SpatialHashStats::default(), |mut acc, e| {
            acc.hash_update_duration += e.hash_update_duration;
            acc.map_update_duration += e.map_update_duration;
            acc.moved_entities += e.moved_entities;
            acc.total += e.total;
            acc
        })
    }
}

impl Div<u32> for SpatialHashStats {
    type Output = Self;

    fn div(self, rhs: u32) -> Self::Output {
        Self {
            hash_update_duration: self.hash_update_duration.div(rhs),
            map_update_duration: self.map_update_duration.div(rhs),
            moved_entities: self.moved_entities.div(rhs as usize),
            total: self.total.div(rhs),
        }
    }
}

/// Smoothed timing statistics
#[derive(Resource, Debug, Reflect)]
pub struct SmoothedStat<T>
where
    for<'a> T: FromWorld + Sum<&'a T> + Div<u32, Output = T>,
{
    queue: VecDeque<T>,
    avg: T,
}

impl<T> FromWorld for SmoothedStat<T>
where
    for<'a> T: FromWorld + Sum<&'a T> + Div<u32, Output = T>,
{
    fn from_world(world: &mut World) -> Self {
        SmoothedStat {
            queue: VecDeque::new(),
            avg: T::from_world(world),
        }
    }
}

impl<T> SmoothedStat<T>
where
    for<'a> T: FromWorld + Sum<&'a T> + Div<u32, Output = T>,
{
    fn push(&mut self, value: T) -> &mut Self {
        self.queue.truncate(63);
        self.queue.push_front(value);
        self
    }

    fn compute_avg(&mut self) -> &mut Self {
        self.avg = self.queue.iter().sum::<T>() / self.queue.len() as u32;
        self
    }

    /// Get the smoothed average value.
    pub fn avg(&self) -> &T {
        &self.avg
    }
}
