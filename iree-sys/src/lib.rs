#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(feature = "compiler")]
pub mod compiler;
#[cfg(feature = "runtime")]
pub mod runtime;
#[cfg(not(feature = "std"))]
extern crate externc_libm;
//#[cfg(not(feature = "std"))]
//extern crate compiler_builtins;
#[cfg(not(feature = "std"))]
extern crate tinyrlibc;
#[no_mangle]
pub extern "C" fn _sbrk() {}

#[no_mangle]
pub extern "C" fn _write() {}

#[no_mangle]
pub extern "C" fn _close() {}

#[no_mangle]
pub extern "C" fn _lseek() {}

#[no_mangle]
pub extern "C" fn _read() {}

#[no_mangle]
pub extern "C" fn _fstat() {}

#[no_mangle]
pub extern "C" fn _isatty() {}

#[no_mangle]
pub extern "C" fn _exit() {}

#[no_mangle]
pub extern "C" fn _open() {}

#[no_mangle]
pub extern "C" fn _kill() {}

#[no_mangle]
pub extern "C" fn _getpid() {}

#[no_mangle]
pub extern "C" fn _fini() {}
