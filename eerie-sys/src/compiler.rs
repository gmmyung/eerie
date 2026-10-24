#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    clippy::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/compiler.rs"));
}
pub use bindings::*;

/// A failure to open a compiler library or use its embedding API.
#[derive(Debug)]
pub enum LoadError {
    Library(libloading::Error),
    MissingApi(libloading::Error),
    IncompatibleApi { major: u16, minor: u16 },
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Library(error) => error.fmt(f),
            Self::MissingApi(error) => write!(
                f,
                "compiler library is missing a required embedding API symbol: {error}"
            ),
            Self::IncompatibleApi { major, minor } => write!(
                f,
                "compiler embedding API {major}.{minor} is incompatible; expected API 1.4 or newer within major version 1"
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Library(error) | Self::MissingApi(error) => Some(error),
            Self::IncompatibleApi { .. } => None,
        }
    }
}

impl From<libloading::Error> for LoadError {
    fn from(error: libloading::Error) -> Self {
        Self::Library(error)
    }
}

impl IreeCompiler {
    /// Opens an IREE compiler library and checks its embedding API before
    /// resolving the remaining functions. The library stays loaded with this table.
    ///
    /// # Safety
    ///
    /// The path must identify a trusted IREE compiler library whose initialization
    /// and termination routines are safe to execute in this process. The caller
    /// must initialize the compiler before using its compilation APIs, and keep
    /// this table alive for every compiler object created through it.
    pub unsafe fn open(path: impl AsRef<std::ffi::OsStr>) -> Result<Self, LoadError> {
        #[cfg(unix)]
        let library: libloading::Library = {
            let flags = libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_LOCAL;
            #[cfg(target_vendor = "apple")]
            // Resolve functions against this library rather than its dependencies.
            let flags = flags | libc::RTLD_FIRST;
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            // Prefer the compiler's own LLVM/MLIR over a host application's.
            let flags = flags | libc::RTLD_DEEPBIND;
            libloading::os::unix::Library::open(Some(path.as_ref()), flags)?.into()
        };
        #[cfg(windows)]
        let library: libloading::Library = {
            use libloading::os::windows::{
                Library, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
            };
            let mut flags = LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
            if std::path::Path::new(path.as_ref()).is_absolute() {
                // Find adjacent DLL dependencies without editing the process PATH.
                flags |= LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR;
            }
            Library::load_with_flags(path.as_ref(), flags)?.into()
        };
        let version = library
            .get::<unsafe extern "C" fn() -> i32>(b"ireeCompilerGetAPIVersion\0")
            .map_err(LoadError::MissingApi)?();
        let (major, minor) = ((version as u32 >> 16) as u16, version as u16);
        if major != 1 || minor < 4 {
            return Err(LoadError::IncompatibleApi { major, minor });
        }
        Self::from_library(library).map_err(LoadError::MissingApi)
    }
}
