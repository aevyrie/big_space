<div align="center">
  
# Big Space

[![crates.io](https://img.shields.io/crates/v/big_space)](https://crates.io/crates/big_space)
[![docs.rs](https://docs.rs/big_space/badge.svg)](https://docs.rs/big_space)
[![test suite](https://github.com/aevyrie/big_space/actions/workflows/rust.yml/badge.svg)](https://github.com/aevyrie/big_space/actions/workflows/rust.yml)
[![Bevy tracking](https://img.shields.io/badge/Bevy%20tracking-main-lightblue)](https://github.com/bevyengine/bevy/blob/main/docs/plugins_guidelines.md#main-branch-tracking)

A floating origin plugin for [Bevy](https://github.com/bevyengine/bevy).

https://user-images.githubusercontent.com/2632925/215318129-5bab3095-a7dd-455b-a4b6-71840cde096c.mp4

### [Read the docs](https://docs.rs/big_space)

</div>

## Features

Lots of space to play in.

This is a floating origin plugin, useful if you want to work with very large or very small scales. It works with bevy's existing `f32`-based `Transform`s, which means it's largely compatible with the bevy ecosystem. The plugin positions entities within large fixed precision grids, effectively adding precision to the location of objects.

Additionally, you can use reference frames to nest high precision coordinate systems. For example you might want to put all entities on a planet's surface into the same reference frame. You can then rotate this reference frame with the planet, and orbit that planet around a star.

The plugin is generic over a few integer types, to trade off scale and precision for memory use. Some fun numbers with a worst case precision of 0.5mm:
  - `i8`: 2,560 km = 74% of the diameter of the Moon
  - `i16`: 655,350 km = 85% of the diameter of the Moon's orbit around Earth
  - `i32`: 0.0045 light years = ~4 times the width of the solar system
  - `i64`: 19.5 million light years = ~100 times the width of the milky way galaxy
  - `i128`: 3.6e+26 light years = ~3.9e+15 times the width of the observable universe

This can also be used for small scales. With a cell edge length of `1e-11`, and using `i128`, there is enough precision to render objects the size of quarks anywhere in the observable universe.

From the docs: https://docs.rs/big_space/latest/big_space/precision/trait.GridPrecision.html

# Bevy Version Support

| bevy | big_space |
| ---- | --------- |
| 0.14 | 0.7       |
| 0.13 | 0.5, 0.6  |
| 0.12 | 0.4       |
| 0.11 | 0.3       |
| 0.10 | 0.2       |
| 0.9  | 0.1       |

# License

This project is dual licensed:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
