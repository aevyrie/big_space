//! This example demonstrates error accumulating from parent to children in nested grids.
use bevy::{color::palettes, math::DVec3, prelude::*};
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpacePlugin::default(),
            CameraControllerPlugin::default(),
            FloatingOriginDebugPlugin::default(),
        ))
        .add_systems(Startup, setup_scene)
        .run();
}

// The nearby object is NEARBY meters away from us. The distance object is DISTANT meters away from
// us, and has a child that is DISTANT meters toward us (relative its parent) minus NEARBY meters.
//
// The result is two spheres that should perfectly overlap, even though one of those spheres is a
// child of an object more than one quadrillion meters away. This example intentionally results in a
// small amount of error, to demonstrate the scales and precision available even between different
// grids.
//
// Note that as you increase the distance further, there are still no rendering errors, and the
// green sphere does not vanish, however, as you move farther away, you will see that the green
// sphere will pop into neighboring cells due to rounding error.
const DISTANT: DVec3 = DVec3::new(1e17, 1e17, 1e17);
const SPHERE_RADIUS: f32 = 1.0;
const NEARBY: Vec3 = Vec3::new(SPHERE_RADIUS * 20.0, SPHERE_RADIUS * 20.0, 0.0);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_handle = meshes.add(Sphere::new(SPHERE_RADIUS).mesh());

    commands.spawn_big_space(Grid::new(SPHERE_RADIUS * 100.0, 0.0), |root_grid| {
        root_grid.spawn_spatial((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(materials.add(Color::from(palettes::css::BLUE))),
            Transform::from_translation(NEARBY),
        ));

        let parent = root_grid.grid().translation_to_grid(DISTANT);
        root_grid.with_grid(Grid::new(SPHERE_RADIUS * 100.0, 0.0), |parent_grid| {
            // This function introduces a small amount of error, because it can only work up
            // to double precision floats. (f64).
            let child = parent_grid
                .grid()
                .translation_to_grid(-DISTANT + NEARBY.as_dvec3());
            parent_grid.insert((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(materials.add(Color::from(palettes::css::RED))),
                Transform::from_translation(parent.1),
            ));
            parent_grid.insert(parent.0);

            // A green sphere that is a child of the sphere very far from the origin. This
            // child is very far from its parent, and should be located exactly at the
            // NEARBY position (if there was no floating point error). The distance from the
            // green sphere to the blue sphere is the error caused by float imprecision.
            // Note that the sphere does not have any rendering artifacts, its position just
            // has a fixed error.
            parent_grid.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(materials.add(Color::from(palettes::css::GREEN))),
                Transform::from_translation(child.1),
                child.0,
            ));
        });

        root_grid.spawn_spatial((
            DirectionalLight::default(),
            Transform::from_xyz(4.0, -10.0, -4.0),
        ));

        root_grid.spawn_spatial((
            Camera3d::default(),
            Transform::from_translation(NEARBY + Vec3::new(0.0, 0.0, SPHERE_RADIUS * 10.0))
                .looking_at(NEARBY, Vec3::Y),
            Projection::Perspective(PerspectiveProjection {
                near: (SPHERE_RADIUS * 0.1).min(0.1),
                ..default()
            }),
            FloatingOrigin,
            big_space::camera::CameraController::default() // Built-in camera controller
                .with_speed_bounds([10e-18, 10e35])
                .with_smoothness(0.9, 0.8)
                .with_speed(1.0),
        ));
    });
}
