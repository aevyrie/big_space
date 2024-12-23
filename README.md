<div align="center">
  
# Big Space

[![crates.io](https://img.shields.io/crates/v/big_space)](https://crates.io/crates/big_space)
[![docs.rs](https://docs.rs/big_space/badge.svg)](https://docs.rs/big_space)
[![test suite](https://github.com/aevyrie/big_space/actions/workflows/rust.yml/badge.svg)](https://github.com/aevyrie/big_space/actions/workflows/rust.yml)

![Big space logo](https://raw.githubusercontent.com/aevyrie/big_space/refs/heads/main/assets/bigspacebanner.svg)

Huge worlds, high performance, no dependencies, ecosystem compatibility.
 [Read the docs](https://docs.rs/big_space)

</div>

## Highlights

- Enough precision to render protons across the observable universe.
- Uses `Transform`, making it compatible with most of the Bevy ecosystem.
- No added dependencies.
- Absolute coordinates without drift, unlike camera-relative or periodic recentering solutions.
- Chunks the world into integer grids, from `i8` up to `i128`.
- Grids can be nested.
- Spatial hashing for fast grid cell lookups and neighbor search.
- Spatial partitioning to group sets of disconnected entities.
- 3-5x faster than Bevy's transform propagation for wide hierarchies.
- 👉 [Extensive documentation you should read.](https://docs.rs/big_space)

## Bevy Version Support

| bevy | big_space |
| ---- | --------- |
| 0.15 | 0.8       |
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
