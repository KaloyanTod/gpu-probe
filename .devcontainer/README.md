# Dev Container for gpu-probe

This `.devcontainer/` gives you a reproducible environment (GitHub Codespaces or
VS Code "Reopen in Container") with the Rust toolchain and every system library
gpu-probe needs to **build, test, and lint**.

## What you get

- Rust stable (clippy + rustfmt) from the official devcontainer image.
- A C toolchain for rusqlite's `bundled` SQLite — no system SQLite required.
- A Mesa **software** graphics stack (Vulkan *lavapipe* + OpenGL *llvmpipe*) so
  `wgpu` can enumerate an adapter even though a Codespace has no physical GPU.
- `vulkaninfo` and the `sqlite3` CLI for poking at results.
- rust-analyzer, TOML, LLDB, and SQLite VS Code extensions.

The container runs `cargo build --release` on creation to warm the cache.

## Important: you will NOT get a GPU score in a plain Codespace

gpu-probe is an *instrument* — it deliberately **refuses to score a CPU as a
GPU**. A GPU-less Codespace only exposes the lavapipe **software** adapter, which
`wgpu` reports as `DeviceType::Cpu`. gpu-probe enumerates it, sees it is a CPU
adapter, prints a `no_gpu:` message, and exits non-zero.

**This is correct, tested behaviour**, not a setup error. Use this container to:

```sh
cargo build --release      # verify it compiles against the pinned deps
cargo clippy --all-targets # lint
cargo run --release        # exercises setup -> reaches the `no_gpu` path
vulkaninfo | head          # confirm the lavapipe adapter is visible
```

To produce a real benchmark row you need a machine with a real GPU and working
Vulkan / Metal / DX12 / OpenGL drivers (a local machine, or a GPU-enabled host
that passes the device through to the container).
