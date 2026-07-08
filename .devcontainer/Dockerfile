# Official Dev Containers Rust image (Debian bookworm). Ships a current stable
# Rust toolchain, cargo, clippy, rustfmt, and the `vscode` non-root user.
FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# System packages gpu-probe needs at BUILD and RUN time:
#
#   build-essential, pkg-config   -> C compiler for rusqlite's "bundled" SQLite
#                                    (compiled in-tree via the `cc` crate).
#   cmake                          -> occasionally needed by native build deps.
#   libvulkan1, mesa-vulkan-drivers, vulkan-tools
#                                  -> Vulkan loader + Mesa "lavapipe" software
#                                    ICD so `wgpu` can find an adapter with no
#                                    physical GPU present. Provides `vulkaninfo`.
#   libgl1-mesa-dri, libglx-mesa0, libegl1, libgles2, libglvnd0
#                                  -> Mesa "llvmpipe" software OpenGL/EGL stack
#                                    as a secondary backend for `wgpu`.
#
# NOTE: gpu-probe deliberately REJECTS a CPU/software adapter (it refuses to
# score a CPU as a GPU). On a GPU-less Codespace it will therefore enumerate the
# lavapipe adapter, print a `no_gpu` message, and exit non-zero — that is the
# correct, tested behaviour. The software stack is here so build + enumeration
# work end-to-end; a real score requires a host with a real GPU passed through.
RUN apt-get update \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        cmake \
        libvulkan1 \
        mesa-vulkan-drivers \
        vulkan-tools \
        libgl1-mesa-dri \
        libglx-mesa0 \
        libegl1 \
        libgles2 \
        libglvnd0 \
        ca-certificates \
        sqlite3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
