//! Contains the [`GridPrecision`] trait and its implementations.

use std::{hash::Hash, ops::Add};

use bevy_reflect::Reflect;

/// Used to make the floating origin plugin generic over many grid sizes.
///
/// Larger grids result in a larger useable volume, at the cost of increased memory usage. In
/// addition, some platforms may be unable to use larger numeric types (e.g. [`i128`]).
///
/// [`big_space`](crate) is generic over a few integer types to allow you to select the grid size
/// you need. Assuming you are using a grid cell edge length of 10,000 meters, and `1.0` == 1 meter,
/// these correspond to a total usable volume of a cube with the following edge lengths:
///
/// - `i8`: 2,560 km = 74% of the diameter of the Moon
/// - `i16`: 655,350 km = 85% of the diameter of the Moon's orbit around Earth
/// - `i32`: 0.0045 light years = ~4 times the width of the solar system
/// - `i64`: 19.5 million light years = ~100 times the width of the milky way galaxy
/// - `i128`: 3.6e+26 light years = ~3.9e+15 times the width of the observable universe
///
/// where `usable_edge_length = 2^(integer_bits) * cell_edge_length`, resulting in a worst case
/// precision of 0.5mm in any of these cases.
///
/// This can also be used for small scales. With a cell edge length of `1e-11`, and using `i128`,
/// there is enough precision to render objects the size of quarks anywhere in the observable
/// universe.
///
/// # Note
///
/// Be sure you are using the same grid index precision everywhere. It might be a good idea to
/// define a type alias!
///
/// ```
/// # use big_space::GridCell;
/// type GalacticGrid = GridCell<i64>;
/// ```
///
/// Additionally, consider using the provided command extensions in [`crate::commands`] to
/// completely eliminate the use of this generic, and prevent many errors.
pub trait GridPrecision:
    Default
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + Hash
    + Copy
    + Clone
    + Send
    + Sync
    + Reflect
    + Add
    + std::fmt::Debug
    + std::fmt::Display
    + 'static
{
    /// The zero value for this type.
    const ZERO: Self;
    /// The value of `1` for this type.
    const ONE: Self;
    /// Adds `rhs` to `self`, wrapping when overflow would occur.
    fn wrapping_add(self, rhs: Self) -> Self;
    /// Subtracts `rhs` from `self`, wrapping when overflow would occur.
    fn wrapping_sub(self, rhs: Self) -> Self;
    /// Multiplies `self` by `rhs`.
    fn mul(self, rhs: Self) -> Self;
    /// Casts `self` as a double precision float.
    fn as_f64(self) -> f64;
    /// Casts a double precision float into `Self`.
    fn from_f64(input: f64) -> Self;
    /// Casts a single precision float into `Self`.
    fn from_f32(input: f32) -> Self;
}

impl GridPrecision for i8 {
    const ZERO: Self = 0;
    const ONE: Self = 1;

    #[inline]
    fn wrapping_add(self, rhs: Self) -> Self {
        Self::wrapping_add(self, rhs)
    }
    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        Self::wrapping_sub(self, rhs)
    }
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self * rhs
    }
    #[inline]
    fn as_f64(self) -> f64 {
        self as f64
    }
    #[inline]
    fn from_f64(input: f64) -> Self {
        input as Self
    }
    #[inline]
    fn from_f32(input: f32) -> Self {
        input as Self
    }
}

impl GridPrecision for i16 {
    const ZERO: Self = 0;
    const ONE: Self = 1;

    #[inline]
    fn wrapping_add(self, rhs: Self) -> Self {
        Self::wrapping_add(self, rhs)
    }
    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        Self::wrapping_sub(self, rhs)
    }
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self * rhs
    }
    #[inline]
    fn as_f64(self) -> f64 {
        self as f64
    }
    #[inline]
    fn from_f64(input: f64) -> Self {
        input as Self
    }
    #[inline]
    fn from_f32(input: f32) -> Self {
        input as Self
    }
}

impl GridPrecision for i32 {
    const ZERO: Self = 0;
    const ONE: Self = 1;

    #[inline]
    fn wrapping_add(self, rhs: Self) -> Self {
        Self::wrapping_add(self, rhs)
    }
    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        Self::wrapping_sub(self, rhs)
    }
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self * rhs
    }
    #[inline]
    fn as_f64(self) -> f64 {
        self as f64
    }
    #[inline]
    fn from_f64(input: f64) -> Self {
        input as Self
    }
    #[inline]
    fn from_f32(input: f32) -> Self {
        input as Self
    }
}

impl GridPrecision for i64 {
    const ZERO: Self = 0;
    const ONE: Self = 1;

    #[inline]
    fn wrapping_add(self, rhs: Self) -> Self {
        Self::wrapping_add(self, rhs)
    }
    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        Self::wrapping_sub(self, rhs)
    }
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self * rhs
    }
    #[inline]
    fn as_f64(self) -> f64 {
        self as f64
    }
    #[inline]
    fn from_f64(input: f64) -> Self {
        input as Self
    }
    #[inline]
    fn from_f32(input: f32) -> Self {
        input as Self
    }
}

impl GridPrecision for i128 {
    const ZERO: Self = 0;
    const ONE: Self = 1;

    #[inline]
    fn wrapping_add(self, rhs: Self) -> Self {
        Self::wrapping_add(self, rhs)
    }
    #[inline]
    fn wrapping_sub(self, rhs: Self) -> Self {
        Self::wrapping_sub(self, rhs)
    }
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self * rhs
    }
    #[inline]
    fn as_f64(self) -> f64 {
        self as f64
    }
    #[inline]
    fn from_f64(input: f64) -> Self {
        input as Self
    }
    #[inline]
    fn from_f32(input: f32) -> Self {
        input as Self
    }
}
