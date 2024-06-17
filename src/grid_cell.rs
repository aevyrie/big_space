//! Contains the grid cell implementation

use bevy_ecs::prelude::*;
use bevy_reflect::prelude::*;

use crate::*;

use self::{precision::GridPrecision, reference_frame::ReferenceFrame};

/// The cell index an entity within a [`crate::ReferenceFrame`]'s grid. The [`Transform`] of an
/// entity with this component is a transformation from the center of this cell.
///
/// This component adds precision to the translation of an entity's [`Transform`]. In a
/// high-precision [`BigSpace`] world, the position of an entity is described by a [`Transform`]
/// *and* a [`GridCell`]. This component is the index of a cell inside a large grid defined by the
/// [`ReferenceFrame`], and the transform is the position of the entity relative to the center of
/// that cell.
#[derive(Component, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Reflect)]
#[reflect(Component, Default, PartialEq)]
pub struct GridCell<P: GridPrecision> {
    /// The x-index of the cell.
    pub x: P,
    /// The y-index of the cell.
    pub y: P,
    /// The z-index of the cell.
    pub z: P,
}

impl<P: GridPrecision> GridCell<P> {
    /// Construct a new [`GridCell`].
    pub fn new(x: P, y: P, z: P) -> Self {
        Self { x, y, z }
    }

    /// The origin [`GridCell`].
    pub const ZERO: Self = GridCell {
        x: P::ZERO,
        y: P::ZERO,
        z: P::ZERO,
    };

    /// A unit value [`GridCell`]. Useful for offsets.
    pub const ONE: Self = GridCell {
        x: P::ONE,
        y: P::ONE,
        z: P::ONE,
    };

    /// If an entity's transform translation becomes larger than the limit specified in its
    /// [`ReferenceFrame`], it will be relocated to the nearest grid cell to reduce the size of the
    /// transform.
    pub fn recenter_large_transforms(
        reference_frames: Query<&ReferenceFrame<P>>,
        mut changed_transform: Query<(&mut Self, &mut Transform, &Parent), Changed<Transform>>,
    ) {
        changed_transform
            .par_iter_mut()
            .for_each(|(mut grid_pos, mut transform, parent)| {
                let Ok(reference_frame) = reference_frames.get(parent.get()) else {
                    return;
                };
                if transform.as_ref().translation.abs().max_element()
                    > reference_frame.maximum_distance_from_origin()
                {
                    let (grid_cell_delta, translation) = reference_frame
                        .imprecise_translation_to_grid(transform.as_ref().translation);
                    *grid_pos += grid_cell_delta;
                    transform.translation = translation;
                }
            });
    }
}

impl<P: GridPrecision> std::ops::Add for GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_add(rhs.x),
            y: self.y.wrapping_add(rhs.y),
            z: self.z.wrapping_add(rhs.z),
        }
    }
}

impl<P: GridPrecision> std::ops::Sub for GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        GridCell {
            x: self.x.wrapping_sub(rhs.x),
            y: self.y.wrapping_sub(rhs.y),
            z: self.z.wrapping_sub(rhs.z),
        }
    }
}

impl<P: GridPrecision> std::ops::Add for &GridCell<P> {
    type Output = GridCell<P>;

    fn add(self, rhs: Self) -> Self::Output {
        (*self).add(*rhs)
    }
}

impl<P: GridPrecision> std::ops::Sub for &GridCell<P> {
    type Output = GridCell<P>;

    fn sub(self, rhs: Self) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl<P: GridPrecision> std::ops::AddAssign for GridCell<P> {
    fn add_assign(&mut self, rhs: Self) {
        use std::ops::Add;
        *self = self.add(rhs);
    }
}

impl<P: GridPrecision> std::ops::SubAssign for GridCell<P> {
    fn sub_assign(&mut self, rhs: Self) {
        use std::ops::Sub;
        *self = self.sub(rhs);
    }
}

impl<P: GridPrecision> std::ops::Mul<P> for GridCell<P> {
    type Output = GridCell<P>;

    fn mul(self, rhs: P) -> Self::Output {
        GridCell {
            x: self.x.mul(rhs),
            y: self.y.mul(rhs),
            z: self.z.mul(rhs),
        }
    }
}

impl<P: GridPrecision> std::ops::Mul<P> for &GridCell<P> {
    type Output = GridCell<P>;

    fn mul(self, rhs: P) -> Self::Output {
        (*self).mul(rhs)
    }
}
