//! Adds `big_space`-specific commands to bevy's `Commands`.

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_transform::prelude::*;
use smallvec::SmallVec;
use std::marker::PhantomData;

/// Adds `big_space` commands to bevy's `Commands`.
pub trait BigSpaceCommands<P: GridPrecision> {
    /// Spawn a root [`BigSpace`] [`ReferenceFrame`].
    fn spawn_big_space(
        &mut self,
        root_frame: ReferenceFrame<P>,
        child_builder: impl FnOnce(&mut ReferenceFrameCommands<P>),
    );
}

impl<P: GridPrecision> BigSpaceCommands<P> for Commands<'_, '_> {
    fn spawn_big_space(
        &mut self,
        reference_frame: ReferenceFrame<P>,
        root_frame: impl FnOnce(&mut ReferenceFrameCommands<P>),
    ) {
        let mut entity_commands = self.spawn(BigSpaceRootBundle::<P>::default());
        let mut cmd = ReferenceFrameCommands {
            entity: entity_commands.id(),
            commands: entity_commands.commands(),
            reference_frame,
            children: Default::default(),
        };
        root_frame(&mut cmd);
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to reference frames.
pub struct ReferenceFrameCommands<'a, P: GridPrecision> {
    entity: Entity,
    commands: Commands<'a, 'a>,
    reference_frame: ReferenceFrame<P>,
    children: SmallVec<[Entity; 8]>,
}

impl<'a, P: GridPrecision> ReferenceFrameCommands<'a, P> {
    /// Get a reference to the current reference frame.
    pub fn frame(&mut self) -> &ReferenceFrame<P> {
        &self.reference_frame
    }

    /// Insert a component on this reference frame
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.commands.entity(self.entity).insert(bundle);
        self
    }

    /// Spawn an entity in this reference frame.
    pub fn spawn(&mut self, bundle: impl Bundle) -> SpatialEntityCommands<P> {
        let entity = self.commands.spawn(bundle).id();
        self.children.push(entity);
        SpatialEntityCommands {
            entity,
            commands: self.commands.reborrow(),
            phantom: PhantomData,
        }
    }

    /// Add a high-precision spatial entity ([`GridCell`]) to this reference frame, and insert the
    /// provided bundle.
    pub fn spawn_spatial(&mut self, bundle: impl Bundle) -> SpatialEntityCommands<P> {
        let entity = self
            .commands
            .spawn((
                #[cfg(feature = "bevy_render")]
                bevy_render::view::Visibility::default(),
                #[cfg(feature = "bevy_render")]
                bevy_render::view::InheritedVisibility::default(),
                #[cfg(feature = "bevy_render")]
                bevy_render::view::ViewVisibility::default(),
                Transform::default(),
                GlobalTransform::default(),
                GridCell::<P>::default(),
            ))
            .insert(bundle)
            .id();

        self.children.push(entity);

        SpatialEntityCommands {
            entity,
            commands: self.commands.reborrow(),
            phantom: PhantomData,
        }
    }

    /// Returns the [`Entity``] id of the entity.
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Add a high-precision spatial entity ([`GridCell`]) to this reference frame, and apply entity commands to it via the closure. This allows you to insert bundles on this new spatial entities, and add more children to it.
    pub fn with_spatial(
        &mut self,
        spatial: impl FnOnce(&mut SpatialEntityCommands<P>),
    ) -> &mut Self {
        spatial(&mut self.spawn_spatial(()));
        self
    }

    /// Add a high-precision spatial entity ([`GridCell`]) to this reference frame, and apply entity commands to it via the closure. This allows you to insert bundles on this new spatial entities, and add more children to it.
    pub fn with_frame(
        &mut self,
        new_frame: ReferenceFrame<P>,
        builder: impl FnOnce(&mut ReferenceFrameCommands<P>),
    ) -> &mut Self {
        builder(&mut self.spawn_frame(new_frame, ()));
        self
    }

    /// Same as [`Self::with_frame`], but using the default [`ReferenceFrame`] value.
    pub fn with_frame_default(
        &mut self,
        builder: impl FnOnce(&mut ReferenceFrameCommands<P>),
    ) -> &mut Self {
        self.with_frame(ReferenceFrame::default(), builder)
    }

    /// Spawn a reference frame as a child of the current reference frame.
    pub fn spawn_frame(
        &mut self,
        new_frame: ReferenceFrame<P>,
        bundle: impl Bundle,
    ) -> ReferenceFrameCommands<P> {
        let mut entity_commands = self.commands.entity(self.entity);
        let mut commands = entity_commands.commands();

        let entity = commands
            .spawn((
                #[cfg(feature = "bevy_render")]
                bevy_render::view::Visibility::default(),
                #[cfg(feature = "bevy_render")]
                bevy_render::view::InheritedVisibility::default(),
                #[cfg(feature = "bevy_render")]
                bevy_render::view::ViewVisibility::default(),
                Transform::default(),
                GlobalTransform::default(),
                GridCell::<P>::default(),
                ReferenceFrame::<P>::default(),
            ))
            .insert(bundle)
            .id();

        self.children.push(entity);

        ReferenceFrameCommands {
            entity,
            commands: self.commands.reborrow(),
            reference_frame: new_frame,
            children: Default::default(),
        }
    }

    /// Spawn a reference frame as a child of the current reference frame. The first argument in the
    /// closure is the paren't reference frame.
    pub fn spawn_frame_default(&mut self, bundle: impl Bundle) -> ReferenceFrameCommands<P> {
        self.spawn_frame(ReferenceFrame::default(), bundle)
    }

    /// Access the underlying commands.
    pub fn commands(&mut self) -> &mut Commands<'a, 'a> {
        &mut self.commands
    }
}

/// Insert the reference frame on drop.
impl<'a, P: GridPrecision> Drop for ReferenceFrameCommands<'a, P> {
    fn drop(&mut self) {
        let entity = self.entity;
        self.commands
            .entity(entity)
            .insert(std::mem::take(&mut self.reference_frame))
            .push_children(&self.children);
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to reference frames.
pub struct SpatialEntityCommands<'a, P: GridPrecision> {
    entity: Entity,
    commands: Commands<'a, 'a>,
    phantom: PhantomData<P>,
}

impl<'a, P: GridPrecision> SpatialEntityCommands<'a, P> {
    /// Insert a component on this spatial entity.
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.commands.entity(self.entity).insert(bundle);
        self
    }

    /// Removes a `Bundle`` of components from the entity.
    pub fn remove<T>(&mut self) -> &mut Self
    where
        T: Bundle,
    {
        self.commands.entity(self.entity).remove::<T>();
        self
    }

    /// Takes a closure which provides a [`ChildBuilder`].
    pub fn with_children(&mut self, spawn_children: impl FnOnce(&mut ChildBuilder)) -> &mut Self {
        self.commands
            .entity(self.entity)
            .with_children(|child_builder| spawn_children(child_builder));
        self
    }

    /// Returns the [`Entity``] id of the entity.
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Access the underlying commands.
    pub fn commands(&mut self) -> &mut Commands<'a, 'a> {
        &mut self.commands
    }
}
