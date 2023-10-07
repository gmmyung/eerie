use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Result;
use iree_rs::{compiler, runtime};
use rusty_fork::rusty_fork_test;
use test_log::test;
use tracing::debug;

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
        .try_create_default_device("metal")
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

rusty_fork_test! {
    #[test]
    fn append_module() {
        let compiler = compiler::Compiler::new().unwrap();
        let mut compiler_session = compiler.create_session();
        compiler_session
            .set_flags(vec!["--iree-hal-target-backends=metal".to_string()]).unwrap();
        let source = compiler_session
            .create_source_from_file(Path::new("tests/add.mlir")).unwrap();
        let mut invocation = compiler_session.create_invocation();
        let mut output = compiler::MemBufferOutput::new(&compiler).unwrap();
        invocation
            .parse_source(source).unwrap()
            .set_verify_ir(true)
            .set_compile_to_phase("end").unwrap()
            .pipeline(compiler::Pipeline::Std).unwrap()
            .output_vm_byte_code(&mut output).unwrap();
        let vmfb = output.map_memory().unwrap();

        let mut driver_registry = runtime::hal::DriverRegistry::new();
        let options =
            runtime::api::InstanceOptions::new(&mut driver_registry).use_all_available_drivers();
        let instance = runtime::api::Instance::new(&options).unwrap();
        let device = instance
            .try_create_default_device("metal")
            .expect("Failed to create device");
        let session = runtime::api::Session::create_with_device(
            &instance,
            &runtime::api::SessionOptions::default(),
            &device,
        ).unwrap();

        unsafe { session.append_module_from_memory(vmfb) }.unwrap();

        let func = session.lookup_function("arithmetic.simple_add").unwrap();
        debug!("Function found!");

        let call = runtime::api::Call::new(&session, &func).unwrap();
        //let call = runtime::api::Call::from_func_name(&session, "arithmetic.simple_add").unwrap();
    }
}
