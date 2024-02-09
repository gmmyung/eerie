use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn generate_bindings(
    target_arch: &str,
    headers: &[PathBuf],
    include_path: &Path,
    bindings_path: &Path,
) {
    for path in headers {
        let header_path = include_path.join(path);
        println!("cargo:rerun-if-changed={}", header_path.display());
        let out_path = bindings_path.join(path).with_extension("rs");
        if !out_path.parent().unwrap().exists() {
            std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
        }
        let mut builder = bindgen::Builder::default()
            .header(include_path.join(path).display().to_string())
            .clang_arg(format!("-I{}", include_path.display()))
            .derive_default(true)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

        // Include arm-non-eabi headers
        #[cfg(not(feature = "std"))]
        match target_arch {
            "arm" => {
                builder = builder
                    .clang_arg("-I/Applications/ARM/arm-none-eabi/include")
                    .clang_arg("--sysroot=/Applications/ARM/arm-none-eabi");
            }
            _ => {
                unimplemented!("Unsupported target arch: {}", target_arch)
            }
        }

        #[cfg(not(feature = "std"))]
        {
            builder = builder
                .use_core()
                .clang_arg("-DIREE_PLATFORM_GENERIC=1")
                .clang_arg("-Wno-char-subscripts")
                .clang_arg("-Wno-format")
                .clang_arg("-Wno-implicit-function-declaration")
                .clang_arg("-Wno-unused-variable")
                //.clang_arg(r#"-DIREE_TIME_NOW_FN="{ return 0; }""#)
                //.clang_arg(r#"-D'IREE_WAIT_UNTIL_FN(n)="{ return false; }"'"#)
                .clang_arg("-DIREE_SYNCHRONIZATION_DISABLE_UNSAFE=1")
                .clang_arg("-DFLATCC_USE_GENERIC_ALIGNED_ALLOC=1")
                .clang_arg("-DIREE_STATUS_FEATURES=0");
        }

        builder
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file(&out_path)
            .expect("Couldn't write bindings!");
    }
}

