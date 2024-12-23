#![allow(clippy::type_complexity)]

use bevy::{color::palettes, prelude::*};
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::<i64>::default(),
            big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (movement, rotation))
        .run();
}

#[derive(Component)]
struct Mover<const N: usize>;

fn movement(
    time: Res<Time>,
    mut q: ParamSet<(
        Query<&mut Transform, With<Mover<1>>>,
        Query<&mut Transform, With<Mover<2>>>,
        Query<&mut Transform, With<Mover<3>>>,
        Query<&mut Transform, With<Mover<4>>>,
    )>,
) {
    let delta_translation = |offset: f32, scale: f32| -> Vec3 {
        let t_1 = time.elapsed_secs() * 0.1 + offset;
        let dt = time.delta_secs() * 0.1;
        let t_0 = t_1 - dt;
        let pos =
            |t: f32| -> Vec3 { Vec3::new(t.cos() * 2.0, t.sin() * 2.0, (t * 1.3).sin() * 2.0) };
        let p0 = pos(t_0) * scale;
        let p1 = pos(t_1) * scale;
        p1 - p0
    };

    q.p0().single_mut().translation += delta_translation(20.0, 1.0);
    q.p1().single_mut().translation += delta_translation(251.0, 1.0);
    q.p2().single_mut().translation += delta_translation(812.0, 1.0);
    q.p3().single_mut().translation += delta_translation(863.0, 0.4);
}

#[derive(Component)]
struct Rotator;

fn rotation(time: Res<Time>, mut query: Query<&mut Transform, With<Rotator>>) {
    for mut transform in &mut query {
        transform.rotate_z(3.0 * time.delta_secs() * 0.2);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_handle = meshes.add(Sphere::new(0.1).mesh().ico(16).unwrap());
    let matl_handle = materials.add(StandardMaterial {
        base_color: Color::Srgba(palettes::basic::WHITE),
        ..default()
    });

    commands.spawn_big_space::<i64>(Grid::new(1.0, 0.01), |root| {
        root.spawn_spatial((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(matl_handle.clone()),
            Transform::from_xyz(0.0, 0.0, 1.0),
            Mover::<1>,
        ));

        root.spawn_spatial((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(matl_handle.clone()),
            Transform::from_xyz(1.0, 0.0, 0.0),
            Mover::<2>,
        ));

        root.with_grid(Grid::new(0.2, 0.01), |new_grid| {
            new_grid.insert((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(matl_handle.clone()),
                Transform::from_xyz(0.0, 1.0, 0.0),
                Rotator,
                Mover::<3>,
            ));
            new_grid.spawn_spatial((
                Mesh3d(mesh_handle),
                MeshMaterial3d(matl_handle),
                Transform::from_xyz(0.0, 0.5, 0.0),
                Mover::<4>,
            ));
        });

        // light
        root.spawn_spatial((PointLight::default(), Transform::from_xyz(4.0, 8.0, 4.0)));

        // camera
        root.spawn_spatial((
            Camera3d::default(),
            Transform::from_xyz(0.0, 0.0, 8.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            FloatingOrigin,
        ));
    });
}
