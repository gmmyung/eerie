use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

const IREE_BUILD_PATH: &str = "/Users/gmmyung/Developer/iree-build";

fn main() {
    #[cfg(all(feature = "from-python", feature = "from-source"))]
    {
        panic!("Cannot build with both `from-python` and `from-source` features enabled, disable one of them");
    }

    let iree_include_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("iree")
        .join("compiler")
        .join("bindings")
        .join("c");
    println!(
        "cargo:rerun-if-changed={}",
        iree_include_path.to_str().unwrap()
    );

    let bindings = bindgen::Builder::default()
        .header(
            iree_include_path
                .join("iree")
                .join("compiler")
                .join("embedding_api.h")
                .display()
                .to_string(),
        )
        .clang_arg(format!("-I{}", iree_include_path.display()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    println!(
        "cargo:rustc-link-search=native={}",
        PathBuf::from(IREE_BUILD_PATH).join("lib").display()
    );


    // Fetch Shared Library Path
    let iree_lib_path;
    #[cfg(feature = "from-python")]
    {
        iree_lib_path = PathBuf::from_str(
            &String::from_utf8(
                Command::new("python3")
                    .arg("-c")
                    .arg(r#""import iree.compiler as _; print(_.__path__[0])""#)
                    .output()
                    .expect("Failed to fetch the shared library path")
                    .stdout,
            )
            .unwrap(),
        )
        .unwrap()
        .join("_mlir_libs");
    }
    #[cfg(feature = "from-source")]
    {
        iree_lib_path = env::var("IREE_LIB_PATH")
            .map(PathBuf::from)
            .expect("Failed to fetch the shared library path");
    }
    
    println!("cargo:rustc-link-lib=IREECompiler");
    println!("cargo:rustc-link-search=native={}", iree_lib_path.display());
}
