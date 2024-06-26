use thiserror::Error;
extern crate alloc;
use super::base;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("IREE runtime error: {0}")]
    StatusError(#[from] base::status::StatusError),
    #[error("Iree runtime error: Module index out of bounds")]
    OutOfBounds(usize),
}
