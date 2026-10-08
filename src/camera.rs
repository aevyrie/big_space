//! Provides a camera controller compatible with the floating origin plugin.

use crate::hash::map::CellLookup;
use crate::hash::SpatialHashFilter;
use crate::portable_par::PortableParallel;
use crate::prelude::*;
use bevy_app::prelude::*;
use bevy_camera::{
    primitives::Aabb,
    visibility::{InheritedVisibility, RenderLayers},
};
use bevy_ecs::entity::EntityHashSet;
use bevy_ecs::prelude::*;
use bevy_input::{mouse::MouseMotion, prelude::*};
use bevy_math::{prelude::*, DQuat, DVec3};
use bevy_platform::prelude::*;
use bevy_reflect::prelude::*;
use bevy_time::prelude::*;
use bevy_transform::{prelude::*, TransformSystems};
use core::marker::PhantomData;
use smallvec::SmallVec;

/// Runs the [`big_space`](crate) [`BigSpaceCameraController`].
///
/// The type parameter `F` is a [`SpatialHashFilter`] that determines which
/// [`PartitionLookup`] and [`CellLookup`] resources are used for the
/// partition-accelerated nearest-object search. When no matching resources
/// exist, the system falls back to a brute-force scan.
///
/// Defaults to `()` (unfiltered) for backwards compatibility.
pub struct BigSpaceCameraControllerPlugin<F: SpatialHashFilter = ()>(PhantomData<F>);

impl<F: SpatialHashFilter> BigSpaceCameraControllerPlugin<F> {
    /// Create a new instance of [`BigSpaceCameraControllerPlugin`] with the given filter.
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl Default for BigSpaceCameraControllerPlugin<()> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F: SpatialHashFilter> Plugin for BigSpaceCameraControllerPlugin<F> {
    fn build(&self, app: &mut App) {
        app.register_type::<BigSpaceCameraController>()
            .register_type::<BigSpaceCameraInput>()
            .init_resource::<BigSpaceCameraInput>()
            .add_systems(
                PostUpdate,
                (
                    default_camera_inputs
                        .before(camera_controller)
                        .run_if(|input: Res<BigSpaceCameraInput>| !input.defaults_disabled),
                    nearest_objects_in_grid::<F>.before(camera_controller),
                    camera_controller.before(TransformSystems::Propagate),
                ),
            );
    }
}

/// A simple fly-cam camera controller.
///
/// Add to a camera to enable the built-in [`big_space`](crate) camera controller.
#[derive(Clone, Debug, Reflect, Component)]
#[reflect(Component)]
pub struct BigSpaceCameraController {
    /// Smoothness of translation, from `0.0` to `1.0`.
    pub smoothness: f64,
    /// Rotational smoothness, from `0.0` to `1.0`.
    pub rotational_smoothness: f64,
    /// Base speed.
    pub speed: f64,
    /// Rotational yaw speed multiplier.
    pub speed_yaw: f64,
    /// Rotational pitch speed multiplier.
    pub speed_pitch: f64,
    /// Rotational roll speed multiplier.
    pub speed_roll: f64,
    /// Minimum and maximum speed.
    pub speed_bounds: [f64; 2],
    /// Whether the camera should slow down when approaching an entity's [`Aabb`].
    pub slow_near_objects: bool,
    /// Whether the nearest-object search requires entities to be visible. When
    /// `false`, entities with an [`Aabb`] are considered regardless of their
    /// [`InheritedVisibility`] state. Useful when a render culling system strips
    /// visibility from distant objects, but you still want distance-based speed
    /// scaling. Defaults to `true`.
    pub require_visibility: bool,
    nearest_object: Option<(Entity, f64)>,
    vel_translation: DVec3,
    vel_rotation: DQuat,
}

impl BigSpaceCameraController {
    /// Sets the `smoothness` parameter of the controller, and returns the modified result.
    pub fn with_smoothness(mut self, translation: f64, rotation: f64) -> Self {
        self.smoothness = translation;
        self.rotational_smoothness = rotation;
        self
    }

    /// Sets the `slow_near_objects` parameter of the controller, and returns the modified result.
    pub fn with_slowing(mut self, slow_near_objects: bool) -> Self {
        self.slow_near_objects = slow_near_objects;
        self
    }

    /// Sets whether the nearest-object search requires entities to be visible.
    /// When `false`, entities are found by [`Aabb`] alone regardless of render
    /// state. Defaults to `true`.
    pub fn with_require_visibility(mut self, require: bool) -> Self {
        self.require_visibility = require;
        self
    }

