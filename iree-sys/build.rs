use std::env;
use std::path::PathBuf;
use std::process::Command;

const IREE_BUILD_PATH: &str = "/Users/gmmyung/Developer/iree-build";
const IREE_INCLUDE_PATH: &str = "/Users/gmmyung/Developer/iree/compiler/bindings/c";

fn main() {
    let iree_compiler_lib_path = PathBuf::from(IREE_BUILD_PATH)
        .join("lib")
        .join("libIREECompiler.0.dylib");

    // Set the library ID to the path of the library.
    // Refer to https://stackoverflow.com/questions/35220111/install-name-tool-difference-between-change-and-id
    Command::new("install_name_tool")
        .arg("-id")
        .arg(iree_compiler_lib_path.display().to_string())
        .arg(iree_compiler_lib_path.display().to_string())
        .spawn()
        .expect("Failed to set the library ID");
    
    let iree_compiler_include_path = PathBuf::from(IREE_INCLUDE_PATH)
        .join("iree")
        .join("compiler")
        .join("embedding_api.h");

    println!("cargo:rustc-link-search=native={}", PathBuf::from(IREE_BUILD_PATH).join("lib").display());
    println!("cargo:rustc-link-lib=IREECompiler");
    println!("cargo:rerun-if-changed={}", iree_compiler_include_path.display().to_string()); 
    println!("cargo:rerun-if-changed={}", iree_compiler_lib_path.display().to_string());

    let bindings = bindgen::Builder::default()
        .header(iree_compiler_include_path.display().to_string())
        .clang_arg(format!("-I{}", IREE_INCLUDE_PATH))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
