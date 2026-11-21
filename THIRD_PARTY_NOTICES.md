# third party notices

The text input in `src/input.rs` is adapted from the [GPUI 0.2.2 input example](https://docs.rs/crate/gpui/0.2.2/source/examples/input.rs), by Zed Industries and contributors, licensed under Apache-2.0. The packaged source used is available at https://static.crates.io/crates/gpui/gpui-0.2.2.crate. Changes include styling, explicit submission, masked session input, and content limits. The full license is included in `licenses/GPUI-APACHE-2.0.txt`.

Larpcord's implementation is independent of Serein, oxidecord and zcord. Their public repositories were consulted for ecosystem research. Their source is not included in this project.

Third party dependencies retain their respective licenses, recorded in the package metadata and Cargo.lock.

pulldown-cmark 0.13.6 parses markdown. it is copyright 2015 google inc., licensed under mit; the full notice is included in `licenses/PULLDOWN-CMARK-MIT.txt`.

xattr 0.2.3 by steven allen is vendored under mit/apache-2.0 with a small linux compatibility patch documented in `vendor/xattr/LARPCORD-PATCH.md`. original source licenses are retained there.

alhazen-gpui and alhazen-core 0.7.2 by mohamed a. provide native video playback under apache-2.0. their complete license and notice files are included in `licenses/ALHAZEN-*-LICENSE.txt` and `licenses/ALHAZEN-*-NOTICE.txt`, including the vp9-mt attribution.

gpui 0.2.2 is vendored under apache-2.0 with graphics atlas reclamation fixes described in `vendor/gpui/LARPCORD-PATCH.md`. its original license is retained.

Inter and the bundled display-name fonts are distributed under the SIL Open Font License 1.1. Source: https://github.com/google/fonts (ofl/inter, bangers, biorhyme, cherrybombone, chicle, museomoderno, pixelifysans, zillaslab, playpensans, orbitron, newrocker, kalam). Copyright notices and the full license for each family are in `licenses/fonts/`. Unavailable display-name families fall back to Inter.

## gemoji emoji aliases

Unicode emoji names and aliases in assets/emoji-aliases.tsv are derived from github/gemoji db/emoji.json under MIT. The full notice is in licenses/GEMOJI-MIT.txt. No artwork from that project is bundled.

## resvg

Remote Discord badge SVGs are rasterized with resvg 0.45.1 under MIT. Its notice is in licenses/RESVG-MIT.txt. External image and file resolution is disabled.

## Wasmi

Wasmi 2.0.2 executes local composer tools. It is copyright 2018–2024 Robin Freyler, and the MIT notice is in `licenses/WASMI-MIT.txt`.
