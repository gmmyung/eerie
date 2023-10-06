use std::{path::{Path, PathBuf}, str::FromStr};

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

#[test]
fn append_module() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = compiler::Compiler::new().unwrap();
    let mut compiler_session = compiler.create_session();
    compiler_session
        .set_flags(vec!["--iree-hal-target-backends=metal".to_string()])?;
    let source = compiler_session
        .create_source_from_file(Path::new("tests/add.mlir"))?;
    let mut invocation = compiler_session.create_invocation();
    let mut output = compiler::MemBufferOutput::new(&compiler).unwrap();
    invocation
        .parse_source(source)?
        .set_verify_ir(true)
        .set_compile_to_phase("end")?
        .pipeline(compiler::Pipeline::Std)?
        .output_vm_byte_code(&mut output)?;
    let vmfb = output.map_memory()?;

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

    debug!("vmfb size: {}", vmfb.len());
    debug!("vmfb ptr: {:?}", vmfb.as_ptr());

    unsafe { session.append_module_from_memory(vmfb) }.unwrap();

    //unsafe { session.append_module_from_file(&PathBuf::from_str("tests/add.vmfb").unwrap())}.unwrap();
    
    debug!("vmfb size: {}", vmfb.len());

    Ok(())
}
