# sum_tree Vendor Patch

This directory vendors the upstream `sum_tree` crate so AgentHouse can keep the
GPUI dependency path free of GPL-family licenses for the public Beta line.

Upstream source:

- Repository: `https://github.com/zed-industries/zed`
- Revision: `5b948e5aaba15cf4446cfdf9cef1cfddcb062bee`
- Upstream crate path: `crates/sum_tree`
- Upstream crate license: Apache-2.0
- Upstream copyright: Copyright 2022 - 2025 Zed Industries, Inc.

Local modifications:

- Removed the normal dependency on `ztracing`.
- Removed seven `#[instrument(skip_all)]` tracing attributes.
- Replaced the vendor crate's test-only `zlog::init_test()` call with
  `env_logger` so local tests do not depend on GPL-3.0-or-later
  tracing crates.
- Kept the public `sum_tree` API and implementation behavior otherwise
  aligned with the pinned upstream source.

The Apache-2.0 license text is available in the repository root `LICENSE`.
See `THIRD_PARTY_NOTICES.md` for the repository-level attribution record.
