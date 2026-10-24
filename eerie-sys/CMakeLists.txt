cmake_minimum_required(VERSION 3.26)
project(eerie_runtime LANGUAGES C CXX)

add_subdirectory(iree)

# Include inline HAL modules in the same archive as the hosted runtime. Generic
# targets deliberately omit the full runtime and its proactor/driver dependencies.
set(IREE_PACKAGE_ROOT_DIR "${CMAKE_CURRENT_SOURCE_DIR}")
set(IREE_PACKAGE_ROOT_PREFIX eerie)
set(_EERIE_RUNTIME_DEPS
  iree::modules::hal::inline
  iree::modules::hal::loader
  iree::hal::local::loaders::embedded_elf_loader
  iree::vm::bytecode::module
)
if(NOT EERIE_BARE_METAL)
  list(APPEND _EERIE_RUNTIME_DEPS iree::runtime::impl)
endif()
iree_cc_library(NAME runtime_impl DEPS ${_EERIE_RUNTIME_DEPS} PUBLIC)
iree_cc_unified_library(NAME runtime ROOT ::runtime_impl)

if(MSVC AND (MSVC_C_ARCHITECTURE_ID MATCHES "64" OR MSVC_CXX_ARCHITECTURE_ID MATCHES "64"))
  # Upstream adds this generated MASM object to its static archive after
  # declaring the object library. Unified archives collect the object library
  # and therefore need the Windows-to-System-V calling bridges explicitly.
  set(_EERIE_ELF_THUNKS "${CMAKE_CURRENT_BINARY_DIR}/iree/runtime/src/iree/hal/local/elf/arch/x86_64_msvc.obj")
  set_source_files_properties("${_EERIE_ELF_THUNKS}" PROPERTIES GENERATED TRUE EXTERNAL_OBJECT TRUE)
  target_sources(eerie_runtime PRIVATE "${_EERIE_ELF_THUNKS}")
  add_dependencies(eerie_runtime iree_hal_local_elf_arch)
endif()