    /// Sets the speed of the controller, and returns the modified result.
    pub fn with_speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// Sets the yaw angular velocity of the controller, and returns the modified result.
    pub fn with_speed_yaw(mut self, speed: f64) -> Self {
        self.speed_yaw = speed;
        self
    }

    /// Sets the pitch angular velocity of the controller, and returns the modified result.
    pub fn with_speed_pitch(mut self, speed: f64) -> Self {
        self.speed_pitch = speed;
        self
    }

    /// Sets the pitch angular velocity of the controller, and returns the modified result.
    pub fn with_speed_roll(mut self, speed: f64) -> Self {
        self.speed_roll = speed;
        self
    }

    /// Sets the speed of the controller, and returns the modified result.
    pub fn with_speed_bounds(mut self, speed_limits: [f64; 2]) -> Self {
        self.speed_bounds = speed_limits;
        self
    }

    /// Returns the translational and rotational velocity of the camera.
    pub fn velocity(&self) -> (DVec3, DQuat) {
        (self.vel_translation, self.vel_rotation)
    }

    /// Returns the object nearest the camera, and its distance.
    pub fn nearest_object(&self) -> Option<(Entity, f64)> {
        self.nearest_object
    }
}

impl Default for BigSpaceCameraController {
    fn default() -> Self {
        Self {
            smoothness: 0.85,
            rotational_smoothness: 0.8,
            speed: 1.0,
            speed_pitch: 2.0,
            speed_yaw: 2.0,
            speed_roll: 1.0,
            speed_bounds: [1e-17, 1e30],
            slow_near_objects: true,
            require_visibility: true,
            nearest_object: None,
            vel_translation: DVec3::ZERO,
            vel_rotation: DQuat::IDENTITY,
        }
    }
}

/// `ButtonInput` state used to command [`BigSpaceCameraController`] motion. Reset every time the values
/// are read to update the camera. Allows you to map any input to camera motions. Uses aircraft
/// principle axes conventions.
#[derive(Clone, Debug, Default, Reflect, Resource)]
#[reflect(Resource)]
pub struct BigSpaceCameraInput {
    /// When disabled, the camera input system is not run.
    pub defaults_disabled: bool,
    /// Z-negative
    pub forward: f64,
    /// Y-positive
    pub up: f64,
    /// X-positive
    pub right: f64,
    /// Positive = right wing down
    pub roll: f64,
    /// Positive = nose up
    pub pitch: f64,
    /// Positive = nose right
    pub yaw: f64,
    /// Modifier to increase speed, e.g. "sprint"
    pub boost: bool,
}

impl BigSpaceCameraInput {
    /// Reset the controller back to zero to ready for the next grid.
    pub fn reset(&mut self) {
        *self = BigSpaceCameraInput {
            defaults_disabled: self.defaults_disabled,
            ..Default::default()
        };
    }

