#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(not(feature = "std"), feature = "compiler"))]
compile_error!("feature `std` is required for `compiler`");

#[cfg(all(feature = "std", feature = "compiler"))]
pub mod compiler;
#[cfg(feature = "runtime")]
pub mod runtime;
