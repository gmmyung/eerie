#[cfg(any(feature = "compiler", feature = "runtime"))]
use std::{
    env,
    path::{Path, PathBuf},
};

#[cfg(any(feature = "compiler", feature = "runtime"))]
fn binding_builder(
    sysroot: Option<&PathBuf>,
    header_path: &Path,
    include_path: &Path,
) -> bindgen::Builder {
    println!("cargo:rerun-if-changed={}", header_path.display());
    let mut builder = bindgen::Builder::default()
        .header(header_path.display().to_string())
        .clang_arg(format!("-I{}", include_path.display()))
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    #[cfg(not(feature = "std"))]
    {
        builder = builder.use_core();
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        builder = builder
            .clang_arg("-DIREE_PLATFORM_GENERIC=1")
            .clang_arg("-Wno-char-subscripts")
            .clang_arg("-Wno-format")
            .clang_arg("-Wno-implicit-function-declaration")
            .clang_arg("-Wno-unused-variable")
            .clang_arg("-DFLATCC_USE_GENERIC_ALIGNED_ALLOC=1")
    }

    if let Some(sysroot) = sysroot {
        builder = builder.clang_arg(format!("--sysroot={}", sysroot.display()));
    }
    builder
}

#[cfg(feature = "compiler")]
fn generate_compiler_bindings(
    sysroot: Option<&PathBuf>,
    headers: &[PathBuf],
    include_path: &Path,
    bindings_path: &Path,
) {
    for path in headers {
        let out_path = bindings_path.join(path).with_extension("rs");
        std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
        binding_builder(sysroot, &include_path.join(path), include_path)
            .allowlist_function("ireeCompiler.*")
            .dynamic_library_name("IreeCompiler")
            .dynamic_link_require_all(true)
            .generate()
            .expect("Unable to generate compiler bindings")
            .write_to_file(&out_path)
            .expect("Couldn't write compiler bindings");
    }
}

#[cfg(feature = "runtime")]
fn generate_runtime_bindings(
    sysroot: Option<&PathBuf>,
    header_path: &Path,
    include_path: &Path,
    bindings_path: &Path,
    additional_include_paths: &[PathBuf],
) {
    let mut builder = binding_builder(sysroot, header_path, include_path)
        // Platform headers are implementation details, not part of IREE's API.
        // In particular, Windows SDK structs can carry padding that cannot
        // derive Default on Rust arrays larger than 32 elements.
        .allowlist_type("iree_.*")
        .allowlist_function("iree_.*")
        .allowlist_var("(iree_|IREE_).*")
        .size_t_is_usize(true)
        .blocklist_function("^(bcmp|memcmp|memcpy|memmove|memset|strlen)$")
        // glibc exposes these both as macros and enum variants. They are not
        // part of IREE's API and would otherwise generate duplicate constants.
        .blocklist_item("^FP_(NAN|INFINITE|ZERO|SUBNORMAL|NORMAL)$");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        // The Rust-backed pthread compatibility ABI needs this C header type.
        builder = builder.allowlist_type("timespec");
    }
    for additional_include_path in additional_include_paths {
        println!(
            "cargo:rerun-if-changed={}",
            additional_include_path.display()
        );
        builder = builder.clang_arg(format!("-I{}", additional_include_path.display()));
    }

    builder
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(bindings_path)
        .expect("Couldn't write bindings!");
}

#[cfg(feature = "runtime")]
const RUNTIME_CMAKE_DEFINITIONS: &[(&str, &str)] = &[
    ("IREE_ENABLE_LIBBACKTRACE", "OFF"),
    ("IREE_BUILD_COMPILER", "OFF"),
    ("IREE_BUILD_TESTS", "OFF"),
    ("IREE_BUILD_BENCHMARKS", "OFF"),
    ("IREE_BUILD_SAMPLES", "OFF"),
    ("IREE_BUILD_BINDINGS_TFLITE", "OFF"),
    ("IREE_BUILD_BINDINGS_TFLITE_JAVA", "OFF"),
];

