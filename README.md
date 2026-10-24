# Eerie 👻

[![CI](https://img.shields.io/github/actions/workflow/status/gmmyung/eerie/rust.yml)](https://github.com/gmmyung/eerie/actions/workflows/rust.yml) [![Crates.io](https://img.shields.io/crates/v/eerie)](https://crates.io/crates/eerie) [![Documentation](https://docs.rs/eerie/badge.svg)](https://docs.rs/eerie)

Rust bindings for [IREE](https://iree.dev): compile MLIR to VMFB modules and run
inference on CPUs, GPUs, or embedded devices. Supports Linux, macOS, Windows x64
(MSVC), and `no_std` targets.

The API is experimental and may change between releases. Eerie 0.7 uses
**IREE 3.13.0**; use the matching compiler library and recompile models generated
by IREE 3.11 or earlier.

Using Melior and Eerie in one process remains unsupported on macOS with stock
compiler wheels because their LLVM/MLIR symbols can collide.

## Installation

```toml
[dependencies]
eerie = "0.7.0"
```

The default features enable `compiler`, `runtime`, and `std`. To run precompiled
models without the compiler library:

```toml
[dependencies]
eerie = { version = "0.7.0", default-features = false, features = ["std", "runtime"] }
```

Building the runtime requires **CMake 3.26+** and **Clang/libclang**. On macOS,
install Xcode and the macOS SDK. On Windows, install Visual Studio C++ build
tools, Ninja, and LLVM; build for `x86_64-pc-windows-msvc` from an x64 Visual
Studio developer shell and set `LIBCLANG_PATH` to LLVM's `bin` directory.

### Compiler library

The `compiler` feature requires the IREE shared library. Install it with:

```sh
python -m pip install iree-base-compiler==3.13.0
```

`Compiler::new()` discovers the library in your active Python environment and
loads it automatically. Building your Rust project requires no compiler library
paths or linker configuration.

For custom builds, use `Compiler::with_library(path)` with a library file or its
directory. If you have multiple Python installations, `EERIE_PYTHON` can select
the Python executable used for discovery.

## Compile and run a model

Save this as `simple_mul.mlir`:

```mlir
module @arithmetic {
  func.func @simple_mul(%lhs: tensor<4xf32>, %rhs: tensor<4xf32>) -> tensor<4xf32> {
    %result = arith.mulf %lhs, %rhs : tensor<4xf32>
    return %result : tensor<4xf32>
  }
}
```

Compile it into owned VMFB bytes:

```rust
#[cfg(feature = "compiler")]
fn compile_model() -> Result<Vec<u8>, eerie::compiler::CompilerError> {
    use eerie::compiler::Compiler;

    Compiler::new()?.compile_file(
        "simple_mul.mlir",
        ["--iree-hal-target-backends=llvm-cpu"],
    )
}
```

Compiler handles can be created, cloned, and reused for multiple models.
`compile_str` accepts MLIR text, and `compile_bytes` accepts MLIR text or bytecode.

Load the bytes, create input buffers, and invoke the function:

```rust
#[cfg(feature = "runtime")]
fn run_model(vmfb: &[u8]) -> Result<Vec<f32>, eerie::runtime::RuntimeError> {
    use eerie::runtime::{BufferView, DeviceSpec, Runtime};

    let runtime = Runtime::new(DeviceSpec::local_sync())?;
    let program = runtime.load_vmfb(vmfb)?;
    let lhs = runtime.buffer_view(&[4], &[1.0_f32, 2.0, 3.0, 4.0])?;
    let rhs = runtime.buffer_view(&[4], &[4.0_f32, 3.0, 2.0, 1.0])?;
    let function = program.function("arithmetic.simple_mul")?;
    let output: BufferView<f32> = function.invoke_one([&lhs, &rhs])?;
    output.read() // [4.0, 6.0, 6.0, 4.0]
}
```

`invoke_one` returns one typed result; `invoke` supports multiple results.
`BufferView::read_into` reuses an existing output slice. Buffers support booleans,
8/16/32/64-bit integers, `f32`, and `f64`; enable `half` for `f16` and `bf16`.

## Devices

Choose a device with `Runtime::new(DeviceSpec::…)` and compile your model for the
corresponding backend:

| DeviceSpec constructor | Compiler backend | Requirements |
| --- | --- | --- |
| `local_sync()` / `local_task()` | `llvm-cpu` or `vmvx` | Linux, macOS, Windows |
| `metal()` | `metal-spirv` | macOS with Metal |
| `cuda()` | `cuda` | `cuda` feature, CUDA toolkit and compatible driver |
| `vulkan()` | `vulkan-spirv` | `vulkan` feature, Linux/Windows, Vulkan 1.3 |
| `local_inline()` | `vmvx-inline` | Inline execution model; see below |

Use `.ordinal(0)` to select a device by index, or query
`Runtime::available_devices(driver)` and pass a result's `.spec()` to `Runtime::new`.
Vulkan requires timeline semaphores, scalar block layout, and synchronization2.
CUDA's default `sm_89` output requires a driver supporting CUDA 12.4; use
`--iree-cuda-target-features` to select a different PTX version if needed.

## Embedded / no_std

Enable only `runtime` to build with `no_std` and `alloc`. Supply an embedded C/C++
toolchain, a global allocator, a `critical-section` implementation, and your
board's startup code and linker script. See the [Cortex-M example](https://github.com/gmmyung/eerie/tree/main/tests/bare-metal).

Select `DeviceSpec::local_inline()` and compile a portable inline model:

```sh
iree-compile model.mlir --iree-hal-target-backends=vmvx-inline \
  --iree-execution-model=inline-static -o model.vmfb
```

For native embedded kernels, use `llvm-cpu` with
`--iree-execution-model=inline-dynamic` and your target's triple, CPU, features,
and ABI flags. Models compiled for the ordinary execution model cannot run on
an inline runtime.

## More

- [API documentation](https://docs.rs/eerie)
- [Examples](https://github.com/gmmyung/eerie/tree/main/examples)
- [Changelog](https://github.com/gmmyung/eerie/blob/main/CHANGELOG.md)
- [Contributing](https://github.com/gmmyung/eerie/blob/main/CONTRIBUTING.md)

Licensed under [Apache 2.0](https://github.com/gmmyung/eerie/blob/main/LICENSE).
