//! Provides a camera controller compatible with the floating origin plugin.

use bevy::{
    input::mouse::MouseMotion, prelude::*, render::primitives::Aabb, transform::TransformSystem,
    utils::HashMap,
};

/// Adds the `big_space` camera controller
pub struct CameraControllerPlugin;
impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraInput>().add_system_set_to_stage(
            CoreStage::PostUpdate,
            SystemSet::new()
                .with_system(default_camera_inputs.before(camera_controller))
                .with_system(camera_controller.after(TransformSystem::TransformPropagate)),
        );
    }
}

/// Per-camera settings for the `big_space` floating origin camera controller.
#[derive(Clone, Debug, Reflect, Component)]
pub struct CameraController {
    /// Smoothness of motion, from `0.0` to `1.0`.
    pub smoothness: f32,
    /// Maximum possible speed.
    pub max_speed: f32,
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
    /// Z-negative
    pub forward: f32,
    /// Y-positive
    pub up: f32,
    /// X-positive
    pub right: f32,
    /// Positive = right wing down
    pub roll: f32,
    /// Positive = nose up
    pub pitch: f32,
    /// Positive = nose right
    pub yaw: f32,
}

impl CameraInput {
    /// Reset the controller back to zero to ready fro the next frame.
    pub fn reset(&mut self) {
        *self = CameraInput::default();
    }

    /// Returns the desired velocity transform.
    pub fn target_velocity(&self, speed: f32, dt: f32) -> Transform {
        let mut new_transform = Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            self.pitch * dt,
            self.yaw * dt,
            self.roll * dt,
        ));

        let delta = Vec3::new(self.right, self.up, self.forward) * speed * dt;

        new_transform.translation = delta;
        new_transform
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
    if let Some(total_mouse_motion) = mouse_move.iter().map(|e| e.delta).reduce(|sum, i| sum + i) {
        cam.pitch += total_mouse_motion.y * -0.1;
        cam.yaw += total_mouse_motion.x * -0.1;
    }
}

/// Uses [`CameraInput`] state to update the camera position.
pub fn camera_controller(
    time: Res<Time>,
    mut input: ResMut<CameraInput>,
    mut camera: Query<(
        Entity,
        &mut Transform,
        &GlobalTransform,
        &mut CameraController,
    )>,
    objects: Query<(&GlobalTransform, &Aabb)>,
    mut velocities: Local<HashMap<Entity, Transform>>,
) {
    let (entity, mut cam_transform, cam_global_transform, controller) = camera.single_mut();

    let speed = if controller.slow_near_objects {
        let mut nearest_object = f32::MAX;
        for (transform, aabb) in &objects {
            let distance = (transform.translation() + Vec3::from(aabb.center)
                - cam_global_transform.translation())
            .length()
                - aabb.half_extents.max_element();
            nearest_object = nearest_object.min(distance);
        }
        nearest_object.abs().clamp(1.0, controller.max_speed)
    } else {
        controller.max_speed
    };

    let lerp_val = 1.0 - controller.smoothness.clamp(0.0, 0.99999999); // The lerp factor

    let v_current = velocities.entry(entity).or_default();
    let v_target = input.target_velocity(speed, time.delta_seconds());

    let v_next = Transform {
        translation: v_current.translation.lerp(v_target.translation, lerp_val),
        rotation: v_current.rotation.slerp(v_target.rotation, lerp_val),
        ..default()
    };
    let cam_rot = cam_transform.rotation;
    cam_transform.translation += cam_rot * v_next.translation;
    cam_transform.rotation *= v_next.rotation;
    *v_current = v_next;

    input.reset();
}
