# Eerie 👻
![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/gmmyung/eerie/rust.yml) ![GitHub License](https://img.shields.io/github/license/gmmyung/eerie) ![Crates.io](https://img.shields.io/crates/v/eerie)


Eerie is a Rust binding of the IREE library. It aims to be a safe and robust API that is close to the IREE C API. 

By the way, this crate is experimental, and nowhere near completion. It might contain several unsound code, and API will have breaking changes in the future. If you encounter any problem, feel free to leave an issue.

### What can I do with this?
Rust has a few ML frameworks such as [Candle](https://github.com/huggingface/candle) and [Burn](https://github.com/burn-rs/burn), but none of them supports JIT/AOT model compilation. By using the IREE compiler/runtime, one can build a full blown ML library that can generate compiled models in runtime.

Also, the runtime of IREE can be small as ~30KB in bare-metal environments, so this can be used to deploy ML models in embedded rust in the future.

### Supported OS
- [x] MacOS
- [x] Linux
- [ ] Windows


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
Running the tensor operation in a IREE runtime environment
```rust
fn run_vmfb(vmfb: &[u8]) -> Vec<f32> {
    use eerie::runtime::*;
    use eerie::runtime::vm::{List, ToRef};
    let instance = api::Instance::new(
        &api::InstanceOptions::new(&mut hal::DriverRegistry::new())
            .use_all_available_drivers(),
    )
    .unwrap();
    let device = instance
        .try_create_default_device("local-task")
        .expect("Failed to create device");
    let session = api::Session::create_with_device(
        &instance,
        &api::SessionOptions::default(),
        &device,
    )
    .unwrap();
    unsafe { session.append_module_from_memory(vmfb) }.unwrap();
    let function = session.lookup_function("arithmetic.simple_mul").unwrap();
    let input_list = vm::DynamicList::<vm::Ref<hal::BufferView<f32>>>::new(
        2, &instance,
        )
        .unwrap();
    let input_buffer = hal::BufferView::<f32>::new(
        &session,
        &[4],
        hal::EncodingType::DenseRowMajor,
        &[1.0, 2.0, 3.0, 4.0]
    ).unwrap();
    let input_buffer_ref = input_buffer.to_ref(&instance).unwrap();
    input_list.push_ref(&input_buffer_ref).unwrap();
    let output_list = vm::DynamicList::<vm::Ref<hal::BufferView<f32>>>::new(
        1, &instance,
        )
        .unwrap();
    function.invoke(&input_list, &output_list).unwrap();
    let output_buffer_ref = output_list.get_ref(0).unwrap();
    let output_buffer: hal::BufferView<f32> = output_buffer_ref.to_buffer_view(&session);
    let output_mapping = hal::BufferMapping::new(output_buffer).unwrap();
    let out = output_mapping.data().to_vec();
    out
}
```
More examples [here](https://github.com/gmmyung/eerie/tree/main/examples)

## Installation
The crate is divided into two sections: compiler and runtime. You can enable each functionality by toggling the "compiler" and "runtime" feature flags.

### Runtime
Eerie builds the IREE runtime from source during compilation, so there is no need for setup.

### Compiler
The user must source the precompiled shared library. (This is necessary because it takes ~20 min to build the compiler) The shared library can be sourced from a python package installation of iree-compiler.
```sh
pip3 install iree-compiler=20231113.707
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

## References
- Also look at [SamKG/iree-rs](https://github.com/SamKG/iree-rs/tree/main)
- Rustic MLIR Bindings [raviqqe/melior](https://github.com/raviqqe/melior)

## License
[Apache 2.0](https://github.com/gmmyung/eerie/blob/main/LICENSE)
