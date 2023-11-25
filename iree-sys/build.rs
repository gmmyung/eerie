use std::env;
use std::path::PathBuf;
use std::process::Command;
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
    #[cfg(all(feature = "from-python", feature = "from-source"))]
    {
        panic!("Cannot build with both `from-python` and `from-source` features enabled, disable one of them");
    }

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

        // Fetch Compiler Shared Library Path
        let compiler_lib_path;
        #[cfg(feature = "from-python")]
        {
            let mut process = Command::new("python3")
                .arg("-c")
                .arg("import iree.compiler as _; print(_.__path__[0])")
                .output()
                .expect("Failed to execute python3")
                .stdout;
            process.pop();

            compiler_lib_path = PathBuf::from_str(&String::from_utf8(process).unwrap())
                .unwrap()
                .join("_mlir_libs");
        }
        #[cfg(feature = "from-source")]
        {
            iree_lib_path = env::var("IREE_LIB_PATH")
                .map(PathBuf::from)
                .expect("Failed to fetch the shared library path");
        }
        println!(
            "cargo:rustc-link-search={}",
            compiler_lib_path.display()
        );
        println!("cargo:rustc-link-lib=dylib=IREECompiler");

        // Should be removed when the issue is fixed
        // https://github.com/rust-lang/cargo/issues/5077
        #[cfg(target_os = "macos")]
        {
            Command::new("install_name_tool")
                .arg("-id")
                .arg(
                    compiler_lib_path
                        .join("libIREECompiler.dylib")
                        .display()
                        .to_string(),
                )
                .arg(
                    compiler_lib_path
                        .join("libIREECompiler.dylib")
                        .display()
                        .to_string(),
                )
                .output()
                .expect("Failed to execute install_name_tool");
        }
    }

    #[cfg(feature="runtime")]
    {
        // Build IREE Runtime
        let runtime_lib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("iree-samples")
            .join("runtime-library");

        let build_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime_build");
            
        cmake::Config::new(runtime_lib_path)
            .define("BUILD_SHARED_LIBS", "OFF")
            .define("IREERT_ENABLE_LTO", "ON")
            .define("IREE_ROOT_DIR", PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("iree").to_str().unwrap())
            .out_dir(&build_path)
            .build();

        generate_bindings(&[
            PathBuf::from("iree")
                .join("runtime")
                .join("api.h"),
        ], &build_path.join("build").join("include"), &PathBuf::from(env::var("OUT_DIR").unwrap()).join("runtime"));
        
        
        println!("cargo:rustc-link-search={}", build_path.join("build").join("iree_core").join("third_party").join("cpuinfo").display());
        println!("cargo:rustc-link-search={}", build_path.join("build").join("iree_core").join("build_tools").join("third_party").join("flatcc").display());
        println!("cargo:rustc-link-search={}", build_path.join("build").join("lib").display());
        
        println!("cargo:rustc-link-lib=static=cpuinfo");
        println!("cargo:rustc-link-lib=static=flatcc_parsing");
        println!("cargo:rustc-link-lib=static=iree");

        #[cfg(target_os = "linux")]
        println!("cargo:rustc-link-lib=stdc++");

        
        #[cfg(target_os = "macos")]
        {
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=Metal");
        }
    }
}
