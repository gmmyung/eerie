#![cfg(feature = "compiler")]

use eerie::compiler::{Compiler, CompilerError, CompilerLoadError};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "eerie-compiler-loading-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn child(mode: &str, path: Option<&Path>) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "loader_child", "--nocapture"])
        .env("EERIE_TEST_LOADER_MODE", mode)
        .env_remove("LIB_IREE_COMPILER");
    if let Some(path) = path {
        command.env("EERIE_TEST_LIBRARY_PATH", path);
    }
    command
}

fn assert_success(output: Output) {
    assert!(
        output.status.success(),
        "child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn automatic_discovery_and_independent_handles() {
    assert_success(child("discover", None).output().unwrap());
}

#[test]
fn global_options_cannot_change_while_other_handles_have_sessions() {
    assert_success(child("live-session", None).output().unwrap());
}

#[test]
fn failed_discovery_can_retry_with_an_explicit_library() {
    let compiler = Compiler::new().unwrap();
    let mut command = child("retry", Some(compiler.library_path()));
    command.env("EERIE_PYTHON", "eerie-python-that-does-not-exist");
    assert_success(command.output().unwrap());
}

#[test]
fn custom_libraries_support_spaces_unicode_and_directory_overrides() {
    let compiler = Compiler::new().unwrap();
    let directory = TestDirectory::new();
    let unicode_directory = directory.0.join("compiler Δ 😀");
    std::fs::create_dir(&unicode_directory).unwrap();
    let destination = unicode_directory.join(compiler.library_path().file_name().unwrap());
    std::fs::copy(compiler.library_path(), &destination).unwrap();
    assert_success(child("explicit", Some(&destination)).output().unwrap());
    let mut command = child("discover", None);
    command.env("LIB_IREE_COMPILER", &unicode_directory);
    assert_success(command.output().unwrap());
}

fn mock_library(directory: &Path, version: u32) -> PathBuf {
    let source = directory.join("mock.c");
    std::fs::write(
        &source,
        format!(
            "#ifdef _WIN32\n__declspec(dllexport)\n#endif\nint ireeCompilerGetAPIVersion(void) {{ return {version}; }}\n"
        ),
    )
    .unwrap();
    let destination = directory.join(if cfg!(windows) {
        "mock.dll"
    } else if cfg!(target_os = "macos") {
        "libmock.dylib"
    } else {
        "libmock.so"
    });
    #[cfg(windows)]
    let output = Command::new("cl")
        .current_dir(directory)
        .args(["/nologo", "/LD", "mock.c"])
        .arg(format!("/Fe:{}", destination.display()))
        .output()
        .unwrap();
    #[cfg(not(windows))]
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .current_dir(directory)
        .args(if cfg!(target_os = "macos") {
            vec!["-dynamiclib"]
        } else {
            vec!["-shared", "-fPIC"]
        })
        .arg(&source)
        .arg("-o")
        .arg(&destination)
        .output()
        .unwrap();
    assert_success(output);
    destination
}

#[test]
fn incompatible_apis_and_missing_symbols_are_recoverable_errors() {
    for (version, mode) in [
        (2 << 16, "bad-api"),
        ((1 << 16) | 3, "bad-api"),
        ((1 << 16) | 4, "missing-symbol"),
    ] {
        let directory = TestDirectory::new();
        let library = mock_library(&directory.0, version);
        assert_success(child(mode, Some(&library)).output().unwrap());
    }
}

#[test]
fn loader_child() {
    let Ok(mode) = std::env::var("EERIE_TEST_LOADER_MODE") else {
        return;
    };
    let path = std::env::var_os("EERIE_TEST_LIBRARY_PATH").map(PathBuf::from);
    let compiler = match mode.as_str() {
        "discover" => Compiler::new().unwrap(),
        "explicit" => Compiler::with_library(path.as_ref().unwrap()).unwrap(),
        "live-session" => {
            let compiler = Compiler::new().unwrap();
            let session = compiler.create_session();
            let mut other = compiler.clone();
            assert!(matches!(
                other.setup_global_cl(vec!["eerie-loading-tests".into()]),
                Err(CompilerError::GlobalCLInUse)
            ));
            drop(session);
            other
                .setup_global_cl(vec!["eerie-loading-tests".into()])
                .unwrap();
            other
        }
        "retry" => {
            let error = Compiler::new().err().unwrap();
            assert!(matches!(
                error,
                CompilerError::CompilerLoadError(CompilerLoadError::Discovery { .. })
            ));
            assert!(error
                .to_string()
                .contains("python -m pip install iree-base-compiler==3.13.0"));
            Compiler::with_library(path.as_ref().unwrap()).unwrap()
        }
        "bad-api" | "missing-symbol" => {
            let error = Compiler::with_library(path.as_ref().unwrap())
                .err()
                .unwrap();
            assert!(matches!(
                error,
                CompilerError::CompilerLoadError(CompilerLoadError::Library { .. })
            ));
            assert!(error.to_string().contains(if mode == "bad-api" {
                "incompatible; expected API 1.4"
            } else {
                "missing a required embedding API symbol"
            }));
            // A rejected library must not claim process-wide initialization.
            Compiler::new().unwrap()
        }
        _ => panic!("unknown loader test mode"),
    };
    let selected = compiler.library_path().to_owned();
    #[cfg(windows)]
    assert!(
        !selected.to_string_lossy().starts_with(r"\\?\"),
        "IREE's cmd.exe tool launcher cannot use verbatim library paths"
    );
    let clone = compiler.clone();
    drop(compiler);
    assert!(!clone
        .compile_str(
            include_str!("mul.mlir"),
            ["--iree-hal-target-backends=vmvx"]
        )
        .unwrap()
        .is_empty());
    let recreated = Compiler::new().unwrap();
    assert_eq!(recreated.library_path(), selected);
    assert!(matches!(
        Compiler::with_library("another-compiler-library"),
        Err(CompilerError::CompilerLoadError(
            CompilerLoadError::AlreadyLoaded { .. }
        ))
    ));
    let threads = (0..2)
        .map(|_| {
            std::thread::spawn(|| {
                let compiler = Compiler::new().unwrap();
                assert!(!compiler
                    .compile_str(
                        include_str!("mul.mlir"),
                        ["--iree-hal-target-backends=vmvx"]
                    )
                    .unwrap()
                    .is_empty());
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}
