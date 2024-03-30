# Reference Frames

## 2024-03-21: Condensed Outline

Attempting to further refine the concept of reference frames into something more concrete.

### Hierarchy Requirements

Any entity with a `GridCell` must fit one of the following:

- Parentless; exists in the global reference frame.
- Has a parent with a `ReferenceFrame`.

There can only be a single floating origin, and it must also meet the above criteria.



### Transform Propagation

Transform propagation starts from the floating origin and propagates outward.

- Entities in the same reference frame as the floating origin work the same as they do today without reference frames.
- To maximize precision between reference frames, sibling reference frames should have their transform computed relative to reference frame that contains the floating origin.
  - The naive solution would instead compute the offset from the floating origin reference frame, to the parent reference frame, then back to the siblings, which can accumulate significantly more error.
  - Should probably start with the naive solution.
- When propagating transforms, accumulate an offset (similar to bevy's transform propagation) that describes the accumulated transform from the floating origin's grid cell to the current reference frame's transform. This has a few nice properties:
  - Transform propagation is simpler - simply compute the double precision transform of every entity in the reference frame relative to the origin cell, then add the reference frame's offset.
  - Moving the floating origin between reference frames is easier, and doesn't require a hierarchy traversal. The offset is global, not local.

#### Pseudocode

Note: composing isometries:

```
pub fn mul_transform(&self, transform: Transform) -> Self {
        let translation = self.transform_point(transform.translation);
        let rotation = self.rotation * transform.rotation;

pub fn transform_point(&self, mut point: Vec3) -> Vec3 {
    point = self.scale * point;
    point = self.rotation * point;
    point += self.translation;
    point
}
```

1. Compute the `origin_transform` for all `ReferenceFrame`s by propagating the floating origin grid cell's position through the reference frame hierarchy.
    1. Start in the floating origin's reference frame (RF). Set the `origin_transform` to the floating origin gridcell and an identity translation and rotation.
    2. Recurse up and down the tree, computing the `origin_transform` for each RF.
        - Parent of this RF: 
            1. Current RF's `origin_transform` into a `DMat4`
            2. Temp `DMat4` from `origin_transform` of the current RF's gridcell and Transform. 
            3. Multiply (2) * (1)
            4. Convert the transform into a new RelativeOriginTransform
        - Sibling of this RF:
            1. Current RF's `origin_transform` into a `DMat4`
            2. `DMat4` from the sibling to the current RF using the delta in `GridCell` and `Transform`s
            3. Multiply (2) * (1)
            4. Convert the translation component into gridcell and translation, and extract the rotation
        - Child of this RF:
            1. 
2. Compute the `GlobalTransform` for all `GridCell` entities using their `ReferenceFrame`.
  - For all entities in this reference frame, compute their `GlobalTransform` using the normal global-transform-from-gridcell technique, then multiply by the ***inverse*** of the `origin_transform`'s translation and rotation (as a transform). You take the inverse here for the same reason the view transform is the inverse of the camera transform. The inverse is how you get into the floating origin RF's space.
3. Propagate transforms normally using the entities in (2) as root transforms, as well as normally
   running propagation for transforms that do not have a gridcell.


1. Compute the reference frame's `origin_transform`.
  - This step is lossy because we are limited to double precision.
  - Very important: this is relative to the origin of the floating origin's gridcell, ignoring the floating origin's transform within that cell. Recall that when we compute the `GlobalTransform`, we are doing this relative to the grid, not the floating origin itself.
  - If this is the initial RF (floating origin's RF) we can compute this directly, the result is simply the floating origin's gridcell, and a transform of identity.
  - If this is *not* the initial RF, we need to do a lossy conversion. Convert the f64 translation into a gridcell and small translation in the current RF. Combine the small translation with the rotation, to make a small transform. This transform represents how to transform to the floating origin's grid cell.
2. For all entities in this reference frame, compute their `GlobalTransform` using the normal global-transform-from-gridcell technique, then multiply by the ***inverse*** of the `origin_transform`'s translation and rotation (as a transform). You take the inverse here for the same reason the view transform is the inverse of the camera transform. The inverse is how you get into the floating origin RF's space.
 
## 2024-03-17: Gather thoughts

### Using Reference Frames with a Floating Origin

- Any entity with `GridCell` and `ReferenceFrame` components will be treated as a reference frame.
- The position of any entity is defined in high precision using a `GridCell` and a `Transform`, and is relative to the reference frame that entity is a child of.
- Rendering uses the reference frame and the position of the floating origin within that reference frame to compute the `GlobalTransform` of all entities relative to this floating origin. This step is lossy, but does not affect the absolute position of objects in high precision.
  - To maximize rendering precision, the floating origin needs to be able to move into different reference frames. Rendering precision is then dependent on the reference frame you are in.
    - If you are in the reference frame of a planet, you won't be able to render objects on the surface of its moon with high precision; these will accumulate precision loss like a traditional transform hierarchy.

### Computing `GlobalTransform`s for rendering

Run in three passes. The first pass computes a high precision offset from the floating origin to the origin of this reference frame. The second pass computes the `GlobalTransform` of all `GridCell` entities, using this offset. The third pass is a normal bevy transform propagation for entity hierarchies without

1. Compute the grid cell the floating origin is located within for every reference frame.

   - Algorithm summary:

     - reset `completed` flags on all reference frames.
     - Set the floating origin's reference frame as `Offset::Origin(DMat4)`, where the transform is the `grid_position_double` of the floating origin.
     - starting at the floating origin's reference frame, where the current reference frame is `current_frame`, do:
     - For each sibling `ReferenceFrame`, `sibling_frame`: take `current_frame.offset` and multiply by the transform from the `current_frame`'s `Transform` to `sibling_frame`'s `Transform`. These transforms tell you:

       - `(floating_origin -> current_frame)`: this is `current_frame.offset`
       - `(current_frame -> sibling_frame) `: computing now
       - `(floating_origin -> sibling_frame)`: output
       - These offsets should be `Offset::Global(DMat4)`.

     - Recurse depth-first into each reference frame, adding the `Offset` to each child reference frame's `DMat4` relative to the origin
     - update the parent's reference frame offset with the negative of the current reference frame's `grid_position_double`.
     - Set the parent reference frame as the current reference frame, and repeat from (1).

   - a. Starting from the reference frame the floating origin is in, set the transform offset. For this first iteration, the transform offset will be zero, because the origin is inside this reference frame.
   - b. Recurse down the tree, marking reference frames that have been computed this frame. The reference frame that is a child of this one will have a transform offset equal to the `grid_position_double` relative to the floating origin in this grid. Must account for rotation! This offset is the transform of the origin of a reference frame.
   - c. Recurse up the tree, marking reference frames that have been computed this frame. The reference frame that is the parent of this one will have a transform offset equal to the **_negative_** of the `grid_position_double` of the floating origin's reference frame entity.
   - d. Repeat (c) up the tree breadth-first. We do _not_ want to do this depth first, because we will accumulate unnecessary error. Instead, we should compute all reference frame offsets of all _siblings_ of the current reference frame, because we can do this with high precision using `grid_position_double` on the _difference_ between the current reference frame, and the one we want to compute the offset for.
     - For example, we should compute the position of galaxies relative to the current galaxy, instead of computing them relative to the origin of the visible universe or local supercluster.

### Valid entity hierarchies with `GridCell`

Any entity with a `GridCell` must match one of the following definitions, or it is invalid:

- with no parent: in the global reference frame
- with a parent: that parent _MUST_ have a `ReferenceFrame`
  - update_global_from_grid: use `ReferenceFrame::floating_origin_cell` to compute the offset.

## 2024-03-15: Brainstorming

Children of an object in the grid are positioned relative to their parent. How is their `GlobalTransform` computed?

- GT is computed as the parent location with this entities transform applied
- if the parent is very far from the origin, maybe if it is the center of a planet for example, then transforming the object close to the camera could lose precision.

Adding reference frames

- Current: global reference frame
- Needed: reference frames based on entities. Children of a reference frame entity will be positioned using their parent reference frame
  - This does not solve any issues with precision for rendering. First, the existing systems need to take into account reference frames
    - when checking if an entity is outside of its grid cell, current systems should work, they only look at the grid cell settings, which are global, to determine if a transform is large enough to move to an adjacent grid cell. This only uses transform translations and the grid size.
    - This should enable high precision movement withing a reference frame
  - To make this useful for rendering, we need a few other things:
    - the floating origin's grid cell needs to be computed for each of these reference frames. This should be stored in the reference frame, e.g. where is the floating origin in this grid? It's possible this could overflow.
    - (No, don't need this) Maybe the reference frame should have bounds? If the floating origin is not within a reference frame's bounds, do we revert back to simple transform calculations?
      - once inside the bounds, each entity in that frame would have its global transform computed based on the floating origin's grid cell within that frame,
      - otherwise, the current transform propagation system that exists in the PR should be used, where we convert grid cell + transform -> big translation
      - ... --> this might just be an optimization we don't need? The user should be despawning unimportant things, it might be better to stick to a unified system, where the positions are always computed with floating origin, and every reference frame is computing the floating origin's position within that reference frame

### Entity with gridcell

- with no parent: in the global reference frame
- with a parent: that parent _MUST_ have a `ReferenceFrame`
  - update_global_from_grid: use `ReferenceFrame::floating_origin_cell` to compute the offset.
    - Need to account for parent's rotation though???
    - Also need to account for parent's translation in the grid
    - Basically, the grid has an offset, that is related to the parent reference frame's position and orientation within _its parent's_ grid. This is the parent's small Transform within its cell
    - Compute the offset of the entity relative to the floating origin's cell, this is the entity's `Transform`, effectively. Then, apply the reference frame's `Transform`, just like you were doing any other transform propagation.
      - need to do recursively. This is just like existing system, except we also account for the floating origin's position within this referemce frame.
    - Computing the floating origin's position within the reference frame could be a bit lossy. Need to be able to do this with the grid cell integers, plus rotation.
      1. Compute the grid offset of the reference frame from the floating origin (at the current level of recursion, this may not be the global reference frame)
      2. Note: this is always going to be lossy? However it should affect all entities in a reference frame equally, because the error is just what grid cell the origin is currently in
    - This seems like an unsolvable problem, without changing approach.
      - Computing the position of the origin w.r.t. the current reference frame is a lossy calculation. If you are at the edge of one reference frame, even with double precision - e.g. at the edge of the solar system - something in that reference frame may jump between grid cells due to the imprecision, as the reference frame itself is moving relative to the global reference frame.
      - It seems like the floating origin always needs to be within a certain reference frame for this to work?
        - Instead of the floating origin always being in the global reference frame, maybe instead it needs to hop into reference frames. So if it is at the edge of the solar system, the floating origin would just exist in that reference frame. If it is near a planet, it needs to go into that reference frame. The floating origin needs to be at the deepest nested reference frame where it makes sense to be.
        - We can take a lossy approach to computing reference frame positions, but this is now okay because we shouldn't have two objects interacting in different reference frames. E.g. if the camera is near an asteroid, they should both be in the same reference frame.
