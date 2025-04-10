# Changelog

## UNRELEASED

### New: `no_std` Support

Thanks to `bushrat011899`'s efforts upstream and in this crate, it is now possible to use the plugin without the rust standard library. This is particularly useful when targeting embedded or console targets.

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