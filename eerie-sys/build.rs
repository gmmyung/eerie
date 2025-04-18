use std::env;
use std::path::{Path, PathBuf};

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
                .clang_arg("-DIREE_SYNCHRONIZATION_DISABLE_UNSAFE=1")
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

fn main() {
    let iree_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("iree");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    #[cfg(all(target_os = "none", feature = "std"))]
    {
        compile_error!("The std feature cannot be used in a no-std environment");
    }

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
                .args(["install", "iree-compiler"])
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
        let build_path = out_path.join("runtime_build");

        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();

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
        let multi_dir_output = cc::Build::new()
            .get_compiler()
            .to_command()
            .arg("--print-multi-directory")
            .output()
            .expect("Failed to execute command");
        let multi_dir: Option<PathBuf> = match multi_dir_output.status.success() {
            true => Some(
                String::from_utf8(multi_dir_output.stdout)
                    .expect("Failed to parse multi-dir")
                    .trim()
                    .into(),
            ),
            false => None,
        };
        generate_bindings(
            sysroot.as_ref(),
            &[PathBuf::from("iree").join("runtime").join("api.h")],
            &iree_path.join("runtime").join("src"),
            &PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime"),
        );

        // The build process requires runtime tools: iree-flatcc-cli and generate_embed_data
        // So there has to be a host tool build before the actual runtime build in no-std builds.
        #[cfg(not(feature = "std"))]
        {
            let mut host_config = cmake::Config::new(&iree_path);

            // CMake config for host tool
            let cmake_host_defs = vec![
                ("IREE_HAL_DRIVER_DEFAULTS", "OFF"),
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
                .out_dir(&build_path.join("host"));
            host_config.build();
            host_config.build_target("generate_embed_data").build();
        }
        #[cfg(not(feature = "std"))]
        let host_bin_dir = build_path.join("host/build/tools");

        let mut config = cmake::Config::new(iree_path);

        // CMake config for IREE runtime build
        let mut cmake_defs = vec![
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
                cflags.push("-Oz");
                cmake_defs.push(("IREE_SIZE_OPTIMIZED", "ON"));
            }
            "3" => {
                cflags.push("-O3");
            }
            "2" => {
                cflags.push("-O2");
            }
            "1" => {
                cflags.push("-O1");
            }
            "0" => {
                cflags.push("-O0");
            }
            _ => {}
        }

        // If bare metal (no-std), use the following config.
        #[cfg(not(feature = "std"))]
        {
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
                ("IREE_HOST_BIN_DIR", host_bin_dir.to_str().unwrap()),
                ("CMAKE_SYSTEM_NAME", "Generic"),
            ]);
            // C flags for no-std runtime build
            cflags.extend(vec![
                "-specs=nosys.specs",
                "-DIREE_PLATFORM_GENERIC=1",
                "-DIREE_FILE_IO_ENABLE=0",
                "-DIREE_SYNCHRONIZATION_DISABLE_UNSAFE=1",
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
            ]);
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

        // Print order is important.
        println!("cargo:rustc-link-lib=iree_runtime_unified");
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
                println!(
                    "cargo:rustc-link-search={}/lib/{}",
                    sysroot.unwrap().display(),
                    multi_dir.unwrap().display()
                );
                println!("cargo:rustc-link-lib=nosys");
                println!("cargo:rustc-link-lib=c");
                println!("cargo:rustc-link-lib=m");
            }
            _ => {
                panic!("Only Linux, macOS, and no-std targets are supported");
            }
        }
    }
}
