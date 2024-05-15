use std::f32::consts::PI;

use bevy::prelude::*;
use big_space::{
    camera::{CameraController, CameraControllerPlugin},
    BigSpacePlugin, BigSpaceRootBundle, FloatingOrigin, GridCell,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::<i32>::default(),
            CameraControllerPlugin::<i32>::default(),
        ))
        .add_systems(Startup, setup)
        .run()
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn(BigSpaceRootBundle::<i32>::default())
        .with_children(|root| {
            root.spawn((
                FloatingOrigin,
                Camera3dBundle::default(),
                CameraController::default()
                    .with_speed(0.01)
                    .with_speed_bounds([0.1, 10.0]),
                GridCell::<i32>::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .with_children(|camera| {
                camera.spawn(SceneBundle {
                    scene: asset_server.load("models/low_poly_spaceship/scene.gltf#Scene0"),
                    transform: Transform::from_xyz(0.0, -2.0, -14.0)
                        .with_rotation(Quat::from_rotation_y(PI)),
                    ..default()
                });
            });
        });
}
