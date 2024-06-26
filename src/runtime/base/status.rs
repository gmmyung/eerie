use super::allocator;
use eerie_sys::runtime as sys;

/// IREE runtime status
pub struct Status {
    pub(crate) ctx: sys::iree_status_t,
}

impl From<sys::iree_status_t> for Status {
    fn from(value: sys::iree_status_t) -> Self {
        Self { ctx: value }
    }
}

impl Status {
    pub(crate) fn from_code(status_kind: StatusErrorKind) -> Self {
        let status: sys::iree_status_code_e = status_kind.into();
        Status {
            ctx: &STATUS_CODES[status as usize] as *const usize as *mut usize as *mut _,
        }
    }

    pub(crate) fn is_ok(&self) -> bool {
        self.ctx as usize == 0
    }

    /// Converts from `Status` to `Result<(), StatusError>`.
    pub fn to_result(self) -> Result<(), StatusError> {
        if self.is_ok() {
            Ok(())
        } else {
            Err(StatusError { status: self })
        }
    }

    /// Returns a new status that is `base_status` if not OK and otherwise returns
    /// `new_status`. This allows for chaining failure handling code that may also
    /// return statuses.
    pub fn chain(self, other: Self) -> Self {
        Self {
            ctx: unsafe { sys::iree_status_join(self.ctx, other.ctx) },
        }
    }
}

impl core::fmt::Debug for StatusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::fmt::Display for StatusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut bufptr = core::ptr::null_mut();
        let allocator = allocator::Allocator::get_global();
        let mut size: usize = 0;
        if !(unsafe {
            sys::iree_status_to_string(self.status.ctx, &allocator.ctx, &mut bufptr, &mut size)
        }) {
            return write!(f, "Status: <failed to convert to string>");
        }
        let buf =
            core::str::from_utf8(unsafe { core::slice::from_raw_parts(bufptr as *const u8, size) })
                .map_err(|_| core::fmt::Error)?;
        let write_result = write!(f, "Status: {:?}", buf);
        unsafe {
            sys::iree_allocator_free(allocator.ctx, bufptr as *mut _);
        }
        write_result
    }
}

/// IREE runtime status error
pub struct StatusError {
    status: Status,
}

// TODO: change this when #![feature(error_in_core)] is stabilized
#[cfg(feature = "std")]
impl std::error::Error for StatusError {}

impl Drop for Status {
    fn drop(&mut self) {
        unsafe {
            if !self.is_ok() {
                sys::iree_status_ignore(self.ctx);
            }
        }
    }
}

// Necessary because status code lifetime is not specified in the C API
static STATUS_CODES: [usize; 18] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];

/// IREE runtime status error
pub enum StatusErrorKind {
    Cancelled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
    Deferred,
    UnknownStatus,
}

impl From<sys::iree_status_code_e> for StatusErrorKind {
    fn from(status: sys::iree_status_code_e) -> Self {
        match status {
            sys::iree_status_code_e_IREE_STATUS_CANCELLED => Self::Cancelled,
            sys::iree_status_code_e_IREE_STATUS_UNKNOWN => Self::Unknown,
            sys::iree_status_code_e_IREE_STATUS_INVALID_ARGUMENT => Self::InvalidArgument,
            sys::iree_status_code_e_IREE_STATUS_DEADLINE_EXCEEDED => Self::DeadlineExceeded,
            sys::iree_status_code_e_IREE_STATUS_NOT_FOUND => Self::NotFound,
            sys::iree_status_code_e_IREE_STATUS_ALREADY_EXISTS => Self::AlreadyExists,
            sys::iree_status_code_e_IREE_STATUS_PERMISSION_DENIED => Self::PermissionDenied,
            sys::iree_status_code_e_IREE_STATUS_RESOURCE_EXHAUSTED => Self::ResourceExhausted,
            sys::iree_status_code_e_IREE_STATUS_FAILED_PRECONDITION => Self::FailedPrecondition,
            sys::iree_status_code_e_IREE_STATUS_ABORTED => Self::Aborted,
            sys::iree_status_code_e_IREE_STATUS_OUT_OF_RANGE => Self::OutOfRange,
            sys::iree_status_code_e_IREE_STATUS_UNIMPLEMENTED => Self::Unimplemented,
            sys::iree_status_code_e_IREE_STATUS_INTERNAL => Self::Internal,
            sys::iree_status_code_e_IREE_STATUS_UNAVAILABLE => Self::Unavailable,
            sys::iree_status_code_e_IREE_STATUS_DATA_LOSS => Self::DataLoss,
            sys::iree_status_code_e_IREE_STATUS_UNAUTHENTICATED => Self::Unauthenticated,
            sys::iree_status_code_e_IREE_STATUS_DEFERRED => Self::Deferred,
            _ => Self::UnknownStatus,
        }
    }
}

impl From<StatusErrorKind> for sys::iree_status_code_t {
    fn from(status: StatusErrorKind) -> Self {
        use StatusErrorKind::*;
        match status {
            Cancelled => sys::iree_status_code_e_IREE_STATUS_CANCELLED,
            Unknown => sys::iree_status_code_e_IREE_STATUS_UNKNOWN,
            InvalidArgument => sys::iree_status_code_e_IREE_STATUS_INVALID_ARGUMENT,
            DeadlineExceeded => sys::iree_status_code_e_IREE_STATUS_DEADLINE_EXCEEDED,
            NotFound => sys::iree_status_code_e_IREE_STATUS_NOT_FOUND,
            AlreadyExists => sys::iree_status_code_e_IREE_STATUS_ALREADY_EXISTS,
            PermissionDenied => sys::iree_status_code_e_IREE_STATUS_PERMISSION_DENIED,
            ResourceExhausted => sys::iree_status_code_e_IREE_STATUS_RESOURCE_EXHAUSTED,
            FailedPrecondition => sys::iree_status_code_e_IREE_STATUS_FAILED_PRECONDITION,
            Aborted => sys::iree_status_code_e_IREE_STATUS_ABORTED,
            OutOfRange => sys::iree_status_code_e_IREE_STATUS_OUT_OF_RANGE,
            Unimplemented => sys::iree_status_code_e_IREE_STATUS_UNIMPLEMENTED,
            Internal => sys::iree_status_code_e_IREE_STATUS_INTERNAL,
            Unavailable => sys::iree_status_code_e_IREE_STATUS_UNAVAILABLE,
            DataLoss => sys::iree_status_code_e_IREE_STATUS_DATA_LOSS,
            Unauthenticated => sys::iree_status_code_e_IREE_STATUS_UNAUTHENTICATED,
            Deferred => sys::iree_status_code_e_IREE_STATUS_DEFERRED,
            UnknownStatus => panic!("Unknown status"),
        }
    }
}
