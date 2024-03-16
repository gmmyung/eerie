use eerie_sys::compiler as sys;
use log::{debug, error};
use std::{
    ffi::{CStr, CString},
    fmt::{Debug, Display, Formatter},
    marker::{PhantomData, PhantomPinned},
    os::fd::AsRawFd,
    path::Path,
    pin::Pin,
    sync::{Mutex, OnceLock},
};
use thiserror::Error;

/// Errors from the IREE compiler
pub struct Error {
    message: String,
}

impl Error {
    fn from_ptr(ptr: *mut sys::iree_compiler_error_t) -> Self {
        let c_str = unsafe { std::ffi::CStr::from_ptr(sys::ireeCompilerErrorGetMessage(ptr)) };
        let message = c_str.to_str().expect("Invalid UTF-8 string").to_string();
        unsafe { sys::ireeCompilerErrorDestroy(ptr) }
        Self { message }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

/// Get the api version of the IREE compiler
/// The compiler API is versioned. Within a major version, symbols may be
/// added, but existing symbols must not be removed or changed to alter
/// previously exposed functionality. A major version bump implies an API
/// break and no forward or backward compatibility is assumed across major
/// versions.
pub fn get_api_version() -> (u16, u16) {
    let version_bytes = unsafe { sys::ireeCompilerGetAPIVersion() } as u32;
    let major = (version_bytes >> 16) as u16;
    let minor = (version_bytes & 0xFFFF) as u16;
    (major, minor)
}

static IS_INITIALIZED: OnceLock<()> = OnceLock::new();

/// The IREE compiler. The compiler is globally initialized when it is created. It can not be
/// instantiated more than once.
pub struct Compiler {}

impl Compiler {
    /// Create a new IREE compiler.
    /// This should only be called once through the lifetime of the program.
    pub fn new() -> Result<Self, CompilerError> {
        match IS_INITIALIZED.set(()) {
            Ok(_) => {
                unsafe {
                    debug!("Global initializing compiler");
                    sys::ireeCompilerGlobalInitialize();
                }
                Ok(Self {})
            }
            Err(_) => Err(CompilerError::AlreadyInitialized),
        }
    }

    /// Gets the build revision of the IREE compiler. In official releases, this
    /// will be a string with the build tag. In development builds, it may be an
    /// empty string. The returned is valid for as long as the compiler is
    /// initialized.
    pub fn get_revision(&self) -> Result<String, CompilerError> {
        let rev_str =
            unsafe { std::ffi::CStr::from_ptr(sys::ireeCompilerGetRevision()) }.to_str()?;
        Ok(rev_str.to_string())
    }

    /// Initializes the command line environment from an explicit argc/argv
    /// This uses dark magic to setup the usual array of expected signal handlers.
    /// This API is not yet considered version-stable. If using out of tree, please
    /// contact the developers.
    pub fn setup_global_cl(&mut self, argv: Vec<String>) -> Result<&mut Self, CompilerError> {
        let c_str_vec = argv
            .iter()
            .map(|arg| std::ffi::CString::new(arg.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut ptr_array = c_str_vec.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        let banner = std::ffi::CString::new("IREE Compiler")?;
        unsafe {
            sys::ireeCompilerSetupGlobalCL(
                argv.len() as i32,
                ptr_array.as_mut_ptr(),
                banner.as_ptr(),
                false,
            )
        }
        debug!("Global CL setup");
        Ok(self)
    }

    extern "C" fn capture_registered_hal_target_backend_callback(
        backend: *const std::os::raw::c_char,
        user_data: *mut std::ffi::c_void,
    ) {
        debug!("Capturing registered HAL target backend");
        let hal_target_backend_list = unsafe {
            let ptr = user_data as *mut Mutex<Vec<String>>;
            &mut *ptr
        }
        .get_mut()
        .unwrap();
        let backend_name = unsafe { std::ffi::CStr::from_ptr(backend) };
        hal_target_backend_list.push(backend_name.to_str().unwrap().to_string());
        debug!("Backend name: {}", backend_name.to_str().unwrap());
    }

    /// Enumerates the registered HAL target backends.
    pub fn get_registered_hal_target_backends(&self) -> Vec<String> {
        let mut registered_hal_target_backends = Mutex::new(Vec::new());
        debug!("Enumerating registered HAL target backends");
        unsafe {
            sys::ireeCompilerEnumerateRegisteredHALTargetBackends(
                Some(Self::capture_registered_hal_target_backend_callback),
                &mut registered_hal_target_backends as *mut Mutex<Vec<String>> as *mut _,
            );
        }
        let registered_hal_target_backends = registered_hal_target_backends.lock().unwrap();
        registered_hal_target_backends.clone()
    }

    extern "C" fn capture_plugin_callback(
        backend: *const std::os::raw::c_char,
        user_data: *mut std::ffi::c_void,
    ) {
        debug!("Capturing registered HAL target backend");
        let hal_target_backend_list = unsafe {
            let ptr = user_data as *mut Mutex<Vec<String>>;
            &mut *ptr
        }
        .get_mut()
        .unwrap();
        let backend_name = unsafe { std::ffi::CStr::from_ptr(backend) };
        hal_target_backend_list.push(backend_name.to_str().unwrap().to_string());
        debug!("Backend name: {}", backend_name.to_str().unwrap());
    }

    /// Enumerates the registered plugins.
    pub fn get_plugins(&self) -> Vec<String> {
        let mut plugins = Mutex::new(Vec::new());
        debug!("Enumerating plugins");
        unsafe {
            sys::ireeCompilerEnumeratePlugins(
                Some(Self::capture_plugin_callback),
                &mut plugins as *mut Mutex<Vec<String>> as *mut _,
            );
        }
        let plugins = plugins.lock().unwrap();
        plugins.clone()
    }

    /// Creates a new session.
    pub fn create_session(&self) -> Session {
        Session::new(self)
    }
}

impl Drop for Compiler {
    fn drop(&mut self) {
        unsafe {
            debug!("Global shutting down compiler");
            sys::ireeCompilerGlobalShutdown();
        }
    }
}

/// A session represents a scope where one or more runs can be executed.
/// Internally, it consists of an MLIRContext and a private set of session
/// options. If the CL environment was initialized, session options will be
/// bootstrapped from global flags.
pub struct Session<'a> {
    ctx: *mut sys::iree_compiler_session_t,
    _compiler: &'a Compiler,
}

impl<'a> Session<'a> {
    /// Creates a new session.
    pub fn new(compiler: &'a Compiler) -> Self {
        let ctx: *mut sys::iree_compiler_session_t;
        unsafe {
            debug!("Creating session");
            ctx = sys::ireeCompilerSessionCreate();
        }
        Self {
            ctx,
            _compiler: compiler,
        }
    }

    /// Sets session flags.
    pub fn set_flags(&mut self, argv: Vec<String>) -> Result<&mut Self, CompilerError> {
        let c_str_vec = argv
            .iter()
            .map(|arg| std::ffi::CString::new(arg.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let ptr_array = c_str_vec.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        let err_ptr: *mut sys::iree_compiler_error_t;
        unsafe {
            debug!("Setting session flags");
            err_ptr =
                sys::ireeCompilerSessionSetFlags(self.ctx, argv.len() as i32, ptr_array.as_ptr());
            debug!("Session flags set");
        }
        if err_ptr.is_null() {
            Ok(self)
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }

    extern "C" fn capture_flags_callback(
        flag: *const std::os::raw::c_char,
        _length: usize,
        user_data: *mut std::ffi::c_void,
    ) {
        debug!("Capturing session flags");
        let flags = unsafe {
            let ptr = user_data as *mut Mutex<Vec<String>>;
            &mut *ptr
        }
        .get_mut()
        .unwrap();
        let flag = unsafe { std::ffi::CStr::from_ptr(flag) };
        flags.push(flag.to_str().unwrap().to_string());
        debug!("Flag: {}", flag.to_str().unwrap());
    }

    /// Gets textual flags actually in effect from any source. Optionally, only
    /// calls back for non-default valued flags.
    pub fn get_flags(&self, non_default_only: bool) -> Vec<String> {
        let mut flags = Mutex::new(Vec::new());
        debug!("Getting session flags");
        unsafe {
            sys::ireeCompilerSessionGetFlags(
                self.ctx,
                non_default_only,
                Some(Self::capture_flags_callback),
                &mut flags as *mut Mutex<Vec<String>> as *mut _,
            );
        }
        let flags = flags.lock().unwrap();
        flags.clone()
    }

    /// Creates a new invocation.
    pub fn create_invocation(&self) -> Invocation {
        Invocation::new(self)
    }

    /// Creates a new source from a file.
    pub fn create_source_from_file(
        &'a self,
        file_name: &Path,
    ) -> Result<Source<'a, '_>, CompilerError> {
        Source::from_file(self, file_name)
    }

    /// Creates a new source from a C string.
    /// Rust string is not supported due to c api limitations.
    pub fn create_source_from_cstr<'b>(
        &'a self,
        buffer: &'b CStr,
    ) -> Result<Source<'a, 'b>, CompilerError> {
        Source::from_cstr(self, buffer)
    }

    /// Creates a new source from a mlir framebuffer.
    pub fn create_source_from_buf<'b>(
        &'a self,
        buffer: &'b [u8],
    ) -> Result<Source<'a, 'b>, CompilerError> {
        Source::from_buf(self, buffer)
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe {
            debug!("Destroying session");
            sys::ireeCompilerSessionDestroy(self.ctx);
        }
    }
}

/// An invocation represents a single run of a session.
pub struct Invocation<'a> {
    ctx: *mut sys::iree_compiler_invocation_t,
    diagnostic_queue: Pin<Box<Diagnostics>>,
    session: &'a Session<'a>,
}

/// IREE Compiler diagnostics.
#[derive(Clone)]
pub enum Diagnostic {
    Note(String),
    Warning(String),
    Error(String),
    Remark(String),
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Diagnostic::Note(s) => write!(f, "Note: {}", s),
            Diagnostic::Warning(s) => write!(f, "Warning: {}", s),
            Diagnostic::Error(s) => write!(f, "Error: {}", s),
            Diagnostic::Remark(s) => write!(f, "Remark: {}", s),
        }
    }
}

impl Debug for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

/// IREE Compiler diagnostics.
#[derive(Debug)]
pub struct Diagnostics {
    data: Mutex<Vec<Diagnostic>>,
    _pin: PhantomPinned,
}

impl Display for Diagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let vec = self.data.lock().unwrap();
        for diagnostic in vec.iter() {
            writeln!(f, "{}", diagnostic)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Diagnostics {
    fn new(data: Vec<Diagnostic>) -> Self {
        Self {
            data: Mutex::new(data),
            _pin: PhantomPinned,
        }
    }

    fn clear(&self) {
        self.data.lock().unwrap().clear();
    }

    fn push(&self, diagnostic: Diagnostic) {
        self.data.lock().unwrap().push(diagnostic);
    }
}

impl Clone for Diagnostics {
    fn clone(&self) -> Self {
        let vec = self.data.lock().unwrap();
        Self::new(vec.clone())
    }
}

/// IREE Compiler pipelines.
pub enum Pipeline {
    /// IREE's full compilation pipeline.
    Std,
    /// Pipeline to translate a single hal.executable into a target-specific
    /// binary form (such as an ELF file or a flatbuffer containing a SPIR-V
    /// blob).
    HalExecutable,
    /// IREE's precompilation pipeline, which does input preprocessing and
    /// pre-fusion global optimization.
    /// This is experimental and this should be changed as we move to a more
    /// cohesive approach for managing compilation phases.
    Precompile,
}

impl From<Pipeline> for sys::iree_compiler_pipeline_t {
    fn from(val: Pipeline) -> Self {
        match val {
            Pipeline::Std => sys::iree_compiler_pipeline_t_IREE_COMPILER_PIPELINE_STD,
            Pipeline::HalExecutable => {
                sys::iree_compiler_pipeline_t_IREE_COMPILER_PIPELINE_HAL_EXECUTABLE
            }
            Pipeline::Precompile => sys::iree_compiler_pipeline_t_IREE_COMPILER_PIPELINE_PRECOMPILE,
        }
    }
}

impl<'a> Invocation<'a> {
    /// Creates a new invocation from a session.
    pub fn new(session: &'a Session<'a>) -> Self {
        let ctx: *mut sys::iree_compiler_invocation_t;
        unsafe {
            debug!("Creating invocation");
            ctx = sys::ireeCompilerInvocationCreate(session.ctx);
        }
        let diagnostic_queue = Box::pin(Diagnostics::new(Vec::new()));
        unsafe {
            sys::ireeCompilerInvocationEnableCallbackDiagnostics(
                ctx,
                0,
                Some(Self::capture_diagnostics_callback),
                diagnostic_queue.as_ref().get_ref() as *const Diagnostics as *mut _,
            );
        }
        Self {
            ctx,
            diagnostic_queue,
            session,
        }
    }

    extern "C" fn capture_diagnostics_callback(
        severity: sys::iree_compiler_diagnostic_severity_t,
        message: *const std::os::raw::c_char,
        _length: usize,
        user_data: *mut std::ffi::c_void,
    ) {
        debug!("Capturing callback diagnostics");
        let message = unsafe { std::ffi::CStr::from_ptr(message) };
        let diagnostic = match severity {
            sys::iree_compiler_diagnostic_severity_t_IREE_COMPILER_DIAGNOSTIC_SEVERITY_NOTE => {
                Diagnostic::Note(message.to_str().unwrap().to_string())
            }
            sys::iree_compiler_diagnostic_severity_t_IREE_COMPILER_DIAGNOSTIC_SEVERITY_WARNING => {
                Diagnostic::Warning(message.to_str().unwrap().to_string())
            }
            sys::iree_compiler_diagnostic_severity_t_IREE_COMPILER_DIAGNOSTIC_SEVERITY_ERROR => {
                Diagnostic::Error(message.to_str().unwrap().to_string())
            }
            sys::iree_compiler_diagnostic_severity_t_IREE_COMPILER_DIAGNOSTIC_SEVERITY_REMARK => {
                Diagnostic::Remark(message.to_str().unwrap().to_string())
            }
            _ => {
                panic!("Unknown diagnostic severity");
            }
        };
        debug!("Diagnostic: {:?}", diagnostic);
        unsafe {
            let diagnostic_queue = &*(user_data as *const Diagnostics);
            debug!("Before push");
            debug!("Diagnostics queue: {:?}", diagnostic_queue);
            diagnostic_queue.push(diagnostic);
        }
    }

    /// Enables default, pretty-printed diagnostics to the console. This is usually
    /// the right thing to do for command-line tools, but other mechanisms are
    /// preferred for library use.
    pub fn enable_console_diagnostics(&mut self) -> &mut Self {
        debug!("Enabling console diagnostics");
        unsafe {
            sys::ireeCompilerInvocationEnableConsoleDiagnostics(self.ctx);
        }
        self
    }

    /// Parses a source into this instance in preparation for performing a
    /// compilation action.
    pub fn parse_source(&mut self, source: Source) -> Result<&mut Self, CompilerError> {
        self.diagnostic_queue.clear();
        debug!("Parsing source");
        match unsafe { sys::ireeCompilerInvocationParseSource(self.ctx, source.ctx) } {
            true => Ok(self),
            false => Err(CompilerError::IREECompilerDiagnosticsError(
                self.diagnostic_queue.as_ref().get_ref().clone(),
            )),
        }
    }

    /// Parses a source from a file
    pub fn parse_source_from_file(&mut self, file_name: &Path) -> Result<&mut Self, CompilerError> {
        let source = Source::from_file(self.session, file_name)?;
        self.parse_source(source)
    }

    /// Sets a mnemonic phase name to run compilation from. Default is "input".
    /// The meaning of this is pipeline specific. See IREEVMPipelinePhase
    pub fn set_compile_from_phase(&mut self, phase: &str) -> Result<&mut Self, CompilerError> {
        debug!("Setting compile from phase");
        let phase = CString::new(phase)?;
        unsafe { sys::ireeCompilerInvocationSetCompileFromPhase(self.ctx, phase.as_ptr()) }
        Ok(self)
    }

    /// Sets a mnemonic phase name to run compilation to. Default is "end".
    /// The meaning of this is pipeline specific. See IREEVMPipelinePhase
    /// for the standard pipeline.
    pub fn set_compile_to_phase(&mut self, phase: &str) -> Result<&mut Self, CompilerError> {
        debug!("Setting compile to phase");
        let phase = CString::new(phase)?;
        unsafe { sys::ireeCompilerInvocationSetCompileToPhase(self.ctx, phase.as_ptr()) }
        Ok(self)
    }

    /// Enables/disables verification of IR after each pass. Defaults to enabled.
    pub fn set_verify_ir(&mut self, enable: bool) -> &mut Self {
        debug!("Setting verify IR");
        unsafe { sys::ireeCompilerInvocationSetVerifyIR(self.ctx, enable) }
        self
    }

    /// Runs an arbitrary pass pipeline.
    pub fn pipeline(&mut self, pipeline: Pipeline) -> Result<&mut Self, CompilerError> {
        self.diagnostic_queue.clear();
        debug!("Running pipeline");
        match unsafe { sys::ireeCompilerInvocationPipeline(self.ctx, pipeline.into()) } {
            true => Ok(self),
            false => Err(CompilerError::IREECompilerDiagnosticsError(
                self.diagnostic_queue.as_ref().get_ref().clone(),
            )),
        }
    }

    /// Runs an arbitrary pass pipeline.
    pub fn run_pass_pipeline(
        &mut self,
        text_pass_pipeline: &str,
    ) -> Result<&mut Self, CompilerError> {
        self.diagnostic_queue.clear();
        debug!("Running pass pipeline");
        let text_pass_pipeline = CString::new(text_pass_pipeline)?;
        match unsafe {
            sys::ireeCompilerInvocationRunPassPipeline(self.ctx, text_pass_pipeline.as_ptr())
        } {
            true => Ok(self),
            false => Err(CompilerError::IREECompilerDiagnosticsError(
                self.diagnostic_queue.as_ref().get_ref().clone(),
            )),
        }
    }

    /// Outputs the current compiler state as textual IR to the output.
    pub fn output_ir(&self, output: &mut impl Output) -> Result<&Self, CompilerError> {
        debug!("Outputting IR");
        self.diagnostic_queue.clear();
        let output_ptr = output.as_ptr();
        let err_ptr = unsafe { sys::ireeCompilerInvocationOutputIR(self.ctx, output_ptr) };
        if err_ptr.is_null() {
            Ok(self)
        } else {
            let diagnostic_queue = self.diagnostic_queue.as_ref().get_ref().clone();
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                diagnostic_queue,
            ))
        }
    }

