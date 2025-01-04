//! Experimental
//! 
//! Built on top of big_space to define a distributed simulation architecture.
//! 
//! Conductor:
//! - tracks positions and partitions all entities
//! - assigns partitions to simulation servers
//! - informs simulation server of nearby partitions
//! 
//! Simulation servers: adds and removes entities based on instructions from the conductor.
//! 
//! - Simulations should be spun up as needed
//! - Each partition will be assigned a simulation server (physics + whatever)
//! - That server will be given stable IDs (not entity)
//! - Server is responsible for... running the game simulation and sending any data back to the user

use std::marker::PhantomData;

use bevy_app::prelude::*;
use big_space::prelude::*;

pub struct BigPhysicsPlugin<P, F>
where
    P: GridPrecision,
    F: big_space::hash::GridHashMapFilter,
{
    spooky: PhantomData<(P, F)>,
}

impl<P, F> Plugin for BigPhysicsPlugin<P, F>
where
    P: GridPrecision,
    F: big_space::hash::GridHashMapFilter,
{
    fn build(&self, _app: &mut bevy_app::App) {}
}
