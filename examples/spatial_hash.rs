//! Demonstrates the included optional spatial hashing and partitioning of grid cells.
//!
//! Spawns multiple independent grids to stress-test worlds with many smaller grids
//! rather than one mega grid.

use bevy::{
    core_pipeline::tonemapping::Tonemapping, post_process::bloom::Bloom, prelude::*,
    render::view::Hdr, window::CursorOptions,
};
use bevy_ecs::entity::EntityHasher;
use bevy_math::DVec3;
use bevy_tasks::{available_parallelism, ComputeTaskPool, TaskPoolBuilder};
use big_space::prelude::*;
use core::hash::Hasher;
use noise::{NoiseFn, Simplex};
use turborand::prelude::*;

/// How fast the non-stationary entities oscillate, causing them to move into neighboring cells.
const MOVEMENT_SPEED: f32 = 5.0;

/// Grid configurations to spawn on startup. Each entry produces an independent sub-grid
/// positioned so they don't overlap.
const GRIDS: &[GridConfig] = &[
    GridConfig {
        seed: 111,
        half_width: 15.0,
        cell_width: 15.0,
        percent_static: 0.999,
        has_player: true,
        initial_entities: 1_000,
    },
    GridConfig {
        seed: 222,
        half_width: 15.0,
        cell_width: 15.0,
        percent_static: 1.0,
        has_player: false,
        initial_entities: 1_000,
    },
    GridConfig {
        seed: 333,
        half_width: 15.0,
        cell_width: 15.0,
        percent_static: 0.999,
        has_player: false,
        initial_entities: 1_000,
    },
    GridConfig {
        seed: 444,
        half_width: 15.0,
        cell_width: 15.0,
        percent_static: 1.0,
        has_player: false,
        initial_entities: 1_000,
    },
];

/// Configuration for a single stress-test grid.
///
/// Stored as a component on the grid entity so systems can look up per-grid settings.
#[derive(Component, Clone, Debug)]
struct GridConfig {
    /// RNG seed for reproducible entity placement.
    seed: u64,
    /// Half the number of cells across each dimension. Controls the spread of spawned entities.
    half_width: f32,
    /// Edge length of each cell in this grid.
    cell_width: f32,
    /// Fraction of entities that are [`Stationary`] (0.0 to 1.0).
    percent_static: f32,
    /// Whether this grid contains the roaming [`Player`] entity used for neighbor highlighting.
    has_player: bool,
    /// Number of entities to spawn on startup.
    initial_entities: usize,
}

impl GridConfig {
    /// World-space extent of this grid along one axis.
    fn extent(&self) -> f32 {
        self.half_width * self.cell_width * 2.0
    }
}

fn main() {
    ComputeTaskPool::get_or_init(|| {
        TaskPoolBuilder::new()
            .num_threads(available_parallelism())
            .build()
    });

    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.build().disable::<TransformPlugin>(),
        BigSpaceDefaultPlugins,
        CellHashingPlugin::default(),
        PartitionPlugin::default(),
        PartitionChangePlugin::default(),
    ))
    .add_systems(Startup, (spawn, setup_ui))
    .add_systems(
        PostUpdate,
        (
            move_player
                .after(TransformSystems::Propagate)
                .after(SpatialHashSystems::UpdateCellLookup),
            draw_grid_axes.after(TransformSystems::Propagate),
            draw_partitions.after(SpatialHashSystems::UpdatePartitionLookup),
            highlight_changed_entities.after(draw_partitions),
        ),
    )
    .add_systems(Update, (cursor_grab, spawn_spheres))
    .init_resource::<MaterialPresets>();

    app.run();
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
    changed: Handle<StandardMaterial>,
    sphere: Handle<Mesh>,
}

impl FromWorld for MaterialPresets {
    fn from_world(world: &mut World) -> Self {
        // Use the first grid's half_width for sphere size, or a reasonable default.
        let half_width = GRIDS.first().map_or(50.0, |g| g.half_width);

        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();

        let default = materials.add(StandardMaterial {
            base_color: Color::from(Srgba::new(0.5, 0.5, 0.5, 1.0)),
            perceptual_roughness: 0.2,
            metallic: 0.0,
            ..Default::default()
        });
        let highlight = materials.add(Color::from(Srgba::new(2.0, 0.0, 8.0, 1.0)));
        let flood = materials.add(Color::from(Srgba::new(1.1, 0.1, 1.0, 1.0)));
        let changed = materials.add(Color::from(Srgba::new(10.0, 0.0, 0.0, 1.0)));

        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let sphere = meshes.add(
            Sphere::new(half_width / 1_000_000_f32.powf(0.33) * 0.5)
                .mesh()
                .ico(0)
                .unwrap(),
        );

        Self {
            default,
            highlight,
            flood,
            changed,
            sphere,
        }
    }
}

