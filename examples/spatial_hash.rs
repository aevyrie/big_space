use std::hash::Hasher;

use bevy::{
    core_pipeline::{bloom::Bloom, fxaa::Fxaa, tonemapping::Tonemapping},
    prelude::*,
};
use bevy_ecs::entity::EntityHasher;
use bevy_math::DVec3;
use big_space::prelude::*;
use noise::{NoiseFn, Simplex};
use smallvec::SmallVec;
use turborand::prelude::*;

// Try bumping this up to really stress test. I'm able to push a million entities with an M3 Max.
const HALF_WIDTH: f32 = 50.0;
const CELL_WIDTH: f32 = 10.0;
// How fast the entities should move, causing them to move into neighboring cells.
const MOVEMENT_SPEED: f32 = 5.0;
const PERCENT_STATIC: f32 = 0.99;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            BigSpacePlugin::default(),
            GridHashPlugin::<()>::default(),
            GridPartitionPlugin::<()>::default(),
            big_space::camera::CameraControllerPlugin::default(),
        ))
        .add_systems(Startup, (spawn, setup_ui))
        .add_systems(
            PostUpdate,
            (
                move_player.after(TransformSystem::TransformPropagate),
                draw_partitions.after(GridHashMapSystem::UpdatePartition),
            ),
        )
        .add_systems(Update, (cursor_grab, spawn_spheres))
        .init_resource::<MaterialPresets>()
        .run();
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct NonPlayer;

#[derive(Resource)]
struct MaterialPresets {
    default: Handle<StandardMaterial>,
    highlight: Handle<StandardMaterial>,
    flood: Handle<StandardMaterial>,
    sphere: Handle<Mesh>,
}

impl FromWorld for MaterialPresets {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        let default = materials.add(StandardMaterial {
            base_color: Color::from(Srgba::new(0.5, 0.5, 0.5, 1.0)),
            perceptual_roughness: 0.2,
            metallic: 0.0,
            ..Default::default()
        });
        let highlight = materials.add(Color::from(Srgba::new(2.0, 0.0, 8.0, 1.0)));
        let flood = materials.add(Color::from(Srgba::new(1.1, 0.1, 1.0, 1.0)));

        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let sphere = meshes.add(
            Sphere::new(HALF_WIDTH / (1_000_000_f32).powf(0.33) * 0.5)
                .mesh()
                .ico(0)
                .unwrap(),
        );

        Self {
            default,
            highlight,
            flood,
            sphere,
        }
    }
}

fn draw_partitions(
    mut gizmos: Gizmos,
    partitions: Res<GridPartitionMap>,
    grids: Query<(&GlobalTransform, &Grid)>,
    camera: Query<&GridHash, With<Camera>>,
) {
    for (id, p) in partitions.iter().take(10_000) {
        let Ok((transform, grid)) = grids.get(p.grid()) else {
            return;
        };
        let l = grid.cell_edge_length();

        let mut hasher = EntityHasher::default();
        hasher.write_u64(id.id());
        let f = hasher.finish();
        let hue = (f % 360) as f32;

        p.iter()
            .filter(|hash| *hash != camera.single())
            .take(1_000)
            .for_each(|h| {
                let center = [h.cell().x as i32, h.cell().y as i32, h.cell().z as i32];
                let local_trans = Transform::from_translation(IVec3::from(center).as_vec3() * l)
                    .with_scale(Vec3::splat(l));
                gizmos.cuboid(
                    transform.mul_transform(local_trans),
                    Hsla::new(hue, 1.0, 0.5, 0.05),
                );
            });

        let min = IVec3::from([p.min().x as i32, p.min().y as i32, p.min().z as i32]).as_vec3() * l;
        let max = IVec3::from([p.max().x as i32, p.max().y as i32, p.max().z as i32]).as_vec3() * l;

        let size = max - min;
        let center = min + (size) * 0.5;
        let local_trans = Transform::from_translation(center).with_scale(size + l * 2.0);

        gizmos.cuboid(
            transform.mul_transform(local_trans),
            Hsla::new(hue, 1.0, 0.5, 0.2),
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn move_player(
    time: Res<Time>,
    mut _gizmos: Gizmos,
    mut player: Query<(&mut Transform, &mut GridCell, &Parent, &GridHash), With<Player>>,
    mut non_player: Query<
        (&mut Transform, &mut GridCell, &Parent),
        (Without<Player>, With<NonPlayer>),
    >,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>, Without<Player>>,
    mut neighbors: Local<Vec<Entity>>,
    grids: Query<&Grid>,
    hash_grid: Res<GridHashMap>,
    material_presets: Res<MaterialPresets>,
    mut text: Query<&mut Text>,
    hash_stats: Res<big_space::timing::SmoothedStat<big_space::timing::GridHashStats>>,
    prop_stats: Res<big_space::timing::SmoothedStat<big_space::timing::PropagationStats>>,
) {
    let n_entities = non_player.iter().len();
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = materials.get_mut(*neighbor) {
            **material = material_presets.default.clone_weak();
        }
    }

    let t = time.elapsed_secs() * 1.0;
    let scale = MOVEMENT_SPEED / HALF_WIDTH;
    if scale.abs() > 0.0 {
        // Avoid change detection
        for (i, (mut transform, _, _)) in non_player.iter_mut().enumerate() {
            if i < ((1.0 - PERCENT_STATIC) * n_entities as f32) as usize {
                transform.translation.x += t.sin() * scale;
                transform.translation.y += t.cos() * scale;
                transform.translation.z += (t * 2.3).sin() * scale;
            } else {
                break;
            }
        }
    }

    let t = time.elapsed_secs() * 0.01;
    let (mut transform, mut cell, parent, hash) = player.single_mut();
    let absolute_pos = HALF_WIDTH
        * CELL_WIDTH
        * 0.8
        * Vec3::new((5.0 * t).sin(), (7.0 * t).cos(), (20.0 * t).sin());
    (*cell, transform.translation) = grids
        .get(parent.get())
        .unwrap()
        .imprecise_translation_to_grid(absolute_pos);

    neighbors.clear();

    hash_grid.flood(hash, None).entities().for_each(|entity| {
        neighbors.push(entity);
        if let Ok(mut material) = materials.get_mut(entity) {
            **material = material_presets.flood.clone_weak();
        }

        // let grid = grid.get(entry.grid).unwrap();
        // let transform = grid.global_transform(
        //     &entry.cell,
        //     &Transform::from_scale(Vec3::splat(grid.cell_edge_length() * 0.99)),
        // );
        // gizmos.cuboid(transform, Color::linear_rgba(1.0, 1.0, 1.0, 0.2));
    });

    hash_grid
        .get(hash)
        .unwrap()
        .nearby(&hash_grid)
        .entities()
        .for_each(|entity| {
            neighbors.push(entity);
            if let Ok(mut material) = materials.get_mut(entity) {
                **material = material_presets.highlight.clone_weak();
            }
        });

    let mut text = text.single_mut();
    text.0 = format!(
        "\
Controls:
WASD to move, QE to roll
F to spawn 1,000, G to double

Population: {: >8} Entities

Transform Propagation
Cell Recentering: {: >11.1?}
LP Root: {: >20.1?}
Frame Origin: {: >15.1?}
LP Propagation: {: >13.1?}
HP Propagation: {: >13.1?}

Spatial Hashing
Moved Cells: {: >7?} Entities
Compute Hashes: {: >13.1?}
Update Maps: {: >16.1?}
Update Partitions: {: >10.1?}

Total: {: >22.1?}",
        n_entities
            .to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(std::str::from_utf8)
            .collect::<Result<Vec<&str>, _>>()
            .unwrap()
            .join(","),
        //
        prop_stats.avg().grid_recentering(),
        prop_stats.avg().low_precision_root_tagging(),
        prop_stats.avg().local_origin_propagation(),
        prop_stats.avg().low_precision_propagation(),
        prop_stats.avg().high_precision_propagation(),
        //
        hash_stats.avg().moved_cell_entities(),
        hash_stats.avg().hash_update_duration(),
        hash_stats.avg().map_update_duration(),
        hash_stats.avg().update_partition(),
        //
        prop_stats.avg().total() + hash_stats.avg().total(),
    );
}

fn spawn(mut commands: Commands) {
    commands.spawn_big_space(Grid::new(CELL_WIDTH, 0.0), |root| {
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
            GridCell::new(0, 0, HALF_WIDTH as GridPrecision / 2),
        ))
        .with_children(|b| {
            b.spawn(DirectionalLight::default());
        });

        root.spawn_spatial(Player);
    });
}

