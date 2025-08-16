//! Adds `big_space`-specific commands to bevy's `Commands`.

use crate::prelude::*;
use bevy_ecs::{prelude::*, relationship::RelatedSpawnerCommands};
use bevy_transform::prelude::*;
use smallvec::SmallVec;

/// Adds `big_space` commands to bevy's `Commands`.
pub trait BigSpaceCommands {
    /// Spawn a root [`BigSpace`] [`Grid`].
    fn spawn_big_space(&mut self, root_grid: Grid, child_builder: impl FnOnce(&mut GridCommands));

    /// Spawn a root [`BigSpace`] with default [`Grid`] settings.
    fn spawn_big_space_default(&mut self, child_builder: impl FnOnce(&mut GridCommands));

    /// Access the [`GridCommands`] of an entity by passing in the [`Entity`] and [`Grid`]. Note
    /// that the value of `grid` will be inserted in this entity when the command is applied.
    fn grid(&mut self, entity: Entity, grid: Grid) -> GridCommands;
}

impl BigSpaceCommands for Commands<'_, '_> {
    fn spawn_big_space(&mut self, grid: Grid, root_grid: impl FnOnce(&mut GridCommands)) {
        let mut entity_commands = self.spawn(BigSpaceRootBundle::default());
        let mut cmd = GridCommands {
            entity: entity_commands.id(),
            commands: entity_commands.commands(),
            grid,
            children: Default::default(),
        };
        root_grid(&mut cmd);
    }

    fn spawn_big_space_default(&mut self, child_builder: impl FnOnce(&mut GridCommands)) {
        self.spawn_big_space(Grid::default(), child_builder);
    }

    fn grid(&mut self, entity: Entity, grid: Grid) -> GridCommands {
        GridCommands {
            entity,
            commands: self.reborrow(),
            grid,
            children: Default::default(),
        }
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to grids.
pub struct GridCommands<'a> {
    entity: Entity,
    commands: Commands<'a, 'a>,
    grid: Grid,
    children: SmallVec<[Entity; 8]>,
}

impl<'a> GridCommands<'a> {
    /// Get a reference to the current grid.
    pub fn grid(&mut self) -> &Grid {
        &self.grid
    }

    /// Insert a component on this grid
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.commands.entity(self.entity).insert(bundle);
        self
    }

    /// Spawn an entity in this grid.
    #[inline]
    pub fn spawn(&mut self, bundle: impl Bundle) -> SpatialEntityCommands {
        let entity = self.commands.spawn(bundle).id();
        self.children.push(entity);
        SpatialEntityCommands {
            entity,
            commands: self.commands.reborrow(),
        }
    }

    /// Add a high-precision spatial entity ([`CellCoord`]) to this grid, and insert the provided
    /// bundle.
    #[inline]
    pub fn spawn_spatial(&mut self, bundle: impl Bundle) -> SpatialEntityCommands {
        let entity = self
            .spawn((
                #[cfg(feature = "bevy_render")]
                bevy_render::view::Visibility::default(),
                Transform::default(),
                CellCoord::default(),
            ))
            .insert(bundle)
            .id();

        SpatialEntityCommands {
            entity,
            commands: self.commands.reborrow(),
        }
    }

    /// Returns the [`Entity`] id of the entity.
    #[inline]
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Add a high-precision spatial entity ([`CellCoord`]) to this grid, and apply entity commands
    /// to it via the closure. This allows you to insert bundles on this new spatial entities, and
    /// add more children to it.
    #[inline]
    pub fn with_spatial(&mut self, spatial: impl FnOnce(&mut SpatialEntityCommands)) -> &mut Self {
        spatial(&mut self.spawn_spatial(()));
        self
    }

    /// Add a high-precision spatial entity ([`CellCoord`]) to this grid, and apply entity commands
    /// to it via the closure. This allows you to insert bundles on this new spatial entities, and
    /// add more children to it.
    #[inline]
    pub fn with_grid(
        &mut self,
        new_grid: Grid,
        builder: impl FnOnce(&mut GridCommands),
    ) -> &mut Self {
        builder(&mut self.spawn_grid(new_grid, ()));
        self
    }

    /// Same as [`Self::with_grid`], but using the default [`Grid`] value.
    #[inline]
    pub fn with_grid_default(&mut self, builder: impl FnOnce(&mut GridCommands)) -> &mut Self {
        self.with_grid(Grid::default(), builder)
    }

    /// Spawn a grid as a child of the current grid.
    #[inline]
    pub fn spawn_grid(&mut self, new_grid: Grid, bundle: impl Bundle) -> GridCommands {
        let entity = self
            .spawn((
                #[cfg(feature = "bevy_render")]
                bevy_render::view::Visibility::default(),
                Transform::default(),
                CellCoord::default(),
                Grid::default(),
            ))
            .insert(bundle)
            .id();

        GridCommands {
            entity,
            commands: self.commands.reborrow(),
            grid: new_grid,
            children: Default::default(),
        }
    }

    /// Spawn a grid as a child of the current grid.
    pub fn spawn_grid_default(&mut self, bundle: impl Bundle) -> GridCommands {
        self.spawn_grid(Grid::default(), bundle)
    }

    /// Access the underlying commands.
    #[inline]
    pub fn commands(&mut self) -> &mut Commands<'a, 'a> {
        &mut self.commands
    }

    /// Spawns the passed bundle which provides this grid, and adds it to this entity as a child.
    #[inline]
    pub fn with_child<B: Bundle>(&mut self, bundle: B) -> &mut Self {
        self.commands.entity(self.entity).with_child(bundle);
        self
    }
}

/// Insert the grid on drop.
impl Drop for GridCommands<'_> {
    fn drop(&mut self) {
        let entity = self.entity;
        self.commands
            .entity(entity)
            .insert(core::mem::take(&mut self.grid))
            .add_children(&self.children);
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to grids.
pub struct SpatialEntityCommands<'a> {
    entity: Entity,
    commands: Commands<'a, 'a>,
}

impl<'a> SpatialEntityCommands<'a> {
    /// Insert a component into this grid.
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.commands.entity(self.entity).insert(bundle);
        self
    }

    /// Removes a `Bundle` of components from the entity.
    pub fn remove<T>(&mut self) -> &mut Self
    where
        T: Bundle,
    {
        self.commands.entity(self.entity).remove::<T>();
        self
    }

    /// Spawns children of this entity (with a [`ChildOf`] relationship) by taking a function that operates on a [`ChildSpawner`].
    pub fn with_children(
        &mut self,
        spawn_children: impl FnOnce(&mut RelatedSpawnerCommands<'_, ChildOf>),
    ) -> &mut Self {
        self.commands
            .entity(self.entity)
            .with_children(|child_builder| spawn_children(child_builder));
        self
    }

    /// Spawns the passed bundle and adds it to this entity as a child.
    ///
    /// For efficient spawning of multiple children, use [`with_children`].
    ///
    /// [`with_children`]: SpatialEntityCommands::with_children
    pub fn with_child<B: Bundle>(&mut self, bundle: B) -> &mut Self {
        self.commands.entity(self.entity).with_child(bundle);
        self
    }

    /// Returns the [`Entity`] id of the entity.
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Access the underlying commands.
    pub fn commands(&mut self) -> &mut Commands<'a, 'a> {
        &mut self.commands
    }
}
