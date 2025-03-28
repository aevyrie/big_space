//! A [`rapier3d`] implementation of [`BigPhysicsBackend`].

use crate::physics::*;

#[cfg(feature = "serde-serialize")]
use serde::*;

#[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
pub struct RapierContext {
    entity_map: EntityHashMap<(RigidBodyHandle, ColliderHandle)>,
    gravity: Vector<f32>,
    integration_parameters: IntegrationParameters,
    islands: IslandManager,
    broad_phase: Box<dyn BroadPhase>,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: Option<QueryPipeline>,
    hooks: Box<dyn PhysicsHooks>,
    events: Box<dyn EventHandler>,
    pipeline: PhysicsPipeline,
}

impl Default for RapierContext {
    fn default() -> Self {
        Self {
            entity_map: Default::default(),
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
            pipeline: Default::default(),
        }
    }
}

impl BigPhysicsBackend for RapierContext {
    fn move_origin(&mut self, translation: Vec3) {
        for (_, body) in self.bodies.iter_mut() {
            let mut iso = *body.position();
            iso.translation.vector -= Vector::from(translation);
            body.set_position(iso, false);
            body.set_next_kinematic_position(iso);
        }
    }

    fn step(&mut self) {
        self.pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.islands,
            self.broad_phase.as_mut(),
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            self.query_pipeline.as_mut(),
            self.hooks.as_ref(),
            self.events.as_ref(),
        );
    }

    fn insert_or_update_entity(
        &mut self,
        entity: Entity,
        properties: &BigRigidBody,
        isometry: (Vec3, Quat),
    ) {
        let shape = match properties.shape {
            Shape::Cuboid(primitives::Cuboid { half_size: l }) => {
                SharedShape::cuboid(l.x, l.y, l.z)
            }
            Shape::Sphere(primitives::Sphere { radius }) => SharedShape::ball(radius),
            Shape::Capsule(primitives::Capsule3d {
                radius,
                half_length,
            }) => SharedShape::capsule(
                (Vec3::Y * half_length).into(),
                (Vec3::Y * half_length).into(),
                radius,
            ),
        };

        let update_mass = |collider: &mut Collider| match properties.inertia {
            Inertia::Automatic { mass } => {
                collider.set_mass(mass);
            }
            Inertia::Manual {
                mass,
                center_of_mass_local,
                principal_inertia,
                principal_inertia_local_frame,
            } => {
                let com = center_of_mass_local;
                let pmi = principal_inertia;
                collider.set_mass_properties(MassProperties::with_principal_inertia_frame(
                    Point::new(com.x, com.y, com.z),
                    mass,
                    AngVector::new(pmi.x, pmi.y, pmi.z),
                    principal_inertia_local_frame.into(),
                ))
            }
        };

        match self.entity_map.get(&entity) {
            // Insert New
            None => {
                // Behavior
                let pos = Isometry::from_parts(isometry.0.into(), isometry.1.into());
                let body = match properties.behavior {
                    Behavior::Fixed => RigidBodyBuilder::fixed(),
                    Behavior::Dynamic(_) => RigidBodyBuilder::dynamic(),
                    Behavior::Kinematic(_) => RigidBodyBuilder::kinematic_velocity_based(),
                }
                .position(pos);
                let body = self.bodies.insert(body);

                // Shape
                let mut collider = ColliderBuilder::new(shape).build();

                // Inertia
                update_mass(&mut collider);

                let collider = self
                    .colliders
                    .insert_with_parent(collider, body, &mut self.bodies);
                self.entity_map.insert(entity, (body, collider));
            }
            // Update Existing
            Some((body, collider)) => {
                // TODO: per-property change detection - only change the properties that are
                // actually different.

                // Behavior
                let body = &mut self.bodies[*body];
                body.set_body_type(
                    match properties.behavior {
                        Behavior::Fixed => RigidBodyType::Fixed,
                        Behavior::Dynamic(_) => RigidBodyType::Dynamic,
                        Behavior::Kinematic(_) => RigidBodyType::KinematicVelocityBased,
                    },
                    true,
                );

                // Shape
                let collider = &mut self.colliders[*collider];
                collider.set_shape(shape);

                // Inertia
                update_mass(collider)
            }
        }
    }

    fn remove_entity(&mut self, entity: Entity) {
        let Some((body, _collider)) = self.entity_map.remove(&entity) else {
            return;
        };
        self.bodies.remove(
            body,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    #[inline(always)]
    fn position(&self, entity: Entity) -> Option<(Vec3, Quat)> {
        let (body, _collider) = self.entity_map.get(&entity)?;
        let body = self.bodies.get(*body)?;
        let position = body.position();
        Some((position.translation.into(), position.rotation.into()))
    }
}

// fn move_entity(
//     entity: Entity,
//     from_body: RigidBodyHandle,
//     from_collider: ColliderHandle,
//     from: &mut RapierContext,
//     into: &mut RapierContext,
// ) {
//     if let Some((from_collider, from_body)) = from
//         .colliders
//         .remove(from_collider, &mut from.islands, &mut from.bodies, false)
//         .zip(from.bodies.remove(
//             from_body,
//             &mut from.islands,
//             &mut from.colliders,
//             &mut from.impulse_joints,
//             &mut from.multibody_joints,
//             false,
//         ))
//     {
//         let into_body = into.bodies.insert(RigidBody::from(from_body));
//         let into_collider = into.colliders.insert_with_parent(
//             Collider:: from(from_collider),
//             into_body,
//             &mut into.bodies,
//         );
//         into.entity_map.insert(entity, (into_body, into_collider));
//     }
// }

// fn migrate_single(&mut self, entity: Entity, other: &mut BigPhysics, other_to_self: Vec3) {
//     let Some(other) = other.context.downcast_mut::<Self>() else {
//         error!(
//             "Cannot migrate an entity from {} into a different type of physics context.",
//             std::any::type_name::<Self>()
//         );
//         return;
//     };

//     let Some((body, collider)) = other.entity_map.remove(&entity) else {
//         return;
//     };

//     // Move the body into the destination coordinate space.
//     if let Some(body) = self.bodies.get_mut(body) {
//         let mut iso = *body.position();
//         iso.translation.vector += Vector::from(other_to_self);
//         body.set_position(iso, false);
//     } else {
//         return;
//     }

//     move_entity(entity, body, collider, self, other);
// }

// fn consume(&mut self, mut other: BigPhysics, other_to_self: Vec3) {
//     let Some(other) = other.context.downcast_mut::<Self>() else {
//         error!(
//             "Cannot merge a {} physics context into a different type of physics context.",
//             std::any::type_name::<Self>()
//         );
//         return;
//     };
//     // Move the origin so the other bodies are in the destination coordinate system.
//     other.move_origin(other_to_self);
//     // Move all pre-translated entities into the destination physics context
//     let entities_to_move = std::mem::take(&mut other.entity_map);
//     for (entity, (body, collider)) in entities_to_move {
//         move_entity(entity, body, collider, self, other);
//     }
// }
