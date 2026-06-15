# Changelog

## 0.3.0 - 2026-06-15

Pinned IREE version:

- IREE submodule: `v3.11.0`
- IREE submodule commit: `e4a3b0405d7d23554da26403658d0e8c3c5ecf25`
- Compiler wheel used by CI/docs.rs fallback: `iree-base-compiler==3.11.0`

### Changed

- Reworked the runtime bindings around lower-level VM/HAL objects and removed the old high-level runtime API wrapper.
- Moved runtime objects to retained/released ownership where the IREE C API supports reference counting, reducing unnecessary Rust lifetime coupling.
- Expanded HAL and VM coverage, including first-class buffer view/tensor handling, device queries, bytecode modules, function invocation, list handling, and additional reflection helpers.
- Updated examples and tests to use the new runtime API directly.
- Added a Nix flake/devshell for local development.

### Added

- Added embedded `no_std` runtime support for `target_os = "none"`.
- Added a Rust-backed C ABI support path using `tinyrlibc`, `libm` wrappers, compiler builtins, and a small critical-section-backed synchronization shim.
- Added local bare-metal `pthread.h`/`thread.h` compatibility headers for IREE's generic synchronization fallback.
- Added tests for newly exposed compiler/runtime API paths.
- Added a checked-in VMVX test fixture for runtime tests.

### Fixed

- Pinned CI to `iree-base-compiler==3.11.0` so the linked `libIREECompiler` matches the generated bindings.
- Updated GitHub Actions checkout to `actions/checkout@v6.0.3` for Node 24 compatibility.
- Removed obsolete no-std `_fini` and `end` linker stubs after verifying the Rust-based embedded path no longer needs them.

### Notes

- Hosted runtime builds use IREE's default runtime HAL driver selection for the host platform.
- Bare-metal runtime builds disable IREE thread creation, file I/O, and blocking wait support, while keeping synchronization enabled through the local critical-section shim.
