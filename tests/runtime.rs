#![cfg(feature = "runtime")]
use eerie::runtime::{
    self,
    hal::BufferView,
    vm::{List, ToRef, ToValue, Value},
};
use log::{debug, info};
use test_log::test;

#[test]
fn test_instance() {
    let mut driver_registry = runtime::hal::DriverRegistry::new();
    debug!("DriverRegistry created");
    let options =
        runtime::api::InstanceOptions::new(&mut driver_registry).use_all_available_drivers();
    debug!("InstanceOptions created");
    runtime::api::Instance::new(&options).unwrap();
}

#[test]
fn test_session() {
    let mut driver_registry = runtime::hal::DriverRegistry::new();
    debug!("DriverRegistry created");
    let options =
        runtime::api::InstanceOptions::new(&mut driver_registry).use_all_available_drivers();
    debug!("InstanceOptions created");
    let instance = runtime::api::Instance::new(&options).unwrap();
    debug!("Instance created");
    let device = instance
        .try_create_default_device("local-sync")
        .expect("Failed to create device");
    debug!("Device created");
    let session = runtime::api::Session::create_with_device(
        &instance,
        &runtime::api::SessionOptions::default(),
        &device,
    )
    .expect("Failed to create session");
    session.trim().expect("Failed to trim session");
}

#[test]
fn dynamic_list() {
    let instance = runtime::api::Instance::new(
        &runtime::api::InstanceOptions::new(&mut runtime::hal::DriverRegistry::new())
            .use_all_available_drivers(),
    )
    .unwrap();
    let list = runtime::vm::DynamicList::<Value<i32>>::new(4, &instance).unwrap();
    list.push_value(1.to_value()).unwrap();
    list.push_value(2.to_value()).unwrap();
    list.push_value(3.to_value()).unwrap();
    list.push_value(4.to_value()).unwrap();
    let val = list.get_value::<i32>(0).unwrap();
    drop(list);
    assert_eq!(val.from_value(), 1);
}

#[test]
fn ref_list() {
    let instance = runtime::api::Instance::new(
        &runtime::api::InstanceOptions::new(&mut runtime::hal::DriverRegistry::new())
            .use_all_available_drivers(),
    )
    .unwrap();
    let device = instance
        .try_create_default_device("local-sync")
        .expect("Failed to create device");
    let session = runtime::api::Session::create_with_device(
        &instance,
        &runtime::api::SessionOptions::default(),
        &device,
    )
    .unwrap();
    let list =
        runtime::vm::DynamicList::<runtime::vm::Ref<BufferView<f32>>>::new(4, &instance).unwrap();
    let buffer = BufferView::<f32>::new(
        &session,
        &[2, 2],
        runtime::hal::EncodingType::DenseRowMajor,
        &[1.0, 2.0, 3.0, 4.0],
    )
    .unwrap();
    info!("buffer: {:?}", buffer);
    let buffer_ref = buffer.to_ref(&instance).unwrap();
    list.push_ref(&buffer_ref).unwrap();
    list.push_ref(&buffer_ref).unwrap();
    let buffer_ref_2: runtime::vm::Ref<BufferView<f32>> = list.get_ref(0).unwrap();
    info!("buffer_ref_2: {:?}", buffer_ref_2.to_buffer_view(&session));

    let mapping = runtime::hal::BufferMapping::new(buffer_ref_2.to_buffer_view(&session)).unwrap();
    info!("mapping: {:?}", mapping.data());
}

#[cfg(feature = "compiler")]
mod integration_tests {
    use eerie::compiler;
    use eerie::runtime;
    use eerie::runtime::hal::{BufferMapping, BufferView, EncodingType};
    use eerie::runtime::vm::{List, ToRef};
    use log::{debug, info};
    use rusty_fork::rusty_fork_test;
    use std::path::Path;
    rusty_fork_test! {
    #[test]
    fn append_module() {
        let compiler = compiler::Compiler::new().unwrap();
        let mut compiler_session = compiler.create_session();
        compiler_session
            .set_flags(vec!["--iree-hal-target-backends=llvm-cpu".to_string()])
            .unwrap();
        let source = compiler_session
            .create_source_from_file(Path::new("tests/mul.mlir"))
            .unwrap();
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
        let vmfb = output.map_memory().unwrap();

        let mut driver_registry = runtime::hal::DriverRegistry::new();
        let options =
            runtime::api::InstanceOptions::new(&mut driver_registry).use_all_available_drivers();
        let instance = runtime::api::Instance::new(&options).unwrap();
        let device = instance
            .try_create_default_device("local-sync")
            .expect("Failed to create device");
        let session = runtime::api::Session::create_with_device(
            &instance,
            &runtime::api::SessionOptions::default(),
            &device,
        )
        .unwrap();

        unsafe { session.append_module_from_memory(vmfb) }.unwrap();

        let func = session.lookup_function("arithmetic.simple_mul").unwrap();
        debug!("Function found!");

        let mut call = runtime::api::Call::new(&session, &func).unwrap();
        let vec = Vec::from_iter((0..100).map(|i| i as f32));

        let input = BufferView::<f32>::new(
            &session,
            &[100],
            EncodingType::DenseRowMajor,
            vec.as_slice(),
        )
        .unwrap();

        info!("Input: {:?}", input);

        call.inputs_push_back_buffer_view(&input).unwrap();
        call.inputs_push_back_buffer_view(&input).unwrap();

        call.invoke().unwrap();

        let output = call.outputs_pop_front_buffer_view::<f32>().unwrap();

        info!("Output: {:?}", output);

        let input = BufferView::<f32>::new(
            &session,
            &[100],
            EncodingType::DenseRowMajor,
            vec.as_slice(),
        ).unwrap();
        let input_list =
            runtime::vm::DynamicList::<runtime::vm::Ref<BufferView<f32>>>::new(1, &instance).unwrap();
        input_list
            .push_ref(&input.to_ref(&instance).unwrap())
            .unwrap();
        input_list
            .push_ref(&input.to_ref(&instance).unwrap())
            .unwrap();
        let output_list = runtime::vm::DynamicList::<runtime::vm::Ref<BufferView<f32>>>::new(0, &instance).unwrap();
        let function = session.lookup_function("arithmetic.simple_mul").unwrap();
        function.invoke(&input_list, &output_list).unwrap();
        let output = output_list.get_ref(0).unwrap();
        let output_mapping: BufferMapping<f32> = runtime::hal::BufferMapping::new(output.to_buffer_view(&session)).unwrap();
        info!("Output: {:?}", output_mapping.data());
    }
    }
}
