#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]
include!(concat!(
    env!("OUT_DIR"),
    "/compiler/iree/compiler/embedding_api.rs"
));
