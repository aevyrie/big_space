//! A helper query argument that ensures you don't forget to handle the [`GridCell`] when you work
//! with a [`Transform`].

use crate::prelude::*;
use bevy::ecs::query::QueryData;
use bevy::math::{prelude::*, DVec3};
use bevy::transform::prelude::*;

#[derive(QueryData)]
#[query_data(mutable)]
/// A convenience query argument that groups a [`Transform`] with its [`GridCell`]. If you only want
/// to read from the position, use [`CellTransformReadOnly`] instead, as this will allow the bevy
/// ECS to run multiple queries using [`CellTransformReadOnly`] at the same time (just like multiple
/// queries with `&Transform` are fine).
pub struct CellTransform {
    /// The grid to which `transform` is relative to.
    pub cell: &'static mut GridCell,
    /// Grid local transform
    pub transform: &'static mut Transform,
}

impl CellTransformItem<'_, '_> {
    /// Compute the global position with double precision.
    pub fn position_double(&self, grid: &Grid) -> DVec3 {
        grid.grid_position_double(&self.cell, &self.transform)
    }

    /// Compute the global position.
    pub fn position(&self, grid: &Grid) -> Vec3 {
        grid.grid_position(&self.cell, &self.transform)
    }

    /// Get a copy of the fields to work with.
    pub fn to_owned(&self) -> CellTransformOwned {
        CellTransformOwned {
            transform: *self.transform,
            cell: *self.cell,
        }
    }
}

impl CellTransformReadOnlyItem<'_, '_> {
    /// Compute the global position with double precision.
    pub fn position_double(&self, grid: &Grid) -> DVec3 {
        grid.grid_position_double(self.cell, self.transform)
    }

    /// Compute the global position.
    pub fn position(&self, grid: &Grid) -> Vec3 {
        grid.grid_position(self.cell, self.transform)
    }

    /// Get a copy of the fields to work with.
    pub fn to_owned(&self) -> CellTransformOwned {
        CellTransformOwned {
            transform: *self.transform,
            cell: *self.cell,
        }
    }
}

/// A convenience wrapper that allows working with grid and transform easily
#[derive(Copy, Clone)]
pub struct CellTransformOwned {
    /// Grid local transform
    pub transform: Transform,
    /// The grid to which `transform` is relative to.
    pub cell: GridCell,
}

impl core::ops::Sub for CellTransformOwned {
    type Output = Self;

    /// Compute a new transform that maps from `source` to `self`.
    fn sub(mut self, source: Self) -> Self {
        self.cell -= source.cell;
        self.transform.translation -= source.transform.translation;
        self.transform.scale /= source.transform.scale;
        self.transform.rotation *= source.transform.rotation.inverse();
        self
    }
}

impl core::ops::Add for CellTransformOwned {
    type Output = Self;

    /// Compute a new transform that shifts, scales and rotates `self` by `diff`.
    fn add(mut self, diff: Self) -> Self {
        self.cell += diff.cell;
        self.transform.translation += diff.transform.translation;
        self.transform.scale *= diff.transform.scale;
        self.transform.rotation *= diff.transform.rotation;
        self
    }
}

impl CellTransformOwned {
    /// Compute the global position with double precision.
    pub fn position_double(&self, grid: &Grid) -> DVec3 {
        grid.grid_position_double(&self.cell, &self.transform)
    }

    /// Compute the global position.
    pub fn position(&self, grid: &Grid) -> Vec3 {
        grid.grid_position(&self.cell, &self.transform)
    }
}
