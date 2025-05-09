//! Adds `big_space`-specific commands to bevy's `Commands`.

use crate::prelude::*;
use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_transform::prelude::*;
use smallvec::SmallVec;
use std::marker::PhantomData;

/// Adds `big_space` commands to bevy's `Commands`.
pub trait BigSpaceCommands {
    /// Spawn a root [`BigSpace`] [`Grid`].
    fn spawn_big_space<P: GridPrecision>(
        &mut self,
        root_grid: Grid<P>,
        child_builder: impl FnOnce(&mut GridCommands<P>),
    );

    /// Spawn a root [`BigSpace`] with default [`Grid`] settings.
    fn spawn_big_space_default<P: GridPrecision>(
        &mut self,
        child_builder: impl FnOnce(&mut GridCommands<P>),
    );
}

impl BigSpaceCommands for Commands<'_, '_> {
    fn spawn_big_space<P: GridPrecision>(
        &mut self,
        grid: Grid<P>,
        root_grid: impl FnOnce(&mut GridCommands<P>),
    ) {
        let mut entity_commands = self.spawn(BigSpaceRootBundle::<P>::default());
        let mut cmd = GridCommands {
            entity: entity_commands.id(),
            commands: entity_commands.commands(),
            grid,
            children: Default::default(),
        };
        root_grid(&mut cmd);
    }

    fn spawn_big_space_default<P: GridPrecision>(
        &mut self,
        child_builder: impl FnOnce(&mut GridCommands<P>),
    ) {
        self.spawn_big_space(Grid::default(), child_builder);
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to grids.
pub struct GridCommands<'a, P: GridPrecision> {
    entity: Entity,
    commands: Commands<'a, 'a>,
    grid: Grid<P>,
    children: SmallVec<[Entity; 8]>,
}

impl<'a, P: GridPrecision> GridCommands<'a, P> {
    /// Dynamic construct a new grid command.
    pub fn new(
        entity: Entity,
        commands: Commands<'a, 'a>,
        grid: Grid<P>,
        children: SmallVec<[Entity; 8]>,
    ) -> Self {
        Self {
            entity,
            commands,
            grid,
            children,
        }
    }

    /// Get a reference to the current grid.
    pub fn grid(&mut self) -> &Grid<P> {
        &self.grid
    }

    /// Insert a component on this grid
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.commands.entity(self.entity).insert(bundle);
        self
    }

    /// Spawn an entity in this grid.
    pub fn spawn(&mut self, bundle: impl Bundle) -> SpatialEntityCommands<P> {
        let entity = self.commands.spawn(bundle).id();
        self.children.push(entity);
        SpatialEntityCommands {
            entity,
            commands: self.commands.reborrow(),
            phantom: PhantomData,
        }
    }

    /// Add a high-precision spatial entity ([`GridCell`]) to this grid, and insert the provided
    /// bundle.
    pub fn spawn_spatial(&mut self, bundle: impl Bundle) -> SpatialEntityCommands<P> {
        let entity = self
            .commands
            .spawn((
                #[cfg(feature = "bevy_render")]
                bevy_render::view::Visibility::default(),
                Transform::default(),
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

    /// Add a high-precision spatial entity ([`GridCell`]) to this grid, and apply entity commands
    /// to it via the closure. This allows you to insert bundles on this new spatial entities, and
    /// add more children to it.
    pub fn with_spatial(
        &mut self,
        spatial: impl FnOnce(&mut SpatialEntityCommands<P>),
    ) -> &mut Self {
        spatial(&mut self.spawn_spatial(()));
        self
    }

    /// Add a high-precision spatial entity ([`GridCell`]) to this grid, and apply entity commands
    /// to it via the closure. This allows you to insert bundles on this new spatial entities, and
    /// add more children to it.
    pub fn with_grid(
        &mut self,
        new_grid: Grid<P>,
        builder: impl FnOnce(&mut GridCommands<P>),
    ) -> &mut Self {
        builder(&mut self.spawn_grid(new_grid, ()));
        self
    }

    /// Same as [`Self::with_grid`], but using the default [`Grid`] value.
    pub fn with_grid_default(&mut self, builder: impl FnOnce(&mut GridCommands<P>)) -> &mut Self {
        self.with_grid(Grid::default(), builder)
    }

    /// Spawn a grid as a child of the current grid.
    pub fn spawn_grid(&mut self, new_grid: Grid<P>, bundle: impl Bundle) -> GridCommands<P> {
        let mut entity_commands = self.commands.entity(self.entity);
        let mut commands = entity_commands.commands();

        let entity = commands
            .spawn((
                #[cfg(feature = "bevy_render")]
                bevy_render::view::Visibility::default(),
                Transform::default(),
                GridCell::<P>::default(),
                Grid::<P>::default(),
            ))
            .insert(bundle)
            .id();

        self.children.push(entity);

        GridCommands {
            entity,
            commands: self.commands.reborrow(),
            grid: new_grid,
            children: Default::default(),
        }
    }

    /// Spawn a grid as a child of the current grid.
    pub fn spawn_grid_default(&mut self, bundle: impl Bundle) -> GridCommands<P> {
        self.spawn_grid(Grid::default(), bundle)
    }

    /// Access the underlying commands.
    pub fn commands(&mut self) -> &mut Commands<'a, 'a> {
        &mut self.commands
    }

    /// Spawns the passed bundle which provides this grid, and adds it to this entity as a child.
    pub fn with_child<B: Bundle>(&mut self, bundle: B) -> &mut Self {
        self.commands.entity(self.entity).with_child(bundle);
        self
    }
}

/// Insert the grid on drop.
impl<P: GridPrecision> Drop for GridCommands<'_, P> {
    fn drop(&mut self) {
        let entity = self.entity;
        self.commands
            .entity(entity)
            .insert(std::mem::take(&mut self.grid))
            .add_children(&self.children);
    }
}

/// Build [`big_space`](crate) hierarchies more easily, with access to grids.
pub struct SpatialEntityCommands<'a, P: GridPrecision> {
    entity: Entity,
    commands: Commands<'a, 'a>,
    phantom: PhantomData<P>,
}

impl<'a, P: GridPrecision> SpatialEntityCommands<'a, P> {
    /// Insert a component on this grid
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

    /// Spawns the passed bundle and adds it to this entity as a child.
    pub fn with_child<B: Bundle>(&mut self, bundle: B) -> &mut Self {
        self.commands.entity(self.entity).with_child(bundle);
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
