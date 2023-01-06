//! Contains the [`GridPrecision`] trait and its implementations.

use std::{hash::Hash, ops::Add};

use bevy::reflect::Reflect;

/// Used to make the floating origin plugin generic over many grid sizes.
///
/// Larger grids result in a larger useable volume, at the cost of increased memory usage. In
/// addition, some platforms may be unable to use larger numeric types (e.g. [`i128`]).
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
