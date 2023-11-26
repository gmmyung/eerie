use eerie::compiler::*;
use rusty_fork::rusty_fork_test;
use std::path::Path;
use test_log::test;
use tracing::{debug, info};

// forking is necessary to avoid the compiler being initialized multiple times in the same process
rusty_fork_test! {
    #[test]
    fn test_compiler() {
        let compiler = Compiler::new();
        assert!(compiler.is_ok());
        let compiler2 = Compiler::new();
        assert!(compiler2.is_err());
    }

    #[test]
    fn test_get_api_version() {
        let (major, minor) = get_api_version();
        info!("API Version: {}.{}", major, minor);
        assert!(major == 1);
        assert!(minor == 4);
    }

    #[test]
    fn get_revision() {
        let rev = Compiler::new()
            .unwrap()
            .get_revision()
            .unwrap();
        debug!("Revision: \"{}\"", rev);
    }

    #[test]
    fn setup_global_cl() {
        Compiler::new()
            .unwrap()
            .setup_global_cl(
                vec!["--iree-example-flag=false".to_string()])
            .unwrap();
    }

    #[test]
    fn get_registered_hal_target_backends() {
        let backends = Compiler::new()
            .unwrap()
            .get_registered_hal_target_backends();
        info!("Input Backends: {:?}", backends);
    }

    #[test]
    fn get_plugins() {
        let plugins = Compiler::new()
            .unwrap()
            .get_plugins();
        info!("Plugins: {:?}", plugins);
    }

    #[test]
    fn test_session() {
        Compiler::new()
            .unwrap()
            .create_session();
    }

    #[test]
    fn session_set_get_flags() {
        let flags = Compiler::new()
            .unwrap()
            .create_session()
            //.set_flags(vec!["--iree-input-type=tosa".to_string()])
            //.unwrap()
            .get_flags(false);
        info!("Flags: {:?}", flags);
    }

    #[test]
    fn init_invocation() {
        Compiler::new()
            .unwrap()
            .create_session()
            .create_invocation();
    }

    #[test]
    fn source_from_file() {
        let compiler = Compiler::new().unwrap();
        let session = compiler.create_session();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = Source::from_file(&session, Path::new("tests/mul.mlir")).unwrap();
        invocation.parse_source(source).unwrap();
    }

    #[test]
    fn source_from_cstr() {
        let source_ir = r#"
        module @arithmetic {
            func.func @simple_add(%arg0: tensor<4xf32>, %arg1: tensor<4xf32>) -> tensor<4xf32> {
                %0 = arith.addf %arg0, %arg1 : tensor<4xf32>
                return %0 : tensor<4xf32>
            }
        }"#;
        let source_ir_cstr = std::ffi::CString::new(source_ir).unwrap();
        let compiler = Compiler::new().unwrap();
        let session = compiler.create_session();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = session.create_source_from_cstr(&source_ir_cstr).unwrap();
        invocation.parse_source(source).unwrap();

    }

    #[test]
    fn source_from_invalid_cstr() {
        let source_ir = r#"
        module @arithmetic {
            func.func @simple_add(%arg0: tensor<4xf32>, %arg1: tensor<4xf32>) -> tensor<4xf32> {
                %0 = arith.addf %arg0, %arg1 : tensor<4xf32>
                return %0 : tensor<4xf32>
                INVALID!!!!
            }
        }"#;
        let source_ir_cstr = std::ffi::CString::new(source_ir).unwrap();
        let compiler = Compiler::new().unwrap();
        let session = compiler.create_session();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = session.create_source_from_cstr(&source_ir_cstr).unwrap();
        assert!(invocation.parse_source(source).is_err());
    }

    #[test]
    fn output_byte_code() {
        let mut compiler = Compiler::new().unwrap();
        compiler.setup_global_cl(vec!["--iree-hal-target-backends=llvm-cpu".to_string()]).unwrap();   
        let mut session = compiler.create_session();
        session.set_flags(vec!["--iree-hal-target-backends=llvm-cpu".to_string()]).unwrap();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = Source::from_file(&session, Path::new("tests/mul.mlir")).unwrap();
        let mut output = MemBufferOutput::new(&compiler).unwrap();
        invocation.set_compile_to_phase("end").unwrap();
        invocation.parse_source(source).unwrap();
        invocation.pipeline(Pipeline::Std).unwrap();
        invocation.output_vm_byte_code(&mut output).unwrap();
        let out_buf = output.map_memory().unwrap();
        info!("Output: {}", unsafe{std::ffi::CStr::from_ptr(out_buf.as_ptr() as *const i8)}.to_str().unwrap());
    }
}