fn main() {
    let cargo_target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let cargo_target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    #[cfg(feature = "compiler")]
    {
        generate_bindings(
            &cargo_target_arch,
            &[PathBuf::from("iree")
                .join("compiler")
                .join("embedding_api.h")],
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("iree")
                .join("compiler")
                .join("bindings")
                .join("c"),
            &PathBuf::from(env::var("OUT_DIR").unwrap()).join("compiler"),
        );

        // IREECompiler lib location by environment variable
        let compiler_lib_path = PathBuf::from_str(&env::var("LIB_IREE_COMPILER").unwrap()).unwrap();
        println!("cargo:rustc-link-search={}", compiler_lib_path.display());

        println!("cargo:rustc-link-lib=dylib=IREECompiler");
    }

    #[cfg(feature = "runtime")]
    {
        let iree_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("iree");
        let build_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime_build");
        let mut config = cmake::Config::new(iree_path.clone());

        config
            .define("IREE_BUILD_COMPILER", "OFF")
            .define("IREE_BUILD_TESTS", "OFF")
            .define("IREE_BUILD_SAMPLES", "OFF")
            .define("IREE_BUILD_BINDINGS_TFLITE", "OFF")
            .define("IREE_BUILD_BINDINGS_TFLITE_JAVA", "OFF")
            .build_target("iree_runtime_unified")
            .out_dir(&build_path);

        #[cfg(feature = "std")]
        config
            .define("CMAKE_C_COMPILER", "clang")
            .define("CMAKE_CXX_COMPILER", "clang++");

        println!("host: {}", &std::env::var("HOST").unwrap());

        // If target is no-std, build first on host machine
        #[cfg(not(feature = "std"))]
        {
            let mut host_config = cmake::Config::new(iree_path.clone());
            host_config
                .define(
                    "CMAKE_INSTALL_PREFIX",
                    build_path.join("install").to_str().unwrap(),
                )
                .define("CMAKE_C_COMPILER", "clang")
                .define("CMAKE_CXX_COMPILER", "clang++")
                .define("IREE_HAL_DRIVER_DEFAULTS", "OFF")
                .define("IREE_BUILD_COMPILER", "OFF")
                .define("IREE_BUILD_TESTS", "OFF")
                .define("IREE_BUILD_SAMPLES", "OFF")
                .define("IREE_BUILD_BINDINGS_TFLITE", "OFF")
                .define("IREE_BUILD_BINDINGS_TFLITE_JAVA", "OFF")
                .target(&std::env::var("HOST").unwrap())
                .build_target("iree-flatcc-cli")
                .out_dir(&build_path.join("host"));

            host_config.build();

            host_config.build_target("generate_embed_data").build();
        }

        // if bare metal (no-std), use the following
        #[cfg(not(feature = "std"))]
        config
            .define(
                "CMAKE_TOOLCHAIN_FILE",
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("arm-none-eabi-toolchain.cmake"),
            )
            .define(
                "IREE_HOST_BIN_DIR",
                build_path.join("host/build/tools").to_str().unwrap(),
            )
            .define("IREE_ENABLE_THREADING", "OFF")
            .define("IREE_HAL_DRIVER_DEFAULTS", "OFF")
            .define("IREE_HAL_DRIVER_LOCAL_SYNC", "ON")
            .define("IREE_HAL_EXECUTABLE_LOADER_DEFAULTS", "OFF")
            .define("IREE_HAL_EXECUTABLE_LOADER_EMBEDDED_ELF", "ON")
            .define("IREE_HAL_EXECUTABLE_LOADER_VMVX_MODULE", "ON")
            .define("IREE_HAL_EXECUTABLE_PLUGIN_DEFAULTS", "OFF")
            .define("IREE_HAL_EXECUTABLE_PLUGIN_EMBEDDED_ELF", "ON");

        config.build();

        generate_bindings(
            &cargo_target_arch,
            &[PathBuf::from("iree").join("runtime").join("api.h")],
            &iree_path.join("runtime").join("src"),
            &PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime"),
        );

        match cargo_target_os.as_str() {
            "linux" => {
                println!(
                    "cargo:rustc-link-search={}",
                    build_path.join("build").join("lib").display()
                );
                println!("cargo:rustc-link-lib=iree");
                println!("cargo:rustc-link-lib=stdc++");
                println!(
                    "cargo:rustc-link-search={}",
                    build_path
                        .join("build")
                        .join("iree_core")
                        .join("build_tools")
                        .join("third_party")
                        .join("flatcc")
                        .display()
                );
                println!("cargo:rustc-link-lib=flatcc_parsing");
            }
            "none" => {
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

                println!("cargo:rustc-link-lib=iree_runtime_unified");
                println!("cargo:rustc-link-lib=flatcc_parsing");
                //println!("cargo:rustc-link-lib=nosys");
                //println!("cargo:rustc-link-lib=c");
                //println!("cargo:rustc-link-lib=m");
                //println!(
                //    "cargo:rustc-link-search={}",
                //    "/Applications/ARM/arm-none-eabi/lib/thumb/v7e-m+fp/hard"
                //);
            }
            "macos" => {
                println!(
                    "cargo:rustc-link-search={}",
                    build_path
                        .join("build")
                        .join("runtime/src/iree/runtime")
                        .display()
                );
                println!("cargo:rustc-link-lib=iree_runtime_unified");
                println!(
                    "cargo:rustc-link-search={}",
                    build_path
                        .join("build/build_tools/third_party/flatcc")
                        .display()
                );
                println!("cargo:rustc-link-lib=static=flatcc_parsing");
                println!("cargo:rustc-link-lib=framework=Foundation");
                println!("cargo:rustc-link-lib=framework=Metal");
            }
            _ => {
                unimplemented!("Unsupported target OS: {}", cargo_target_os)
            }
        }
    }
}
