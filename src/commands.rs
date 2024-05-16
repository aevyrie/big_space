//! Adds `big_space`-specific commands to bevy's `Commands`.

use bevy::ecs::system::EntityCommands;

use crate::{reference_frame::ReferenceFrame, GridPrecision, *};

/// Adds `big_space` commands to bevy's `Commands`.
pub trait BigSpaceCommandExt<P: GridPrecision> {
    /// Spawn a root [`BigSpace`] [`ReferenceFrame`].
    fn spawn_big_space(
        &mut self,
        reference_frame: ReferenceFrame<P>,
        frame: impl FnOnce(&mut ReferenceFrameCommands<P>),
    );
}

impl<P: GridPrecision> BigSpaceCommandExt<P> for Commands<'_, '_> {
    fn spawn_big_space(
        &mut self,
        reference_frame: ReferenceFrame<P>,
        root_frame: impl FnOnce(&mut ReferenceFrameCommands<P>),
    ) {
        let entity_commands = self.spawn((
            #[cfg(feature = "bevy_render")]
            Visibility::default(),
            #[cfg(feature = "bevy_render")]
            InheritedVisibility::default(),
            #[cfg(feature = "bevy_render")]
            ViewVisibility::default(),
            BigSpace::default(),
        ));
        let mut cmd = ReferenceFrameCommands {
            entity_commands,
            reference_frame,
        };
        root_frame(&mut cmd);
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to reference frames.
pub struct ReferenceFrameCommands<'a, P: GridPrecision> {
    entity_commands: EntityCommands<'a>,
    reference_frame: ReferenceFrame<P>,
}

impl<'a, P: GridPrecision> ReferenceFrameCommands<'a, P> {
    /// Get a reference to the current reference frame.
    pub fn get_frame(&mut self) -> &ReferenceFrame<P> {
        &self.reference_frame
    }

    /// Insert a component on this reference frame
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.entity_commands.insert(bundle);
        self
    }

    /// Add a high-precision spatial entity ([`GridCell`]) to this reference frame, which is
    /// provided to help position the entity with high precision using
    /// [`ReferenceFrame::translation_to_grid`].
    pub fn spawn_spatial(
        &mut self,
        spatial: impl FnOnce(&ReferenceFrame<P>, &mut SpatialEntityCommands<P>),
    ) -> &mut Self {
        let Self {
            entity_commands,
            reference_frame,
        } = self;

        entity_commands.with_children(move |child_builder| {
            let entity_commands = child_builder.spawn((
                #[cfg(feature = "bevy_render")]
                Visibility::default(),
                #[cfg(feature = "bevy_render")]
                InheritedVisibility::default(),
                #[cfg(feature = "bevy_render")]
                ViewVisibility::default(),
                Transform::default(),
                GlobalTransform::default(),
                GridCell::<P>::default(),
            ));

            let mut cmd = SpatialEntityCommands {
                entity_commands,
                phantom: PhantomData,
            };

            spatial(reference_frame, &mut cmd);
        });
        self
    }

    /// Spawn a reference frame as a child of the current reference frame.
    pub fn spawn_frame(
        &mut self,
        child_reference_frame: ReferenceFrame<P>,
        frame: impl FnOnce(&ReferenceFrame<P>, &mut ReferenceFrameCommands<P>),
    ) -> &mut Self {
        let Self {
            entity_commands,
            reference_frame,
        } = self;

        entity_commands.with_children(move |child_builder| {
            let entity_commands = child_builder.spawn((
                #[cfg(feature = "bevy_render")]
                Visibility::default(),
                #[cfg(feature = "bevy_render")]
                InheritedVisibility::default(),
                #[cfg(feature = "bevy_render")]
                ViewVisibility::default(),
                Transform::default(),
                GlobalTransform::default(),
                GridCell::<P>::default(),
                ReferenceFrame::<P>::default(),
            ));

            let mut cmd = ReferenceFrameCommands {
                entity_commands,
                reference_frame: child_reference_frame,
            };

            frame(reference_frame, &mut cmd);
        });
        self
    }

    /// Spawn a reference frame as a child of the current reference frame. The first argument in the
    /// closure is the paren't reference frame.
    pub fn spawn_frame_default(
        &mut self,
        frame: impl FnOnce(&ReferenceFrame<P>, &mut ReferenceFrameCommands<P>),
    ) -> &mut Self {
        self.spawn_frame(ReferenceFrame::default(), frame)
    }

    /// Takes a closure which provides this reference frame and a [`ChildBuilder`].
    pub fn with_children(
        &mut self,
        spawn_children: impl FnOnce(&ReferenceFrame<P>, &mut ChildBuilder),
    ) -> &mut Self {
        self.entity_commands
            .with_children(|child_builder| spawn_children(&self.reference_frame, child_builder));
        self
    }
}

/// Insert the reference frame on drop.
impl<'a, P: GridPrecision> Drop for ReferenceFrameCommands<'a, P> {
    fn drop(&mut self) {
        self.entity_commands
            .insert(std::mem::take(&mut self.reference_frame));
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to reference frames.
pub struct SpatialEntityCommands<'a, P: GridPrecision> {
    entity_commands: EntityCommands<'a>,
    phantom: PhantomData<P>,
}

impl<'a, P: GridPrecision> SpatialEntityCommands<'a, P> {
    /// Insert a component on this reference frame
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.entity_commands.insert(bundle);
        self
    }

    /// Takes a closure which provides a [`ChildBuilder`].
    pub fn with_children(&mut self, spawn_children: impl FnOnce(&mut ChildBuilder)) -> &mut Self {
        self.entity_commands
            .with_children(|child_builder| spawn_children(child_builder));
        self
    }
}