    /// Outputs the current compiler state as bytecode IR to the output.
    /// Emits as the given bytecode version or most recent if -1.
    pub fn output_ir_bytecode(
        &self,
        output: &mut impl Output,
        bytecode_version: i32,
    ) -> Result<&Self, CompilerError> {
        debug!("Outputting bytecode");
        self.diagnostic_queue.clear();
        let output_ptr = output.as_ptr();
        let err_ptr = unsafe {
            sys::ireeCompilerInvocationOutputIRBytecode(self.ctx, output_ptr, bytecode_version)
        };
        if err_ptr.is_null() {
            Ok(self)
        } else {
            let diagnostic_queue = self.diagnostic_queue.as_ref().get_ref().clone();
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                diagnostic_queue,
            ))
        }
    }

    /// Assuming that the compiler has produced VM IR, converts it to bytecode
    /// and outputs it. This is a valid next step after running the
    /// Std pipeline.
    pub fn output_vm_byte_code(&self, output: &mut impl Output) -> Result<&Self, CompilerError> {
        debug!("Outputting VM byte code");
        self.diagnostic_queue.clear();
        let output_ptr = output.as_ptr();
        let err_ptr = unsafe { sys::ireeCompilerInvocationOutputVMBytecode(self.ctx, output_ptr) };
        if err_ptr.is_null() {
            Ok(self)
        } else {
            let diagnostic_queue = self.diagnostic_queue.as_ref().get_ref().clone();
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                diagnostic_queue,
            ))
        }
    }

    // Assuming that the compiler has produced VM IR, converts it to textual
    // C source and output it. This is a valid next step after running the
    // Std pipeline.
    pub fn output_vm_c_source(&self, output: &mut impl Output) -> Result<&Self, CompilerError> {
        debug!("Outputting VM source");
        self.diagnostic_queue.clear();
        let output_ptr = output.as_ptr();
        let err_ptr = unsafe { sys::ireeCompilerInvocationOutputVMCSource(self.ctx, output_ptr) };
        if err_ptr.is_null() {
            Ok(self)
        } else {
            let diagnostic_queue = self.diagnostic_queue.as_ref().get_ref().clone();
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                diagnostic_queue,
            ))
        }
    }

    /// Outputs the contents of a single HAL executable as binary data.
    /// This is a valid next step after running the
    /// HalExecutable pipeline.
    pub fn output_hal_executable(&self, output: &mut impl Output) -> Result<&Self, CompilerError> {
        debug!("Outputting HAL executable");
        let output_ptr = output.as_ptr();
        let err_ptr =
            unsafe { sys::ireeCompilerInvocationOutputHALExecutable(self.ctx, output_ptr) };
        if err_ptr.is_null() {
            Ok(self)
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }
}

