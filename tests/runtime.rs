#![cfg(feature = "runtime")]

use eerie::runtime::{BufferView, DeviceSpec, Driver, Runtime, RuntimeError, Value};
#[cfg(feature = "half")]
use half::f16;
use test_log::test;

#[test]
fn buffer_view_metadata_and_readback() {
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    let buffer = runtime
        .buffer_view(&[2, 2], &[1.0_f32, 2.0, 3.0, 4.0])
        .unwrap();

    assert_eq!(buffer.shape(), vec![2, 2]);
    assert_eq!(buffer.rank(), 2);
    assert_eq!(buffer.element_count(), 4);
    assert_eq!(buffer.byte_length(), 16);
    assert_eq!(buffer.read().unwrap(), vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn scalar_buffer_view_has_empty_shape() {
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    let buffer = runtime.buffer_view(&[], &[42.0_f32]).unwrap();

    assert_eq!(buffer.shape(), Vec::<usize>::new());
    assert_eq!(buffer.rank(), 0);
    assert_eq!(buffer.element_count(), 1);
    assert_eq!(buffer.read().unwrap(), vec![42.0]);
}

#[test]
fn read_into_reuses_host_storage_and_checks_lengths() {
    for spec in [DeviceSpec::local_sync(), DeviceSpec::local_inline()] {
        let runtime = Runtime::new(spec).unwrap();
        let buffer = runtime.buffer_view(&[2], &[5_i64, i64::MAX]).unwrap();
        let mut destination = [0; 2];
        for _ in 0..3 {
            buffer.read_into(&mut destination).unwrap();
            assert_eq!(destination, [5, i64::MAX]);
        }
        let mut wrong_length = [7; 3];
        assert!(buffer.read_into(&mut wrong_length).is_err());
        assert_eq!(wrong_length, [7; 3]);
        let bools = runtime.buffer_view(&[2], &[true, false]).unwrap();
        let mut destination = [false; 2];
        bools.read_into(&mut destination).unwrap();
        assert_eq!(destination, [true, false]);
        let empty = runtime.buffer_view(&[0, 2], &[] as &[f32]).unwrap();
        assert_eq!(empty.element_count(), 0);
        empty.read_into(&mut []).unwrap();
    }
}

#[test]
fn buffer_shape_mismatch_is_rejected() {
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    let err = runtime
        .buffer_view(&[2, 3], &[1.0_f32, 2.0, 3.0, 4.0])
        .unwrap_err();

    assert!(matches!(err, RuntimeError::InvalidArgument(_)));
}

#[cfg(feature = "half")]
#[test]
fn fp16_buffer_roundtrip() {
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
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
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    let buffer = runtime
        .buffer_view(&[2, 2], &[true, false, true, false])
        .unwrap();

    assert_eq!(buffer.read().unwrap(), vec![true, false, true, false]);
}

#[test]
fn available_devices_can_create_runtime() {
    let devices = Runtime::available_devices(Driver::local_sync()).unwrap();
    assert!(!devices.is_empty());
    assert_eq!(devices[0].driver().as_str(), "local-sync");
    assert_eq!(devices[0].name(), "default");

    let runtime = Runtime::new(devices[0].spec()).unwrap();
    let buffer = runtime.buffer_view(&[2], &[1.0_f32, 2.0]).unwrap();
    assert_eq!(buffer.read().unwrap(), vec![1.0, 2.0]);
}

#[test]
fn fixture_vmfb_invokes_buffer_views() {
    let output = run_mul(include_bytes!("mul_vmvx.vmfb"), DeviceSpec::local_sync());
    assert_eq!(output[0], 0.0);
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn inline_fixture_invokes_buffer_views() {
    let output = run_mul(
        include_bytes!("mul_vmvx_inline.vmfb"),
        DeviceSpec::local_inline(),
    );
    assert_eq!(output[7], 49.0);
    assert_eq!(output[99], 9801.0);
}

#[test]
fn internal_i64_calls_preserve_high_words() {
    let runtime = Runtime::new(DeviceSpec::local_inline()).unwrap();
    let program = runtime.load_vmfb(include_bytes!("wide.vmfb")).unwrap();
    let function = program.function("wide.check").unwrap();
    assert!(matches!(
        function.invoke_one::<f32>(core::iter::empty::<Value>()),
        Err(RuntimeError::InvalidArgument(_))
    ));
    assert!(function
        .invoke(core::iter::empty::<Value>())
        .unwrap()
        .is_empty());
}

#[test]
fn device_selectors_and_creation_failures() {
    for spec in [
        DeviceSpec::local_sync().ordinal(0),
        DeviceSpec::local_sync().path(""),
    ] {
        let runtime = Runtime::new(spec).unwrap();
        assert_eq!(
            runtime.buffer_view(&[1], &[7_i64]).unwrap().read().unwrap(),
            vec![7]
        );
    }
    assert!(Runtime::new(DeviceSpec::local_sync().ordinal(usize::MAX)).is_err());
    assert!(Runtime::new(DeviceSpec::local_sync().path("missing")).is_err());
    assert!(Runtime::new(DeviceSpec::custom("missing-driver")).is_err());
    assert!(Runtime::new(DeviceSpec::local_inline().ordinal(1)).is_err());
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    assert_eq!(
        runtime.buffer_view(&[1], &[9_i64]).unwrap().read().unwrap(),
        vec![9]
    );
    let devices = Runtime::available_devices(Driver::local_inline()).unwrap();
    assert_eq!(devices.len(), 1);
    Runtime::new(devices[0].spec()).unwrap();
}

#[test]
fn multiple_programs_and_retained_objects() {
    for (spec, bytes) in [
        (
            DeviceSpec::local_sync(),
            include_bytes!("mul_vmvx.vmfb").as_slice(),
        ),
        (
            DeviceSpec::local_task(),
            include_bytes!("mul_vmvx.vmfb").as_slice(),
        ),
        (
            DeviceSpec::local_inline(),
            include_bytes!("mul_vmvx_inline.vmfb").as_slice(),
        ),
    ] {
        let runtime = Runtime::new(spec).unwrap();
        let first = runtime.load_vmfb(bytes).unwrap();
        let second = runtime.load_vmfb(bytes).unwrap();
        let a = first.function("arithmetic.simple_mul").unwrap();
        let b = second.function("arithmetic.simple_mul").unwrap();
        let input = runtime.buffer_view(&[100], &[3_f32; 100]).unwrap();
        drop(runtime);
        drop(first);
        drop(second);
        let output: BufferView<f32> = a.invoke_one([&input, &input]).unwrap();
        assert_eq!(output.read().unwrap(), vec![9.0; 100]);
        drop(a);
        assert!(matches!(
            b.invoke_one::<i32>([&input, &input]),
            Err(RuntimeError::InvalidArgument(_))
        ));
        let output = take_output::<f32>(b.invoke([&input, &input]).unwrap());
        drop(b);
        drop(input);
        assert_eq!(output.read().unwrap(), vec![9.0; 100]);
    }
}

#[test]
fn failed_load_and_invoke_leave_runtime_usable() {
    for (spec, bytes) in [
        (
            DeviceSpec::local_sync(),
            include_bytes!("mul_vmvx.vmfb").as_slice(),
        ),
        (
            DeviceSpec::local_inline(),
            include_bytes!("mul_vmvx_inline.vmfb").as_slice(),
        ),
    ] {
        let runtime = Runtime::new(spec).unwrap();
        assert!(runtime.load_vmfb(b"invalid archive").is_err());
        let program = runtime.load_vmfb(bytes).unwrap();
        let function = program.function("arithmetic.simple_mul").unwrap();
        let input = runtime.buffer_view(&[100], &[2_f32; 100]).unwrap();
        assert!(function.invoke([&input]).is_err());
        let output = take_output::<f32>(function.invoke([&input, &input]).unwrap());
        assert_eq!(output.read().unwrap(), vec![4.0; 100]);
    }
}

#[test]
fn overflowing_and_empty_shapes() {
    for spec in [DeviceSpec::local_sync(), DeviceSpec::local_inline()] {
        let runtime = Runtime::new(spec).unwrap();
        assert!(runtime.buffer_view(&[usize::MAX, 2], &[1_u8]).is_err());
        let empty = runtime
            .buffer_view(&[usize::MAX, 2, 0], &[] as &[u8])
            .unwrap();
        assert_eq!(empty.read().unwrap(), vec![]);
    }
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
            let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
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
                let output = run_mul(&vmfb, DeviceSpec::local_sync());
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

fn run_mul(vmfb: &[u8], spec: DeviceSpec) -> Vec<f32> {
    let runtime = Runtime::new(spec).unwrap();
    let program = runtime.load_vmfb(vmfb).unwrap();
    let input_data = Vec::from_iter((0..100).map(|i| i as f32));
    let lhs = runtime.buffer_view(&[100], input_data.as_slice()).unwrap();
    let rhs = runtime.buffer_view(&[100], input_data.as_slice()).unwrap();
    let function = program.function("arithmetic.simple_mul").unwrap();
    let output = take_output::<f32>(function.invoke([&lhs, &rhs]).unwrap());
    output.read().unwrap()
}

#[cfg(feature = "compiler")]
fn run_bool_not(vmfb: &[u8], driver: DeviceSpec) -> Vec<bool> {
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
fn run_is_positive(vmfb: &[u8], driver: DeviceSpec) -> Vec<bool> {
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
fn run_select_masked(vmfb: &[u8], driver: DeviceSpec) -> Vec<f32> {
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
    use eerie::runtime::DeviceSpec;
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
        compile_mlir_with_flags(target_backend, path, &[])
    }

    fn compile_mlir_with_flags(target_backend: &str, path: &Path, extra: &[&str]) -> Vec<u8> {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let mut flags = vec![format!("--iree-hal-target-backends={target_backend}")];
        flags.extend(extra.iter().map(|flag| flag.to_string()));
        if target_backend == "metal-spirv" {
            flags.push("--iree-metal-compile-to-metallib=false".to_string());
        }
        compiler
            .as_ref()
            .unwrap()
            .compile_file(path, flags)
            .unwrap()
    }

    #[test]
    fn append_module() {
        let vmfb = compile_mul("llvm-cpu");
        let output = run_mul(&vmfb, DeviceSpec::local_sync());
        info!("Output: {:?}", output);
        assert_eq!(output[0], 0.0);
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }

    #[test]
    fn inline_dynamic_llvm_cpu() {
        let vmfb = compile_mlir_with_flags(
            "llvm-cpu",
            Path::new("tests/mul.mlir"),
            &["--iree-execution-model=inline-dynamic"],
        );
        let output = run_mul(&vmfb, DeviceSpec::local_inline());
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }

    #[test]
    fn invoke_one_rejects_multiple_results() {
        use eerie::runtime::{BufferView, Runtime, RuntimeError};

        init_compiler();
        let vmfb = COMPILER.lock().unwrap().as_ref().unwrap().compile_str(
            "module @test { func.func @pair(%x: tensor<2xf32>) -> (tensor<2xf32>, tensor<2xf32>) { return %x, %x : tensor<2xf32>, tensor<2xf32> } }",
            ["--iree-hal-target-backends=vmvx-inline", "--iree-execution-model=inline-static"],
        ).unwrap();
        let runtime = Runtime::new(DeviceSpec::local_inline()).unwrap();
        let program = runtime.load_vmfb(&vmfb).unwrap();
        let function = program.function("test.pair").unwrap();
        let input = runtime.buffer_view(&[2], &[1_f32, 2.0]).unwrap();
        assert!(matches!(
            function.invoke_one::<f32>([&input]),
            Err(RuntimeError::InvalidArgument(_))
        ));
        let outputs = function.invoke([&input]).unwrap();
        assert_eq!(outputs.len(), 2);
        for output in outputs {
            let output: BufferView<f32> = output.try_into().unwrap();
            assert_eq!(output.read().unwrap(), [1.0, 2.0]);
        }
    }

    #[test]
    fn bool_buffer_view_invoke() {
        let vmfb = compile_bool_not("llvm-cpu");
        let output = run_bool_not(&vmfb, DeviceSpec::local_sync());
        assert_eq!(output, vec![false, true, true, false]);
    }

    #[test]
    fn mixed_dtype_invoke() {
        let vmfb = compile_mixed("llvm-cpu");
        let output = run_is_positive(&vmfb, DeviceSpec::local_sync());
        assert_eq!(output, vec![false, false, true, true]);
    }

    #[test]
    fn mixed_input_dtype_invoke() {
        let vmfb = compile_mixed("llvm-cpu");
        let output = run_select_masked(&vmfb, DeviceSpec::local_sync());
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
            let output = run_mul(&vmfb, DeviceSpec::metal());
            assert_eq!(output[0], 0.0);
            assert_eq!(output[7], 49.0);
            assert_eq!(output[99], 9801.0);
        }
    }

    #[cfg(feature = "cuda")]
    #[test]
    fn cuda_smoke() {
        if std::env::var("EERIE_TEST_CUDA").as_deref() != Ok("1") {
            return;
        }
        let chip =
            std::env::var("EERIE_CUDA_TARGET").expect("set EERIE_CUDA_TARGET for the test GPU");
        let flag = format!("--iree-cuda-target={chip}");
        let vmfb = compile_mlir_with_flags("cuda", Path::new("tests/mul.mlir"), &[&flag]);
        let output = run_mul(&vmfb, DeviceSpec::cuda());
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }

    #[cfg(feature = "vulkan")]
    #[test]
    fn vulkan_smoke() {
        if std::env::var("EERIE_TEST_VULKAN").as_deref() != Ok("1") {
            return;
        }
        let vmfb = compile_mul("vulkan-spirv");
        let output = run_mul(&vmfb, DeviceSpec::vulkan());
        assert_eq!(output[7], 49.0);
        assert_eq!(output[99], 9801.0);
    }
}