fn draw_grid_axes(mut gizmos: Gizmos, grids: Query<(&GlobalTransform, &Grid), With<GridConfig>>) {
    for (gt, grid) in grids.iter() {
        let origin = gt.translation();
        let len = grid.cell_edge_length() * 2.0;
        gizmos.ray(origin, gt.right() * len, Color::linear_rgb(1.0, 0.0, 0.0));
        gizmos.ray(origin, gt.up() * len, Color::linear_rgb(0.0, 1.0, 0.0));
        gizmos.ray(origin, gt.back() * len, Color::linear_rgb(0.0, 0.0, 1.0));
    }
}

fn draw_partitions(
    mut gizmos: Gizmos,
    partitions: Res<PartitionLookup>,
    grids: Query<(&GlobalTransform, &Grid)>,
    camera: Query<&CellId, With<Camera>>,
) -> Result {
    let camera = camera.single()?;

    for (id, p) in partitions.iter().take(10_000) {
        let Ok((transform, grid)) = grids.get(p.grid()) else {
            return Ok(());
        };
        let l = grid.cell_edge_length();

        let mut hasher = EntityHasher::default();
        hasher.write_u64(id.id());
        let f = hasher.finish();
        let hue = (f % 360) as f32;

        p.iter()
            .filter(|hash| *hash != camera)
            .take(1_000)
            .for_each(|h| {
                let center = [h.coord().x as i32, h.coord().y as i32, h.coord().z as i32];
                let local_trans = Transform::from_translation(IVec3::from(center).as_vec3() * l)
                    .with_scale(Vec3::splat(l));
                gizmos.cube(
                    transform.mul_transform(local_trans),
                    Hsla::new(hue, 1.0, 0.5, 0.2),
                );
            });

        let min = IVec3::from([p.min().x as i32, p.min().y as i32, p.min().z as i32]).as_vec3() * l;
        let max = IVec3::from([p.max().x as i32, p.max().y as i32, p.max().z as i32]).as_vec3() * l;

        let size = max - min;
        let center = min + (size) * 0.5;
        let local_trans = Transform::from_translation(center).with_scale(size + l * 2.0);

        gizmos.cube(
            transform.mul_transform(local_trans),
            Hsla::new(hue, 1.0, 0.5, 0.9),
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn move_player(
    time: Res<Time>,
    mut player: Query<(&mut Transform, &mut CellCoord, &ChildOf, &CellId), With<Player>>,
    mut non_player: Query<
        (&mut Transform, &mut CellCoord, &ChildOf),
        (Without<Player>, With<NonPlayer>, Without<Stationary>),
    >,
    count: Query<(), With<NonPlayer>>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>, Without<Player>>,
    mut neighbors: Local<Vec<Entity>>,
    grids: Query<(&Grid, Option<&GridConfig>)>,
    hash_grid: Res<CellLookup>,
    material_presets: Res<MaterialPresets>,
    mut text: Query<&mut Text>,
    hash_stats: Res<big_space::timing::SmoothedStat<big_space::timing::GridHashStats>>,
    prop_stats: Res<big_space::timing::SmoothedStat<big_space::timing::PropagationStats>>,
) -> Result {
    let n_entities = count.count();
    for neighbor in neighbors.iter() {
        if let Ok(mut material) = materials.get_mut(*neighbor) {
            material.set_if_neq(material_presets.default.clone().into());
        }
    }

    let t = time.elapsed_secs() * 1.0;
    let scale = MOVEMENT_SPEED;
    if scale.abs() > 0.0 {
        for (mut transform, _, parent) in non_player.iter_mut() {
            // Scale movement relative to the parent grid's half_width so entities don't
            // immediately escape smaller grids.
            let hw = grids
                .get(parent.parent())
                .ok()
                .and_then(|(_, cfg)| cfg)
                .map_or(50.0, |c| c.half_width);
            let s = scale / hw;
            transform.translation.x += t.sin() * s;
            transform.translation.y += t.cos() * s;
            transform.translation.z += (t * 2.3).sin() * s;
        }
    }

    // Move the player along a path within its parent grid's extent.
    let (mut transform, mut cell, child_of, hash) = player.single_mut()?;
    let (grid, cfg) = grids.get(child_of.parent())?;
    let hw = cfg.map_or(50.0, |c| c.half_width);
    let cw = grid.cell_edge_length();

    let t = time.elapsed_secs() * 0.01;
    let absolute_pos =
        hw * cw * 0.8 * Vec3::new((5.0 * t).sin(), (7.0 * t).cos(), (20.0 * t).sin());
    (*cell, transform.translation) = grid.imprecise_translation_to_grid(absolute_pos);

    neighbors.clear();

    // hash_grid.flood(hash, None).entities().for_each(|entity| {
    //     neighbors.push(entity);
    //     if let Ok(mut material) = materials.get_mut(entity) {
    //         material.set_if_neq(material_presets.flood.clone().into());
    //     }
    // });
    //
    // hash_grid
    //     .get(hash)
    //     .unwrap()
    //     .nearby(&hash_grid)
    //     .entities()
    //     .for_each(|entity| {
    //         neighbors.push(entity);
    //         if let Ok(mut material) = materials.get_mut(entity) {
    //             material.set_if_neq(material_presets.highlight.clone().into());
    //         }
    //     });

    let mut text = text.single_mut()?;
    text.0 = format!(
        "\
Controls:
WASD to move, QE to roll
F to spawn 1,000/grid, G to double

Grids: {: >15}
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
        GRIDS.len(),
        n_entities
            .to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(core::str::from_utf8)
            .collect::<Result<Vec<&str>, _>>()?
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

    Ok(())
}

fn spawn(mut commands: Commands, material_presets: Res<MaterialPresets>) {
    // Compute positions: lay grids out in a line along the X axis with spacing.
    let spacing = 1.5; // multiplier for gap between grids
    let mut offsets: Vec<f32> = Vec::with_capacity(GRIDS.len());
    let mut x_cursor: f32 = 0.0;
    for (i, cfg) in GRIDS.iter().enumerate() {
        if i > 0 {
            x_cursor += GRIDS[i - 1].extent() * 0.5 * spacing + cfg.extent() * 0.5 * spacing;
        }
        offsets.push(x_cursor);
    }
    // Center the whole layout around the origin.
    let total_center = if offsets.is_empty() {
        0.0
    } else {
        (offsets[0] + offsets[offsets.len() - 1]) * 0.5
    };

    // Use the first grid's cell width for the root BigSpace. The sub-grids each have their own.
    let root_cell_width = GRIDS.first().map_or(10.0, |g| g.cell_width);

    commands.spawn_big_space(Grid::new(root_cell_width, 0.0), |root| {
        // Camera as a direct child of the root.
        let first_half_width = GRIDS.first().map_or(50.0, |g| g.half_width);
        let first_cell_width = GRIDS.first().map_or(10.0, |g| g.cell_width);
        root.spawn_spatial((
            FloatingOrigin,
            Camera3d::default(),
            Hdr,
            Camera::default(),
            Tonemapping::AcesFitted,
            Transform::from_xyz(0.0, 0.0, first_half_width * first_cell_width * 2.0),
            BigSpaceCameraController::default()
                .with_smoothness(0.98, 0.93)
                .with_slowing(false)
                .with_speed(15.0),
            Bloom::default(),
            CellCoord::new(0, 0, first_half_width as GridPrecision / 2),
        ))
        .with_children(|b| {
            b.spawn(DirectionalLight::default());
        });

        // Spawn each configured sub-grid.
        for (i, cfg) in GRIDS.iter().enumerate() {
            let x_offset = offsets[i] - total_center;
            let grid = Grid::new(cfg.cell_width, 0.0);

            root.with_grid(grid, |sub_grid| {
                // Derive a deterministic rotation from the seed so each grid is visibly misaligned.
                let angle = (cfg.seed as f32) % core::f32::consts::TAU;
                let rotation = Quat::from_euler(EulerRot::YXZ, angle, angle * 0.7, angle * 0.3);
                sub_grid.insert((
                    Transform::from_xyz(x_offset, 0.0, 0.0).with_rotation(rotation),
                    cfg.clone(),
                ));

                if cfg.has_player {
                    sub_grid.spawn_spatial(Player);
                }

                // Spawn initial entities.
                let grid_entity = sub_grid.id();
                spawn_entities_in_grid(
                    sub_grid.commands(),
                    grid_entity,
                    cfg,
                    cfg.initial_entities,
                    &material_presets,
                );
            });
        }
    });
}

fn spawn_spheres(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    material_presets: Res<MaterialPresets>,
    grids: Query<(Entity, &GridConfig)>,
    non_players: Query<(), With<NonPlayer>>,
) -> Result {
    let total_existing = non_players.iter().len().max(1);
    let n_spawn_per_grid = if input.pressed(KeyCode::KeyG) {
        // Double: distribute evenly across grids.
        total_existing / GRIDS.len().max(1)
    } else if input.pressed(KeyCode::KeyF) {
        1_000
    } else {
        return Ok(());
    };

    for (entity, cfg) in grids.iter() {
        spawn_entities_in_grid(
            &mut commands,
            entity,
            cfg,
            n_spawn_per_grid,
            &material_presets,
        );
    }
    Ok(())
}

/// Single spawn point for all stress-test entities. Both startup and runtime spawning
/// funnel through here so there is one place to tweak the entity bundle.
fn spawn_entities_in_grid(
    commands: &mut Commands,
    grid_entity: Entity,
    cfg: &GridConfig,
    count: usize,
    _material_presets: &MaterialPresets,
) {
    let noise = Simplex::new(cfg.seed as u32);
    let rng = Rng::with_seed(cfg.seed);
    let num_moving = ((1.0 - cfg.percent_static) * count as f32) as usize;

    commands.entity(grid_entity).with_children(|builder| {
        for (i, value) in
            sample_noise(count, cfg.half_width, cfg.cell_width, &noise, &rng).enumerate()
        {
            let hash = CellId::new_manual(grid_entity, &CellCoord::default());

            // -- Common components for every entity. Comment out lines here to
            //    disable mesh rendering / visibility etc. across all spawn paths. --
            let common = (
                Transform::from_xyz(value.x, value.y, value.z),
                GlobalTransform::default(),
                CellCoord::default(),
                CellHash::from(hash),
                hash,
                NonPlayer,
                // Mesh3d(_material_presets.sphere.clone()),
                // MeshMaterial3d(_material_presets.default.clone()),
                // bevy_camera::visibility::VisibilityRange {
                //     start_margin: 1.0..5.0,
                //     end_margin: cfg.half_width * cfg.cell_width * 0.5
                //         ..cfg.half_width * cfg.cell_width * 0.8,
                //     use_aabb: false,
                // },
                // bevy_camera::visibility::NoFrustumCulling,
            );

            // Branch to avoid an extra .insert() and archetype move.
            if i < num_moving {
                builder.spawn(common);
            } else {
                builder.spawn((common, Stationary));
            }
        }
    });
}

#[inline]
fn sample_noise<'a, T: NoiseFn<f64, 3>>(
    n_entities: usize,
    half_width: f32,
    cell_width: f32,
    noise: &'a T,
    rng: &'a Rng,
) -> impl Iterator<Item = Vec3> + use<'a, T> {
    core::iter::repeat_with(move || loop {
        let noise_scale = 0.05 * half_width as f64;
        let threshold = 0.50;
        let rng_val = || rng.f64_normalized() * noise_scale;
        let coord = [rng_val(), rng_val(), rng_val()];
        if noise.get(coord) > threshold {
            return DVec3::from_array(coord).as_vec3() * half_width * cell_width
                / noise_scale as f32;
        }
    })
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
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BorderColor::all(Color::linear_rgba(0.03, 0.03, 0.03, 0.95)),
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
    mut windows: Query<&mut CursorOptions, With<bevy::window::PrimaryWindow>>,
) -> Result {
    let mut cursor_options = windows.single_mut()?;
    if mouse.just_pressed(MouseButton::Left) {
        cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor_options.visible = false;
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
        cursor_options.visible = true;
    }
    Ok(())
}

fn highlight_changed_entities(
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    material_presets: Res<MaterialPresets>,
    entity_partitions: Res<PartitionEntities>,
    mut active: Local<Vec<(Entity, u8)>>,
) {
    let mut next_active: Vec<(Entity, u8)> =
        Vec::with_capacity(active.len() + entity_partitions.changed.len());

    for entity in entity_partitions.changed.keys().copied() {
        if let Ok(mut mat) = materials.get_mut(entity) {
            mat.set_if_neq(material_presets.changed.clone().into());
        }
        next_active.push((entity, 10));
    }

    for (entity, mut frames_left) in active.drain(..) {
        if entity_partitions.changed.contains_key(&entity) {
            continue;
        }
        if frames_left > 0 {
            frames_left -= 1;
            if frames_left > 0 {
                if let Ok(mut mat) = materials.get_mut(entity) {
                    mat.set_if_neq(material_presets.changed.clone().into());
                }
                next_active.push((entity, frames_left));
            } else {
                if let Ok(mut mat) = materials.get_mut(entity) {
                    mat.set_if_neq(material_presets.default.clone().into());
                }
            }
        }
    }

    *active = next_active;
}
