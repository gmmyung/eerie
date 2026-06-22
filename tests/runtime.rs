#![cfg(feature = "runtime")]

use eerie::runtime::{
    self,
    error::RuntimeError,
    hal::{Buffer, BufferMapping, BufferParams, BufferView, ElementType, Encoding},
    vm::{FunctionLinkage, List, ToRef, ToValue, Undefined, Value},
};
use half::f16;
use log::info;
use test_log::test;

fn local_sync_device() -> (
    runtime::hal::DriverRegistry,
    runtime::hal::Driver,
    runtime::hal::Device,
) {
    let registry = runtime::hal::DriverRegistry::with_available_drivers().unwrap();
    let driver = registry.create_driver("local-sync").unwrap();
    let device = driver.create_default_device().unwrap();
    (registry, driver, device)
}

#[test]
fn test_instance() {
    runtime::vm::Instance::new().unwrap();
}

#[test]
fn test_context_with_hal_module() {
    let instance = runtime::vm::Instance::new().unwrap();
    let (_registry, _driver, device) = local_sync_device();
    let hal_module = runtime::vm::Module::hal(&instance, &device).unwrap();
    runtime::vm::Context::with_modules(&instance, &[&hal_module]).unwrap();
}

#[test]
fn device_metadata() {
    let (_registry, driver, device) = local_sync_device();
    let devices = driver.available_devices().unwrap();
    assert!(!devices.is_empty());
    assert_eq!(devices[0].ordinal, 0);
    assert!(!devices[0].name.is_empty() || !devices[0].path.is_empty());
    assert!(!device.id().is_empty());
    device.capabilities().unwrap();
    assert!(device
        .query_i64("eerie.missing.query.category", "missing-key")
        .is_err());
    device.trim().unwrap();
}

#[test]
fn dynamic_list() {
    let instance = runtime::vm::Instance::new().unwrap();
    let mut list = List::<Value<i32>>::new(4, &instance).unwrap();
    list.push_value(1.to_value()).unwrap();
    list.push_value(2.to_value()).unwrap();
    list.push_value(3.to_value()).unwrap();
    list.push_value(4.to_value()).unwrap();
    let val = list.get_value::<i32>(0).unwrap();
    drop(list);
    assert_eq!(val.get(), 1);
}

#[test]
fn ref_list() {
    let instance = runtime::vm::Instance::new().unwrap();
    let (_registry, _driver, device) = local_sync_device();
    let mut list = List::<Undefined>::new(4, &instance).unwrap();
    let buffer = BufferView::<f32>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[1.0, 2.0, 3.0, 4.0],
    )
    .unwrap();
    info!("buffer: {:?}", buffer);
    let buffer_ref = buffer.to_ref(&instance).unwrap();
    list.push_ref(&buffer_ref).unwrap();
    list.push_ref(&buffer_ref).unwrap();
    let buffer_ref_2 = list.get_ref::<BufferView<f32>>(0).unwrap();
    let buffer_2 = buffer_ref_2.to_buffer_view().unwrap();
    info!("buffer_ref_2: {:?}", buffer_2);

    let mapping = BufferMapping::map_read(&buffer_2).unwrap();
    info!("mapping: {:?}", mapping.data());
}

#[test]
fn fp16_buffer() {
    let (_registry, _driver, device) = local_sync_device();
    let buffer = BufferView::<f16>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[
            f16::from_f32(1.0),
            f16::from_f32(2.0),
            f16::from_f32(3.0),
            f16::from_f32(4.0),
        ],
    )
    .unwrap();

    let mapping = BufferMapping::map_read(&buffer).unwrap();
    assert_eq!(
        mapping.data(),
        &[
            f16::from_f32(1.0),
            f16::from_f32(2.0),
            f16::from_f32(3.0),
            f16::from_f32(4.0)
        ]
    );
}

