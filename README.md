<div align="center">

# Big Space

<img src="https://raw.githubusercontent.com/aevyrie/big_space/refs/heads/main/assets/bigspacebanner.svg" width="80%" alt="partitioning screenshot">

Huge worlds, high performance, no dependencies, ecosystem compatibility. [Read the docs](https://docs.rs/big_space)

[![crates.io](https://img.shields.io/crates/v/big_space)](https://crates.io/crates/big_space)
[![docs.rs](https://docs.rs/big_space/badge.svg)](https://docs.rs/big_space)
[![test suite](https://github.com/aevyrie/big_space/actions/workflows/rust.yml/badge.svg)](https://github.com/aevyrie/big_space/actions/workflows/rust.yml)

</div>

## Highlights

- Enough precision to render proton-sized meshes across the observable universe.
- Uses `Transform`, making it compatible with most of the Bevy ecosystem.
- No added dependencies.
- Absolute coordinates without drift, unlike camera-relative or periodic recentering solutions.
- Chunks the world into nestable integer grids, from `i8` up to `i128`.
- Spatial hashing for fast grid cell lookups and neighbor search.
- Spatial partitioning to group sets of connected cells.
- Great performance scaling and parallelism with massive entity counts.
- 👉 [Extensive documentation you should read.](https://docs.rs/big_space)

![screenshot](https://github.com/user-attachments/assets/736a1dec-91a1-4ac1-9382-82084ebe6c1c)

## Showcase

### [Proton to Observable Universe scale](examples/demo.rs)

https://github.com/user-attachments/assets/430624ee-e3a4-4ba3-b7cf-72f3d7f00b5f

### [Floating origin demonstration](examples/error.rs)

https://github.com/user-attachments/assets/9ce5283f-7d48-47dc-beef-9a7626858ed4

## Bevy Version Support

| bevy | big_space |
|------|-----------|
| 0.16 | 0.10      |
| 0.15 | 0.8, 0.9  |
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
