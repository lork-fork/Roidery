# sf-core, vendored from StitchFoundry

This crate is StitchFoundry's embroidery core (geometry, the embroidery object model and the stitch
engine), copied from <https://github.com/StitchFoundry/StitchFoundry>, `crates/sf-core`, at commit
`df75a96c0e5b7100471d41d38c52236757a154de` (2026-11-16).

Copyright (c) 2026 Bruce Wattendorf (SiteSentrix) and the StitchFoundry contributors.
Licensed MIT OR Apache-2.0, the same as Roidery (see `LICENSE-MIT`, `LICENSE-APACHE` and `NOTICE`
at the repository root).

Roidery uses it for the Satin Fill stitch type (`roidery-stitch`'s `foundry` module is the bridge).

## Changes from upstream

Kept as close to upstream as possible, so it can be updated by copying the folder again:

- `src/ai.rs` and `src/ai/` (Design from Prompt, Ask About Embroidery and the cloud requests) are
  left out, with their `pub mod ai;` line and the `sha2` dependency only they used.
- `Cargo.toml` names the versions of `i_overlay` and `base64` itself rather than taking them from
  the workspace, and points `repository` at StitchFoundry.
- `examples/` is left out.

To update: copy `crates/sf-core/src` from StitchFoundry over `src`, delete `ai.rs` and `ai/`, remove
`pub mod ai;` from `lib.rs`, and run `cargo test -p sf-core -p roidery-stitch`.
