# Changelog

## UNRELEASED

### Renamed types for consistency

Redundant `Grid` prefix removed.

- `GridCell` -> `CellCoord`: This is the coordinate of the cell, not the cell itself.
- `GridHash` -> `CellId`: This is a globally unique ID of the cell, not just a hash.
- `FastGridHash` -> `CellHash`: This is a hash of the cell's ID.
- `GridHashMap` -> `CellLookup`: Makes distinct from the `CellHashMap`.
- `GridHashEntry` -> `CellLookupEntry`
- `GridPartition` -> `Partition`
- `GridPartitionId` -> `PartitionId`
- `GridPartitionMap` -> `PartitionLookup`: Consistent with `CellLookup`.
- `GridHashMapFilter` -> `SpatialHashFilter`: Filter is used for all spatial hashing.

## v0.10.0 - 2025-06-21

Bevy 0.16, `no_std`, removed precision generics, switch to plugin groups, naming consistency pass.

### Updated: Bevy 0.16

Due to changes in bevy, this plugin once again requires you to disable bevy's built-in transform system:

```rs
DefaultPlugins.build().disable::<TransformPlugin>(),
````

### Removed: `GridPrecision` generics

The precision of grids is now a feature flag instead of a generic, and `GridPrecision` is now an integer type instead of a trait.

Having multiple precisions in the same app has been a persistent footgun, and is flexibility that no one uses or needs. This change has simplified the API by removing significant amounts of line noise caused by `::<P>`. For example `GridCell<P>` is now simply `GridCell`.

The integer precision used for grids is now selected from a set of features: `i8`, `i16`, `i32`, `i64`. and `i128`. If no feature is specified, `big_space` will default to `i64`. If multiple precision features are specified, the highest will be used.

### Changed: `BigSpaceDefaultPlugins` plugin group

Instead of adding common plugins individually, they have been grouped into the `BigSpaceDefaultPlugins` plugin group, similar to the `DefaultPlugins` plugin group in Bevy.

For example, the hierarchy validation plugin is enabled whenever debug assertions are enabled but can be manually enabled or disabled to override this behavior:

```rs
BigSpaceDefaultPlugins
    .build()
    .enable::<BigSpaceValidationPlugin>()
```

Plugins that are behind feature flags are automatically enabled when their corresponding feature is enabled; you no longer need to manually add the camera controller plugin, you only need to enable the feature and add `BigSpaceDefaultPlugins` to your app.

This replaces `BigSpacePlugin`.

The existing plugin structure has been organized into more fine grained plugins, with the addition of the `BigSpaceMinimalPlugins` composed of `BigSpaceCorePlugin` and `BigSpacePropagationPlugin`. These are particularly useful for tests, benchmarks, and serverside applications. Future serverside physics will likely only use the `BigSpaceCorePlugin` to handle grid cell recentering, and not do any propagation which is only needed for rendering.

### Changed: Naming consistency

To avoid common name collisions and improve searchability, names have been standardized:

- `FloatingOriginSystem` -> `BigSpaceSystems`
- `CameraControllerPlugin` -> `BigSpaceCameraControllerPlugin`
    - `CameraController` -> `BigSpaceCameraController`
    - `CameraInput` -> `BigSpaceCameraInput`
- `TimingStatsPlugin` -> `BigSpaceTimingStatsPlugin`
- `FloatingOriginDebugPlugin` -> `BigSpaceDebugPlugin`
- `BigSpaceValidationPlugin` (new)
- `BigSpaceDefaultPlugins` (new)
- `BigSpaceMinimalPlugins` (new)
- `BigSpaceCorePlugin` (new)
- `BigSpacePropagationPlugin` (new)

Based on user feedback, the world query types have also been renamed:

- `GridTransform` -> `CellTransform`
- `GridTransformOwned` -> `CellTransformOwned`
- `GridTransformReadOnly` -> `CellTransformReadOnly`

### Changed: Default plugin filters

Plugins that accept an optional query filter no longer require specifying the default empty filter tuple turbofish `::<()>`:

- `GridHashPlugin::<()>::default()` -> `GridHashPlugin::default()`
- `GridPartitionPlugin::<()>::default()` -> `GridPartitionPlugin::default()`

To construct a plugin with a custom filter, use the `new()` method:
`GridHashPlugin::<With<Player>>::new()`

### New: `no_std` Support

Thanks to `bushrat011899`'s efforts upstream and in this crate, it is now possible to use the plugin without the rust standard library. This is particularly useful when targeting embedded or console targets.

### Optimized: Partition Bounds

Partition bounds are now computed and cached when the partition is updated instead of on the fly. Adding a cell to the partition is an O(1) update to the partition bounds, and a removal is O(n). Previously, every bound query was O(n).

## v0.9.0 - 2024-12-23

### New: `GridCell` Spatial Hashing

Spatial hashing makes fast spatial queries and neighbor lookups possible. This release adds the `GridHashMap`, an automatically updated map of the entities in each grid cell. This makes it possible to query things like:

- What other entities are in the same cell as this entity?
- Are these two entities in the same cell?
- What entities are in adjacent grid cells?

This introduces a new component, the `GridHash`, which is automatically kept up to date, and stores a precomputed hash. This makes the spatial hash map especially fast, because hashing is only done when an entity moves between cells, not every time a hash map lookup is needed.

The map has received a few rounds of optimization passes to make incremental updates and neighbor lookups especially fast. This map does not suffer from hash collisions.

### New: Spatial Partitioning

Built on top of the new spatial hashing feature is the `GridPartitionMap`. This map tracks groups of adjacent grid cells that have at least one entity. Each of these partitions contains many entities, and each partition is independent. That is, entities in partition A are guaranteed to be unable to collide with entities in partition B.

This lays the groundwork for adding physics integrations. Because each partition is a clump of entities independent of all other entities, it should be possible to have independent physics simulations for each partition. Not only will this allow for extreme parallelism, it becomes possible to use 32-bit physics simulations in a 160-bit big_space.

### `ReferenceFrame` Renamed `Grid`

While revisiting documentation, it became clear that the naming scheme can be confusing and inconsistent. Most notably, it wasn't immediately clear there is a relationship between `ReferenceFrame` and `GridCell`. Additionally, there were multiple places where reference frames were clarified to be fixed precision grids.

To clear this up, `ReferenceFrame` has been renamed `Grid`. The core spatial types in this library are now:

- `Grid`: Defines the size of a grid for its child cells.
- `GridCell`: Cell index of an entity within its parent's grid.
- `GridPrecision`: Integer precision of a grid.

The newly added types follow this pattern:

- `GridHash`: The spatial hash of an entity's grid cell.
- `GridHashMap`: A map for entity, grid cell, and neighbor lookups.
- `GridPartition`: Group of adjacent grid cells.
- `GridPartitionMap`: A map for finding independent partitions of entities.

It should now be more clear how all of the `Grid` types are related to each other.