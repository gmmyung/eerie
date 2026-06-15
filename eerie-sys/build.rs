use std::env;
use std::path::{Path, PathBuf};

#[cfg(feature = "compiler")]
fn generate_bindings(
    sysroot: Option<&PathBuf>,
    headers: &[PathBuf],
    include_path: &Path,
    bindings_path: &Path,
) {
    for path in headers {
        let header_path = include_path.join(path);
        println!("cargo:rerun-if-changed={}", header_path.display());
        // Generate binding file name is the same as the header file
        let out_path = bindings_path.join(path).with_extension("rs");
        // Create the parent directory if it doesn't exist
        if !out_path.parent().unwrap().exists() {
            std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
        }
        let mut builder = bindgen::Builder::default()
            .header(include_path.join(path).display().to_string())
            .clang_arg(format!("-I{}", include_path.display()))
            .derive_default(true)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

        #[cfg(not(feature = "std"))]
        {
            builder = builder
                .use_core()
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
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file(&out_path)
            .expect("Couldn't write bindings!");
    }
}

fn generate_binding(
    sysroot: Option<&PathBuf>,
    header_path: &Path,
    include_path: &Path,
    bindings_path: &Path,
    additional_include_paths: &[PathBuf],
) {
    println!("cargo:rerun-if-changed={}", header_path.display());
    let mut builder = bindgen::Builder::default()
        .header(header_path.display().to_string())
        .clang_arg(format!("-I{}", include_path.display()))
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    #[cfg(not(feature = "std"))]
    {
        builder = builder
            .use_core()
            .clang_arg("-DIREE_PLATFORM_GENERIC=1")
            .clang_arg("-Wno-char-subscripts")
            .clang_arg("-Wno-format")
            .clang_arg("-Wno-implicit-function-declaration")
            .clang_arg("-Wno-unused-variable")
            .clang_arg("-DFLATCC_USE_GENERIC_ALIGNED_ALLOC=1")
    }

    for additional_include_path in additional_include_paths {
        println!(
            "cargo:rerun-if-changed={}",
            additional_include_path.display()
        );
        builder = builder.clang_arg(format!("-I{}", additional_include_path.display()));
    }

    if let Some(sysroot) = sysroot {
        builder = builder.clang_arg(format!("--sysroot={}", sysroot.display()));
    }

    builder
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(bindings_path)
        .expect("Couldn't write bindings!");
}

fn main() {
    let iree_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("iree");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bare_metal_sync_include_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bare_metal_sync/include");

    // The compiler feature enables the IREE compiler features. The IREE compiler binaries should
    // be downloaded from an external source such as pypi, and the downloaded binaries should be
    // exported as environment variables. See README.md for more details.
    #[cfg(feature = "compiler")]
    {
        // The compiler feature cannot be used in a no-std environment
        #[cfg(not(feature = "std"))]
        {
            panic!("The compiler feature cannot be used in a no-std environment");
        }

        generate_bindings(
            None,
            // Path to compiler headers
            &[PathBuf::from("iree/compiler/embedding_api.h")],
            // Path to IREE compiler header sources
            &iree_path.join("compiler/bindings/c"),
            // Path to generated compiler bindings
            &out_path.join("compiler"),
        );

        let compiler_lib_path = if std::env::var("DOCS_RS").is_ok() {
            // Docs.rs automatically downloads the IREE compiler from pypi
            // and sets the LIB_IREE_COMPILER environment variable

            std::process::Command::new("pip3")
                .args(["install", "iree-base-compiler==3.11.0"])
                .status()
                .map_err(|e| format!("Failed to install IREE compiler: {}", e))
                .unwrap();

            // Find the IREE compiler library
            std::str::from_utf8(
                &std::process::Command::new(out_path.join("python3"))
                    .args([
                        "-c",
                        "import iree.compiler as _; print(f'{_.__path__[0]}/_mlir_libs/')",
                    ])
                    .output()
                    .expect("Failed to find IREE compiler library")
                    .stdout,
            )
            .unwrap()
            .to_string()
        } else {
            // The user can set the LIB_IREE_COMPILER environment variable
            env::var("LIB_IREE_COMPILER").expect(
				"The LIB_IREE_COMPILER environment variable must be set to the path to the IREE compiler library")
        };
        // The linker needs to find the IREE compiler dynamic library
        println!("cargo:rustc-link-search={}", compiler_lib_path);
        println!("cargo:rustc-link-lib=dylib=IREECompiler");
    }

    // The runtime feature enables the IREE runtime. It configures available runtime backends based
    // on the target os and architecture. The IREE runtime is built from source using CMake and
    // clang/llvm. Other compilers can be used as well, but clang is recommended when cross-compiling.
    #[cfg(feature = "runtime")]
    {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
        let bare_metal = target_os == "none";
        if bare_metal && cfg!(feature = "std") {
            panic!("The std feature cannot be used for target_os = \"none\"");
        }
        let build_path =
            out_path
                .join("runtime_build")
                .join(if bare_metal { "bare_metal" } else { "hosted" });

        let sysroot_output = cc::Build::new()
            .get_compiler()
            .to_command()
            .arg("--print-sysroot")
            .output()
            .expect("Failed to execute command");
        let sysroot: Option<PathBuf> = match sysroot_output.status.success() {
            true => Some(
                String::from_utf8(sysroot_output.stdout)
                    .expect("Failed to parse sysroot")
                    .trim()
                    .into(),
            ),
            false => None,
        };
        generate_binding(
            sysroot.as_ref(),
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime_api.h"),
            &iree_path.join("runtime").join("src"),
            &PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime.rs"),
            if bare_metal {
                std::slice::from_ref(&bare_metal_sync_include_path)
            } else {
                &[]
            },
        );

        // The bare-metal build process requires runtime tools: iree-flatcc-cli and
        // iree-c-embed-data, so there has to be a host tool build before the
        // actual runtime cross-build.
        let host_bin_dir = if bare_metal {
            let mut host_config = cmake::Config::new(&iree_path);

            // CMake config for host tool
            let cmake_host_defs = [
                ("IREE_HAL_DRIVER_DEFAULTS", "OFF"),
                ("IREE_ENABLE_LIBBACKTRACE", "OFF"),
                ("IREE_BUILD_COMPILER", "OFF"),
                ("IREE_BUILD_TESTS", "OFF"),
                ("IREE_BUILD_SAMPLES", "OFF"),
                ("IREE_BUILD_BINDINGS_TFLITE", "OFF"),
                ("IREE_BUILD_BINDINGS_TFLITE_JAVA", "OFF"),
            ];
            cmake_host_defs.iter().for_each(|(k, v)| {
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

        let mut config = cmake::Config::new(iree_path);

        // CMake config for IREE runtime build
        let mut cmake_defs = vec![
            ("IREE_ENABLE_LIBBACKTRACE", "OFF"),
            ("IREE_BUILD_COMPILER", "OFF"),
            ("IREE_BUILD_TESTS", "OFF"),
            ("IREE_BUILD_SAMPLES", "OFF"),
            ("IREE_BUILD_BINDINGS_TFLITE", "OFF"),
            ("IREE_BUILD_BINDINGS_TFLITE_JAVA", "OFF"),
        ];

        #[cfg(feature = "cuda")]
        cmake_defs.push(("IREE_HAL_DRIVER_CUDA", "ON"));

        let mut cflags = vec![];

        match std::env::var("OPT_LEVEL").unwrap().as_str() {
            "z" => {
                cflags.push("-Oz".to_string());
                cmake_defs.push(("IREE_SIZE_OPTIMIZED", "ON"));
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

        // If bare metal, use the following config.
        if bare_metal {
            // CMake config for no-std runtime build
            cmake_defs.extend(vec![
                ("IREE_ENABLE_THREADING", "OFF"),
                ("IREE_HAL_DRIVER_DEFAULTS", "OFF"),
                ("IREE_HAL_DRIVER_LOCAL_SYNC", "ON"),
                ("IREE_HAL_EXECUTABLE_LOADER_DEFAULTS", "OFF"),
                ("IREE_HAL_EXECUTABLE_LOADER_EMBEDDED_ELF", "ON"),
                ("IREE_HAL_EXECUTABLE_LOADER_VMVX_MODULE", "ON"),
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
                    "-Wno-char-subscripts",
                    "-Wno-format",
                    "-Wno-error=unused-variable",
                    "-Wl,--gc-sections",
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

        config
            .build_target("iree_runtime_unified")
            .out_dir(&build_path);

        // Build IREE runtime
        config.build();

        // The IREE runtime is compiled as static library, and it requires iree_runtime_unified,
        // flatcc_parsing, and platform-specific libraries. When cross-compiling, lld is
        // recommended.
        println!(
            "cargo:rustc-link-search={}",
            build_path.join("build/runtime/src/iree/runtime").display()
        );
        println!(
            "cargo:rustc-link-search={}",
            build_path
                .join("build/build_tools/third_party/flatcc")
                .display()
        );
        println!(
            "cargo:rustc-link-search={}",
            build_path
                .join("build/build_tools/third_party/printf")
                .display()
        );

        // Print order is important.
        println!("cargo:rustc-link-lib=iree_runtime_unified");
        println!("cargo:rustc-link-lib=printf_printf");
        println!("cargo:rustc-link-lib=flatcc_parsing");

        match target_os.as_str() {
            "linux" => {
                println!("cargo:rustc-link-lib=stdc++");
            }

            "macos" => {
                println!("cargo:rustc-link-lib=framework=Foundation");
                println!("cargo:rustc-link-lib=framework=Metal");
            }

            "none" => {
                println!("cargo:rustc-link-lib=gcc");
            }
            _ => {
                panic!("Only Linux, macOS, and no-std targets are supported");
            }
        }
    }
}