    /// Returns the desired velocity transform.
    pub fn target_velocity(
        &self,
        controller: &BigSpaceCameraController,
        speed: f64,
        dt: f64,
    ) -> (DVec3, DQuat) {
        let rotation = DQuat::from_euler(
            EulerRot::XYZ,
            self.pitch * dt * controller.speed_pitch,
            self.yaw * dt * controller.speed_yaw,
            self.roll * dt * controller.speed_roll,
        );

        let translation = DVec3::new(self.right, self.up, self.forward) * speed * dt;

        (translation, rotation)
    }
}

/// Provides sensible keyboard and mouse input defaults
pub fn default_camera_inputs(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_move: MessageReader<MouseMotion>,
    mut cam: ResMut<BigSpaceCameraInput>,
) {
    keyboard.pressed(KeyCode::KeyW).then(|| cam.forward -= 1.0);
    keyboard.pressed(KeyCode::KeyS).then(|| cam.forward += 1.0);
    keyboard.pressed(KeyCode::KeyA).then(|| cam.right -= 1.0);
    keyboard.pressed(KeyCode::KeyD).then(|| cam.right += 1.0);
    keyboard.pressed(KeyCode::Space).then(|| cam.up += 1.0);
    keyboard
        .pressed(KeyCode::ControlLeft)
        .then(|| cam.up -= 1.0);
    keyboard.pressed(KeyCode::KeyQ).then(|| cam.roll += 2.0);
    keyboard.pressed(KeyCode::KeyE).then(|| cam.roll -= 2.0);
    keyboard
        .pressed(KeyCode::ShiftLeft)
        .then(|| cam.boost = true);
    if let Some(total_mouse_motion) = mouse_move.read().map(|e| e.delta).reduce(|sum, i| sum + i) {
        cam.pitch += total_mouse_motion.y as f64 * -0.1;
        cam.yaw += total_mouse_motion.x as f64 * -0.1;
    }
}

/// Find the object nearest the camera, within the same grid as the camera.
///
/// When a [`PartitionLookup`] and [`CellLookup`] are available (via [`CellHashingPlugin`] +
/// [`PartitionPlugin`] with a matching [`SpatialHashFilter`]), the search is accelerated by
/// first finding the nearest partition AABB, then only checking entities inside that partition.
/// Otherwise, falls back to a parallel scan over all entities.
pub fn nearest_objects_in_grid<F: SpatialHashFilter>(
    objects: Query<(
        Entity,
        &GlobalTransform,
        &Aabb,
        Option<&RenderLayers>,
        Option<&InheritedVisibility>,
    )>,
    mut camera: Query<(
        Entity,
        &mut BigSpaceCameraController,
        &GlobalTransform,
        &CellCoord,
        Option<&RenderLayers>,
    )>,
    children: Query<&Children>,
    spatial: Query<(), With<CellCoord>>,
    grids: Query<&Grid>,
    partitions: Option<Res<PartitionLookup<F>>>,
    cell_lookup: Option<Res<CellLookup<F>>>,
) {
    let Ok((cam_entity, mut camera, cam_pos, cam_cell, cam_layer)) = camera.single_mut() else {
        return;
    };
    if !camera.slow_near_objects {
        return;
    }
    let ctx = NearestCtx {
        cam_children: children.iter_descendants(cam_entity).collect(),
        cam_layer: cam_layer.cloned().unwrap_or_default(),
        cam_pos,
        require_visibility: camera.require_visibility,
    };

    let nearest_object = match (partitions, cell_lookup) {
        (Some(partitions), Some(cell_lookup)) => nearest_via_partitions(
            &objects,
            &children,
            &spatial,
            &ctx,
            cam_cell,
            &grids,
            &partitions,
            &cell_lookup,
        ),
        _ => nearest_brute_force(&objects, &ctx),
    };

    // Only update when we found something. When nothing is visible (e.g., all
    // entities have been render-culled), preserve the last known distance so the
    // camera maintains its speed instead of snapping to the base speed.
    if nearest_object.is_some() {
        camera.nearest_object = nearest_object;
    }
}

/// Camera-side inputs to the nearest-object search, shared by the brute-force and
/// partition-accelerated paths so both apply identical candidate filtering.
struct NearestCtx<'a> {
    /// Descendants of the camera entity, excluded so the camera never slows near itself.
    cam_children: EntityHashSet,
    cam_layer: RenderLayers,
    cam_pos: &'a GlobalTransform,
    require_visibility: bool,
}

/// Distance from the camera to `entity`'s AABB surface, or `None` if the entity is not a valid
/// candidate: it is a descendant of the camera, on a non-intersecting render layer, hidden while
/// visibility is required, or at a non-finite distance.
fn candidate_distance(
    ctx: &NearestCtx,
    entity: Entity,
    obj_pos: &GlobalTransform,
    aabb: &Aabb,
    obj_layer: Option<&RenderLayers>,
    visibility: Option<&InheritedVisibility>,
) -> Option<f64> {
    if ctx.cam_children.contains(&entity)
        || !ctx.cam_layer.intersects(obj_layer.unwrap_or_default())
        || (ctx.require_visibility && !visibility.is_some_and(|v| v.get()))
    {
        return None;
    }
    let distance = entity_nearest_distance(ctx.cam_pos, obj_pos, aabb);
    distance.is_finite().then_some(distance)
}

