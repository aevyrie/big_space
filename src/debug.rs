//! Contains tools for debugging the floating origin.

use crate::hash::SpatialHashFilter;
use crate::prelude::*;
use bevy_app::prelude::*;
#[cfg(feature = "bevy_camera")]
use bevy_camera::primitives::Frustum;
use bevy_color::prelude::*;
use bevy_ecs::prelude::*;
use bevy_gizmos::prelude::*;
use bevy_math::prelude::*;
use bevy_reflect::Reflect;
use bevy_transform::prelude::*;
use core::hash::Hasher;
use core::marker::PhantomData;

/// This plugin will render the bounds of occupied grid cells.
pub struct BigSpaceDebugPlugin<F: SpatialHashFilter = ()>(PhantomData<F>);

impl Default for BigSpaceDebugPlugin<()> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F: SpatialHashFilter> BigSpaceDebugPlugin<F> {
    /// Construct a new instance.
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<F: SpatialHashFilter> Plugin for BigSpaceDebugPlugin<F> {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<BigSpaceGizmoConfig>()
            .add_systems(Startup, setup_gizmos)
            .add_systems(
                PostUpdate,
                (update_debug_bounds::<F>, update_grid_axes)
                    .chain()
                    .after(TransformSystems::Propagate)
                    .after(SpatialHashSystems::UpdatePartitionLookup),
            );
    }
}

fn setup_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<BigSpaceGizmoConfig>();
    config.line.perspective = false;
    config.line.joints = GizmoLineJoint::Round(4);
    config.line.width = 1.0;
}

const MAX_PARTITIONS: usize = 100;
const MAX_CELLS_PER_PARTITION: usize = 100;

/// Build a [`bevy_camera::primitives::Aabb`] for a partition in grid-local space, accounting for
/// the cell edge length padding.
#[cfg(feature = "bevy_camera")]
fn partition_local_aabb(partition: &Partition, l: f32) -> bevy_camera::primitives::Aabb {
    let min = IVec3::from([
        partition.min().x as i32,
        partition.min().y as i32,
        partition.min().z as i32,
    ])
    .as_vec3()
        * l
        - l;
    let max = IVec3::from([
        partition.max().x as i32,
        partition.max().y as i32,
        partition.max().z as i32,
    ])
    .as_vec3()
        * l
        + l;
    bevy_camera::primitives::Aabb::from_min_max(min, max)
}

/// Update the rendered debug bounds for the nearest partitions to the floating origin.
fn update_debug_bounds<F: SpatialHashFilter>(
    mut gizmos: Gizmos,
    partitions: Option<Res<PartitionLookup<F>>>,
    grids: Query<(&GlobalTransform, &Grid)>,
    origins: Query<(&CellCoord, &ChildOf), With<FloatingOrigin>>,
    #[cfg(feature = "bevy_camera")] cameras: Query<&Frustum, With<bevy_camera::Camera>>,
) -> Result {
    let Some(partitions) = partitions else {
        return Ok(());
    };
    let Ok((origin_cell, origin_parent)) = origins.single() else {
        return Ok(());
    };

    #[cfg(feature = "bevy_camera")]
    let frustum = cameras.iter().next();

    let oc = IVec3::new(
        origin_cell.x as i32,
        origin_cell.y as i32,
        origin_cell.z as i32,
    );

    // Sort partitions by distance from the floating origin.
    let mut sorted_pids: Vec<_> = partitions
        .iter()
        .filter(|(_, p)| p.grid() == origin_parent.parent())
        .map(|(&pid, p)| {
            let min = IVec3::new(p.min().x as i32, p.min().y as i32, p.min().z as i32);
            let max = IVec3::new(p.max().x as i32, p.max().y as i32, p.max().z as i32);
            let clamped = oc.clamp(min, max);
            let diff = (clamped - oc).as_i64vec3();
            let dist_sq = diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;
            (pid, dist_sq)
        })
        .collect();
    sorted_pids.sort_unstable_by_key(|(_, d)| *d);
    sorted_pids.truncate(MAX_PARTITIONS);

    for (pid, _) in &sorted_pids {
        let Some(p) = partitions.resolve(pid) else {
            continue;
        };
        let Ok((transform, grid)) = grids.get(p.grid()) else {
            continue;
        };
        let l = grid.cell_edge_length();

        // Frustum cull: skip partitions not visible to any camera.
        #[cfg(feature = "bevy_camera")]
        if let Some(frustum) = frustum {
            let aabb = partition_local_aabb(p, l);
            if !frustum.intersects_obb(&aabb, &transform.affine(), true, false) {
                continue;
            }
        }

        let mut hasher = bevy_ecs::entity::EntityHasher::default();
        hasher.write_u64(pid.id());
        let hue = (hasher.finish() % 360) as f32;

        // Draw individual occupied cells.
        for h in p.iter().take(MAX_CELLS_PER_PARTITION) {
            let center = [h.coord().x as i32, h.coord().y as i32, h.coord().z as i32];
            let local_trans = Transform::from_translation(IVec3::from(center).as_vec3() * l)
                .with_scale(Vec3::splat(l));
            gizmos.cube(
                transform.mul_transform(local_trans),
                Hsla::new(hue, 1.0, 0.5, 0.6),
            );
        }

        // Draw partition AABB.
        let min = IVec3::from([p.min().x as i32, p.min().y as i32, p.min().z as i32]).as_vec3() * l;
        let max = IVec3::from([p.max().x as i32, p.max().y as i32, p.max().z as i32]).as_vec3() * l;
        let size = max - min;
        let center = min + size * 0.5;
        let local_trans = Transform::from_translation(center).with_scale(size + l * 2.0);

        gizmos.cube(
            transform.mul_transform(local_trans),
            Hsla::new(hue, 1.0, 0.5, 0.6),
        );
    }

    Ok(())
}

#[derive(Default, Reflect)]
struct BigSpaceGizmoConfig;

impl GizmoConfigGroup for BigSpaceGizmoConfig {}

/// Draw axes for grids.
fn update_grid_axes(
    mut gizmos: Gizmos<BigSpaceGizmoConfig>,
    grids: Query<(&GlobalTransform, &Grid)>,
) {
    for (transform, grid) in grids.iter() {
        let start = transform.translation();
        // Scale with distance
        let len = (start.length().powf(0.9)).max(grid.cell_edge_length()) * 0.5;
        gizmos.ray(
            start,
            transform.right() * len,
            Color::linear_rgb(1.0, 0.0, 0.0),
        );
        gizmos.ray(
            start,
            transform.up() * len,
            Color::linear_rgb(0.0, 1.0, 0.0),
        );
        gizmos.ray(
            start,
            transform.back() * len,
            Color::linear_rgb(0.0, 0.0, 1.0),
        );
    }
}
