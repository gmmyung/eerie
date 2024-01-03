use std::env;
use std::path::PathBuf;
use std::str::FromStr;

fn generate_bindings(headers: &[PathBuf], include_path: &PathBuf, bindings_path: &PathBuf) {
    for path in headers {
        let header_path = include_path.join(path);
        println!("cargo:rerun-if-changed={}", header_path.display());
        let out_path = bindings_path.join(path).with_extension("rs");
        if !out_path.parent().unwrap().exists() {
            std::fs::create_dir_all(&out_path.parent().unwrap()).unwrap();
        }
        bindgen::Builder::default()
            .header(include_path.join(path).display().to_string())
            .clang_arg(format!("-I{}", include_path.display()))
            .derive_default(true)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file(&out_path)
            .expect("Couldn't write bindings!");
    }
}

fn main() {
    #[cfg(feature = "compiler")]
    {
        generate_bindings(
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

    #[cfg(feature="runtime")]
    {
        // Build IREE Runtime
        let runtime_lib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("iree-samples")
            .join("runtime-library");

        let build_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime_build");
            
        let mut config = cmake::Config::new(runtime_lib_path);
        
        let var = env::var("PATH").unwrap();
        let mut paths = env::split_paths(&var);
        let mut find_program = |name: &str| {
            paths.find(|path| path.join(name).exists())
        };

        config.define("BUILD_SHARED_LIBS", "OFF")
            .define("IREERT_ENABLE_LTO", "ON")
            .define("IREE_ROOT_DIR", PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("iree").to_str().unwrap())
            .define("CMAKE_C_COMPILER", find_program("clang").unwrap().join("clang"))
            .define("CMAKE_CXX_COMPILER", find_program("clang++").unwrap().join("clang++"));

        // if bare metal (no-std), use the following
        #[cfg(not(feature = "std"))]
        config.define("CMAKE_SYSTEM_NAME", "Generic")
            .define("IREE_BUILD_COMPILER", "OFF")
            .define("IREE_ENABLE_THREADING", "OFF")
            .define("IREE_HAL_DRIVER_DEFAULTS", "OFF")
            .define("IREE_HAL_DRIVER_LOCAL_SYNC", "ON")
            .define("IREE_HAL_EXECUTABLE_LOADER_DEFAULTS", "OFF")
            .define("IREE_HAL_EXECUTABLE_LOADER_EMBEDDED_ELF", "ON")
            .define("IREE_HAL_EXECUTABLE_LOADER_VMVX_MODULE", "ON")
            .define("IREE_HAL_EXECUTABLE_PLUGIN_DEFAULTS", "OFF")
            .define("IREE_HAL_EXECUTABLE_PLUGIN_EMBEDDED_ELF", "ON")
            .define("IREE_BUILD_TESTS", "OFF")
            .define("IREE_BUILD_SAMPLES", "ON");

        #[cfg(not(feature = "std"))]
        println!("THIS IS NO_STD");

        config.out_dir(&build_path)
            .build();

        generate_bindings(&[
            PathBuf::from("iree")
                .join("runtime")
                .join("api.h"),
        ], &build_path.join("build").join("include"), &PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime"));
        
        
        #[cfg(target_os = "linux")]
        {
            println!("cargo:rustc-link-search={}", build_path.join("build").join("lib").display()); 
            println!("cargo:rustc-link-lib=iree");
            println!("cargo:rustc-link-lib=stdc++");
            //println!("cargo:rustc-link-search={}", build_path.join("build").join("iree_core").join("third_party").join("cpuinfo").display());
            //println!("cargo:rustc-link-lib=cpuinfo");
            println!("cargo:rustc-link-search={}", build_path.join("build").join("iree_core").join("build_tools").join("third_party").join("flatcc").display());
            println!("cargo:rustc-link-lib=flatcc_parsing");
        }

        #[cfg(target_os = "macos")]
        {
            println!("cargo:rustc-link-search=framework={}", build_path.join("build").join("lib").display()); 
            println!("cargo:rustc-link-lib=framework=iree");
            //println!("cargo:rustc-link-search={}", build_path.join("build").join("iree_core").join("third_party").join("cpuinfo").display());
            //println!("cargo:rustc-link-lib=static=cpuinfo");
            println!("cargo:rustc-link-search={}", build_path.join("build").join("iree_core").join("build_tools").join("third_party").join("flatcc").display());
            println!("cargo:rustc-link-lib=static=flatcc_parsing");
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=Metal");
        }
    }
}
