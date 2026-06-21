# cbindgen Scene Header Shim

This crate patches the crates.io `cbindgen` dependency used by the pinned
`gpui_macos` build scripts.

Files:

- `src/lib.rs`: AgentHouse-authored Apache-2.0 compatibility shim.
- `src/scene.h`: generated header from the pinned GPUI macOS scene generation
  path.

The upstream build uses `cbindgen` only to generate a Metal shader C header from
a fixed set of GPUI scene types. `cbindgen` is MPL-2.0, so AgentHouse replaces
it with this Apache-2.0 shim for the public Beta line.

The bundled `src/scene.h` is the generated scene header for upstream revision
`5b948e5aaba15cf4446cfdf9cef1cfddcb062bee`. Keep the generated header intact:
Metal shader compilation is sensitive to the exact generated declarations.

When upgrading GPUI, regenerate the header with upstream `cbindgen`, compare it
against `src/scene.h`, and update this shim if any scene type layout or shader
binding index changed.
