#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]
#[cfg(all(feature = "std", feature = "compiler"))]
pub mod compiler;
#[cfg(feature = "runtime")]
pub mod runtime;