fn main() {
    println!("cargo:rerun-if-changed=CMakeLists.txt");
    println!("cargo:rerun-if-changed=iree/CMakeLists.txt");
    #[cfg(feature = "runtime")]
    println!("cargo:rerun-if-changed=iree/runtime/src");
    #[cfg(any(feature = "compiler", feature = "runtime"))]
    let iree_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("iree");
    #[cfg(any(feature = "compiler", feature = "runtime"))]
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    #[cfg(feature = "runtime")]
    let bare_metal_sync_include_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bare_metal_sync/include");

    // Generate a runtime-loaded API table. Building the crate does not require
    // an installed compiler library; discovery happens in Compiler::new().
    #[cfg(feature = "compiler")]
    {
        if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            assert_eq!(
                env::var("CARGO_CFG_TARGET_ENV").as_deref(),
                Ok("msvc"),
                "Windows builds require the MSVC Rust target"
            );
        }
        generate_compiler_bindings(
            None,
            // Path to compiler headers
            &[PathBuf::from("iree/compiler/embedding_api.h")],
            // Path to IREE compiler header sources
            &iree_path.join("compiler/bindings/c"),
            // Path to generated compiler bindings
            &out_path.join("compiler"),
        );
    }

    // The runtime feature enables the IREE runtime. It configures available runtime backends based
    // on the target os and architecture. The IREE runtime is built from source using CMake and
    // clang/llvm. Other compilers can be used as well, but clang is recommended when cross-compiling.
    #[cfg(feature = "runtime")]
    {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
        let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
        if target_os == "windows" && !msvc {
            panic!("Windows builds require the MSVC Rust target");
        }
        let bare_metal = target_os == "none";
        if bare_metal && cfg!(feature = "std") {
            panic!("The std feature cannot be used for target_os = \"none\"");
        }
        if target_os == "macos" && cfg!(feature = "vulkan") {
            panic!("The vulkan feature is not supported on macOS. Use the metal backend instead.");
        }
        let build_path =
            out_path
                .join("runtime_build")
                .join(if bare_metal { "bare_metal" } else { "hosted" });

        let sysroot = if msvc {
            None
        } else {
            let sysroot_output = cc::Build::new()
                .get_compiler()
                .to_command()
                .arg("--print-sysroot")
                .output()
                .expect("Failed to execute command");
            sysroot_output
                .status
                .success()
                .then(|| {
                    PathBuf::from(
                        String::from_utf8(sysroot_output.stdout)
                            .expect("Failed to parse sysroot")
                            .trim(),
                    )
                })
                .filter(|path| !path.as_os_str().is_empty())
        };
        // The bare-metal build process requires runtime tools: iree-flatcc-cli and
        // iree-c-embed-data, so there has to be a host tool build before the
        // actual runtime cross-build.
        let host_bin_dir = if bare_metal {
            let mut host_config = cmake::Config::new(&iree_path);

            // CMake config for host tool
            host_config.define("IREE_HAL_DRIVER_DEFAULTS", "OFF");
            RUNTIME_CMAKE_DEFINITIONS.iter().for_each(|(k, v)| {
                host_config.define(k, v);
            });

            // TODO: Change this once cmake-rs supports multiple targets
            host_config
                .target(&std::env::var("HOST").unwrap())
                .build_target("iree-flatcc-cli")
                .out_dir(build_path.join("host"));
            host_config.build();
            host_config.build_target("iree-c-embed-data").build();
            Some(build_path.join("host/build/tools"))
        } else {
            None
        };

        let mut config = cmake::Config::new(env!("CARGO_MANIFEST_DIR"));
        if msvc {
            // Keep archive paths independent of Visual Studio's multi-config
            // output directories; Ninja still uses the MSVC compiler and CRT.
            config.generator("Ninja");
            // Rust uses the release CRT even for debug builds. CMake otherwise
            // selects the debug CRT, causing LNK4098 when Rust links the archive.
            let static_crt = env::var("CARGO_CFG_TARGET_FEATURE")
                .unwrap_or_default()
                .split(',')
                .any(|feature| feature == "crt-static");
            config.static_crt(static_crt).define(
                "CMAKE_MSVC_RUNTIME_LIBRARY",
                if static_crt {
                    "MultiThreaded"
                } else {
                    "MultiThreadedDLL"
                },
            );
        }

        // CMake config for IREE runtime build
        let mut cmake_defs = RUNTIME_CMAKE_DEFINITIONS.to_vec();
        // Rust links this archive; native GCC/Clang LTO objects may use an
        // incompatible format or LLVM version. Optimize without C LTO.
        cmake_defs.push(("IREE_RUNTIME_OPTIMIZATION_PROFILE", ""));

        #[cfg(feature = "cuda")]
        cmake_defs.push(("IREE_HAL_DRIVER_CUDA", "ON"));
        #[cfg(feature = "vulkan")]
        cmake_defs.push(("IREE_HAL_DRIVER_VULKAN", "ON"));

        let mut cflags = vec![];

        if !msvc {
            match std::env::var("OPT_LEVEL").unwrap().as_str() {
                "z" => {
                    cflags.push("-Oz".to_string());
                }
                "3" => {
                    cflags.push("-O3".to_string());
                }
                "2" => {
                    cflags.push("-O2".to_string());
                }
                "1" => {
                    cflags.push("-O1".to_string());
                }
                "0" => {
                    cflags.push("-O0".to_string());
                }
                _ => {}
            }
        }

        // If bare metal, use the following config.
        if bare_metal {
            // CMake config for no-std runtime build
            cmake_defs.extend(vec![
                ("IREE_ENABLE_THREADING", "OFF"),
                ("IREE_HAL_DRIVER_DEFAULTS", "OFF"),
                ("IREE_HAL_DRIVER_LOCAL_SYNC", "OFF"),
                ("EERIE_BARE_METAL", "ON"),
                ("IREE_ENABLE_POSIX", "OFF"),
                ("CMAKE_C_EXTENSIONS", "ON"),
                ("IREE_HAL_EXECUTABLE_LOADER_DEFAULTS", "OFF"),
                ("IREE_HAL_EXECUTABLE_LOADER_EMBEDDED_ELF", "ON"),
                ("IREE_HAL_EXECUTABLE_LOADER_VMVX_MODULE", "OFF"),
                ("IREE_HAL_EXECUTABLE_PLUGIN_DEFAULTS", "OFF"),
                ("IREE_HAL_EXECUTABLE_PLUGIN_EMBEDDED_ELF", "ON"),
                ("IREE_ENABLE_POSITION_INDEPENDENT_CODE", "OFF"),
                (
                    "IREE_HOST_BIN_DIR",
                    host_bin_dir.as_ref().unwrap().to_str().unwrap(),
                ),
                ("CMAKE_SYSTEM_NAME", "Generic"),
                ("CMAKE_TRY_COMPILE_TARGET_TYPE", "STATIC_LIBRARY"),
            ]);
            // C flags for no-std runtime build
            cflags.push(format!("-I{}", bare_metal_sync_include_path.display()));
            cflags.extend(
                [
                    "-DIREE_PLATFORM_GENERIC=1",
                    "-DIREE_FILE_IO_ENABLE=0",
                    "-DIREE_TIME_NOW_FN=\"{return 0; }\"",
                    "-D'IREE_WAIT_UNTIL_FN(n)=false'",
                    "-DFLATCC_USE_GENERIC_ALIGNED_ALLOC",
                    "-DIREE_STATUS_FEATURES=0",
                    "-fdata-sections",
                    "-ffunction-sections",
                    "-fno-unwind-tables",
                    "-fno-asynchronous-unwind-tables",
                    "-Wno-char-subscripts",
                    "-Wno-format",
                    "-Wno-error=unused-variable",
                ]
                .into_iter()
                .map(String::from),
            );
        }

        cmake_defs.iter().for_each(|(k, v)| {
            config.define(k, v);
        });

        cflags.iter().for_each(|v| {
            config.cflag(v);
            config.cxxflag(v);
        });

        config.build_target("eerie_runtime").out_dir(&build_path);

        // Build IREE runtime
        config.build();

        // Public bytecode headers now include the generated ISA op table, so
        // generate Rust bindings after CMake has built the native library.
        let mut binding_includes = vec![
            build_path.join("build/iree/runtime/src"),
            iree_path.join("third_party/flatcc/include"),
        ];
        if bare_metal {
            binding_includes.push(bare_metal_sync_include_path);
            // GCC's header layout can differ from Clang's sysroot convention
            // (notably Debian's Arm packages). Query the compiler for the C
            // header roots instead of deriving a path from a runtime archive.
            let output = cc::Build::new()
                .get_compiler()
                .to_command()
                .env("LC_ALL", "C")
                .args(["-E", "-x", "c", "-v", "-"])
                .stdin(std::process::Stdio::null())
                .output()
                .expect("Failed to query embedded C header search paths");
            let mut in_search_list = false;
            for line in String::from_utf8_lossy(&output.stderr).lines() {
                let line = line.trim();
                if line == "#include <...> search starts here:" {
                    in_search_list = true;
                } else if line == "End of search list." {
                    break;
                } else if in_search_list {
                    let path = PathBuf::from(line);
                    // Let Clang use its own builtin headers, adding only the
                    // toolchain's standard C headers needed by IREE.
                    if path.join("stdlib.h").is_file() {
                        binding_includes.push(path);
                    }
                }
            }
        }
        generate_runtime_bindings(
            sysroot.as_ref(),
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime_api.h"),
            &iree_path.join("runtime/src"),
            &out_path.join("runtime.rs"),
            &binding_includes,
        );

        // The IREE runtime is compiled as static library, and it requires eerie_runtime,
        // flatcc_parsing, and platform-specific libraries. When cross-compiling, lld is
        // recommended.
        println!(
            "cargo:rustc-link-search={}",
            build_path.join("build").display()
        );
        println!(
            "cargo:rustc-link-search={}",
            build_path
                .join("build/iree/build_tools/third_party/flatcc")
                .display()
        );
        // Print order is important.
        println!("cargo:rustc-link-lib=static=eerie_runtime");
        println!("cargo:rustc-link-lib=flatcc_parsing");

        match target_os.as_str() {
            "linux" => {
                println!("cargo:rustc-link-lib=stdc++");
            }

            "macos" => {
                println!("cargo:rustc-link-lib=framework=Foundation");
                println!("cargo:rustc-link-lib=framework=Metal");
            }

            "windows" => {
                for library in [
                    "kernel32",
                    "advapi32",
                    "bcrypt",
                    "ws2_32",
                    "synchronization",
                    "dbghelp",
                ] {
                    println!("cargo:rustc-link-lib={library}");
                }
            }

            "none" => {
                // Rust's compiler_builtins supplies target ABI helpers without
                // pulling in libgcc's hosted unwinder or a C runtime.
            }
            _ => {
                panic!("Only Linux, macOS, Windows MSVC, and no-std targets are supported");
            }
        }
    }
}
