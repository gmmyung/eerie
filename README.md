# Eerie 👻
[![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/gmmyung/eerie/rust.yml)](https://github.com/gmmyung/eerie/actions/workflows/rust.yml) [![GitHub License](https://img.shields.io/github/license/gmmyung/eerie)](https://github.com/gmmyung/eerie/blob/main/LICENSE) [![Crates.io](https://img.shields.io/crates/v/eerie)](https://crates.io/crates/eerie)


Eerie is a Rust binding of the IREE library. It aims to be a safe and robust API that is close to the IREE C API. 

By the way, this crate is experimental, and nowhere near completion. It might contain several unsound code, and API will have breaking changes in the future. If you encounter any problem, feel free to leave an issue.

### What can I do with this?
Rust has a few ML frameworks such as [Candle](https://github.com/huggingface/candle) and [Burn](https://github.com/burn-rs/burn), but none of them supports JIT/AOT model compilation. By using the IREE compiler/runtime, one can build a full blown ML library that can generate compiled models in runtime.

Also, the runtime of IREE can be small as ~30KB in bare-metal environments, so this can be used to deploy ML models in embedded rust in the future.

### Supported OS
- [x] macOS
- [x] Linux
- [ ] Windows
- [x] Bare-Metal (thumb, rv32)


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
    use eerie::compiler::*;
    use std::path::Path;
    let compiler = Compiler::new().unwrap();
    let mut session = compiler.create_session();
    session
        .set_flags(vec![
            "--iree-hal-target-backends=llvm-cpu".to_string(),
        ])
        .unwrap();
    let source = Source::from_file(&session, Path::new("simple_mul.mlir")).unwrap();
    let mut invocation = session.create_invocation();
    let mut output = MemBufferOutput::new(&compiler).unwrap();
    invocation
        .parse_source(source)
        .unwrap()
        .set_verify_ir(true)
        .set_compile_to_phase("end")
        .unwrap()
        .pipeline(Pipeline::Std)
        .unwrap()
        .output_vm_byte_code(&mut output)
        .unwrap();
    Vec::from(output.map_memory().unwrap())
}
```
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
    let outputs = function.invoke([&lhs, &rhs]).unwrap();
    let output: BufferView<f32> = outputs[0].clone().try_into().unwrap();
    output.read().unwrap()
}
```
More examples [here](https://github.com/gmmyung/eerie/tree/main/examples)

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
`Program::function`, invoke with `Function::invoke`, convert returned `Value`s
into typed `BufferView<T>` handles, and read outputs with `BufferView::read`.
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

`local-sync` is always available. `local-task` is available with `std`. `metal`
is the supported GPU path on macOS/Apple Silicon. `cuda` is enabled with the
`cuda` feature. `vulkan` can be requested with the `vulkan` feature on
non-macOS targets with a usable Vulkan loader/device; macOS Vulkan is not
supported by eerie.

#### macOS
Install Xcode and macOS SDKs.

#### No-std
The runtime library can be compiled without the default `std` feature for bare-metal targets. This requires a C/C++ embedded toolchain (`arm-none-eabi-gcc`/`riscv64-unknown-elf-gcc`) and a Rust target with `alloc` support. The embedded runtime path uses Rust `compiler_builtins`, `libm`, `tinyrlibc`, and a small `critical-section` backed synchronization shim instead of linking `libc`, `libm`, `nosys`, or pthreads.

Targets must provide:
- a global allocator
- a `critical-section` implementation, such as the `cortex-m` `critical-section-single-core` feature
- a linker script and startup/runtime appropriate for the board or emulator


### Compiler
The user must source the precompiled shared library. (This is necessary because it takes ~20 min to build the compiler) The shared library can be sourced from a python package installation of iree-base-compiler.
```sh
pip3 install iree-base-compiler==3.11.0
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
cargo check --no-default-features --features runtime
```

Do not use `cargo test --all-features` as a portable release check. It enables
optional backend features such as `cuda` and `vulkan`; CUDA requires a CUDA
toolkit configured for IREE's CMake build, and Vulkan is intentionally
unsupported on macOS.

## References
- Also look at [SamKG/iree-rs](https://github.com/SamKG/iree-rs/tree/main)
- Rustic MLIR Bindings [raviqqe/melior](https://github.com/raviqqe/melior)

## License
[Apache 2.0](https://github.com/gmmyung/eerie/blob/main/LICENSE)
