//! Provides a camera controller compatible with the floating origin plugin.

use std::marker::PhantomData;

use bevy::{
    input::mouse::MouseMotion,
    math::{DQuat, DVec3},
    prelude::*,
    render::primitives::Aabb,
    transform::TransformSystem,
};

use crate::{precision::GridPrecision, FloatingOriginSettings, GridCell};

/// Adds the `big_space` camera controller
#[derive(Default)]
pub struct CameraControllerPlugin<P: GridPrecision>(PhantomData<P>);
impl<P: GridPrecision> Plugin for CameraControllerPlugin<P> {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraInput>()
            //CoreStage::PostUpdate,
            .add_systems(
                (
                    default_camera_inputs
                        .before(camera_controller::<P>)
                        .run_if(|input: Res<CameraInput>| !input.defaults_disabled),
                    nearest_objects.before(camera_controller::<P>),
                    camera_controller::<P>.before(TransformSystem::TransformPropagate),
                )
                    .in_base_set(CoreSet::PostUpdate),
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
    /// Maximum possible speed.
    pub max_speed: f64,
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

    /// Sets the `max_speed` parameter of the controller, and returns the modified result.
    pub fn with_max_speed(mut self, max_speed: f64) -> Self {
        self.max_speed = max_speed;
        self
    }

    /// Sets the `slow_near_objects` parameter of the controller, and returns the modified result.
    pub fn with_slowing(mut self, slow_near_objects: bool) -> Self {
        self.slow_near_objects = slow_near_objects;
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
            max_speed: 10e8,
            slow_near_objects: true,
            nearest_object: None,
            vel_translation: DVec3::ZERO,
            vel_rotation: DQuat::IDENTITY,
        }
    }
}

/// Input state used to command camera motion. Reset every time the values are read to update the
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
            ..default()
        };
    }

    /// Returns the desired velocity transform.
    pub fn target_velocity(&self, speed: f64, dt: f64) -> (DVec3, DQuat) {
        let rotation = DQuat::from_euler(
            EulerRot::XYZ,
            self.pitch * dt,
            self.yaw * dt,
            self.roll * dt,
        );

        let translation =
            DVec3::new(self.right as f64, self.up as f64, self.forward as f64) * speed * dt as f64;

        (translation, rotation)
    }
}

/// Provides sensible keyboard and mouse input defaults
pub fn default_camera_inputs(
    keyboard: Res<Input<KeyCode>>,
    mut mouse_move: EventReader<MouseMotion>,
    mut cam: ResMut<CameraInput>,
) {
    keyboard.pressed(KeyCode::W).then(|| cam.forward -= 1.0);
    keyboard.pressed(KeyCode::S).then(|| cam.forward += 1.0);
    keyboard.pressed(KeyCode::A).then(|| cam.right -= 1.0);
    keyboard.pressed(KeyCode::D).then(|| cam.right += 1.0);
    keyboard.pressed(KeyCode::Space).then(|| cam.up += 1.0);
    keyboard.pressed(KeyCode::LControl).then(|| cam.up -= 1.0);
    keyboard.pressed(KeyCode::Q).then(|| cam.roll += 1.0);
    keyboard.pressed(KeyCode::E).then(|| cam.roll -= 1.0);
    keyboard.pressed(KeyCode::LShift).then(|| cam.boost = true);
    if let Some(total_mouse_motion) = mouse_move.iter().map(|e| e.delta).reduce(|sum, i| sum + i) {
        cam.pitch += total_mouse_motion.y as f64 * -0.1;
        cam.yaw += total_mouse_motion.x as f64 * -0.1;
    }
}

/// Find the object nearest the camera
pub fn nearest_objects(
    objects: Query<(Entity, &GlobalTransform, &Aabb)>,
    mut camera: Query<(&GlobalTransform, &mut CameraController)>,
) {
    for (cam_global_transform, mut controller) in camera.iter_mut() {
        let nearest_object = objects
            .iter()
            .map(|(entity, transform, aabb)| {
                let dist = (transform.translation().as_dvec3() + aabb.center.as_dvec3()
                    - cam_global_transform.translation().as_dvec3())
                .length()
                    - aabb.half_extents.as_dvec3().max_element();
                (entity, dist)
            })
            .reduce(|nearest, this| if this.1 < nearest.1 { this } else { nearest });
        controller.nearest_object = nearest_object;
    }
}

/// Uses [`CameraInput`] state to update the camera position.
pub fn camera_controller<P: GridPrecision>(
    time: Res<Time>,
    settings: Res<FloatingOriginSettings>,
    mut input: ResMut<CameraInput>,
    mut camera: Query<(&mut Transform, &mut CameraController, &mut GridCell<P>)>,
) {
    for (mut cam_transform, mut controller, mut cell) in camera.iter_mut() {
        let speed = match (controller.nearest_object, controller.slow_near_objects) {
            (Some(nearest), true) => nearest.1,
            _ => controller.max_speed,
        } * (1.0 + input.boost as usize as f64);

        let lerp_translation = 1.0 - controller.smoothness.clamp(0.0, 0.999);
        let lerp_rotation = 1.0 - controller.rotational_smoothness.clamp(0.0, 0.999);

        let (vel_t_current, vel_r_current) = (controller.vel_translation, controller.vel_rotation);
        let (vel_t_target, vel_r_target) = input.target_velocity(speed, time.delta_seconds_f64());

        let cam_rot = cam_transform.rotation.as_f64();
        let vel_t_next = cam_rot * vel_t_target; // Orients the translation to match the camera
        let vel_t_next = vel_t_current.lerp(vel_t_next, lerp_translation);
        // Convert the high precision translation to a grid cell and low precision translation
        let (cell_offset, new_translation) = settings.translation_to_grid(vel_t_next);
        *cell += cell_offset;
        cam_transform.translation += new_translation;

        let new_rotation = vel_r_current.slerp(vel_r_target, lerp_rotation);
        cam_transform.rotation *= new_rotation.as_f32();

        // Store the new velocity to be used in the next frame
        controller.vel_translation = if vel_t_next.length().abs() < 0.001 {
            DVec3::ZERO
        } else {
            vel_t_next
        };
        controller.vel_rotation = if new_rotation.to_axis_angle().1.abs() < 0.001 {
            DQuat::IDENTITY
        } else {
            new_rotation
        };

        input.reset();
    }
}
