use super::sys;
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{Mutex, OnceLock},
};
use thiserror::Error;

const IREE_VERSION: &str = "3.13.0";
const PYTHON_PROBE: &str = "import importlib.metadata as m; d=m.distribution('iree-base-compiler'); print(d.version); print(d.locate_file('iree/compiler/_mlir_libs'))";

#[cfg(target_os = "macos")]
const LIBRARY_NAME: &str = "libIREECompiler.dylib";
#[cfg(target_os = "windows")]
const LIBRARY_NAME: &str = "IREECompiler.dll";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const LIBRARY_NAME: &str = "libIREECompiler.so";

/// Errors discovering or loading the compiler shared library.
#[derive(Debug, Error)]
pub enum CompilerLoadError {
    #[error("Could not discover a compatible IREE compiler.\n{details}\nInstall it with `python -m pip install iree-base-compiler=={}` in your active Python environment, or use Compiler::with_library(path).", IREE_VERSION)]
    Discovery { details: String },
    #[error("Could not resolve compiler library path {}: {source}", path.display())]
    Path {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not load IREE compiler from {}: {source}", path.display())]
    Library {
        path: PathBuf,
        #[source]
        source: sys::LoadError,
    },
    #[error("IREE compiler is already loaded from {}; cannot switch to {} in the same process", loaded.display(), requested.display())]
    AlreadyLoaded { loaded: PathBuf, requested: PathBuf },
}

pub(super) struct LoadedCompiler {
    pub api: sys::IreeCompiler,
    pub path: PathBuf,
}

// IREE's final GlobalShutdown permanently disables initialization. Retain one
// initialization and its library for the process lifetime; Rust handles and
// sessions can then be created and dropped independently without dangling code.
static LOADED: OnceLock<LoadedCompiler> = OnceLock::new();
static LOAD_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn api() -> &'static sys::IreeCompiler {
    &LOADED
        .get()
        .expect("compiler objects require a successfully loaded Compiler")
        .api
}

fn library_path(path: &Path) -> Result<PathBuf, CompilerLoadError> {
    let path = if path.is_dir() {
        path.join(LIBRARY_NAME)
    } else {
        path.to_owned()
    };
    // IREE finds adjacent compiler tools using the loaded module's path.
    // Its Windows tool launcher uses cmd.exe, which rejects verbatim paths.
    #[cfg(windows)]
    let canonical = dunce::canonicalize(&path);
    #[cfg(not(windows))]
    let canonical = path.canonicalize();
    canonical.map_err(|source| CompilerLoadError::Path { path, source })
}

pub(super) fn load(
    explicit_path: Option<&Path>,
) -> Result<&'static LoadedCompiler, CompilerLoadError> {
    let _guard = LOAD_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(loaded) = LOADED.get() {
        if let Some(path) = explicit_path {
            let requested = library_path(path)?;
            if loaded.path != requested {
                return Err(CompilerLoadError::AlreadyLoaded {
                    loaded: loaded.path.clone(),
                    requested,
                });
            }
        }
        return Ok(loaded);
    }

    let loaded = if let Some(path) = explicit_path {
        open(path)?
    } else {
        discover()?
    };
    // The entire load/initialize/publish sequence is serialized. Failed library
    // discovery leaves LOADED empty so installation or an explicit path can retry.
    unsafe { loaded.api.ireeCompilerGlobalInitialize() };
    LOADED
        .set(loaded)
        .unwrap_or_else(|_| unreachable!("compiler load is serialized"));
    Ok(LOADED.get().unwrap())
}

fn open(path: &Path) -> Result<LoadedCompiler, CompilerLoadError> {
    let path = library_path(path)?;
    // Compiler libraries installed or explicitly selected by the application
    // must be trusted, like the libraries previously loaded by the system linker.
    let api =
        unsafe { sys::IreeCompiler::open(&path) }.map_err(|source| CompilerLoadError::Library {
            path: path.clone(),
            source,
        })?;
    Ok(LoadedCompiler { api, path })
}

fn python_candidates() -> Vec<(OsString, Vec<&'static str>)> {
    if let Some(python) = env::var_os("EERIE_PYTHON") {
        return vec![(python, vec![])];
    }
    let candidates = vec![
        (OsString::from("python"), vec![]),
        (OsString::from("python3"), vec![]),
    ];
    #[cfg(windows)]
    let candidates = {
        let mut candidates = candidates;
        candidates.push((OsString::from("py"), vec!["-3"]));
        candidates
    };
    candidates
}

fn probe_python(program: &OsString, arguments: &[&str]) -> std::io::Result<Output> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .args(["-X", "utf8", "-c", PYTHON_PROBE])
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for GUI applications.
    }
    command.output()
}

fn compiler_path(output: Output) -> Result<PathBuf, String> {
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap_or("Python compiler discovery failed")
            .to_owned());
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let (version, directory) = stdout
        .split_once('\n')
        .ok_or_else(|| "Python returned no compiler package location".to_owned())?;
    if version.trim() != IREE_VERSION {
        return Err(format!(
            "iree-base-compiler {} is installed; expected {IREE_VERSION}",
            version.trim()
        ));
    }
    let directory = directory.trim_end_matches(['\r', '\n']);
    if directory.is_empty() {
        return Err("Python returned an empty compiler package location".to_owned());
    }
    Ok(Path::new(directory).join(LIBRARY_NAME))
}

fn discover() -> Result<LoadedCompiler, CompilerLoadError> {
    for (program, arguments) in python_candidates() {
        let output = match probe_python(&program, &arguments) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            result => result,
        };
        let path = output
            .map_err(|error| error.to_string())
            .and_then(compiler_path)
            .map_err(|error| CompilerLoadError::Discovery {
                details: format!("{}: {error}", program.to_string_lossy()),
            })?;
        return open(&path);
    }
    Err(CompilerLoadError::Discovery {
        details:
            "No Python executable was found. Add Python to PATH or select it with EERIE_PYTHON."
                .to_owned(),
    })
}
