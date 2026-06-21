# dirs Shim

This crate patches the crates.io `dirs` dependency used by pinned GPUI and
font-kit dependencies.

Files:

- `src/lib.rs`: AgentHouse-authored Apache-2.0 compatibility shim.

AgentHouse currently needs only:

- `dirs::home_dir()` from upstream `util`
- `dirs::home_dir()` and `dirs::data_dir()` from upstream font-kit

The upstream `dirs` crate is MIT OR Apache-2.0, but its `dirs-sys` dependency
pulls `option-ext` under MPL-2.0. This Apache-2.0 shim keeps the current
macOS Beta dependency graph free of MPL while preserving the API surface used
by the pinned dependencies.
