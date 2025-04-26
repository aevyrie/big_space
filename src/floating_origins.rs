//! A floating origin for camera-relative rendering, to maximize precision when converting to f32.

use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use bevy_reflect::prelude::*;

/// Marks the entity to use as the floating origin.
///
/// This can also be thought of as the location of the low precision 32 bit rendering origin. More
/// accurately, the *cell* that this entity is located in defines the position of the rendering
/// origin. As this entity moves through space, the floating origin used for computing
/// [`GlobalTransform`](bevy_transform::components::GlobalTransform)s will only change when the
/// entity moves into a new cell.
///
/// The [`GlobalTransform`](bevy_transform::components::GlobalTransform) of all entities within this
/// [`BigSpace`] will be computed relative to this floating origin's cell. There should always be
/// exactly one entity marked with this component within a [`BigSpace`].
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct FloatingOrigin;

/// A "big space" is a hierarchy of high precision [`Grid`](crate::Grid)s, rendered relative to a
/// [`FloatingOrigin`]. This component marks the root of a high precision hierarchy, and tracks the
/// [`FloatingOrigin`] inside this hierarchy.
///
/// This component must also be paired with a [`Grid`](crate::Grid), which defines the properties of
/// this root grid. A hierarchy can have many nested [`Grid`](crate::Grid)s, but only one
/// [`BigSpace`], at the root.
///
/// Your world can have multiple [`BigSpace`]s, and they will remain completely independent. Each
/// big space uses the floating origin contained within it to compute the
/// [`GlobalTransform`](bevy_transform::components::GlobalTransform) of all spatial entities within
/// that [`BigSpace`]. This is needed for features like split screen, where you may need to render
/// the world from viewpoints that are very far from each other.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
// We do not require Grid, because we want more control over when the grid is inserted, especially
// with the command extension.
pub struct BigSpace {
    /// Set the entity to use as the floating origin within this high precision hierarchy.
    ///
    /// This is automatically set by [`Self::find_floating_origin`].
    pub floating_origin: Option<Entity>,
}

impl BigSpace {
    /// Return this grid's floating origin if it exists and is a descendant of this root.
    ///
    /// `this_entity`: the entity this component belongs to.
    pub(crate) fn validate_floating_origin(
        &self,
        this_entity: Entity,
        parents: &Query<&ChildOf>,
    ) -> Option<Entity> {
        let floating_origin = self.floating_origin?;
        let origin_root_entity = parents.iter_ancestors(floating_origin).last()?;
        Some(floating_origin).filter(|_| origin_root_entity == this_entity)
    }

    /// Automatically update all [`BigSpace`]s, finding the current floating origin entity within
    /// their hierarchy. There should be one, and only one, [`FloatingOrigin`] component in a
    /// `BigSpace` hierarchy.
    pub fn find_floating_origin(
        floating_origins: Query<Entity, With<FloatingOrigin>>,
        parent_query: Query<&ChildOf>,
        mut big_spaces: Query<(Entity, &mut BigSpace)>,
    ) {
        let mut spaces_set = HashMap::<_, _>::default();
        // Reset all floating origin fields, so we know if any are missing.
        for (entity, mut space) in &mut big_spaces {
            space.floating_origin = None;
            spaces_set.insert(entity, 0);
        }
        // Navigate to the root of the hierarchy, starting from each floating origin. This is faster
        // than the reverse direction because it is a tree, and an entity can only have a single
        // parent, but many children. The root should have an empty floating_origin field.
        for origin in &floating_origins {
            let maybe_root = parent_query.iter_ancestors(origin).last();
            if let Some((root, mut space)) =
                maybe_root.and_then(|root| big_spaces.get_mut(root).ok())
            {
                let space_origins = spaces_set.entry(root).or_default();
                *space_origins += 1;
                if *space_origins > 1 {
                    tracing::error!(
                        "BigSpace {root:#?} has multiple floating origins. There must be exactly one. Resetting this big space and disabling the floating origin to avoid unexpected propagation behavior.",
                    );
                    space.floating_origin = None;
                } else {
                    space.floating_origin = Some(origin);
                }
                continue;
            }
        }
        // Check if any big spaces did not have a floating origin.
        for space in spaces_set
            .iter()
            .filter(|(_k, v)| **v == 0)
            .map(|(k, _v)| k)
        {
            tracing::error!("BigSpace {space:#} has no floating origins. There must be exactly one. Transform propagation will not work until there is a FloatingOrigin in the hierarchy.",);
        }
    }
}
