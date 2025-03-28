//! Physics support for `big_space`.

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::entity::EntityHashMap;
use bevy_ecs::prelude::*;
use bevy_math::{primitives, Quat, Vec3};
use bevy_transform::TransformSystem::TransformPropagate;
use downcast_rs::{impl_downcast, Downcast};
use rapier3d::prelude::*;

pub mod rapier;

pub struct BigPhysicsPlugin;

impl Plugin for BigPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                Self::assign_entities_to_contexts,
                Self::read_kinematic_velocities,
                Self::simulate,
                Self::write_ecs,
            )
                .chain()
                .before(TransformPropagate),
        );
    }
}

impl BigPhysicsPlugin {
    /// Insert and remove entities from physics contexts as they move between partitions, spawn, and
    /// despawn.
    fn assign_entities_to_contexts() {
        // Run the partition and spatial hashing plugins with a `With<BigPhysics>` filter
        // Each partition is assigned a physics entity
        // Get the list of added and removed grid cells for each partition
        //      Look up all entities in the added and removed cells, and add/remove them
    }

    /// Read kinematic velocities from the ECS, and update these in physics contexts.
    fn read_kinematic_velocities() {}

    /// Advance all physics simulations.
    fn simulate() {}

    /// Write physics positions out to the ECS.
    fn write_ecs() {}
}

#[derive(Component)]
pub struct BigPhysics {
    context: Box<dyn BigPhysicsBackend>,
}

/// The interface of a `big_space` physics backend that can be implemented for any physics engine.
///
/// Implement this trait on a standalone physics context to synchronize the physics simulation with
/// entities positioned in a `big_space`, and use it in a `BigPhysics` component.
pub trait BigPhysicsBackend: Downcast + Send + Sync {
    /// Move the origin of the physics simulation.
    ///
    /// This happens any time the plugin determines the origin of the simulation and the entities in
    /// that simulation are far enough apart to cause precision issues.
    ///
    /// ### Implementors
    ///
    /// This should be likely implemented by shifting all entities by the negation of this offset.
    fn move_origin(&mut self, translation: Vec3);

    /// Advance the simulation by one time step.
    fn step(&mut self);

    /// Update an entity's properties in the simulation, inserting it if it does not exist. This
    /// should only be called if the properties have changed.
    fn insert_or_update_entity(
        &mut self,
        entity: Entity,
        properties: &BigRigidBody,
        isometry: (Vec3, Quat),
    );

    /// Remove an entity from the simulation context.
    fn remove_entity(&mut self, entity: Entity);

    /// Return the physics position of the entity, relative to the physics origin.
    fn position(&self, entity: Entity) -> Option<(Vec3, Quat)>;
}

impl_downcast!(BigPhysicsBackend);

/// A `big_space` physics entity.
#[derive(Component, Debug, PartialEq, Clone)]
pub struct BigRigidBody {
    behavior: Behavior,
    shape: Shape,
    inertia: Inertia,
}

impl BigRigidBody {
    pub fn new_fixed(shape: Shape, inertia: Inertia) -> Self {
        Self {
            behavior: Behavior::Fixed,
            shape,
            inertia,
        }
    }

    pub fn new_dynamic(shape: Shape, inertia: Inertia) -> Self {
        Self {
            behavior: Behavior::Dynamic(ReadVelocity::ZERO),
            shape,
            inertia,
        }
    }

    pub fn new_kinematic(shape: Shape, inertia: Inertia) -> Self {
        Self {
            behavior: Behavior::Kinematic(WriteVelocity::ZERO),
            shape,
            inertia,
        }
    }

    /// Get a mutable reference to the velocity, only if this has [`Behavior::Kinematic`].
    pub fn velocity_mut(&mut self) -> Option<&mut WriteVelocity> {
        if let Behavior::Kinematic(ref mut write) = self.behavior {
            Some(write)
        } else {
            None
        }
    }

    pub fn velocity(&self) -> ReadVelocity {
        self.behavior.velocity()
    }
}

/// A read-only velocity.
///
/// Physics objects can only have their velocity set by the user in specific cases, such as
/// [`Behavior::Kinematic`].
///
/// This type is used along with [`WriteVelocity`] to clearly communicate and enforce whether a
/// velocity acquired from the physics engine can be written to.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct ReadVelocity {
    linear: Vec3,
    angular: Quat,
}

impl ReadVelocity {
    /// Private to the crate to disallow constructing an instance of the struct.
    pub(crate) const ZERO: Self = ReadVelocity {
        linear: Vec3::ZERO,
        angular: Quat::IDENTITY,
    };
}

/// A velocity that can be controlled by the user, with [`Behavior::Kinematic`]. The physics
/// engine will not write to this velocity.
///
/// This type is used along with [`ReadVelocity`] to clearly communicate and enforce whether a
/// velocity acquired from the physics engine can be written to.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct WriteVelocity {
    pub linear: Vec3,
    pub angular: Quat,
}

impl WriteVelocity {
    pub const ZERO: Self = WriteVelocity {
        linear: Vec3::ZERO,
        angular: Quat::IDENTITY,
    };
}

/// The inertial properties of a rigid body, describing how heavy it is and how that mass is
/// distributed.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Inertia {
    /// Compute the inertial properties of the body by assuming it has a uniform density with the
    /// associated [`Shape`] and supplied total `mass`.
    Automatic {
        /// kg
        mass: f32,
    },
    /// Manually specified mass properties.
    Manual {
        /// kg
        mass: f32,
        /// m
        center_of_mass_local: Vec3,
        /// kg·m²
        /// The angular inertia along the coordinate axes
        principal_inertia: Vec3,
        /// Rotation applied to the local frame to define the principal axes of inertia
        principal_inertia_local_frame: Quat,
    },
}

#[allow(missing_docs)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Shape {
    Cuboid(primitives::Cuboid),
    Sphere(primitives::Sphere),
    Capsule(primitives::Capsule3d),
}

/// Configure how the object behaves in the physics simulation.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Behavior {
    /// An immovable object.
    Fixed,
    /// An object controlled entirely by the physics engine, reacting to collisions and forces.
    Dynamic(ReadVelocity),
    /// An unstoppable force.
    Kinematic(WriteVelocity),
}

impl Behavior {
    pub fn velocity(&self) -> ReadVelocity {
        match self {
            Behavior::Fixed => ReadVelocity::ZERO,
            Behavior::Dynamic(velocity) => *velocity,
            Behavior::Kinematic(v) => ReadVelocity {
                linear: v.linear,
                angular: v.angular,
            },
        }
    }

    pub fn is_fixed(&self) -> bool {
        matches!(self, Behavior::Fixed)
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, Behavior::Dynamic(_))
    }

    pub fn is_kinematic(&self) -> bool {
        matches!(self, Behavior::Kinematic(_))
    }
}
