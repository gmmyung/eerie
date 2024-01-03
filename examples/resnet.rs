use std::{path::PathBuf, str::FromStr};

#[cfg(feature = "runtime")]
use eerie::runtime::{
    hal::{BufferMapping, BufferView},
    vm::List,
};

#[cfg(feature = "compiler")]
fn compile_mlir(data: &[u8]) -> Vec<u8> {
    use eerie::compiler;
    let compiler = compiler::Compiler::new().unwrap();
    let mut compiler_session = compiler.create_session();
    compiler_session
        .set_flags(vec![
            "--iree-hal-target-backends=llvm-cpu".to_string(),
            "--iree-input-type=stablehlo".to_string(),
        ])
        .unwrap();
    let source = compiler_session.create_source_from_buf(data).unwrap();
    let mut invocation = compiler_session.create_invocation();
    let mut output = compiler::MemBufferOutput::new(&compiler).unwrap();
    invocation
        .parse_source(source)
        .unwrap()
        .set_verify_ir(true)
        .set_compile_to_phase("end")
        .unwrap()
        .pipeline(compiler::Pipeline::Std)
        .unwrap()
        .output_vm_byte_code(&mut output)
        .unwrap();
    Vec::from(output.map_memory().unwrap())
}

#[cfg(feature = "std")]
fn load_image_bin(path: PathBuf) -> Vec<f32> {
    let data = std::fs::read(path).unwrap();
    let mut image_bin = Vec::new();
    for i in 0..data.len() / 4 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&data[i * 4..i * 4 + 4]);
        image_bin.push(f32::from_le_bytes(bytes));
    }
    image_bin
}

fn run(vmfb: &[u8], image_bin: &[f32]) -> Vec<f32> {
    use eerie::runtime;
    use eerie::runtime::vm::ToRef;

    let instance = runtime::api::Instance::new(
        &runtime::api::InstanceOptions::new(&mut runtime::hal::DriverRegistry::new())
            .use_all_available_drivers(),
    )
    .unwrap();
    let device = instance
        .try_create_default_device("local-task")
        .expect("Failed to create device");
    let session = runtime::api::Session::create_with_device(
        &instance,
        &runtime::api::SessionOptions::default(),
        &device,
    )
    .unwrap();
    unsafe { session.append_module_from_memory(vmfb) }.unwrap();
    let function = session.lookup_function("module.serving_default").unwrap();
    let input_list =
        runtime::vm::DynamicList::<runtime::vm::Ref<runtime::hal::BufferView<f32>>>::new(
            1, &instance,
        )
        .unwrap();
    let input_buffer = runtime::hal::BufferView::<f32>::new(
        &session,
        &[1, 224, 224, 3],
        runtime::hal::EncodingType::DenseRowMajor,
        image_bin,
    )
    .unwrap();
    let input_buffer_ref = input_buffer.to_ref(&instance).unwrap();
    input_list.push_ref(&input_buffer_ref).unwrap();
    let output_list =
        runtime::vm::DynamicList::<runtime::vm::Ref<runtime::hal::BufferView<f32>>>::new(
            1, &instance,
        )
        .unwrap();
    function.invoke(&input_list, &output_list).unwrap();
    let output_buffer_ref = output_list.get_ref(0).unwrap();
    let output_buffer: BufferView<f32> = output_buffer_ref.to_buffer_view(&session);
    let output_mapping = BufferMapping::new(output_buffer).unwrap();
    let out = output_mapping.data().to_vec();
    out
}

fn main() {
    env_logger::init();
    // timer for compile
    #[cfg(feature = "compiler")]
    {
    let start = std::time::Instant::now();
    let mlir_bytecode = std::fs::read("examples/resnet50.mlir").unwrap();
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
    let id2label_file = std::fs::read_to_string("examples/id2label.txt").unwrap();
    let id2label: Vec<&str> = id2label_file.split("\n").collect();
    println!("The image is classified as: {}", id2label[max_idx]);
    }
}
