#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(feature = "compiler")]
pub mod compiler;

#[cfg(feature = "runtime")]
pub mod runtime;

#[cfg(all(target_os = "none", not(feature = "std"), feature = "runtime"))]
mod bare_metal_sync;
