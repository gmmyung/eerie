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
