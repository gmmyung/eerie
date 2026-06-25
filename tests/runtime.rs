#![cfg(feature = "runtime")]

use eerie::runtime::{BufferView, Driver, Runtime, RuntimeError, Value};
#[cfg(feature = "half")]
use half::f16;
use test_log::test;

#[test]
fn buffer_view_metadata_and_readback() {
    let runtime = Runtime::new(Driver::LocalSync).unwrap();
    let buffer = runtime
        .buffer_view(&[2, 2], &[1.0_f32, 2.0, 3.0, 4.0])
        .unwrap();

    assert_eq!(buffer.shape(), vec![2, 2]);
    assert_eq!(buffer.read().unwrap(), vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn scalar_buffer_view_has_empty_shape() {
    let runtime = Runtime::new(Driver::LocalSync).unwrap();
    let buffer = runtime.buffer_view(&[], &[42.0_f32]).unwrap();

    assert_eq!(buffer.shape(), Vec::<usize>::new());
    assert_eq!(buffer.read().unwrap(), vec![42.0]);
}

#[test]
fn buffer_shape_mismatch_is_rejected() {
    let runtime = Runtime::new(Driver::LocalSync).unwrap();
    let err = runtime
        .buffer_view(&[2, 3], &[1.0_f32, 2.0, 3.0, 4.0])
        .unwrap_err();

    assert!(matches!(err, RuntimeError::InvalidArgument(_)));
}

#[cfg(feature = "half")]
#[test]
fn fp16_buffer_roundtrip() {
    let runtime = Runtime::new(Driver::LocalSync).unwrap();
    let input = [
        f16::from_f32(1.0),
        f16::from_f32(2.0),
        f16::from_f32(3.0),
        f16::from_f32(4.0),
    ];
    let buffer = runtime.buffer_view(&[2, 2], &input).unwrap();

    assert_eq!(buffer.read().unwrap(), input);
}

#[test]
fn bool_buffer_roundtrip() {
    let runtime = Runtime::new(Driver::LocalSync).unwrap();
    let buffer = runtime
        .buffer_view(&[2, 2], &[true, false, true, false])
        .unwrap();

    assert_eq!(buffer.read().unwrap(), vec![true, false, true, false]);
}

#[test]
fn fixture_vmfb_invokes_buffer_views() {
    let output = run_mul(include_bytes!("mul_vmvx.vmfb"), Driver::LocalSync);
    assert_eq!(output[0], 0.0);
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn parallel_program_invocations() {
    let vmfb = std::sync::Arc::new(include_bytes!("mul_vmvx.vmfb").to_vec());
    let thread_count = std::env::var("EERIE_STRESS_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16);
    let iterations = std::env::var("EERIE_STRESS_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
    let mut threads = Vec::new();

    for _ in 0..thread_count {
        let vmfb = vmfb.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let runtime = Runtime::new(Driver::LocalSync).unwrap();
            let program = runtime.load_vmfb(&vmfb).unwrap();
            let function = program.function("arithmetic.simple_mul").unwrap();
            let input_data = Vec::from_iter((0..100).map(|i| i as f32));
            let lhs = runtime.buffer_view(&[100], input_data.as_slice()).unwrap();
            let rhs = runtime.buffer_view(&[100], input_data.as_slice()).unwrap();

            barrier.wait();
            for _ in 0..iterations {
                let output = take_output::<f32>(function.invoke([&lhs, &rhs]).unwrap());
                let output = output.read().unwrap();
                assert_eq!(output[0], 0.0);
                assert_eq!(output[7], 49.0);
                assert_eq!(output[99], 9801.0);
            }
        }));
    }

    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn parallel_runtime_stack_churn() {
    let vmfb = std::sync::Arc::new(include_bytes!("mul_vmvx.vmfb").to_vec());
    let thread_count = std::env::var("EERIE_CHURN_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16);
    let iterations = std::env::var("EERIE_CHURN_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
    let mut threads = Vec::new();

    for _ in 0..thread_count {
        let vmfb = vmfb.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..iterations {
                let output = run_mul(&vmfb, Driver::LocalSync);
                assert_eq!(output[0], 0.0);
                assert_eq!(output[7], 49.0);
                assert_eq!(output[99], 9801.0);
            }
        }));
    }

    for thread in threads {
        thread.join().unwrap();
    }
}

fn run_mul(vmfb: &[u8], driver: Driver) -> Vec<f32> {
    let runtime = Runtime::new(driver).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();
    let input_data = Vec::from_iter((0..100).map(|i| i as f32));
    let lhs = runtime.buffer_view(&[100], input_data.as_slice()).unwrap();
    let rhs = runtime.buffer_view(&[100], input_data.as_slice()).unwrap();
    let function = program.function("arithmetic.simple_mul").unwrap();
    let output = take_output::<f32>(function.invoke([&lhs, &rhs]).unwrap());
    output.read().unwrap()
}

#[cfg(feature = "compiler")]
fn run_bool_not(vmfb: &[u8], driver: Driver) -> Vec<bool> {
    let runtime = Runtime::new(driver).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();
    let input = runtime
        .buffer_view(&[4], &[true, false, false, true])
        .unwrap();
    let function = program.function("bools.logical_not").unwrap();
    let output = take_output::<bool>(function.invoke([&input]).unwrap());
    output.read().unwrap()
}

#[cfg(feature = "compiler")]
fn run_is_positive(vmfb: &[u8], driver: Driver) -> Vec<bool> {
    let runtime = Runtime::new(driver).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();
    let input = runtime
        .buffer_view(&[4], &[-1.0_f32, 0.0, 2.0, 3.5])
        .unwrap();
    let function = program.function("mixed.is_positive").unwrap();
    let output = take_output::<bool>(function.invoke([&input]).unwrap());
    output.read().unwrap()
}

#[cfg(feature = "compiler")]
fn run_select_masked(vmfb: &[u8], driver: Driver) -> Vec<f32> {
    let runtime = Runtime::new(driver).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();
    let values = runtime
        .buffer_view(&[4], &[1.0_f32, 2.0, 3.0, 4.0])
        .unwrap();
    let mask = runtime
        .buffer_view(&[4], &[true, false, true, false])
        .unwrap();
    let function = program.function("mixed.select_masked").unwrap();
    let output = take_output::<f32>(
        function
            .invoke(vec![Value::from(&values), Value::from(&mask)])
            .unwrap(),
    );
    output.read().unwrap()
}

fn take_output<T>(mut outputs: Vec<Value>) -> BufferView<T>
where
    T: eerie::runtime::BufferElement,
    BufferView<T>: TryFrom<Value, Error = RuntimeError>,
{
    assert_eq!(outputs.len(), 1);
    outputs.remove(0).try_into().unwrap()
}

#[cfg(feature = "compiler")]
mod integration_tests {
    use eerie::compiler;
    use eerie::runtime::Driver;
    use log::info;
    use std::path::Path;
    use std::sync::Mutex;

    use super::{run_bool_not, run_is_positive, run_mul, run_select_masked};

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

    fn compile_mixed(target_backend: &str) -> Vec<u8> {
        compile_mlir(target_backend, Path::new("tests/mixed.mlir"))
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
        let output = run_mul(&vmfb, Driver::LocalSync);
        info!("Output: {:?}", output);
        assert_eq!(output[0], 0.0);
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }

    #[test]
    fn bool_buffer_view_invoke() {
        let vmfb = compile_bool_not("llvm-cpu");
        let output = run_bool_not(&vmfb, Driver::LocalSync);
        assert_eq!(output, vec![false, true, true, false]);
    }

    #[test]
    fn mixed_dtype_invoke() {
        let vmfb = compile_mixed("llvm-cpu");
        let output = run_is_positive(&vmfb, Driver::LocalSync);
        assert_eq!(output, vec![false, false, true, true]);
    }

    #[test]
    fn mixed_input_dtype_invoke() {
        let vmfb = compile_mixed("llvm-cpu");
        let output = run_select_masked(&vmfb, Driver::LocalSync);
        assert_eq!(output, vec![1.0, 0.0, 3.0, 0.0]);
    }

    #[test]
    fn metal_smoke() {
        if std::env::var("EERIE_TEST_METAL").as_deref() != Ok("1") {
            return;
        }

        #[cfg(target_os = "macos")]
        {
            let vmfb = compile_mul("metal-spirv");
            let output = run_mul(&vmfb, Driver::Metal);
            assert_eq!(output[0], 0.0);
            assert_eq!(output[7], 49.0);
            assert_eq!(output[99], 9801.0);
        }
    }
}