impl Drop for Invocation<'_> {
    fn drop(&mut self) {
        unsafe {
            debug!("Destroying invocation");
            sys::ireeCompilerInvocationDestroy(self.ctx);
        }
    }
}

/// Compilation sources are loaded into Sources.
/// Sources reference an originating session and may contain a concrete
/// buffer of memory.
pub struct Source<'a, 'b> {
    ctx: *mut sys::iree_compiler_source_t,
    session: &'a Session<'a>,
    _phantom: PhantomData<&'b [u8]>,
}

impl<'a, 'b> Source<'a, 'b> {
    /// Creates a source from a file.
    pub fn from_file(session: &'a Session<'a>, file: &Path) -> Result<Self, CompilerError> {
        debug!("Creating source from file");
        match file.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                return Err(CompilerError::FileNotFound(
                    file.to_path_buf().to_str().unwrap().into(),
                ))
            }
            Err(e) => return Err(e.into()),
        }

        let file = CString::new(file.to_str().unwrap())?;
        let mut source_ptr = std::ptr::null_mut();
        let err_ptr = unsafe {
            debug!("Opening file");
            sys::ireeCompilerSourceOpenFile(session.ctx, file.as_ptr(), &mut source_ptr)
        };
        if err_ptr.is_null() {
            Ok(Source {
                ctx: source_ptr,
                session,
                _phantom: PhantomData,
            })
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }

    fn wrap_buffer(
        session: &'a Session<'a>,
        buf: &'b [u8],
        nullterm: bool,
    ) -> Result<Self, CompilerError> {
        debug!("Creating source from buffer");
        let buf_name = CString::new("buffer")?;
        let mut source_ptr = std::ptr::null_mut();
        debug!("len: {}", buf.len());
        let err_ptr = unsafe {
            sys::ireeCompilerSourceWrapBuffer(
                session.ctx,
                buf_name.as_ptr(),
                buf.as_ptr() as *const core::ffi::c_char,
                buf.len(),
                nullterm,
                &mut source_ptr,
            )
        };

        debug!("buffer name: {:?}", buf_name);
        if err_ptr.is_null() {
            Ok(Source {
                ctx: source_ptr,
                session,
                _phantom: PhantomData,
            })
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }

    /// Creates a source from a CStr.
    pub fn from_cstr(session: &'a Session<'a>, cstr: &'b CStr) -> Result<Self, CompilerError> {
        debug!("Creating source from CStr");
        Self::wrap_buffer(session, cstr.to_bytes_with_nul(), true)
    }

    /// Creates a source from a buffer.
    pub fn from_buf(session: &'a Session<'a>, buf: &'b [u8]) -> Result<Self, CompilerError> {
        debug!("Creating source from buffer");
        Self::wrap_buffer(session, buf, false)
    }

    extern "C" fn split_callback(
        source: *mut sys::iree_compiler_source_t,
        user_data: *mut std::ffi::c_void,
    ) {
        debug!("Splitting source callback");
        let sources =
            unsafe { &mut *(user_data as *mut Mutex<Vec<*mut sys::iree_compiler_source_t>>) }
                .get_mut()
                .unwrap();

        sources.push(source);
    }

    /// Splits the current source buffer, invoking a callback for each "split"
    /// within it. This is per the usual MLIR split rules (see
    /// splitAndProcessBuffer): which split on `// -----`.
    pub fn split(&self) -> Result<Vec<Self>, CompilerError> {
        debug!("Splitting source");
        let mut sources = Mutex::new(Vec::new());
        let err_ptr = unsafe {
            sys::ireeCompilerSourceSplit(
                self.ctx,
                Some(Self::split_callback),
                &mut sources as *mut Mutex<Vec<*mut sys::iree_compiler_source_t>>
                    as *mut std::ffi::c_void,
            )
        };
        if err_ptr.is_null() {
            Ok(sources
                .into_inner()
                .unwrap()
                .into_iter()
                .map(|ctx| Source {
                    ctx,
                    session: self.session,
                    _phantom: PhantomData,
                })
                .collect())
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }
}

impl Drop for Source<'_, '_> {
    fn drop(&mut self) {
        unsafe {
            debug!("Destroying source");
            sys::ireeCompilerSourceDestroy(self.ctx);
        }
    }
}

/// Outputs that can be written by the invocation
pub trait Output {
    fn as_ptr(&self) -> *mut sys::iree_compiler_output_t;
}

/// Output that writes by file name
pub struct FileNameOutput<'a> {
    ctx: *mut sys::iree_compiler_output_t,
    _compiler: &'a Compiler,
}

