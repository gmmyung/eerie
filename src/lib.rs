#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
#[cfg(all(target_os = "none", not(feature = "std"), feature = "runtime"))]
extern crate tinyrlibc as _;

#[cfg(all(target_os = "none", not(feature = "std"), feature = "runtime"))]
mod c_abi;

#[cfg(all(feature = "std", feature = "compiler"))]
pub mod compiler;
#[cfg(feature = "runtime")]
pub mod runtime;