/// Brute-force parallel scan over all entities.
fn nearest_brute_force(
    objects: &Query<(
        Entity,
        &GlobalTransform,
        &Aabb,
        Option<&RenderLayers>,
        Option<&InheritedVisibility>,
    )>,
    ctx: &NearestCtx,
) -> Option<(Entity, f64)> {
    let mut queue = PortableParallel::<Option<(Entity, f64)>>::default();

    objects.par_iter().for_each_init(
        || queue.borrow_local_mut(),
        |local_queue, (entity, obj_pos, aabb, obj_layer, visibility)| {
            let Some(distance) =
                candidate_distance(ctx, entity, obj_pos, aabb, obj_layer, visibility)
            else {
                return;
            };
            if local_queue.is_none_or(|d| distance < d.1) {
                **local_queue = Some((entity, distance));
            }
        },
    );

    queue
        .drain()
        .reduce(|nearest, this| if this.1 < nearest.1 { this } else { nearest })
}

/// Partition-accelerated nearest object search.
///
/// 1. O(partitions): find the partition whose cell AABB is nearest to the camera.
/// 2. O(entities in partition): check entities within that partition, then use the best
///    distance as a bound to skip all other partitions whose AABB is farther.
///
/// Each cell entry entity is searched together with its descendants, so meshes loaded as
/// scene descendants of a `CellCoord` root (typical for GLTF via `WorldAssetRoot`) are found.
/// The descendant walk prunes any nested `CellCoord` subtree, because such an entity is a
/// distinct spatial object visited through its own cell entry; this both bounds the walk to a
/// single object's hierarchy and avoids considering the same entity twice.
#[allow(clippy::too_many_arguments)]
fn nearest_via_partitions<F: SpatialHashFilter>(
    objects: &Query<(
        Entity,
        &GlobalTransform,
        &Aabb,
        Option<&RenderLayers>,
        Option<&InheritedVisibility>,
    )>,
    children: &Query<&Children>,
    spatial: &Query<(), With<CellCoord>>,
    ctx: &NearestCtx,
    cam_cell: &CellCoord,
    grids: &Query<&Grid>,
    partitions: &PartitionLookup<F>,
    cell_lookup: &CellLookup<F>,
) -> Option<(Entity, f64)> {
    // Bail early if no entities match the query (e.g., all have had visibility components
    // stripped by a render culling system), avoiding an exhaustive partition scan.
    if objects.is_empty() {
        return None;
    }

    // Compute cell-space distance from camera to each partition AABB, sorted nearest-first.
    let cam = IVec3::new(cam_cell.x as i32, cam_cell.y as i32, cam_cell.z as i32);
    let mut sorted: Vec<(&PartitionId, &Partition, f64)> = partitions
        .iter()
        .map(|(pid, partition)| {
            let min = partition.min();
            let max = partition.max();
            let min_i = IVec3::new(min.x as i32, min.y as i32, min.z as i32);
            let max_i = IVec3::new(max.x as i32, max.y as i32, max.z as i32);
            // Squared distance from camera cell to AABB (clamped point)
            let clamped = cam.clamp(min_i, max_i);
            let diff = (clamped - cam).as_dvec3();
            (pid, partition, diff.length_squared())
        })
        .collect();
    sorted.sort_unstable_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(core::cmp::Ordering::Equal));

    // Get the grid's cell edge length for converting cell distance to world distance.
    // NOTE: this assumes all partitions share the same grid. Multi-grid scenes may need
    // per-partition cell_edge values.
    let cell_edge = sorted
        .first()
        .and_then(|(_, p, _)| grids.get(p.grid()).ok())
        .map(|g| g.cell_edge_length() as f64)
        .unwrap_or(1.0);

    let mut best: Option<(Entity, f64)> = None;
    // Track how many partitions we've scanned without finding any candidate.
    // If we scan several nearby partitions and find nothing queryable, further
    // partitions are unlikely to help. This caps the worst case when all entities
    // have had visibility/AABB components stripped (e.g. render culling).
    let mut empty_streak = 0u32;
    const MAX_EMPTY_STREAK: u32 = 8;

    for (_pid, partition, aabb_dist_sq) in &sorted {
        // Conservative lower bound on world-space distance to this partition.
        // Subtract one cell_edge to account for entities within a cell being up to
        // cell_edge away from the cell boundary used in the AABB distance calculation.
        let aabb_world_dist = (aabb_dist_sq.sqrt() * cell_edge - cell_edge).max(0.0);
        if let Some((_, best_dist)) = best {
            if aabb_world_dist > best_dist {
                break;
            }
            // Reset streak when we have a candidate - we're now pruning by distance.
            empty_streak = 0;
        } else if empty_streak >= MAX_EMPTY_STREAK {
            // Scanned several nearby partitions without any queryable entity.
            // Further partitions are increasingly unlikely to yield a result.
            break;
        }

        let best_before = best;
        let mut consider = |entity: Entity| {
            let Ok((_, obj_pos, aabb, obj_layer, visibility)) = objects.get(entity) else {
                return;
            };
            if let Some(distance) =
                candidate_distance(ctx, entity, obj_pos, aabb, obj_layer, visibility)
            {
                if best.is_none_or(|(_, d)| distance < d) {
                    best = Some((entity, distance));
                }
            }
        };
        for cell_id in partition.iter() {
            let Some(entry) = cell_lookup.get(cell_id) else {
                continue;
            };
            for &root in entry.entities.iter() {
                // Depth-first walk of this cell entry and its descendants, pruning at nested
                // `CellCoord` boundaries. Considers every `Aabb`-bearing node so multi-mesh
                // scenes are not reduced to one arbitrary mesh.
                let mut stack: SmallVec<[Entity; 16]> = SmallVec::new();
                stack.push(root);
                while let Some(node) = stack.pop() {
                    if node != root && spatial.contains(node) {
                        continue;
                    }
                    consider(node);
                    if let Ok(kids) = children.get(node) {
                        stack.extend(kids.iter());
                    }
                }
            }
        }
        if best.is_none() && best_before.is_none() {
            empty_streak += 1;
        }
    }

    best
}

