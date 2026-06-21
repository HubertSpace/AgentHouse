# Third-Party Notices

This file records third-party code, generated files, assets, and dependency
license facts for AgentHouse. AgentHouse source code is licensed under
Apache-2.0; third-party components remain under their own licenses.

This is an engineering release-readiness record, not legal advice. Before
publishing signed binaries or packaged installers, regenerate the dependency
license report from the exact `Cargo.lock` being released.

## Code And Assets Included In This Repository

### `vendor/zed-sum-tree`

Included files:

- `vendor/zed-sum-tree/src/sum_tree.rs`
- `vendor/zed-sum-tree/src/cursor.rs`
- `vendor/zed-sum-tree/src/tree_map.rs`
- `vendor/zed-sum-tree/src/property_test.rs`

Source project: Zed

Source repository: `https://github.com/zed-industries/zed`

Pinned source revision: `5b948e5aaba15cf4446cfdf9cef1cfddcb062bee`

Source path: `crates/sum_tree`

Source license: Apache-2.0

Source copyright: Copyright 2022 - 2025 Zed Industries, Inc.

AgentHouse use: vendored and patched Rust crate used by the pinned GPUI
dependency path.

Local modifications:

- removed the normal dependency on `ztracing`;
- removed tracing instrumentation attributes tied to that dependency;
- replaced the test-only `zlog::init_test()` call with `env_logger`;
- kept the public `sum_tree` API and implementation behavior otherwise aligned
  with the pinned upstream source.

Reason for inclusion: at the pinned revision, the upstream `sum_tree` dependency
graph pulls GPL-3.0-or-later tracing crates. AgentHouse vendors the Apache-2.0
`sum_tree` crate and removes that tracing-only path so the macOS Beta
normal/build dependency graph remains free of GPL-family licenses.

License text: Apache-2.0 text is available in `LICENSE`.

### `vendor/cbindgen-scene-shim`

Included files:

- `vendor/cbindgen-scene-shim/src/lib.rs`
- `vendor/cbindgen-scene-shim/src/scene.h`

AgentHouse-authored code:

- `vendor/cbindgen-scene-shim/src/lib.rs`

Generated third-party-derived file:

- `vendor/cbindgen-scene-shim/src/scene.h`

Generated from: GPUI macOS scene header generation for the pinned upstream
revision `5b948e5aaba15cf4446cfdf9cef1cfddcb062bee`.

Related source repository: `https://github.com/zed-industries/zed.git`

Related source path: GPUI macOS scene types used by `gpui_macos` shader build
scripts.

License for shim crate: Apache-2.0

License for generated header: treated as part of the Apache-2.0 GPUI source
generation path for the pinned revision.

Reason for inclusion: the upstream macOS build uses the crates.io `cbindgen`
tool to generate a fixed Metal shader C header. `cbindgen` is MPL-2.0.
AgentHouse includes a small Apache-2.0 compatibility shim and the generated
header for the pinned GPUI revision so the macOS Beta build graph does not pull
MPL-2.0 build-time code.

### `vendor/dirs-shim`

Included files:

- `vendor/dirs-shim/src/lib.rs`

Source: AgentHouse-authored compatibility shim.

License: Apache-2.0

API compatibility target: the subset of the crates.io `dirs` API used by pinned
GPUI/font-kit dependencies:

- `dirs::home_dir()`
- `dirs::data_dir()`

Reason for inclusion: the crates.io `dirs` crate is MIT OR Apache-2.0, but its
`dirs-sys` dependency pulls `option-ext` under MPL-2.0. This local shim keeps
the current macOS Beta dependency graph free of MPL while preserving the API
surface used by pinned dependencies.

### Geist Fonts

Included files:

- `crates/agenthouse/assets/fonts/geist/geist-latin.woff2`
- `crates/agenthouse/assets/fonts/geist/geist-latin-ext.woff2`
- `crates/agenthouse/assets/fonts/geist/geist-mono-latin.woff2`
- `crates/agenthouse/assets/fonts/geist/geist-mono-latin-ext.woff2`
- `crates/agenthouse/assets/fonts/geist/LICENSE.md`

