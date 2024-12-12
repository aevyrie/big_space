use std::{collections::VecDeque, hash::Hasher, time::Duration};

use bevy::{
    core_pipeline::{bloom::Bloom, fxaa::Fxaa, tonemapping::Tonemapping},
    prelude::*,
};
use bevy_ecs::entity::EntityHasher;
use bevy_math::DVec3;
use bevy_render::view::VisibilityRange;
use bevy_utils::Instant;
use big_space::prelude::*;
use noise::{NoiseFn, Perlin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::<i32>::default(),
            SpatialHashPlugin::<i32>::default(),
            SpatialPartitionPlugin::<i32>::default(),
            big_space::camera::CameraControllerPlugin::<i32>::default(),
        ))
        .add_systems(Startup, (spawn, setup_ui))
        .add_systems(
            PostUpdate,
            (
                move_player.after(TransformSystem::TransformPropagate),
                draw_partitions.after(SpatialSystem::UpdatePartition),
            ),
        )
        .add_systems(Update, cursor_grab)
        .init_resource::<MaterialPresets>()
        .run();
}

const N_ENTITIES: usize = 100_000;
const HALF_WIDTH: f32 = 50.0;
const CELL_WIDTH: f32 = 10.0;
// How fast the entities should move, causing them to move into neighboring cells.
const MOVEMENT_SPEED: f32 = 1e5;
const PERCENT_STATIC: f32 = 0.9;

#[derive(Component)]
struct Player;

#[derive(Component)]
struct NonPlayer;

#[derive(Resource)]
struct MaterialPresets {
    default: Handle<StandardMaterial>,
    highlight: Handle<StandardMaterial>,
    flood: Handle<StandardMaterial>,
}

impl FromWorld for MaterialPresets {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        let d: StandardMaterial = StandardMaterial {
            base_color: Color::from(Srgba::new(0.5, 0.5, 0.5, 1.0)),
            perceptual_roughness: 0.2,
            metallic: 0.0,
            ..Default::default()
        };
        let h: StandardMaterial = Color::from(Srgba::new(2.0, 0.0, 8.0, 1.0)).into();
        let f: StandardMaterial = Color::from(Srgba::new(1.1, 0.1, 1.0, 1.0)).into();

        Self {
            default: materials.add(d),
            highlight: materials.add(h),
            flood: materials.add(f),
        }
    }
}