#[test]
fn bool_buffer_roundtrip_read_write_copy_and_mapping() {
    let (_registry, _driver, device) = local_sync_device();
    let buffer = BufferView::<bool>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[true, false, true, false],
    )
    .unwrap();

    assert_eq!(buffer.element_type(), ElementType::Bool8);
    assert_eq!(buffer.element_size(), core::mem::size_of::<bool>());
    assert_eq!(
        buffer.read_to_vec(&device).unwrap(),
        vec![true, false, true, false]
    );

    let mapping = BufferMapping::map_read(&buffer).unwrap();
    assert_eq!(mapping.data(), &[true, false, true, false]);
    drop(mapping);

    buffer
        .write_from_slice(&device, &[false, true, false, true])
        .unwrap();
    assert_eq!(
        buffer.read_to_vec(&device).unwrap(),
        vec![false, true, false, true]
    );

    let target = BufferView::<bool>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[false, false, false, false],
    )
    .unwrap();
    buffer.copy_to(&device, &target).unwrap();
    assert_eq!(
        target.read_to_vec(&device).unwrap(),
        vec![false, true, false, true]
    );
}

#[test]
fn bool_buffer_rejects_invalid_bool8_bytes() {
    let (_registry, _driver, device) = local_sync_device();
    let buffer = Buffer::allocate(&device, 1, BufferParams::default()).unwrap();
    let raw_view = BufferView::<u8>::from_buffer(&buffer, &[1], Encoding::DenseRowMajor).unwrap();
    raw_view.write_from_slice(&device, &[2]).unwrap();

    let bool_view =
        BufferView::<bool>::from_buffer(&buffer, &[1], Encoding::DenseRowMajor).unwrap();
    let err = bool_view.read_to_vec(&device).unwrap_err();
    assert!(format!("{err}").contains("invalid Bool8 value 2"));

    let err = match BufferMapping::map_read(&bool_view) {
        Ok(_) => panic!("invalid Bool8 mapping succeeded"),
        Err(err) => err,
    };
    assert!(format!("{err}").contains("invalid Bool8 value 2"));
}

#[test]
fn buffer_metadata() {
    let (_registry, _driver, device) = local_sync_device();
    let buffer = BufferView::<f32>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[1.0, 2.0, 3.0, 4.0],
    )
    .unwrap();

    assert_eq!(buffer.rank(), 2);
    assert_eq!(buffer.shape(), vec![2, 2]);
    assert_eq!(buffer.dim(0), 2);
    assert_eq!(buffer.dim(1), 2);
    assert_eq!(buffer.element_count(), 4);
    assert_eq!(buffer.element_size(), core::mem::size_of::<f32>());
    assert_eq!(buffer.element_type(), ElementType::Float32);
    assert_eq!(buffer.encoding(), Encoding::DenseRowMajor);
}

#[test]
fn raw_buffer_allocation_and_view() {
    let (_registry, _driver, device) = local_sync_device();
    let buffer = Buffer::allocate(
        &device,
        4 * core::mem::size_of::<f32>(),
        BufferParams::default(),
    )
    .unwrap();
    assert_eq!(buffer.byte_offset(), 0);
    assert_eq!(buffer.byte_length(), 4 * core::mem::size_of::<f32>());
    assert!(buffer.allocation_size() >= buffer.byte_length());
    assert_ne!(buffer.memory_type(), 0);
    assert_ne!(buffer.allowed_access(), 0);
    assert_ne!(buffer.allowed_usage(), 0);

    let view = BufferView::<f32>::from_buffer(&buffer, &[4], Encoding::DenseRowMajor).unwrap();
    let raw_buffer = view.raw_buffer();
    assert_eq!(raw_buffer.byte_length(), buffer.byte_length());
    assert_eq!(raw_buffer.memory_type(), buffer.memory_type());
    assert_eq!(raw_buffer.allowed_access(), buffer.allowed_access());
    assert_eq!(raw_buffer.allowed_usage(), buffer.allowed_usage());

    view.write_from_slice(&device, &[1.0, 2.0, 3.0, 4.0])
        .unwrap();
    assert_eq!(view.read_to_vec(&device).unwrap(), vec![1.0, 2.0, 3.0, 4.0]);

    let subspan = buffer.subspan(0, 2 * core::mem::size_of::<f32>()).unwrap();
    assert_eq!(subspan.byte_length(), 2 * core::mem::size_of::<f32>());
}

#[test]
fn buffer_shape_mismatch_is_rejected() {
    let (_registry, _driver, device) = local_sync_device();
    let err = BufferView::<f32>::from_host(
        &device,
        &[2, 3],
        Encoding::DenseRowMajor,
        &[1.0, 2.0, 3.0, 4.0],
    )
    .unwrap_err();

    assert!(matches!(err, RuntimeError::InvalidArgument(_)));
}