Source: Geist font assets copied from the locally installed `next` package used
by the AgentHouse UI design reference.

License: MIT

Copyright: Copyright (c) 2025 Vercel, Inc.

License text: `crates/agenthouse/assets/fonts/geist/LICENSE.md`.

### Application Icon

Included file:

- `crates/agenthouse/assets/app-icon.jpg`

Source: maintainer-supplied AgentHouse project asset.

License: AgentHouse project asset, distributed with this repository under the
project's Apache-2.0 terms unless replaced by a separately licensed asset.

If the icon is later replaced with third-party, generated, or commissioned
artwork that carries attribution or license requirements, update this section
before public binary distribution.

## Direct Git Dependencies

### Alacritty terminal

Package: `alacritty_terminal`

Source: `https://github.com/alacritty/alacritty.git`

Pinned revision: `79adb086002be778cec5d3b90676594bfe2105bb`

License: Apache-2.0

Used by: `crates/ah-terminal`

### GPUI

Packages include `gpui`, `gpui_platform`, `gpui_macos`, `gpui_macros`,
`collections`, `util`, `util_macros`, `media`, and related support crates from
the upstream repository.

Source: `https://github.com/zed-industries/zed.git`

Pinned revision: `5b948e5aaba15cf4446cfdf9cef1cfddcb062bee`

Top-level GPUI package license: Apache-2.0

Notes:

- AgentHouse patches `sum_tree` to `vendor/zed-sum-tree` via
  `[patch."https://github.com/zed-industries/zed.git"]`.
- `gpui_shared_string` and `gpui_util` currently have no package-level license
  field in the pinned upstream manifests; the upstream repository includes
  `LICENSE-APACHE` and `LICENSE-GPL`.
- AgentHouse patches crates.io `dirs` and `cbindgen` to local Apache-2.0 shims
  for the macOS Beta dependency graph.

### font-kit fork

Package: `zed-font-kit`

Source: `https://github.com/zed-industries/font-kit`

Pinned revision: `94b0f28166665e8fd2f53ff6d268a14955c82269`

License: MIT OR Apache-2.0

Pulled through GPUI on macOS.

### wgpu fork

Packages include `wgpu`, `wgpu-core`, `wgpu-hal`, `wgpu-types`, and related
support crates.

Source: `https://github.com/zed-industries/wgpu.git`

Pinned revision: `357a0c56e0070480ad9daea5d2eaa83150b79e88`

License: MIT OR Apache-2.0

Pulled through GPUI.

## crates.io Dependencies

The exact crates.io dependency set is locked in `Cargo.lock`. The dependency
graph currently includes common permissive licenses such as MIT, Apache-2.0,
BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unlicense, CC0-1.0, Unicode-3.0, and
CDLA-Permissive-2.0.

Notable non-MIT/Apache findings from the current macOS `agenthouse`
normal/build dependency graph:

- BSD-family crates remain present through GPUI, image/font rendering,
  Rustls/WebPKI, and build tooling. BSD-2-Clause and BSD-3-Clause are
  permissive open-source licenses and are not treated as release blockers.
- `r-efi`: offers MIT OR Apache-2.0 OR LGPL-2.1-or-later; AgentHouse relies on
  the permissive alternatives where applicable.
- `self_cell`: offers Apache-2.0 OR GPL-2.0-only; AgentHouse relies on the
  Apache-2.0 alternative where applicable.

Current AgentHouse `agenthouse` normal/build dependency graph must not include
`zlog`, `ztracing`, `ztracing_macro`, GPL-family licenses, or MPL licenses.

Release audit command:

```sh
scripts/check-release-licenses.sh
```

Suggested dependency metadata export:

```sh
cargo metadata --format-version=1 \
  | jq -r '.packages[] | select(.source != null) | [.name, .version, (.license // "NO-LICENSE-FIELD"), .source] | @tsv' \
  | sort -u
```

## System Frameworks

AgentHouse uses Apple system frameworks through Rust Objective-C bindings for
AppKit, CoreGraphics, Foundation, and WebKit/WKWebView. These frameworks are
provided by macOS and are not bundled in this repository.