fn draw_partitions(
    mut gizmos: Gizmos,
    partitions: Res<SpatialPartitionMap<i32>>,
    frames: Query<(&GlobalTransform, &ReferenceFrame<i32>)>,
) {
    for (id, p) in partitions.iter() {
        let Ok((transform, frame)) = frames.get(p.reference_frame()) else {
            return;
        };
        let l = frame.cell_edge_length();

        let Some(min) = p
            .iter()
            .map(|h| [h.cell().x, h.cell().y, h.cell().z])
            .reduce(|[ax, ay, az], [ix, iy, iz]| [ax.min(ix), ay.min(iy), az.min(iz)])
            .map(|v| IVec3::from(v).as_vec3() * l)
        else {
            continue;
        };

        let Some(max) = p
            .iter()
            .map(|h| [h.cell().x, h.cell().y, h.cell().z])
            .reduce(|[ax, ay, az], [ix, iy, iz]| [ax.max(ix), ay.max(iy), az.max(iz)])
            .map(|v| IVec3::from(v).as_vec3() * l)
        else {
            continue;
        };

        let size = max - min;
        let center = min + (size) * 0.5;
        let local_trans = Transform::from_translation(center).with_scale(size + l);

        let mut hasher = EntityHasher::default();
        hasher.write_u64(id.id());
        let f = hasher.finish();
        let hue = (f % 360) as f32;

        gizmos.cuboid(
            transform.mul_transform(local_trans),
            Hsla::hsl(hue, 1.0, 0.5),
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn move_player(
    time: Res<Time>,
    mut _gizmos: Gizmos,
    mut player: Query<
        (
            &mut Transform,
            &mut GridCell<i32>,
            &Parent,
            &SpatialHash<i32>,
        ),
        With<Player>,
    >,
    mut non_player: Query<
        (&mut Transform, &mut GridCell<i32>, &Parent),
        (Without<Player>, With<NonPlayer>),
    >,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>, Without<Player>>,
    mut neighbors: Local<Vec<Entity>>,
    reference_frame: Query<&ReferenceFrame<i32>>,
    spatial_hash_map: Res<SpatialHashMap<i32>>,
    material_presets: Res<MaterialPresets>,
    mut text: Query<(&mut Text, &mut StatsText)>,
    hash_stats: Res<big_space::timing::SmoothedStat<big_space::timing::SpatialHashStats>>,
    prop_stats: Res<big_space::timing::SmoothedStat<big_space::timing::PropagationStats>>,
) {
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = materials.get_mut(*neighbor) {
            **material = material_presets.default.clone_weak();
        }
    }

    let t = time.elapsed_secs() * 1.0;
    let scale = MOVEMENT_SPEED / (N_ENTITIES as f32 * HALF_WIDTH);
    if scale.abs() > 0.0 {
        // Avoid change detection
        for (i, (mut transform, _, _)) in non_player.iter_mut().enumerate() {
            if i > (PERCENT_STATIC * N_ENTITIES as f32) as usize {
                transform.translation.x += t.sin() * scale;
                transform.translation.y += t.cos() * scale;
                transform.translation.z += (t * 2.3).sin() * scale;
            }
        }
    }

    let t = time.elapsed_secs() * 0.01;
    let (mut transform, mut cell, parent, hash) = player.single_mut();
    let absolute_pos = HALF_WIDTH
        * CELL_WIDTH
        * 0.8
        * Vec3::new((5.0 * t).sin(), (7.0 * t).cos(), (20.0 * t).sin());
    (*cell, transform.translation) = reference_frame
        .get(parent.get())
        .unwrap()
        .imprecise_translation_to_grid(absolute_pos);

    neighbors.clear();

    spatial_hash_map
        .flood(hash, None)
        .entities()
        .for_each(|entity| {
            neighbors.push(entity);
            if let Ok(mut material) = materials.get_mut(entity) {
                **material = material_presets.flood.clone_weak();
            }

            // let frame = reference_frame.get(entry.reference_frame).unwrap();
            // let transform = frame.global_transform(
            //     &entry.cell,
            //     &Transform::from_scale(Vec3::splat(frame.cell_edge_length() * 0.99)),
            // );
            // gizmos.cuboid(transform, Color::linear_rgba(1.0, 1.0, 1.0, 0.2));
        });

    spatial_hash_map
        .get(hash)
        .unwrap()
        .nearby(&spatial_hash_map)
        .entities()
        .for_each(|entity| {
            neighbors.push(entity);
            if let Ok(mut material) = materials.get_mut(entity) {
                **material = material_presets.highlight.clone_weak();
            }
        });

    // Time this separately, otherwise we just ending up timing how long allocations and pushing
    // to a vec take. Here, we just want to measure how long it takes to library to fulfill the
    // query, so we do as little extra computation as possible.
    //
    // The neighbor query is lazy, which means it only does work when we consume the iterator.
    let lookup_start = Instant::now();
    let total = spatial_hash_map
        .flood(hash, None)
        .map(|neighbor| neighbor.1.entities.len())
        .sum::<usize>();
    let elapsed = lookup_start.elapsed();

    let (mut text, mut stats) = text.single_mut();
    stats.0.truncate(0);
    stats.0.push_front(elapsed);
    let avg = stats
        .0
        .iter()
        .sum::<Duration>()
        .div_f32(stats.0.len() as f32);
    text.0 = format!(
        "\
Population: {: >8} Entities

Neighbor Flood Fill: {: >8.1?}
Neighbors: {: >9} Entities

Spatial Hashing
Moved Cells: {: >7?} Entities
Compute Hashes: {: >13.1?}
Update Maps: {: >16.1?}

Transform Propagation
Cell Recentering: {: >11.1?}
LP Root: {: >20.1?}
Frame Origin: {: >15.1?}
LP Propagation: {: >13.1?}
HP Propagation: {: >13.1?}

Total: {: >22.1?}",
        N_ENTITIES,
        //
        avg,
        total,
        //
        hash_stats.avg().moved_cell_entities(),
        hash_stats.avg().hash_update_duration(),
        hash_stats.avg().map_update_duration(),
        //
        prop_stats.avg().grid_recentering(),
        prop_stats.avg().low_precision_root_tagging(),
        prop_stats.avg().local_origin_propagation(),
        prop_stats.avg().low_precision_propagation(),
        prop_stats.avg().high_precision_propagation(),
        //
        prop_stats.avg().total() + hash_stats.avg().total(),
    );
}

fn spawn(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    material_presets: Res<MaterialPresets>,
) {
    use turborand::prelude::*;
    let rng = Rng::with_seed(342525);
    let noise = Perlin::new(345612);

    let rng = || loop {
        let noise_scale = 5.0;
        let threshold = 0.6;
        let rng_val = || rng.f64_normalized() * noise_scale;
        let coord = [rng_val(), rng_val(), rng_val()];
        if noise.get(coord) > threshold {
            return DVec3::from_array(coord).as_vec3() * HALF_WIDTH * CELL_WIDTH
                / noise_scale as f32;
        }
    };

    let values: Vec<_> = std::iter::repeat_with(rng).take(N_ENTITIES).collect();

    let sphere_mesh_lq = meshes.add(
        Sphere::new(HALF_WIDTH / (N_ENTITIES as f32).powf(0.33) * 1.1)
            .mesh()
            .ico(0)
            .unwrap(),
    );

    commands.spawn_big_space::<i32>(ReferenceFrame::new(CELL_WIDTH, 0.0), |root| {
        root.spawn_spatial((
            FloatingOrigin,
            Camera3d::default(),
            Camera {
                hdr: true,
                ..Default::default()
            },
            Tonemapping::AcesFitted,
            Transform::from_xyz(0.0, 0.0, HALF_WIDTH * CELL_WIDTH * 2.0),
            big_space::camera::CameraController::default()
                .with_smoothness(0.98, 0.93)
                .with_slowing(false)
                .with_speed(15.0),
            Fxaa::default(),
            Bloom::default(),
            GridCell::new(0, 0, HALF_WIDTH as i32 / 2),
        ))
        .with_children(|b| {
            b.spawn(DirectionalLight::default());
        });

        for (i, value) in values.iter().enumerate() {
            let mut sphere_builder = root.spawn((BigSpatialBundle::<i32> {
                transform: Transform::from_xyz(value.x, value.y, value.z),
                ..default()
            },));
            if i == 0 {
                sphere_builder.insert((
                    Player,
                    Mesh3d(meshes.add(Sphere::new(1.0))),
                    MeshMaterial3d(materials.add(Color::from(Srgba::new(20.0, 20.0, 0.0, 1.0)))),
                    Transform::from_scale(Vec3::splat(2.0)),
                ));
            } else {
                sphere_builder.insert((
                    NonPlayer,
                    Mesh3d(sphere_mesh_lq.clone()),
                    MeshMaterial3d(material_presets.default.clone_weak()),
                    VisibilityRange {
                        start_margin: 1.0..5.0,
                        end_margin: HALF_WIDTH * CELL_WIDTH * 2.0..HALF_WIDTH * CELL_WIDTH * 3.0,
                        use_aabb: false,
                    },
                ));
            }
        }
    });
}

#[derive(Component)]
struct StatsText(VecDeque<Duration>);

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Node {
                width: Val::Auto,
                height: Val::Auto,
                padding: UiRect::all(Val::Px(16.)),
                margin: UiRect::all(Val::Px(12.)),
                border: UiRect::all(Val::Px(1.)),
                ..default()
            },
            BorderRadius::all(Val::Px(8.0)),
            BorderColor(Color::linear_rgba(0.03, 0.03, 0.03, 0.95)),
            BackgroundColor(Color::linear_rgba(0.012, 0.012, 0.012, 0.95)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::default(),
                TextFont {
                    font: asset_server.load("fonts/FiraMono-Regular.ttf"),
                    font_size: 14.0,
                    ..default()
                },
                StatsText(Default::default()),
            ));
        });
}

fn cursor_grab(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let mut primary_window = windows.single_mut();
    if mouse.just_pressed(MouseButton::Left) {
        primary_window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        primary_window.cursor_options.visible = false;
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        primary_window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
        primary_window.cursor_options.visible = true;
    }
}
