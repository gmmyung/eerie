# Contributing

## Development setup

Use the Nix shell for the pinned C/C++ toolchain and matching IREE compiler:

```sh
nix develop
cargo fmt --all --check
python -m unittest discover -s tools -p 'test_*.py'
cargo test --locked --release --features half
cargo test --locked --no-default-features --features std,runtime,half
cargo check --locked --no-default-features --features compiler
cargo check --locked --no-default-features --features runtime
python tools/regenerate-fixtures.py --check
```

Without Nix, install the native dependencies and the compiler Python package
as described in the [README](README.md). Windows builds need an x64 Visual Studio
developer shell, Ninja, and LLVM.

Do not use `cargo test --all-features` as a portable check: it enables CUDA,
which needs its toolkit, and Vulkan, which is unsupported on macOS.

## Cross-language LTO

The optional `cross-lang-lto` feature builds the runtime and its native C/C++
dependencies with Clang ThinLTO. It supports hosted Linux and macOS targets.
Native LTO is controlled explicitly by the feature; extra Rust flags are not
translated into C/C++ flags. CMake still uses Cargo's optimization/debug profile.

Use the Nix shell to select Clang, LLVM archive tools, and LLD from the
same LLVM package set as Rust, with the application's linker-plugin settings:

```sh
nix develop .#cross-lang-lto
cargo test --locked --release --no-default-features --features std,runtime,half,cross-lang-lto
python tools/test-cross-lang-lto.py
```

The last command compares feature-off and feature-on builds with identical
Rust linker settings. Both execute the same buffer-rank test; disassembly must
show that only the feature-on build inlines IREE's C getter into the Rust wrapper.

On macOS the shell also provides a linker adapter for rustc's GNU-style LTO
options. ARM builds omit Clang's additional `ccidx` requirement so LLVM can
inline C functions into Rust's baseline target. Use the supplied shell on macOS;
it selects upstream LLVM Clang rather than Xcode's AppleClang.

On Linux without Nix, install compatible Clang, `llvm-ar`, `llvm-ranlib`, and LLD,
set `CC=clang` and `CXX=clang++`, and configure the application's Cargo target:

```toml
# .cargo/config.toml; use your Linux target triple.
[build]
target = "x86_64-unknown-linux-gnu"

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-Clinker-plugin-lto", "-Clink-arg=-fuse-ld=lld"]
```

Then enable `cross-lang-lto` on the `eerie` dependency. These application-level
settings are required in addition to the feature; Cargo profile `lto` settings
alone do not arrange the joint Rust/C link. See the
[Rust toolchain requirements](https://doc.rust-lang.org/rustc/linker-plugin-lto.html).

## Fixtures and IREE updates

`tools/regenerate-fixtures.py` recreates the full-HAL VMVX, inline VMVX, and
64-bit VM call fixtures. Run it without `--check` to update them. Keep the IREE
submodule, compiler package version, Nix environment, workflows, documentation,
and generated fixtures aligned in the same change. Recheck compatibility when
the compiler or HAL ABI changes.

After cloning with submodules, run `python tools/init-runtime-deps.py` to
initialize the runtime's nested dependencies. The CI workflows do this before
building.

## Platform validation

The Rust workflow covers Linux, macOS, Windows x64 MSVC, and Cortex-M execution.
It checks default, runtime-only, compiler-only, and no_std builds, fixture
reproducibility, offline runtime documentation, and packaged raw bindings.
Windows exercises debug and release builds, plus static CRT linkage.

The separate Melior regression uses LLVM/MLIR 22 and checks both initialization
orders in the same process. Run it with `nix develop .#melior -c python tools/test-melior.py`.
CI runs it on Linux. Stock macOS compiler wheels still allow LLVM/MLIR weak
symbols to bind to the host's implementation; local runtime loading does not
isolate those symbols, so this configuration remains unsupported on macOS.

Run the standalone Cortex-M test with the Rust `thumbv7em-none-eabihf` target,
Arm GNU toolchain, Clang/libclang, CMake, and `qemu-system-arm` on `PATH`:

```sh
python tools/test-bare-metal.py
```

The test runs on QEMU's `mps2-an386` board and checks the final link map.
Toolchain C headers are used, but newlib/libc, native libm, nosys, libgcc, and
libatomic archives must not enter the executable. Rust `compiler_builtins`,
`libm`, `tinyrlibc`, and the `critical-section` synchronization shim supply the
runtime support. RISC-V and board-specific native kernels need separate target
configuration and hardware or emulator validation.

GPU checks are opt-in on machines with the corresponding drivers/toolkits:

```sh
EERIE_TEST_METAL=1 cargo test --release --features half  # macOS
EERIE_TEST_VULKAN=1 cargo test --release --features vulkan integration_tests::vulkan_smoke
EERIE_TEST_CUDA=1 EERIE_CUDA_TARGET=sm_89 cargo test --release --features cuda integration_tests::cuda_smoke
```

The Linux CI job also tests Vulkan with Mesa's software driver. The manual
GPU workflow targets a self-hosted Linux runner labeled `gpu` for CUDA/Vulkan
hardware validation.

## Releasing

Pushing a `vX.Y.Z` tag runs the Linux/macOS/Windows and bare-metal suite, then
publishes to crates.io using the repository's `CARGO_REGISTRY_TOKEN` secret.
The tag must match the root `Cargo.toml` version, and its commit must already
be on `main`.

Before tagging:

1. Bump the versions of changed crates and update the root `eerie-sys` dependency.
2. Update the workspace and standalone test `Cargo.lock` files in `tests/bare-metal` and `tests/melior`.
3. Finalize `CHANGELOG.md` and merge the version changes into `main`.

The crate versions may differ; the tag always uses the `eerie` version.
With the native tools and matching compiler installed, preview the release
without uploading anything (replace the version with the intended release):

```sh
python tools/publish.py --dry-run vX.Y.Z
```

Then tag the merged commit:

```sh
git switch main
git pull --ff-only
git tag vX.Y.Z
git push origin vX.Y.Z
```

The publish job verifies both packages before uploading `eerie-sys`, followed
by `eerie`. It skips an existing version only when the original manifest and
source files match. Reusing a version with different sources fails before
either upload. If only one crate was published, rerun the failed workflow to
publish the remaining crate. Releases are serialized to prevent competing
uploads.
