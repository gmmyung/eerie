# Eerie 👻
[![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/gmmyung/eerie/rust.yml)](https://github.com/gmmyung/eerie/actions/workflows/rust.yml) [![GitHub License](https://img.shields.io/github/license/gmmyung/eerie)](https://github.com/gmmyung/eerie/blob/main/LICENSE) [![Crates.io](https://img.shields.io/crates/v/eerie)](https://crates.io/crates/eerie)


Eerie is a Rust binding of the IREE library. It aims to be a safe and robust API that is close to the IREE C API. 

Eerie 0.6 uses IREE **3.13.0** for both the runtime sources and compiler library.
Recompile VMFB artifacts produced by IREE 3.11 or earlier: the HAL module ABI
changed to 0.7 even though the VM bytecode format remains version 17.

By the way, this crate is experimental, and nowhere near completion. It might contain several unsound code, and API will have breaking changes in the future. If you encounter any problem, feel free to leave an issue.

### What can I do with this?
Rust has a few ML frameworks such as [Candle](https://github.com/huggingface/candle) and [Burn](https://github.com/burn-rs/burn), but none of them supports JIT/AOT model compilation. By using the IREE compiler/runtime, one can build a full blown ML library that can generate compiled models in runtime.

Also, the runtime of IREE can be small as ~30KB in bare-metal environments, so this can be used to deploy ML models in embedded rust in the future.

### Supported OS
- [x] macOS
- [x] Linux
- [ ] Windows
- [x] Bare-metal (`no_std`; Cortex-M4 execution tested in QEMU)


## Examples
Compiling MLIR code into IREE VM framebuffer

```mlir
// simple_mul.mlir

module @arithmetic {
  func.func @simple_mul(%arg0: tensor<4xf32>, %arg1: tensor<4xf32>) -> tensor<4xf32> {
    %0 = arith.mulf %arg0, %arg1 : tensor<4xf32>
    return %0 : tensor<4xf32>
  }
}
```
```rust
#[cfg(all(feature = "std", feature = "compiler"))]
fn output_vmfb() -> Vec<u8> {
    use eerie::compiler::Compiler;

    let compiler = Compiler::new().unwrap();
    compiler
        .compile_file("simple_mul.mlir", ["--iree-hal-target-backends=llvm-cpu"])
        .unwrap()
}
```
Reuse one `Compiler` for multiple compilations. `compile_str` accepts MLIR text,
and `compile_bytes` accepts MLIR text or bytecode. Each call uses a fresh session
and returns owned VMFB bytes. The session and invocation APIs remain available
for custom pipelines and output formats.

Running a buffer view operation in an IREE runtime environment
```rust
#[cfg(feature = "runtime")]
fn run_vmfb(vmfb: &[u8]) -> Vec<f32> {
    use eerie::runtime::{BufferView, DeviceSpec, Runtime};

    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();

    let lhs = runtime.buffer_view(&[4], &[1.0, 2.0, 3.0, 4.0]).unwrap();
    let rhs = runtime.buffer_view(&[4], &[4.0, 3.0, 2.0, 1.0]).unwrap();

    let function = program.function("arithmetic.simple_mul").unwrap();
    let output: BufferView<f32> = function.invoke_one([&lhs, &rhs]).unwrap();
    output.read().unwrap()
}
```
More examples [here](https://github.com/gmmyung/eerie/tree/main/examples)

## Releasing

Pushing a `vX.Y.Z` tag runs the full Linux/macOS and bare-metal CI suite, then
publishes to crates.io using the repository's `CARGO_REGISTRY_TOKEN` secret.
The tag must match the version in the root `Cargo.toml`, and its commit must
already be on `main`.

Before tagging, bump the versions of changed crates, update the root
`eerie-sys` dependency to match the native crate's version, update both
`Cargo.lock` files (including `tests/bare-metal/Cargo.lock`), and finalize the
changelog. For the current unreleased changes, the next versions are
`eerie-sys 0.4.1` and `eerie 0.6.1`.

```sh
# After merging the version changes into main:
git switch main
git pull --ff-only
git tag v0.6.1
git push origin v0.6.1
```

The publish job verifies both packaged crates before uploading `eerie-sys`,
followed by `eerie`. Already-published versions are skipped only when their
original manifest and source files match. A reused version with different
sources fails before either upload. If a release stops after publishing one
crate, rerun the failed GitHub Actions run to publish the remaining crate.

With the native build tools and matching IREE compiler configured, preview a
release locally without uploading anything:

```sh
python3 tools/publish.py --dry-run v0.6.1
```

## Installation
The crate is divided into two sections: compiler and runtime. You can enable each functionality by toggling the "compiler" and "runtime" feature flags.

### Runtime
Eerie builds the IREE runtime from source during compilation. CMake, Clang are required.

The runtime API is `runtime::Runtime -> Program -> Function`. It creates the
shared VM instance, HAL device, modules, and VM context for you and serializes
VM/HAL lifecycle setup around IREE's low-level initialization path. The shared
VM instance root is retained for the lifetime of the process or embedded
program because IREE HAL type registration uses process-global adapter state.
Typed tensor-shaped runtime values are represented by `runtime::BufferView<T>`.
The low-level VM/HAL assembly APIs are intentionally crate-private; users create
input buffers through `Runtime::buffer_view`, resolve functions through
`Program::function`, and invoke with `Function::invoke_one` for one typed result
or `Function::invoke` for dynamically typed results. Read outputs with
`BufferView::read`, or reuse host storage with `BufferView::read_into`.
`rank`, `element_count`, and `byte_length` query buffer metadata without
allocating a shape vector.
Supported `BufferView<T>` element types are `bool` (IREE Bool8), signed and
unsigned 8/16/32/64-bit integers, `f32`, and `f64`. The `half` feature adds
`f16` and `bf16` support through the optional `half` crate dependency.

Runtime device selection uses `runtime::DeviceSpec`. Common constructors cover
the built-in drivers:

```rust,no_run
use eerie::runtime::{DeviceSpec, Runtime, RuntimeError};

fn create_runtimes() -> Result<(), RuntimeError> {
    Runtime::new(DeviceSpec::local_sync())?;
    Runtime::new(DeviceSpec::local_task())?;
    Runtime::new(DeviceSpec::metal())?;
    Runtime::new(DeviceSpec::vulkan().ordinal(0))?;
    Ok(())
}
```

`Runtime::available_devices(driver)` queries devices reported by a linked HAL
driver, and `DeviceInfo::spec()` converts a query result back into a
`DeviceSpec`. `DeviceSpec::custom(...)` and `Driver::custom(...)` are available
for downstream IREE drivers without adding new Rust enum variants.

`local-inline` uses a host heap allocator and inline HAL without creating a
device or starting proactor threads. Select it with `DeviceSpec::local_inline()`
and compile modules for the inline execution model described below.

`local-sync` is available on hosted targets and selects inline HAL on bare-metal
targets. `local-task` is available on hosted targets. `metal`
is the supported GPU path on macOS/Apple Silicon. `cuda` is enabled with the
`cuda` feature. `vulkan` can be requested with the `vulkan` feature on
non-macOS targets with a Vulkan 1.3 loader/device supporting timeline semaphores,
scalar block layout, and synchronization2; macOS Vulkan is not
supported by eerie.

CUDA compilation for `sm_89` now defaults to PTX 8.4 and requires a driver that
supports CUDA 12.4. Supply `--iree-cuda-target-features` to override the compiler's
PTX default when appropriate for the target hardware.

#### macOS
Install Xcode and macOS SDKs.

#### No-std
The runtime library can be compiled without the default `std` feature for bare-metal targets. This requires a C/C++ embedded toolchain (`arm-none-eabi-gcc`/`riscv64-unknown-elf-gcc`) and a Rust target with `alloc` support. Toolchain C headers are used during compilation, but the executable does not link newlib/libc, native libm, nosys, libgcc, or libatomic. Rust `compiler_builtins`, the Rust `libm` and `tinyrlibc` crates, and a small `critical-section` backed synchronization shim supply the required runtime support. The QEMU smoke test checks the final link map for forbidden C runtime archives.

Targets must provide:
- a global allocator
- a `critical-section` implementation, such as the `cortex-m` `critical-section-single-core` feature
- a linker script and startup/runtime appropriate for the board or emulator

Bare-metal runtimes use inline HAL because IREE's full HAL drivers now require a
platform proactor. `DeviceSpec::local_inline()` and the existing
`DeviceSpec::local_sync()` select this path; other drivers are unavailable.
The `Runtime -> Program -> Function` and `BufferView<T>` APIs remain the same.

Compile portable embedded workloads with:

```sh
iree-compile tests/mul.mlir \
  --iree-hal-target-backends=vmvx-inline \
  --iree-execution-model=inline-static -o model.vmfb
```

For native embedded ELF kernels, use `llvm-cpu` with
`--iree-execution-model=inline-dynamic` and the target's triple, CPU, features,
and ABI flags. The runtime supplies the `hal_inline` and `hal_loader` modules.
Ordinary full-HAL VMFBs cannot be used by an inline runtime.

The standalone [Cortex-M test](tests/bare-metal) links and runs the Rust API on
QEMU's `mps2-an386` board. With a Rust `thumbv7em-none-eabihf` target, Arm GNU
toolchain, Clang/libclang, CMake, and `qemu-system-arm` on `PATH`, run:

```sh
python3 tools/test-bare-metal.py
```

RISC-V and board-specific native kernels need their own target configuration
and hardware or emulator validation.

### Compiler
The user must source the precompiled shared library. (This is necessary because it takes ~20 min to build the compiler) The shared library can be sourced from a python package installation of iree-base-compiler.
```sh
pip3 install iree-base-compiler==3.13.0
```

In order to export the installed library location, run this script:
```sh
python -c "import iree.compiler as _; print(f'{_.__path__[0]}/_mlir_libs/')"
```

Then, set the rpath and environment variable accordingly. This can be done by adding the following `.cargo/config.toml` to your project directory.
 
macOS
```toml
[build]
rustflags = ["-C", "link-arg=-Wl,-rpath,/path/to/library/"]
rustdocflags = ["-C", "link-arg=-Wl,-rpath,/path/to/library"]
[env]
LIB_IREE_COMPILER = "/path/to/library"
```
Linux
```toml
[build]
rustflags = ["-C", "link-arg=-Wl,-rpath=/path/to/library/"]
rustdocflags = ["-C", "link-arg=-Wl,-rpath=/path/to/library"]
[env]
LIB_IREE_COMPILER = "/path/to/library"
```

## Development
Use the Nix shell for the pinned C/C++ and Rust toolchain:
```sh
nix develop
cargo test --features compiler,runtime,std
cargo test --no-default-features --features std,runtime,half
cargo check --no-default-features --features runtime
EERIE_TEST_METAL=1 cargo test --features half  # macOS with Metal
python tools/regenerate-fixtures.py --check
```

Do not use `cargo test --all-features` as a portable release check. It enables
optional backend features such as `cuda` and `vulkan`; CUDA requires a CUDA
toolkit configured for IREE's CMake build, and Vulkan is intentionally
unsupported on macOS.

`tools/regenerate-fixtures.py` recreates the checked-in full-HAL VMVX, inline
VMVX, and 64-bit VM call fixtures with the pinned compiler. CI checks the
generated bytes on Linux and macOS. Keep source, compiler package, and fixture
updates in the same change.

The CI matrix covers hosted Linux/macOS, runtime-only and compiler-only features,
offline runtime documentation, packaged raw bindings, and Cortex-M execution.
GPU tests can be run on suitable machines with `EERIE_TEST_CUDA=1` or
`EERIE_TEST_VULKAN=1` and the corresponding Cargo feature. CUDA tests also require
`EERIE_CUDA_TARGET` (for example `sm_89`) and an installed CUDA toolkit.

## References
- Also look at [SamKG/iree-rs](https://github.com/SamKG/iree-rs/tree/main)
- Rustic MLIR Bindings [raviqqe/melior](https://github.com/raviqqe/melior)

## License
[Apache 2.0](https://github.com/gmmyung/eerie/blob/main/LICENSE)
