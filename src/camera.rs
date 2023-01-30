//! Provides a camera controller compatible with the floating origin plugin.

use std::marker::PhantomData;

use bevy::{
    ecs::schedule::ShouldRun,
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
            .init_resource::<CameraVelocity>()
            .add_system_set_to_stage(
                CoreStage::PostUpdate,
                SystemSet::new()
                    .with_system(
                        default_camera_inputs
                            .before(camera_controller::<P>)
                            .with_run_criteria(|input: Res<CameraInput>| {
                                if input.defaults_disabled {
                                    ShouldRun::No
                                } else {
                                    ShouldRun::Yes
                                }
                            }),
                    )
                    .with_system(
                        camera_controller::<P>.before(TransformSystem::TransformPropagate),
                    ),
            );
    }
}

/// Per-camera settings for the `big_space` floating origin camera controller.
#[derive(Clone, Debug, Reflect, Component)]
pub struct CameraController {
    /// Smoothness of motion, from `0.0` to `1.0`.
    pub smoothness: f64,
    /// Maximum possible speed.
    pub max_speed: f64,
    /// Whether the camera should slow down when approaching an entity's [`Aabb`].
    pub slow_near_objects: bool,
}
impl Default for CameraController {
    fn default() -> Self {
        Self {
            smoothness: 0.2,
            max_speed: 10e8,
            slow_near_objects: true,
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

/// Tracks the camera's velocity
#[derive(Debug, Default, Resource, Reflect)]
pub struct CameraVelocity {
    entity: Option<Entity>,
    translation: DVec3,
    rotation: DQuat,
}

impl CameraVelocity {
    /// Get the translation component of the camera velocity.
    pub fn translation(&self) -> DVec3 {
        self.translation
    }

    /// Get the rotation component of the camera velocity.
    pub fn rotation(&self) -> DQuat {
        self.rotation
    }
}

/// Uses [`CameraInput`] state to update the camera position.
pub fn camera_controller<P: GridPrecision>(
    time: Res<Time>,
    settings: Res<FloatingOriginSettings>,
    mut input: ResMut<CameraInput>,
    mut camera: Query<(
        Entity,
        &mut Transform,
        &GlobalTransform,
        &CameraController,
        &mut GridCell<P>,
    )>,
    objects: Query<(&GlobalTransform, &Aabb)>,
    mut velocity: ResMut<CameraVelocity>,
) {
    let (entity, mut cam_transform, cam_global_transform, controller, mut cell) =
        camera.single_mut();

    let speed = if controller.slow_near_objects {
        let mut nearest_object = f64::MAX;
        for (transform, aabb) in &objects {
            let distance = (transform.translation().as_dvec3() + aabb.center.as_dvec3()
                - cam_global_transform.translation().as_dvec3())
            .length()
                - aabb.half_extents.as_dvec3().max_element();
            nearest_object = nearest_object.min(distance);
        }
        nearest_object.abs().clamp(1.0, controller.max_speed)
    } else {
        controller.max_speed
    } * (1.0 + input.boost as usize as f64);

    let lerp_val = 1.0 - controller.smoothness.clamp(0.0, 0.999); // The lerp factor

    if velocity.entity != Some(entity) {
        velocity.entity = Some(entity);
        velocity.translation = DVec3::ZERO;
        velocity.rotation = DQuat::IDENTITY;
    }

    let (vel_t_current, vel_r_current) = (velocity.translation, velocity.rotation);
    let (vel_t_target, vel_r_target) = input.target_velocity(speed, time.delta_seconds_f64());

    let cam_rot = cam_transform.rotation.as_f64();
    let vel_t_next = cam_rot * vel_t_target; // Orients the translation to match the camera
    let vel_t_next = vel_t_current.lerp(vel_t_next, lerp_val);
    // Convert the high precision translation to a grid cell and low precision translation
    let (cell_offset, new_translation) = settings.translation_to_grid(vel_t_next);
    *cell += cell_offset;
    cam_transform.translation += new_translation;

    let new_rotation = vel_r_current.slerp(vel_r_target, lerp_val);
    cam_transform.rotation *= new_rotation.as_f32();

    // Store the new velocity to be used in the next frame
    velocity.translation = if vel_t_next.length().abs() < 0.001 {
        DVec3::ZERO
    } else {
        vel_t_next
    };
    velocity.rotation = if new_rotation.to_axis_angle().1.abs() < 0.001 {
        DQuat::IDENTITY
    } else {
        new_rotation
    };

    input.reset();
}