#[test]
fn buffer_view_read_write_and_copy() {
    let (_registry, _driver, device) = local_sync_device();
    let buffer = BufferView::<f32>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[1.0, 2.0, 3.0, 4.0],
    )
    .unwrap();
    assert_eq!(buffer.shape(), vec![2, 2]);
    assert_eq!(
        buffer.read_to_vec(&device).unwrap(),
        vec![1.0, 2.0, 3.0, 4.0]
    );

    buffer
        .write_from_slice(&device, &[5.0, 6.0, 7.0, 8.0])
        .unwrap();
    assert_eq!(
        buffer.read_to_vec(&device).unwrap(),
        vec![5.0, 6.0, 7.0, 8.0]
    );

    let target = BufferView::<f32>::from_host(
        &device,
        &[2, 2],
        Encoding::DenseRowMajor,
        &[0.0, 0.0, 0.0, 0.0],
    )
    .unwrap();
    buffer.copy_to(&device, &target).unwrap();
    assert_eq!(
        target.read_to_vec(&device).unwrap(),
        vec![5.0, 6.0, 7.0, 8.0]
    );
}

#[test]
fn append_module_from_vmvx_fixture() {
    let vmfb = include_bytes!("mul_vmvx.vmfb");
    let output = run_mul(vmfb, "local-sync");
    assert_eq!(output[0], 0.0);
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn function_metadata_and_buffer_view_invoke() {
    let vmfb = include_bytes!("mul_vmvx.vmfb");
    let instance = runtime::vm::Instance::new().unwrap();
    let registry = runtime::hal::DriverRegistry::with_available_drivers().unwrap();
    let driver = registry.create_driver("local-sync").unwrap();
    let device = driver.create_default_device().unwrap();
    let hal_module = runtime::vm::Module::hal(&instance, &device).unwrap();
    let bytecode_module = runtime::vm::Module::bytecode(&instance, vmfb).unwrap();
    let context =
        runtime::vm::Context::with_modules(&instance, &[&hal_module, &bytecode_module]).unwrap();
    let function = context.resolve_function("arithmetic.simple_mul").unwrap();

    assert_eq!(bytecode_module.name(), "arithmetic");
    let module_signature = bytecode_module.signature();
    assert!(module_signature.export_function_count >= 1);
    assert!(bytecode_module.lookup_attr("missing.module.attr").is_none());
    assert!(bytecode_module
        .attr(module_signature.attr_count)
        .unwrap()
        .is_none());
    let function_ref = bytecode_module
        .lookup_export_function("simple_mul")
        .unwrap();
    assert_eq!(function_ref.name(), "simple_mul");
    assert_eq!(function_ref.signature().argument_count, 2);
    assert!(function_ref.lookup_attr("missing.function.attr").is_none());

    let function_ref = bytecode_module
        .lookup_function("simple_mul", FunctionLinkage::Export)
        .unwrap();
    assert_eq!(function_ref.name(), "simple_mul");

    assert_eq!(function.name(), "simple_mul");
    let signature = function.signature();
    assert_eq!(signature.argument_count, 2);
    assert_eq!(signature.result_count, 1);
    assert!(function.lookup_attr("missing.function.attr").is_none());

    let input_data = Vec::from_iter((0..100).map(|i| i as f32));
    let input = BufferView::<f32>::from_host(
        &device,
        &[100],
        Encoding::DenseRowMajor,
        input_data.as_slice(),
    )
    .unwrap();
    let output = invoke_mul_function(&function, &instance, &device, &input);
    assert_eq!(output[0], 0.0);
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn invoke_after_newer_instance_rebinds_hal_types() {
    let vmfb = include_bytes!("mul_vmvx.vmfb");
    let instance = runtime::vm::Instance::new().unwrap();
    let registry = runtime::hal::DriverRegistry::with_available_drivers().unwrap();
    let driver = registry.create_driver("local-sync").unwrap();
    let device = driver.create_default_device().unwrap();
    let hal_module = runtime::vm::Module::hal(&instance, &device).unwrap();
    let bytecode_module = runtime::vm::Module::bytecode(&instance, vmfb).unwrap();
    let context =
        runtime::vm::Context::with_modules(&instance, &[&hal_module, &bytecode_module]).unwrap();
    let function = context.resolve_function("arithmetic.simple_mul").unwrap();

    let _newer_instance = runtime::vm::Instance::new().unwrap();

    let input_data = Vec::from_iter((0..100).map(|i| i as f32));
    let input = BufferView::<f32>::from_host(
        &device,
        &[100],
        Encoding::DenseRowMajor,
        input_data.as_slice(),
    )
    .unwrap();
    let output = invoke_mul_function(&function, &instance, &device, &input);
    assert_eq!(output[0], 0.0);
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn invoke_rebinds_prepared_hal_argument_refs() {
    let vmfb = include_bytes!("mul_vmvx.vmfb");
    let instance = runtime::vm::Instance::new().unwrap();
    let registry = runtime::hal::DriverRegistry::with_available_drivers().unwrap();
    let driver = registry.create_driver("local-sync").unwrap();
    let device = driver.create_default_device().unwrap();
    let hal_module = runtime::vm::Module::hal(&instance, &device).unwrap();
    let bytecode_module = runtime::vm::Module::bytecode(&instance, vmfb).unwrap();
    let context =
        runtime::vm::Context::with_modules(&instance, &[&hal_module, &bytecode_module]).unwrap();
    let function = context.resolve_function("arithmetic.simple_mul").unwrap();

    let input_data = Vec::from_iter((0..100).map(|i| i as f32));
    let input = BufferView::<f32>::from_host(
        &device,
        &[100],
        Encoding::DenseRowMajor,
        input_data.as_slice(),
    )
    .unwrap();
    let mut input_list = List::<Undefined>::new(2, &instance).unwrap();
    input_list
        .push_ref(&input.to_ref(&instance).unwrap())
        .unwrap();
    input_list
        .push_ref(&input.to_ref(&instance).unwrap())
        .unwrap();

    let _newer_instance = runtime::vm::Instance::new().unwrap();

    let mut output_list = List::<Undefined>::new(1, &instance).unwrap();
    function.invoke(&input_list, &mut output_list).unwrap();
    let output = output_list
        .get_ref::<BufferView<f32>>(0)
        .unwrap()
        .to_buffer_view()
        .unwrap()
        .read_to_vec(&device)
        .unwrap();
    assert_eq!(output[0], 0.0);
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn parallel_instance_invocations_rebind_hal_types() {
    let mut threads = Vec::new();
    for _ in 0..2 {
        threads.push(std::thread::spawn(|| {
            let vmfb = include_bytes!("mul_vmvx.vmfb");
            let output = run_mul(vmfb, "local-sync");
            assert_eq!(output[0], 0.0);
            assert_eq!(output[7], 49.0);
            assert_eq!(output[99], 9801.0);
        }));
    }

    for thread in threads {
        thread.join().unwrap();
    }
}

fn run_mul(vmfb: &[u8], driver_name: &str) -> Vec<f32> {
    let instance = runtime::vm::Instance::new().unwrap();
    let registry = runtime::hal::DriverRegistry::with_available_drivers().unwrap();
    let driver = registry.create_driver(driver_name).unwrap();
    let device = driver.create_default_device().unwrap();
    let hal_module = runtime::vm::Module::hal(&instance, &device).unwrap();
    let bytecode_module = runtime::vm::Module::bytecode(&instance, vmfb).unwrap();
    let context =
        runtime::vm::Context::with_modules(&instance, &[&hal_module, &bytecode_module]).unwrap();
    let function = context.resolve_function("arithmetic.simple_mul").unwrap();

    let input_data = Vec::from_iter((0..100).map(|i| i as f32));
    let input = BufferView::<f32>::from_host(
        &device,
        &[100],
        Encoding::DenseRowMajor,
        input_data.as_slice(),
    )
    .unwrap();

    invoke_mul_function(&function, &instance, &device, &input)
}

fn invoke_mul_function(
    function: &runtime::vm::Function,
    instance: &runtime::vm::Instance,
    device: &runtime::hal::Device,
    input: &BufferView<f32>,
) -> Vec<f32> {
    let mut input_list = List::<Undefined>::new(2, instance).unwrap();
    input_list
        .push_ref(&input.to_ref(instance).unwrap())
        .unwrap();
    input_list
        .push_ref(&input.to_ref(instance).unwrap())
        .unwrap();
    let mut output_list = List::<Undefined>::new(1, instance).unwrap();
    function.invoke(&input_list, &mut output_list).unwrap();
    let output_ref = output_list.get_ref::<BufferView<f32>>(0).unwrap();
    output_ref
        .to_buffer_view()
        .unwrap()
        .read_to_vec(&device)
        .unwrap()
}

fn run_bool_not(vmfb: &[u8], driver_name: &str) -> Vec<bool> {
    let instance = runtime::vm::Instance::new().unwrap();
    let registry = runtime::hal::DriverRegistry::with_available_drivers().unwrap();
    let driver = registry.create_driver(driver_name).unwrap();
    let device = driver.create_default_device().unwrap();
    let hal_module = runtime::vm::Module::hal(&instance, &device).unwrap();
    let bytecode_module = runtime::vm::Module::bytecode(&instance, vmfb).unwrap();
    let context =
        runtime::vm::Context::with_modules(&instance, &[&hal_module, &bytecode_module]).unwrap();
    let function = context.resolve_function("bools.logical_not").unwrap();

    let input = BufferView::<bool>::from_host(
        &device,
        &[4],
        Encoding::DenseRowMajor,
        &[true, false, false, true],
    )
    .unwrap();

    let mut input_list = List::<Undefined>::new(1, &instance).unwrap();
    input_list
        .push_ref(&input.to_ref(&instance).unwrap())
        .unwrap();
    let mut output_list = List::<Undefined>::new(1, &instance).unwrap();
    function.invoke(&input_list, &mut output_list).unwrap();
    output_list
        .get_ref::<BufferView<bool>>(0)
        .unwrap()
        .to_buffer_view()
        .unwrap()
        .read_to_vec(&device)
        .unwrap()
}

#[cfg(feature = "compiler")]
mod integration_tests {
    use eerie::compiler;
    use log::info;
    use std::path::Path;
    use std::sync::Mutex;

    use super::{run_bool_not, run_mul};

    static COMPILER: Mutex<Option<compiler::Compiler>> = Mutex::new(None);

    fn init_compiler() {
        let mut global_compiler = COMPILER.lock().unwrap();
        if global_compiler.is_none() {
            let compiler = compiler::Compiler::new().unwrap();
            *global_compiler = Some(compiler);
        }
    }

    fn compile_mul(target_backend: &str) -> Vec<u8> {
        compile_mlir(target_backend, Path::new("tests/mul.mlir"))
    }

    fn compile_bool_not(target_backend: &str) -> Vec<u8> {
        compile_mlir(target_backend, Path::new("tests/bool.mlir"))
    }

    fn compile_mlir(target_backend: &str, path: &Path) -> Vec<u8> {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let mut compiler_session = compiler.as_ref().unwrap().create_session();
        let mut flags = vec![format!("--iree-hal-target-backends={target_backend}")];
        if target_backend == "metal-spirv" {
            flags.push("--iree-metal-compile-to-metallib=false".to_string());
        }
        compiler_session.set_flags(flags).unwrap();
        let source = compiler_session.create_source_from_file(path).unwrap();
        let mut invocation = compiler_session.create_invocation();
        let mut output = compiler::MemBufferOutput::new(compiler.as_ref().unwrap()).unwrap();
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
        output.map_memory().unwrap().to_vec()
    }

    #[test]
    fn append_module() {
        let vmfb = compile_mul("llvm-cpu");
        let output = run_mul(&vmfb, "local-sync");
        info!("Output: {:?}", output);
        assert_eq!(output[0], 0.0);
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }

    #[test]
    fn bool_buffer_view_invoke() {
        let vmfb = compile_bool_not("llvm-cpu");
        let output = run_bool_not(&vmfb, "local-sync");
        assert_eq!(output, vec![false, true, true, false]);
    }

    #[test]
    fn metal_smoke() {
        if std::env::var("EERIE_TEST_METAL").as_deref() != Ok("1") {
            return;
        }

        let vmfb = compile_mul("metal-spirv");
        let output = run_mul(&vmfb, "metal");
        assert_eq!(output[0], 0.0);
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }
}
