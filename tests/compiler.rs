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
    fn get_process_cl_args() {
        init_compiler();
        let args = COMPILER
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get_process_cl_args();
        assert!(!args.is_empty());
        assert_eq!(
            args[0],
            std::env::args_os().next().unwrap().to_string_lossy()
        );
        assert_eq!(
            args,
            COMPILER
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .get_process_cl_args()
        );
        info!("Process CL args: {:?}", args);
    }

    #[test]
    fn setup_global_cl() {
        init_compiler();
        let mut compiler = COMPILER.lock().unwrap();
        let compiler = compiler.as_mut().unwrap();
        assert!(matches!(
            compiler.setup_global_cl(vec!["invalid\0argument".into()]),
            Err(CompilerError::NulError(_))
        ));
        compiler
            .setup_global_cl(vec!["eerie-tests".into()])
            .unwrap();
        assert!(matches!(
            compiler.setup_global_cl(vec!["eerie-tests".into()]),
            Err(CompilerError::GlobalCLAlreadySet)
        ));
    }

    #[test]
    fn compile_owned_vmfb_from_file_text_and_bytes() {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let compiler = compiler.as_ref().unwrap();
        let flags = ["--iree-hal-target-backends=vmvx"];
        assert!(!compiler
            .compile_file("tests/mul.mlir", flags)
            .unwrap()
            .is_empty());
        assert!(!compiler
            .compile_str(include_str!("mul.mlir"), flags)
            .unwrap()
            .is_empty());
        assert!(!compiler
            .compile_bytes(include_bytes!("mul.mlir"), flags)
            .unwrap()
            .is_empty());
        assert!(compiler.compile_str("invalid MLIR", flags).is_err());
        assert!(compiler
            .compile_str(include_str!("mul.mlir"), ["--invalid-eerie-flag"])
            .is_err());
        assert!(matches!(
            compiler.compile_file("tests/missing.mlir", flags),
            Err(CompilerError::FileNotFound(_))
        ));
        // Failed compilations and session flags must not affect later calls.
        compiler
            .compile_str(include_str!("mul.mlir"), flags)
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
    fn invocation_debug_outputs() {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let session = compiler.as_ref().unwrap().create_session();
        let mut invocation = session.create_invocation();
        let directory =
            std::env::temp_dir().join(format!("eerie-diagnostics-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        invocation
            .set_dump_compilation_phases_to(&directory.join("phases"))
            .unwrap()
            .setup_remarks(".*", &directory.join("remarks.yaml"))
            .unwrap()
            .set_crash_reproducer(&directory.join("crash-reproducer.mlir"))
            .unwrap();
        drop(invocation);
        std::fs::remove_dir_all(directory).unwrap();
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

    #[test]
    fn output_write() {
        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let mut output = MemBufferOutput::new(compiler.as_ref().unwrap()).unwrap();
        output.write(b"abc").unwrap();
        assert_eq!(output.map_memory().unwrap(), b"abc");
    }

    #[test]
    fn native_file_outputs_preserve_the_callers_file() {
        use std::io::{Read, Seek, SeekFrom, Write};

        init_compiler();
        let compiler = COMPILER.lock().unwrap();
        let compiler = compiler.as_ref().unwrap();
        let directory = std::env::temp_dir().join(format!("eerie-files-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("output-Δ-😀.bin");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        {
            let mut output = FileOutput::from_file(compiler, &mut file).unwrap();
            output.write(b"abc").unwrap();
        }
        file.write_all(b"def").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"abcdef");
        drop(file);
        {
            let mut output = FileNameOutput::new(compiler, &path).unwrap();
            output.write(b"named output").unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"named output");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
