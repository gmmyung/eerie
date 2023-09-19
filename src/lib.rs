pub mod compiler {
    use std::sync::{OnceLock, Mutex, MutexGuard};
    use tracing::{debug, info, warn, error};
    use thiserror::Error;
    use iree_sys as sys;

    struct Error<'a> {
        ctx: *mut sys::iree_compiler_error_t,
        _marker: std::marker::PhantomData<&'a str>,
    }

    impl<'a> Drop for Error<'a> {
        fn drop(&mut self) {
            unsafe { sys::ireeCompilerErrorDestroy(self.ctx) }
        }
    }


    

    impl<'a> Error<'a> {
        fn message(&self) -> &'a str {
            unsafe {
                let c_str = std::ffi::CStr::from_ptr(sys::ireeCompilerErrorGetMessage(self.ctx));
                c_str.to_str().expect("Invalid UTF-8 string")
            }
        }
    }

    pub fn get_api_version() -> (u16, u16) {
        let version_bytes = unsafe {sys::ireeCompilerGetAPIVersion()} as u32;
        let major = (version_bytes >> 16) as u16;
        let minor = (version_bytes & 0xFFFF) as u16;
        (major, minor)
    }

    static IS_INITIALIZED: OnceLock<()> = OnceLock::new();

    static REGISTERED_HAL_TARGET_BACKENDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    static PLUGINS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    pub struct Compiler<'a> {
        _marker: std::marker::PhantomData<&'a str>,
    } 

    impl<'a> Compiler<'a> {
        pub fn new() -> Result<Self, CompilerError> {
            match IS_INITIALIZED.set(()) {
                Ok(_) => {
                    unsafe {
                        debug!("Global initializing compiler");
                        sys::ireeCompilerGlobalInitialize();
                    }
                    Ok(Compiler{_marker: std::marker::PhantomData})
                },
                Err(_) => Err(CompilerError::AlreadyInitialized), 
            }
        }

        pub fn get_revision(&self) -> &'a str {
            unsafe {
                let c_str = std::ffi::CStr::from_ptr(sys::ireeCompilerGetRevision());
                c_str.to_str().expect("Invalid UTF-8 string")
            }
        }

        pub fn setup_global_cl(&self, argv: Vec<String>) {
            let args = argv.iter()
                .map(|arg| std::ffi::CString::new(arg.as_str()).unwrap())
                .collect::<Vec<_>>();
            let mut c_args = args.iter()
                .map(|arg| arg.as_ptr())
                .collect::<Vec<_>>();
            let banner = std::ffi::CString::new("IREE Compiler").unwrap();
            unsafe {
                sys::ireeCompilerSetupGlobalCL(argv.len() as i32, c_args.as_mut_ptr(), banner.as_ptr(), false)
            }
            debug!("Global CL setup");
        }


        
        
        extern "C" fn capture_hal_target_backend_callback(backend: *const std::os::raw::c_char, user_data: *mut std::ffi::c_void) {
            debug!("Capturing HAL target backend");
            let mut hal_target_backend_list = REGISTERED_HAL_TARGET_BACKENDS.lock().unwrap();
            let backend_name = unsafe {
                std::ffi::CStr::from_ptr(backend)
            };
            hal_target_backend_list.push(backend_name.to_str().unwrap().to_string());
            debug!("Backend name: {}", backend_name.to_str().unwrap());
        }

        extern "C" fn capture_plugin_callback(plugin_name: *const std::os::raw::c_char, user_data: *mut std::ffi::c_void) {
            debug!("Capturing plugin");
            let mut plugin_list = PLUGINS.lock().unwrap();
            let plugin_name = unsafe {
                std::ffi::CStr::from_ptr(plugin_name)
            };
            plugin_list.push(plugin_name.to_str().unwrap().to_string());
            debug!("Plugin name: {}", plugin_name.to_str().unwrap());
        }
        
        pub fn get_registered_hal_target_backends(&self) -> Result<Vec<String>, CompilerError> {
            let user_data = std::ptr::null_mut();
            {
                let mut hal_target_backend_list = REGISTERED_HAL_TARGET_BACKENDS.lock()?;
                hal_target_backend_list.clear();
            }
            debug!("Enumerating registered HAL target backends");
            unsafe {
                sys::ireeCompilerEnumerateRegisteredHALTargetBackends(
                    Some(Self::capture_hal_target_backend_callback),
                    user_data);
            }

            let hal_target_backend_list = REGISTERED_HAL_TARGET_BACKENDS.lock()?;
            Ok(hal_target_backend_list.clone())
        }

        pub fn get_plugins(&self) -> Result<Vec<String>, CompilerError> {
            let user_data = std::ptr::null_mut();
            {
                let mut hal_target_backend_list = REGISTERED_HAL_TARGET_BACKENDS.lock()?;
                hal_target_backend_list.clear();
            }
            debug!("Enumerating registered HAL target backends");
            unsafe {
                sys::ireeCompilerEnumeratePlugins(
                    Some(Self::capture_plugin_callback),
                    user_data);
            }

            let hal_target_backend_list = REGISTERED_HAL_TARGET_BACKENDS.lock()?;
            Ok(hal_target_backend_list.clone())
        }
    }

    impl Drop for Compiler<'_> {
        fn drop(&mut self) {
            unsafe {
                debug!("Global shutting down compiler");
                sys::ireeCompilerGlobalShutdown();
            }
        }
    }

    #[derive(Error, Debug)]
    pub enum CompilerError {
        #[error("Compiler initialized more than once")]
        AlreadyInitialized,
        #[error("mutex: REGISTERED_HAL_TARGET_BACKENDS is poisoned")]
        PoisonedMutex(#[from] std::sync::PoisonError<MutexGuard<'static, Vec<String>>>),
    }



    #[cfg(test)]
    mod tests {
        use test_log::test;
        use tracing::{debug, info, warn, error};
        use std::sync::Mutex;
        use anyhow::Result;

        static COMPILER: Mutex<Option<super::Compiler>> = Mutex::new(None);

        fn init_compiler() {
            info!("Initializing compiler");
            match COMPILER.lock() {
                Ok(mut c) => {
                    if c.is_none() {
                        let compiler = super::Compiler::new();
                        match compiler {
                            Ok(compiler) => {
                                *c = Some(compiler);
                            },
                            Err(err) => {
                                error!("Failed to initialize compiler: {}", err);
                            }
                        }
                    }
                },
                Err(err) => {
                    error!("Failed to lock compiler: {}", err);
                }
            }
        }

        #[test]
        fn get_api_version() {
            let (major, minor) = super::get_api_version();
            debug!("API Version: {}.{}", major, minor);
        }

        #[test]
        fn get_revision() -> Result<()>{
            init_compiler();
            let global_compiler = COMPILER.lock().unwrap();
            let compiler = global_compiler.as_ref().unwrap();
            let revision = compiler.get_revision();
            debug!("Revision: {}", revision);

            let compiler1 = super::Compiler::new();
            let compiler2 = super::Compiler::new();
            assert!(compiler1.is_err());
            assert!(compiler2.is_err());

            Ok(())
        }

        #[test]
        fn setup_global_cl() {
            init_compiler();
            let global_compiler = COMPILER.lock().unwrap();
            let compiler = global_compiler.as_ref().unwrap();
            compiler.setup_global_cl(vec![String::from("testasdafa")]);
        }

        #[test]
        fn get_registered_hal_target_backends() {
            init_compiler();
            let global_compiler = COMPILER.lock().unwrap();
            let compiler = global_compiler.as_ref().unwrap();
            let hal_target_backends = compiler.get_registered_hal_target_backends();
            debug!("HAL Target Backends: {:?}", hal_target_backends);
        }

        #[test]
        fn get_plugins() {
            init_compiler();
            let global_compiler = COMPILER.lock().unwrap();
            let compiler = global_compiler.as_ref().unwrap();
            let plugins = compiler.get_plugins();
            debug!("Plugins: {:?}", plugins);
        }
    }
}
