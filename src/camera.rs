//! Provides a camera controller compatible with the floating origin plugin.

use std::marker::PhantomData;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_input::{mouse::MouseMotion, prelude::*};
use bevy_math::{prelude::*, DQuat, DVec3};
use bevy_reflect::prelude::*;
use bevy_render::{
    primitives::Aabb,
    view::{InheritedVisibility, RenderLayers},
};
use bevy_time::prelude::*;
use bevy_transform::{prelude::*, TransformSystem};
use bevy_utils::HashSet;

use crate::{
    precision::GridPrecision, reference_frame::local_origin::ReferenceFrames,
    world_query::GridTransform,
};

/// Adds the `big_space` camera controller
#[derive(Default)]
pub struct CameraControllerPlugin<P: GridPrecision>(PhantomData<P>);
impl<P: GridPrecision> Plugin for CameraControllerPlugin<P> {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraInput>().add_systems(
            PostUpdate,
            (
                default_camera_inputs
                    .before(camera_controller::<P>)
                    .run_if(|input: Res<CameraInput>| !input.defaults_disabled),
                nearest_objects_in_frame::<P>.before(camera_controller::<P>),
                camera_controller::<P>.before(TransformSystem::TransformPropagate),
            ),
        );
    }
}

/// Per-camera settings for the `big_space` floating origin camera controller.
#[derive(Clone, Debug, Reflect, Component)]
pub struct CameraController {
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
    nearest_object: Option<(Entity, f64)>,
    vel_translation: DVec3,
    vel_rotation: DQuat,
}

impl CameraController {
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

impl Default for CameraController {
    fn default() -> Self {
        Self {
            smoothness: 0.8,
            rotational_smoothness: 0.5,
            speed: 1.0,
            speed_pitch: 1.0,
            speed_yaw: 1.0,
            speed_roll: 1.0,
            speed_bounds: [1e-17, 1e30],
            slow_near_objects: true,
            nearest_object: None,
            vel_translation: DVec3::ZERO,
            vel_rotation: DQuat::IDENTITY,
        }
    }
}

/// ButtonInput state used to command camera motion. Reset every time the values are read to update the
/// camera. Allows you to map any input to camera motions. Uses aircraft principle axes conventions.
#[derive(Clone, Debug, Default, Reflect, Resource)]
pub struct CameraInput {
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

impl CameraInput {
    /// Reset the controller back to zero to ready fro the next frame.
    pub fn reset(&mut self) {
        *self = CameraInput {
            defaults_disabled: self.defaults_disabled,
            ..Default::default()
        };
    }

    /// Returns the desired velocity transform.
    pub fn target_velocity(
        &self,
        controller: &CameraController,
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
    mut mouse_move: EventReader<MouseMotion>,
    mut cam: ResMut<CameraInput>,
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

/// Find the object nearest the camera, within the same reference frame as the camera.
pub fn nearest_objects_in_frame<P: GridPrecision>(
    objects: Query<(
        Entity,
        &Transform,
        &GlobalTransform,
        &Aabb,
        Option<&RenderLayers>,
        Option<&InheritedVisibility>,
    )>,
    mut camera: Query<(
        Entity,
        &mut CameraController,
        &GlobalTransform,
        Option<&RenderLayers>,
    )>,
    children: Query<&Children>,
) {
    let Ok((cam_entity, mut camera, cam_pos, cam_layer)) = camera.get_single_mut() else {
        return;
    };
    let cam_layer = cam_layer.to_owned().unwrap_or_default();
    let cam_children: HashSet<Entity> = children.iter_descendants(cam_entity).collect();

    let nearest_object = objects
        .iter()
        .filter(|(entity, ..)| !cam_children.contains(entity))
        .filter(|(.., obj_layer, _)| {
            let obj_layer = obj_layer.unwrap_or_default();
            cam_layer.intersects(obj_layer)
        })
        .filter(|(.., visibility)| {
            let visibility = visibility.copied().unwrap_or(InheritedVisibility::VISIBLE);
            visibility.get()
        })
        .map(|(entity, object_local, obj_pos, aabb, ..)| {
            let center_distance =
                obj_pos.translation().as_dvec3() - cam_pos.translation().as_dvec3();
            let nearest_distance = center_distance.length()
                - (aabb.half_extents.as_dvec3() * object_local.scale.as_dvec3())
                    .abs()
                    .min_element();
            (entity, nearest_distance)
        })
        .filter(|v| v.1.is_finite())
        .reduce(|nearest, this| if this.1 < nearest.1 { this } else { nearest });
    camera.nearest_object = nearest_object;
}

/// Uses [`CameraInput`] state to update the camera position.
pub fn camera_controller<P: GridPrecision>(
    time: Res<Time>,
    frames: ReferenceFrames<P>,
    mut input: ResMut<CameraInput>,
    mut camera: Query<(Entity, GridTransform<P>, &mut CameraController)>,
) {
    for (camera, mut position, mut controller) in camera.iter_mut() {
        let Some(frame) = frames.parent_frame(camera) else {
            continue;
        };
        let speed = match (controller.nearest_object, controller.slow_near_objects) {
            (Some(nearest), true) => nearest.1.abs(),
            _ => controller.speed,
        } * (controller.speed + input.boost as usize as f64);

        let [min, max] = controller.speed_bounds;
        let speed = speed.clamp(min, max);

        let lerp_translation = 1.0 - controller.smoothness.clamp(0.0, 0.999);
        let lerp_rotation = 1.0 - controller.rotational_smoothness.clamp(0.0, 0.999);

        let (vel_t_current, vel_r_current) = (controller.vel_translation, controller.vel_rotation);
        let (vel_t_target, vel_r_target) =
            input.target_velocity(&controller, speed, time.delta_seconds_f64());

        let cam_rot = position.transform.rotation.as_dquat();
        let vel_t_next = cam_rot * vel_t_target; // Orients the translation to match the camera
        let vel_t_next = vel_t_current.lerp(vel_t_next, lerp_translation);
        // Convert the high precision translation to a grid cell and low precision translation
        let (cell_offset, new_translation) = frame.translation_to_grid(vel_t_next);
        *position.cell += cell_offset;
        position.transform.translation += new_translation;

        let new_rotation = vel_r_current.slerp(vel_r_target, lerp_rotation);
        position.transform.rotation *= new_rotation.as_quat();

        // Store the new velocity to be used in the next frame
        controller.vel_translation = vel_t_next;
        controller.vel_rotation = new_rotation;

        input.reset();
    }
}
