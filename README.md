# Eerie 👻
[![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/gmmyung/eerie/rust.yml)](https://github.com/gmmyung/eerie/actions/workflows/rust.yml) [![GitHub License](https://img.shields.io/github/license/gmmyung/eerie)](https://github.com/gmmyung/eerie/blob/main/LICENSE) [![Crates.io](https://img.shields.io/crates/v/eerie)](https://crates.io/crates/eerie)


Eerie is a Rust binding of the IREE library. It aims to be a safe and robust API that is close to the IREE C API. 

By the way, this crate is experimental, and nowhere near completion. It might contain several unsound code, and API will have breaking changes in the future. If you encounter any problem, feel free to leave an issue.

### What can I do with this?
Rust has a few ML frameworks such as [Candle](https://github.com/huggingface/candle) and [Burn](https://github.com/burn-rs/burn), but none of them supports JIT/AOT model compilation. By using the IREE compiler/runtime, one can build a full blown ML library that can generate compiled models in runtime.

Also, the runtime of IREE can be small as ~30KB in bare-metal environments, so this can be used to deploy ML models in embedded rust in the future.

### Supported OS
- [x] MacOS
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
    use eerie::runtime::{BufferView, Driver, Runtime};

    let runtime = Runtime::new(Driver::LocalSync).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();

    let lhs = runtime.buffer_view(&[4], &[1.0, 2.0, 3.0, 4.0]).unwrap();
    let rhs = runtime.buffer_view(&[4], &[4.0, 3.0, 2.0, 1.0]).unwrap();

    let function = program.function("arithmetic.simple_mul").unwrap();
    let outputs = function.invoke([&lhs, &rhs]).unwrap();
    let output: BufferView<f32> = outputs.into_iter().next().unwrap().try_into().unwrap();
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
unsigned 8/16/32/64-bit integers, `f16`, `bf16`, `f32`, and `f64`.

Runtime driver selection uses `runtime::Driver`, not raw driver strings.
`Driver::LocalSync` is always available. `Driver::LocalTask` is available with
`std`, `Driver::Metal` is available on macOS with `std`, and `Driver::Cuda` is
available with `std` and the `cuda` feature.

#### MacOS
Install XCode and MacOS SDKs.

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

Then, set the rpath and envorinment variable accordingly. This can be done by adding the following `.cargo/config.toml` to your project directory.
 
MacOS
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

`cargo test --all-features` also enables the `cuda` feature and requires a CUDA
toolkit configured for IREE's CMake build.

## References
- Also look at [SamKG/iree-rs](https://github.com/SamKG/iree-rs/tree/main)
- Rustic MLIR Bindings [raviqqe/melior](https://github.com/raviqqe/melior)

## License
[Apache 2.0](https://github.com/gmmyung/eerie/blob/main/LICENSE)
