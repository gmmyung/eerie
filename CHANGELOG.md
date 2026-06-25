# Changelog

## 0.5.0 - 2026-06-26

### Added

- Added `runtime::DeviceSpec`, `runtime::Driver`, and `runtime::DeviceInfo` for explicit runtime device selection and device queries.
- Added optional runtime `f16` and `bf16` buffer view support behind the `half` feature.

### Changed

- Reworked the safe runtime surface around `Runtime -> Program -> Function` so users load VMFB modules, resolve functions, pass typed `BufferView<T>` inputs, and receive dynamically typed `Value` outputs.
- Moved low-level VM/HAL assembly APIs behind the crate-private runtime implementation; public runtime usage now goes through the safe high-level API.
- Use a retained process-global VM instance for hosted runtime setup to avoid concurrent HAL type registration hazards in IREE's low-level initialization path.
- Treat macOS Vulkan as unsupported in eerie; use the Metal backend on macOS.

### Notes

- Published `eerie-sys` `0.3.3` with the new optional `vulkan` feature.

## 0.4.3 - 2026-06-22

### Added

- Added safe `BufferView<bool>` support for IREE Bool8 tensors, including host buffer creation, writes, reads, read mappings, VM ref extraction, and runtime invocation paths.
- Added Bool8 validation when reading bool buffer contents so invalid byte values cannot become invalid Rust `bool` values.

### Notes

- Raw-compatible numeric buffer element types keep their existing direct host transfer and zero-copy read mapping paths.
- `eerie-sys` remains at `0.3.2`; Bool8 constants were already available in the generated raw bindings.

## 0.4.1 - 2026-06-21

### Fixed

- Map Rust signed integer `BufferView<T>` element types to IREE signless integer HAL element types (`Int8`/`Int16`/`Int32`/`Int64`) so MLIR `i*` tensor ABIs round-trip correctly through typed buffer views.

### Notes

- `eerie-sys` remains at `0.3.2`; this patch only changes the safe wrapper crate.

## 0.4.0 - 2026-06-18

### Changed

- Removed the redundant `runtime::hal::Tensor<T>` wrapper. Use `runtime::hal::BufferView<T>` directly for typed HAL buffer views.
- Removed `runtime::vm::Function::invoke_tensors`. Build VM `List` arguments explicitly with `BufferView<T>::to_ref`, call `Function::invoke`, then read `Ref<BufferView<T>>` outputs.
- Updated runtime tests, README snippets, and examples to use `BufferView<T>` as the single typed runtime tensor/buffer abstraction.

### Notes

- `eerie-sys` remains at `0.3.2`; this release only changes the safe wrapper crate API.

## 0.3.3 - 2026-06-15

### Fixed

- Published `eerie-sys` `0.3.2` with IREE libbacktrace disabled in runtime CMake builds so docs.rs can build `std + runtime` documentation offline.
- Updated `eerie` to depend on `eerie-sys` `0.3.2`.

## 0.3.2 - 2026-06-15

### Fixed

- Fixed HAL VM ref type races when multiple VM instances are created or invoked in the same process.
- Rebound IREE's process-global HAL type adapters to the active VM instance around HAL module creation and invocation.
- Removed runtime test serialization and added stale-instance/concurrent-invocation regression coverage.

### Notes

- `eerie-sys` remains at `0.3.1`; this patch only changes the safe wrapper crate.

## 0.3.1 - 2026-06-15

### Fixed

- Updated docs.rs metadata to build documentation with the `runtime` feature enabled while keeping the `compiler` feature disabled.

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
