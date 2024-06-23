// forking is necessary to avoid the compiler being initialized multiple times in the same process
// TODO: make a global compiler object so we don't need this
#![cfg(feature = "compiler")]
mod test {
    use eerie::compiler::*;
    use log::{debug, info};
    use std::path::Path;
    use std::sync::Mutex;
    use test_log::test;

    static COMPILER: Mutex<Option<Compiler>> = Mutex::new(None);

    fn init_compiler() {
        let mut global_compiler = COMPILER.lock().unwrap();
        if global_compiler.is_none() {
            let compiler = Compiler::new().unwrap();
            *global_compiler = Some(compiler);
        }
    }

    #[test]
    fn test_compiler() {
        init_compiler();
        let compiler = Compiler::new();
        assert!(compiler.is_err());
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
        init_compiler();
        let rev = COMPILER
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get_revision()
            .unwrap();
        debug!("Revision: \"{}\"", rev);
    }

    #[test]
    fn setup_global_cl() {
        init_compiler();
        COMPILER
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .setup_global_cl(vec!["--iree-example-flag=false".to_string()])
            .unwrap();
    }

    #[test]
    fn get_registered_hal_target_backends() {
        init_compiler();
        let backends = COMPILER
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get_registered_hal_target_backends();
        info!("Input Backends: {:?}", backends);
    }

    #[test]
    fn get_plugins() {
        init_compiler();
        let plugins = COMPILER.lock().unwrap().as_ref().unwrap().get_plugins();
        info!("Plugins: {:?}", plugins);
    }

    #[test]
    fn test_session() {
        init_compiler();
        COMPILER.lock().unwrap().as_ref().unwrap().create_session();
    }

    #[test]
    fn session_set_get_flags() {
        init_compiler();
        let flags = COMPILER
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .create_session()
            .set_flags(vec!["--iree-input-type=tosa".to_string()])
            .unwrap()
            .get_flags(true);
        info!("Flags: {:?}", flags);
    }

    #[test]
    fn init_invocation() {
        init_compiler();
        COMPILER
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .create_session()
            .create_invocation();
    }

    #[test]
    fn source_from_file() {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let session = compiler.as_ref().unwrap().create_session();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = Source::from_file(&session, Path::new("tests/mul.mlir")).unwrap();
        invocation.parse_source(source).unwrap();
    }

    #[test]
    fn source_from_cstr() {
        init_compiler();
        let source_ir = r#"
        module @arithmetic {
            func.func @simple_add(%arg0: tensor<4xf32>, %arg1: tensor<4xf32>) -> tensor<4xf32> {
                %0 = arith.addf %arg0, %arg1 : tensor<4xf32>
                return %0 : tensor<4xf32>
            }
        }"#;
        let source_ir_cstr = std::ffi::CString::new(source_ir).unwrap();
        let compiler = COMPILER.lock().unwrap();
        let session = compiler.as_ref().unwrap().create_session();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = session.create_source_from_cstr(&source_ir_cstr).unwrap();
        invocation.parse_source(source).unwrap();
    }

    #[test]
    fn source_from_invalid_cstr() {
        init_compiler();
        let source_ir = r#"
        module @arithmetic {
            func.func @simple_add(%arg0: tensor<4xf32>, %arg1: tensor<4xf32>) -> tensor<4xf32> {
                %0 = arith.addf %arg0, %arg1 : tensor<4xf32>
                return %0 : tensor<4xf32>
                INVALID!!!!
            }
        }"#;
        let source_ir_cstr = std::ffi::CString::new(source_ir).unwrap();
        let compiler = COMPILER.lock().unwrap();
        let session = compiler.as_ref().unwrap().create_session();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = session.create_source_from_cstr(&source_ir_cstr).unwrap();
        assert!(invocation.parse_source(source).is_err());
    }

    #[test]
    fn output_byte_code() {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let mut session = compiler.as_ref().unwrap().create_session();
        session
            .set_flags(vec!["--iree-hal-target-backends=llvm-cpu".to_string()])
            .unwrap();
        let mut invocation = session.create_invocation();
        invocation.set_verify_ir(true);
        let source = Source::from_file(&session, Path::new("tests/mul.mlir")).unwrap();
        let mut output = MemBufferOutput::new(compiler.as_ref().unwrap()).unwrap();
        invocation.set_compile_to_phase("end").unwrap();
        invocation.parse_source(source).unwrap();
        invocation.pipeline(Pipeline::Std).unwrap();
        invocation.output_vm_byte_code(&mut output).unwrap();
        let out_buf = output.map_memory().unwrap();
        info!("Output: {}", unsafe {
            std::ffi::CStr::from_ptr(out_buf.as_ptr() as *const core::ffi::c_char)
                .to_str()
                .unwrap()
        });
    }
}
