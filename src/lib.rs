//! This `bevy` plugin makes it possible to build high-precision worlds that exceed the size of the
//! observable universe, with no added dependencies, while remaining largely compatible with the
//! rest of the Bevy ecosystem.
//!
//! The next section explains the problem this solves in more detail, how this plugin works, and a
//! list of other solutions that were considered. If you'd like, you can instead skip ahead to
//! [Usage](crate#usage).
//!
//! ### Problem
//!
//! Objects far from the origin suffer from reduced precision, causing rendered meshes to jitter and
//! jiggle, and transformation calculations to encounter catastrophic cancellation.
//!
//! As a camera moves far from the origin, the values describing its x/y/z coordinates become large,
//! leaving less precision to the right of the decimal place. Consequently, when computing the
//! position of objects in view space, mesh vertices will be displaced due to this lost precision.
//!
//! This is a great little tool to calculate how much precision a floating point value has at a
//! given scale: <http://www.ehopkinson.com/floatprecision.html>.
//!
//! ### Possible Solutions
//!
//! There are many ways to solve this problem!
//!
//! - Periodic recentering: every time the camera moves far enough away from the origin, move it
//!   back to the origin and apply the same offset to all other entities.
//!   - Problem: Objects far from the camera will drift and accumulate error.
//!   - Problem: No fixed reference frame.
//!   - Problem: Recentering triggers change detection even for objects that did not move.
//! - Camera-relative coordinates: don't move the camera, move the world around the camera.
//!   - Problem: Objects far from the camera will drift and accumulate error.
//!   - Problem: No fixed reference frame
//!   - Problem: Math is more complex when everything is relative to the camera.
//!   - Problem: Rotating the camera requires recomputing transforms for everything.
//!   - Problem: Camera movement triggers change detection even for objects that did not move.
//!   - Problem: Incompatible with existing plugins that use `Transform`.
//! - Double precision coordinates: Store transforms in double precision
//!   - Problem: Rendering still requires positions be in single precision, which either requires
//!     using one of the above techniques, or emulating 64 bit precision in shaders.
//!   - Problem: Updating double precision transforms is more expensive than single precision.
//!   - Problem: Computing the `GlobalTransform` is more expensive than single precision.
//!   - Problem: Size is limited to approximately the orbit of Saturn at human scales.
//!   - Problem: Incompatible with existing plugins that use `Transform`.
//! - Chunks: Place objects in a large grid, and track the grid cell they are in,
//!   - Problem: Requires a component to track the grid cell, in addition to the `Transform`.
//!   - Problem: Computing the `GlobalTransform` is more expensive than single precision.
//!
//! ### Solution
//!
//! This plugin uses the last solution listed above. The most significant benefits of this method
//! over the others are:
//! - Absolute high-precision positions in space that do not change when the camera moves. The only
//!   component that is affected by precision loss is the `GlobalTransform` used for rendering. The
//!   `GridCell` and `Transform` only change when an entity moves. This is especially useful for
//!   multiplayer - the server needs a source of truth for position that doesn't drift over time.
//! - Virtually limitless volume and scale; you can work at the scale of subatomic particles, across
//!   the width of the observable universe. Double precision is downright suffocating in comparison.
//! - Uniform precision across the play area. Unlike double precision, the available precision does
//!   not decrease as you move to the edge of the play area, it is instead relative to the distance
//!   from the origin of the current grid cell.
//! - High precision coordinates are invisible if you don't need them. You can move objects using
//!   their `Transform` alone, which results in decent ecosystem compatibility.
//! - High precision is completely opt-in. If you don't add the `GridCell` component to an entity,
//!   it behaves like a normal single precision transform, with the same performance cost, yet it
//!   can exist in the high precision hierarchy. This allows you to load in GLTFs or other
//!   low-precision entity hierarchies with no added effort or cost.
//!
//! While using the [`BigSpacePlugin`], the position of entities is now defined with the
//! [`ReferenceFrame`], [`GridCell`], and [`Transform`] components. The `ReferenceFrame` is a large
//! integer grid of cells; entities are located within this grid using the `GridCell` component.
//! Finally, the `Transform` is used to position the entity relative to the center of its
//! `GridCell`. If an entity moves into a neighboring cell, its transform will be automatically
//! recomputed relative to the center of that new cell. This prevents `Transforms` from ever
//! becoming larger than a single grid cell, and thus prevents floating point precision artifacts.
//!
//! The grid adds precision to your transforms. If you are using (32-bit) `Transform`s on an `i32`
//! grid, you will have 64 bits of precision: 32 bits to address into a large integer grid, and 32
//! bits of floating point precision within a grid cell. This plugin is generic up to `i128` grids,
//! giving you up tp 160 bits of precision of translation.
//!
//! `ReferenceFrame`s - grids - can be nested. This allows you to define moving reference frames,
//! which can make certain use cases much simpler. For example, if you have a planet rotating, and
//! orbiting around its star, it would be very annoying if you had to compute this orbit and
//! rotation for all objects on the surface in high precision. Instead, you can place the planet and
//! all objects on its surface in the same reference frame. The motion of the planet will be
//! inherited by all children in that reference frame, in high precision.
//!
//! Entities at the root of bevy's entity hierarchy are not in any reference frame. This allows
//! plugins from the rest of the ecosystem to operate normally, such as bevy_ui, which relies on the
//! built in transform propagation system. This also means that if you don't need to place entities
//! in a high-precision reference frame, you don't have to, as the process is opt-in. The
//! high-precision hierarchical reference frames are explicit. Each high-precision tree must have a
//! [`BigSpaceRootBundle`] at the root, and each `BigSpace` is independent. This means that each
//! `BigSpace` has its own floating origin, which allows you to do things like rendering two players
//! on opposite ends of the universe simultaneously.
//!
//! All of the above applies to the entity marked with the [`FloatingOrigin`] component. The
//! floating origin can be any high-precision entity in a `BigSpace`, it doesn't need to be a
//! camera. The only thing special about the entity marked as the floating origin, is that it is
//! used to compute the [`GlobalTransform`] of all other entities in that `BigSpace`. To an outside
//! observer, every high-precision entity within a `BigSpace` is confined to a box the size of a
//! grid cell - like a game of *Asteroids*. Only once you render the `BigSpace` from the point of
//! view of the floating origin, by calculating their `GlobalTransform`s, do entities appear very
//! distant from the floating origin.
//!
//! As described above. the `GlobalTransform` of all entities is computed relative to the floating
//! origin's grid cell. Because of this, entities very far from the origin will have very large,
//! imprecise positions. However, this is always relative to the camera (floating origin), so these
//! artifacts will always be too far away to be seen, no matter where the camera moves. Because this
//! only affects the `GlobalTransform` and not the `Transform`, this also means that entities will
//! never permanently lose precision just because they were far from the origin at some point. The
//! lossy calculation only occurs when computing the `GlobalTransform` of entities, the high
//! precision `GridCell` and `Transform` are never touched.
//!
//! # Usage
//!
//! To start using this plugin, you will first need to choose how big your world should be! Do you
//! need an i8, or an i128? See [`GridPrecision`](crate::precision::GridPrecision) for more details
//! and documentation.
//!
//! 1. Disable Bevy's transform plugin: `DefaultPlugins.build().disable::<TransformPlugin>()`
//! 2. Add the [`BigSpacePlugin`] to your `App`
//! 3. Spawn a [`BigSpace`] with [`spawn_big_space`](BigSpaceCommands::spawn_big_space), and spawn
//!    entities in it.
//! 4. Add the [`FloatingOrigin`] to your active camera in the [`BigSpace`].
//!
//! To add more levels to the hierarchy, you can use [`ReferenceFrame`]s, which themselves can
//! contain high-precision spatial entities. Reference frames are useful when you want all objects
//! to move together in space, for example, objects on the surface of a planet rotating on its axis
//! and orbiting a star.
//!
//! Take a look at the [`ReferenceFrame`] component for some useful helper methods. The component
//! defines the scale of the grid, which is very important when computing distances between objects
//! in different cells. Note that the root [`BigSpace`] also has a [`ReferenceFrame`] component.
//!
//! # Moving Entities
//!
//! For the most part, you can update the position of entities normally while using this plugin, and
//! it will automatically handle the tricky bits. If you move an entity too far from the center of
//! its grid cell, the plugin will automatically move it into the correct cell for you. However,
//! there is one big caveat:
//!
//! **Avoid setting position absolutely, instead prefer applying a relative delta**
//!
//! Instead of:
//!
//! ```ignore
//! transform.translation = a_huge_imprecise_position;
//! ```
//!
//! do:
//!
//! ```ignore
//! let delta = new_pos - old_pos;
//! transform.translation += delta;
//! ```
//!
//! ## Absolute Position
//!
//! If you are updating the position of an entity with absolute positions, and the position exceeds
//! the bounds of the entity's grid cell, the floating origin plugin will recenter that entity into
//! its new cell. Every time you update that entity, you will be fighting with the plugin as it
//! constantly recenters your entity. This can especially cause problems with camera controllers
//! which may not expect the large discontinuity in position as an entity moves between cells.
//!
//! The other reason to avoid this is you will likely run into precision issues! This plugin exists
//! because single precision is limited, and the larger the position coordinates get, the less
//! precision you have.
//!
//! However, if you have something that must not accumulate error, like the orbit of a planet, you
//! can instead do the orbital calculation (position as a function of time) to compute the absolute
//! position of the planet with high precision, then directly compute the [`GridCell`] and
//! [`Transform`] of that entity using [`ReferenceFrame::translation_to_grid`].
//!
//! # Next Steps
//!
//! Take a look at the examples to see usage, as well as explanation of these use cases and topics.

#![allow(clippy::type_complexity)]
#![warn(missing_docs)]

use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_transform::prelude::*;

pub mod bundles;
pub mod commands;
pub mod floating_origins;
pub mod grid_cell;
pub mod plugin;
pub mod precision;
pub mod reference_frame;
pub mod validation;
pub mod world_query;

#[cfg(feature = "camera")]
pub mod camera;
#[cfg(feature = "debug")]
pub mod debug;
#[cfg(test)]
mod tests;

pub use bundles::{BigReferenceFrameBundle, BigSpaceRootBundle, BigSpatialBundle};
pub use commands::{BigSpaceCommands, ReferenceFrameCommands, SpatialEntityCommands};
pub use floating_origins::{BigSpace, FloatingOrigin};
pub use grid_cell::GridCell;
pub use plugin::{BigSpacePlugin, FloatingOriginSet};
pub use reference_frame::ReferenceFrame;
