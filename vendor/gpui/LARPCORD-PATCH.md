# image atlas reclamation

gpui 0.2.2, zed industries and contributors, apache-2.0. original source: https://static.crates.io/crates/gpui/gpui-0.2.2.crate

runtime changes are limited to metal_atlas.rs, directx_atlas.rs and blade_atlas.rs. unused upstream examples, documentation, tests and their cargo target declarations are omitted from this vendored dependency. removing an image now deallocates its etagere tile so shared texture pages can reuse space. metal also removes every retired key from its lookup table immediately, matching the other backends. the application waits for submitted frames before retiring its images.

run `cargo run --release --features media-stress --bin media-stress` for a real graphics allocation soak. it never loads a discord account or downloads media.
