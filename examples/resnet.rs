#[cfg(all(feature = "compiler", feature = "runtime"))]
use std::{path::PathBuf, str::FromStr};

#[cfg(all(feature = "compiler", feature = "runtime"))]
use eerie::runtime::hal::Tensor;
#[cfg(all(feature = "compiler", feature = "runtime"))]
fn compile_mlir(data: &[u8]) -> Vec<u8> {
    use eerie::compiler;
    let target_backend =
        std::env::var("EERIE_HAL_TARGET_BACKEND").unwrap_or_else(|_| "llvm-cpu".to_string());
    let compiler = compiler::Compiler::new().expect("failed to initialize IREE compiler");
    let mut compiler_session = compiler.create_session();
    compiler_session
        .set_flags(vec![
            format!("--iree-hal-target-backends={target_backend}"),
            "--iree-input-type=stablehlo".to_string(),
        ])
        .unwrap_or_else(|err| panic!("failed to set compiler flags for {target_backend}: {err:?}"));
    let source = compiler_session
        .create_source_from_buf(data)
        .expect("failed to create compiler source from resnet50.mlir");
    let mut invocation = compiler_session.create_invocation();
    let mut output =
        compiler::MemBufferOutput::new(&compiler).expect("failed to create compiler output");
    invocation
        .parse_source(source)
        .expect("failed to parse resnet50.mlir")
        .set_verify_ir(true)
        .set_compile_to_phase("end")
        .expect("failed to set IREE compile phase")
        .pipeline(compiler::Pipeline::Std)
        .expect("failed to compile resnet50.mlir")
        .output_vm_byte_code(&mut output)
        .expect("failed to emit VM bytecode");
    Vec::from(
        output
            .map_memory()
            .expect("failed to map compiler output memory"),
    )
}

#[cfg(all(feature = "compiler", feature = "runtime"))]
fn load_image_bin(path: PathBuf) -> Vec<f32> {
    let data = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "failed to read image tensor data from {}: {err}",
            path.display()
        )
    });
    assert_eq!(
        data.len() % core::mem::size_of::<f32>(),
        0,
        "image tensor data length is not f32-aligned"
    );
    let mut image_bin = Vec::new();
    for i in 0..data.len() / 4 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&data[i * 4..i * 4 + 4]);
        image_bin.push(f32::from_le_bytes(bytes));
    }
    image_bin
}
#[cfg(all(feature = "compiler", feature = "runtime"))]
fn run(vmfb: &[u8], image_bin: &[f32]) -> Vec<f32> {
    use eerie::runtime;
    let instance = runtime::vm::Instance::new().unwrap();
    let registry = runtime::hal::DriverRegistry::with_available_drivers()
        .expect("failed to create HAL driver registry");
    let hal_driver = std::env::var("EERIE_HAL_DRIVER").unwrap_or_else(|_| "local-task".to_string());
    let driver = registry
        .create_driver(&hal_driver)
        .unwrap_or_else(|err| panic!("failed to create HAL driver {hal_driver}: {err:?}"));
    let device = driver
        .create_default_device()
        .unwrap_or_else(|err| panic!("failed to create default device for {hal_driver}: {err:?}"));
    let hal_module =
        runtime::vm::Module::hal(&instance, &device).expect("failed to create HAL VM module");
    let bytecode_module =
        runtime::vm::Module::bytecode(&instance, vmfb).expect("failed to load bytecode module");
    let context = runtime::vm::Context::with_modules(&instance, &[&hal_module, &bytecode_module])
        .expect("failed to create VM context");
    let function = context
        .resolve_function("module.serving_default")
        .expect("failed to resolve module.serving_default");
    let input = Tensor::<f32>::from_slice(&device, &[1, 3, 224, 224], image_bin)
        .expect("failed to upload input image tensor");
    let outputs = function
        .invoke_tensors(&[&input], 1)
        .expect("failed to invoke module.serving_default");
    outputs[0]
        .read_to_vec(&device)
        .expect("failed to read output tensor to host")
}
#[cfg(all(feature = "compiler", feature = "runtime"))]
fn main() {
    env_logger::init();
    // timer for compile
    let start = std::time::Instant::now();
    let mlir_bytecode =
        std::fs::read("examples/resnet50.mlir").expect("missing examples/resnet50.mlir");
    let compiled_bytecode = compile_mlir(&mlir_bytecode);

    println!("Compiled in {} ms", start.elapsed().as_millis());

    // timer for run
    let start = std::time::Instant::now();
    let image_bin = load_image_bin(PathBuf::from_str("examples/cat.bin").unwrap());
    let output = run(&compiled_bytecode, &image_bin);
    println!("Run in {} ms", start.elapsed().as_millis());
    let max_idx = output
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .unwrap()
        .0;
    let id2label_file =
        std::fs::read_to_string("examples/id2label.txt").expect("missing examples/id2label.txt");
    let id2label: Vec<&str> = id2label_file.split("\n").collect();
    println!("The image is classified as: {}", id2label[max_idx]);
}

#[cfg(not(all(feature = "compiler", feature = "runtime")))]
fn main() {}