/// Compute the nearest distance from the camera to an entity's AABB surface.
///
/// Uses the world scale from `obj_pos` rather than a local `Transform`, so an entity scaled by
/// its ancestors (e.g. a mesh nested under a scaled `WorldAssetRoot`) reports the correct size.
fn entity_nearest_distance(
    cam_pos: &GlobalTransform,
    obj_pos: &GlobalTransform,
    aabb: &Aabb,
) -> f64 {
    let center_distance = obj_pos.translation().as_dvec3() - cam_pos.translation().as_dvec3();
    center_distance.length()
        - (aabb.half_extents.as_dvec3() * obj_pos.scale().as_dvec3())
            .abs()
            .min_element()
}

/// Uses [`BigSpaceCameraInput`] state to update the camera position.
pub fn camera_controller(
    time: Res<Time>,
    grids: Grids,
    mut input: ResMut<BigSpaceCameraInput>,
    mut camera: Query<(
        Entity,
        &mut CellCoord,
        &mut Transform,
        &mut BigSpaceCameraController,
    )>,
) {
    for (camera, mut cell, mut transform, mut controller) in camera.iter_mut() {
        let Some(grid) = grids.parent_grid(camera) else {
            continue;
        };
        let speed = match (controller.nearest_object, controller.slow_near_objects) {
            (Some(nearest), true) => nearest.1.abs(),
            _ => controller.speed,
        } * (controller.speed + input.boost as usize as f64);

        let [min, max] = controller.speed_bounds;
        let speed = speed.clamp(min, max);

        // Clamp to 100ms to prevent flying on perf dips.
        let dt = time.delta_secs_f64().min(0.1);
        // Framerate-independent exponential smoothing. At 60fps (dt=1/60) the exponent
        // is 1.0, reproducing the original per-frame behavior. At other framerates the
        // decay scales correctly so the feel is consistent.
        let lerp_translation = 1.0 - controller.smoothness.clamp(0.0, 0.999).powf(dt * 60.0);
        let lerp_rotation = 1.0
            - controller
                .rotational_smoothness
                .clamp(0.0, 0.999)
                .powf(dt * 60.0);

        let (vel_t_current, vel_r_current) = (controller.vel_translation, controller.vel_rotation);
        let (vel_t_target, vel_r_target) = input.target_velocity(&controller, speed, dt);

        let cam_rot = transform.rotation.as_dquat();
        let vel_t_next = cam_rot * vel_t_target; // Orients the translation to match the camera
        let vel_t_next = vel_t_current.lerp(vel_t_next, lerp_translation);
        // Convert the high precision translation to a grid cell and low precision translation
        let (cell_offset, new_translation) = grid.translation_to_grid(vel_t_next);
        let new = *cell.bypass_change_detection() + cell_offset;
        cell.set_if_neq(new);
        transform.translation += new_translation;

        let new_rotation = vel_r_current.slerp(vel_r_target, lerp_rotation);
        transform.rotation *= new_rotation.as_quat();

        // Store the new velocity to be used in the next grid
        controller.vel_translation = vel_t_next;
        controller.vel_rotation = new_rotation;

        input.reset();
    }
}
