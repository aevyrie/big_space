use bevy_ecs::prelude::*;
use bevy_math::{DVec3, Vec3};
use rapier3d::prelude::*;
use crate::prelude::{GridCell, GridPrecision};

#[derive(Component)]
pub struct BigPhysics {
    context: Box<dyn BigPhysicsContext>,
}

/// Implement this trait to
pub trait BigPhysicsContext {
    /// Move the origin of the physics simulation by a translational offset.
    ///
    /// This occurs when a physics origin is recentered to be closer to the centroid of the entities
    /// in the simulation, to maximize precision. This should be implemented by updating the
    /// translation of all entities by `-offset`.
    fn move_origin(&mut self, offset: DVec3);

    /// Advance the simulation by one time step.
    fn step(&mut self);

    /// Update an entity's properties in the simulation, inserting it if it does not exist.
    fn update(&mut self, entity: Entity, properties: &BigPhysicsProps);

    /// Remove an entity from the simulation.
    fn remove(&mut self, entity: Entity);

    /// Merge a simulation into this simulation.
    fn merge(&mut self, other: &Self);

    /// Return the physics position of the entity, relative to the physics origin.
    fn position(&self, entity: Entity) -> Option<DVec3>;
}

pub struct BigPhysicsProps {
    /// kg
    mass_kg: f32,
    /// m
    center_of_mass_local: Vec3,
    /// radians
    principal_moment_of_inertia_axis: Vec3,
    /// kg·m²
    principal_moment_of_inertia: f32,
}

use std::sync::mpsc::channel;
pub struct RapierContext {
    gravity: Vector<f32>,
    integration_parameters: IntegrationParameters,
    islands: IslandManager,
    broad_phase: Box<dyn BroadPhase>,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints:  MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: Option<QueryPipeline>,
    hooks: Box<dyn PhysicsHooks>,
    events: Box<dyn EventHandler>,
    pipeline: PhysicsPipeline,
}

impl Default for RapierContext {
    fn default() -> Self {
        Self {
            gravity: Default::default(),
            integration_parameters: Default::default(),
            islands: Default::default(),
            broad_phase: Box::new(DefaultBroadPhase::new()),
            narrow_phase: Default::default(),
            bodies: Default::default(),
            colliders: Default::default(),
            impulse_joints: Default::default(),
            multibody_joints: Default::default(),
            ccd_solver: Default::default(),
            query_pipeline: None,
            hooks: Box::new(()),
            events: Box::new(()),
            pipeline: PhysicsPipeline::new(),
        }
    }
}

fn tst() {
    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    /* Create the ground. */
    let collider = ColliderBuilder::cuboid(100.0, 0.1, 100.0).build();
    collider_set.insert(collider);

    /* Create the bounding ball. */
    let rigid_body = RigidBodyBuilder::dynamic()
        .translation(vector![0.0, 10.0, 0.0])
        .build();
    let collider = ColliderBuilder::ball(0.5).restitution(0.7).build();
    let ball_body_handle = rigid_body_set.insert(rigid_body);
    collider_set.insert_with_parent(collider, ball_body_handle, &mut rigid_body_set);


    /* Create other structures necessary for the simulation. */
    let gravity = vector![0.0, -9.81, 0.0];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = DefaultBroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut impulse_joint_set = ImpulseJointSet::new();
    let mut multibody_joint_set = MultibodyJointSet::new();
    let mut ccd_solver = CCDSolver::new();
    let mut query_pipeline = QueryPipeline::new();
    let physics_hooks = ();
    let event_handler = ();

    /* Run the game loop, stepping the simulation once per frame. */
    for _ in 0..200 {
        physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut island_manager,
            &mut broad_phase,
            &mut narrow_phase,
            &mut rigid_body_set,
            &mut collider_set,
            &mut impulse_joint_set,
            &mut multibody_joint_set,
            &mut ccd_solver,
            Some(&mut query_pipeline),
            &physics_hooks,
            &event_handler,
        );

        let ball_body = &rigid_body_set[ball_body_handle];
        eprintln!("Ball altitude: {}", ball_body.translation().y);
    }
}