impl Output for FileNameOutput<'_> {
    fn as_ptr(&self) -> *mut sys::iree_compiler_output_t {
        self.ctx
    }
}

impl Drop for FileNameOutput<'_> {
    fn drop(&mut self) {
        unsafe {
            sys::ireeCompilerOutputKeep(self.ctx);
            sys::ireeCompilerOutputDestroy(self.ctx);
        }
    }
}

impl<'a> FileNameOutput<'a> {
    /// Creates a new filename output
    pub fn new(compiler: &'a Compiler, path: &Path) -> Result<Self, CompilerError> {
        debug!("Creating filename output");
        let path = CString::new(path.to_str().unwrap())?;
        let mut output_ptr = std::ptr::null_mut();
        let err_ptr = unsafe { sys::ireeCompilerOutputOpenFile(path.as_ptr(), &mut output_ptr) };
        if err_ptr.is_null() {
            Ok(FileNameOutput {
                ctx: output_ptr,
                _compiler: compiler,
            })
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }
}

/// Output that writes to a file
pub struct FileOutput<'a, 'b> {
    ctx: *mut sys::iree_compiler_output_t,
    _marker: PhantomData<&'a mut std::fs::File>,
    _compiler: &'b Compiler,
}

impl Output for FileOutput<'_, '_> {
    fn as_ptr(&self) -> *mut sys::iree_compiler_output_t {
        self.ctx
    }
}

impl Drop for FileOutput<'_, '_> {
    fn drop(&mut self) {
        unsafe {
            sys::ireeCompilerOutputKeep(self.ctx);
            sys::ireeCompilerOutputDestroy(self.ctx);
        }
    }
}

impl<'a, 'b> FileOutput<'a, 'b> {
    /// Creates a new file output from std::fs::File
    #[allow(clippy::needless_pass_by_ref_mut)]
    pub fn from_file(
        compiler: &'b Compiler,
        file: &'a mut std::fs::File,
    ) -> Result<Self, CompilerError> {
        debug!("Creating file output");
        let fd = file.as_raw_fd();
        let mut output_ptr = std::ptr::null_mut();
        let err_ptr = unsafe { sys::ireeCompilerOutputOpenFD(fd, &mut output_ptr) };
        if err_ptr.is_null() {
            Ok(FileOutput {
                ctx: output_ptr,
                _marker: PhantomData,
                _compiler: compiler,
            })
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }
}

/// Output that writes to a memory buffer
pub struct MemBufferOutput<'a> {
    ctx: *mut sys::iree_compiler_output_t,
    _compiler: &'a Compiler,
}

impl Output for MemBufferOutput<'_> {
    fn as_ptr(&self) -> *mut sys::iree_compiler_output_t {
        self.ctx
    }
}

