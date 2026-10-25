fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // Keep Melior's LLVM/MLIR definitions private to this executable.
        println!("cargo:rustc-link-arg-bins=-Wl,-no_exported_symbols");
    }
}
