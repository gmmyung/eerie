#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]
// Bindgen uses integer transmutes in generated bitfield accessors.
#![allow(unnecessary_transmutes)]
// Upstream Doxygen comments include constructs such as @param[in,out].
#![allow(rustdoc::broken_intra_doc_links)]
use core::debug_assert;
use core::prelude::rust_2021::*;

include!(concat!(env!("OUT_DIR"), "/runtime.rs"));
