//! Exercise separate MLIR implementations in a single process.
use eerie::compiler::Compiler;
use melior::{
    dialect::DialectRegistry,
    ir::{operation::OperationLike, Module},
    utility::register_all_dialects,
    Context,
};

fn main() {
    let order = std::env::args().nth(1).expect("iree-first or melior-first");
    assert!(matches!(order.as_str(), "iree-first" | "melior-first"));
    let mut compiler = (order == "iree-first").then(|| Compiler::new().unwrap());
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);
    let context = Context::new();
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();
    let module = Module::parse(&context, include_str!("../../mul.mlir")).unwrap();
    {
        let compiler = compiler.take().unwrap_or_else(|| Compiler::new().unwrap());
        for _ in 0..3 {
            // Transfer text, never MLIR handles owned by the other implementation.
            let text = module.as_operation().to_string();
            let vmfb = compiler
                .compile_str(&text, ["--iree-hal-target-backends=vmvx"])
                .unwrap();
            assert!(!vmfb.is_empty());
            assert!(module.as_operation().verify());
        }
    }
    assert!(module.as_operation().verify());
    drop(module);
    drop(context);
    let compiler = Compiler::new().unwrap();
    assert!(!compiler
        .compile_str(
            include_str!("../../mul.mlir"),
            ["--iree-hal-target-backends=vmvx"]
        )
        .unwrap()
        .is_empty());
    println!("Melior coexistence passed ({order})");
}