impl Drop for MemBufferOutput<'_> {
    fn drop(&mut self) {
        unsafe {
            debug!("Destroying membuffer output");
            sys::ireeCompilerOutputDestroy(self.ctx);
        }
    }
}

impl<'a> MemBufferOutput<'a> {
    /// Creates a new membuffer output
    pub fn new(compiler: &'a Compiler) -> Result<Self, CompilerError> {
        debug!("Creating membuffer output");
        let mut output_ptr = std::ptr::null_mut();
        let err_ptr = unsafe { sys::ireeCompilerOutputOpenMembuffer(&mut output_ptr) };
        if err_ptr.is_null() {
            Ok(MemBufferOutput {
                ctx: output_ptr,
                _compiler: compiler,
            })
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }

    /// Maps the memory buffer to a slice
    pub fn map_memory(&self) -> Result<&[u8], CompilerError> {
        debug!("Mapping membuffer output");
        let mut data_ptr = std::ptr::null_mut();
        let mut data_length = 0;
        let err_ptr =
            unsafe { sys::ireeCompilerOutputMapMemory(self.ctx, &mut data_ptr, &mut data_length) };
        if err_ptr.is_null() {
            Ok(unsafe {
                std::slice::from_raw_parts(data_ptr as *const u8, data_length.try_into().unwrap())
            })
        } else {
            Err(CompilerError::IREECompilerError(
                Error::from_ptr(err_ptr),
                Diagnostics::default(),
            ))
        }
    }
}

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("Compiler initialized more than once")]
    AlreadyInitialized,
    #[error("CString contains a null byte")]
    NulError(#[from] std::ffi::NulError),
    #[error("Invalid UTF-8 sequence")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("IREE compiler error: {0:?} {1:?}")]
    IREECompilerError(Error, Diagnostics),
    #[error("IREE compiler error: {0:?}")]
    IREECompilerDiagnosticsError(Diagnostics),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error(transparent)]
    FileIoError(#[from] std::io::Error),
}