fn spawn_spheres(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    material_presets: Res<MaterialPresets>,
    mut grid: Query<(Entity, &Grid, &mut Children)>,
    non_players: Query<(), With<NonPlayer>>,
) {
    let n_entities = non_players.iter().len().max(1);
    let n_spawn = if input.pressed(KeyCode::KeyG) {
        n_entities
    } else if input.pressed(KeyCode::KeyF) {
        1_000
    } else {
        return;
    };

    let (entity, _grid, mut children) = grid.single_mut();
    let mut dyn_parent = bevy_reflect::DynamicTupleStruct::default();
    dyn_parent.insert(entity);
    let dyn_parent = dyn_parent.as_partial_reflect();

    let new_children = sample_noise(n_spawn, &Simplex::new(345612), &Rng::new())
        .map(|value| {
            let hash = GridHash::__new_manual(entity, &GridCell::default());
            commands
                .spawn((
                    Transform::from_xyz(value.x, value.y, value.z),
                    GlobalTransform::default(),
                    GridCell::default(),
                    FastGridHash::from(hash),
                    hash,
                    NonPlayer,
                    Parent::from_reflect(dyn_parent).unwrap(),
                    Mesh3d(material_presets.sphere.clone_weak()),
                    MeshMaterial3d(material_presets.default.clone_weak()),
                    bevy_render::view::VisibilityRange {
                        start_margin: 1.0..5.0,
                        end_margin: HALF_WIDTH * CELL_WIDTH * 0.5..HALF_WIDTH * CELL_WIDTH * 0.8,
                        use_aabb: false,
                    },
                    bevy_render::view::NoFrustumCulling,
                ))
                .id()
        })
        .chain(children.iter().copied())
        .collect::<SmallVec<[Entity; 8]>>();

    let mut dyn_children = bevy_reflect::DynamicTupleStruct::default();
    dyn_children.insert(new_children);
    let dyn_children = dyn_children.as_partial_reflect();

    *children = Children::from_reflect(dyn_children).unwrap();
}

#[inline]
fn sample_noise<'a, T: NoiseFn<f64, 3>>(
    n_entities: usize,
    noise: &'a T,
    rng: &'a Rng,
) -> impl Iterator<Item = Vec3> + use<'a, T> {
    std::iter::repeat_with(
        || loop {
            let noise_scale = 0.05 * HALF_WIDTH as f64;
            let threshold = 0.50;
            let rng_val = || rng.f64_normalized() * noise_scale;
            let coord = [rng_val(), rng_val(), rng_val()];
            if noise.get(coord) > threshold {
                return DVec3::from_array(coord).as_vec3() * HALF_WIDTH * CELL_WIDTH
                    / noise_scale as f32;
            }
        },
        //  Vec3::ONE
    )
    .take(n_entities)
}

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
