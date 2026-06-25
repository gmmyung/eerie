extern crate alloc;
use alloc::rc::Rc;
use core::{alloc::Layout, ffi::c_void, fmt::Display, marker::PhantomData};
use eerie_sys::runtime as sys;
use log::trace;
#[cfg(feature = "std")]
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[cfg(feature = "std")]
static RUNTIME_LOCK: RwLock<()> = RwLock::new(());

pub(crate) type NotSendSync = PhantomData<Rc<()>>;

pub(crate) const fn not_send_sync() -> NotSendSync {
    PhantomData
}

#[cfg(feature = "std")]
pub(crate) struct RuntimeLifecycleGuard {
    _guard: RwLockWriteGuard<'static, ()>,
}

#[cfg(feature = "std")]
pub(crate) struct RuntimeInvocationGuard {
    _guard: RwLockReadGuard<'static, ()>,
}

#[cfg(not(feature = "std"))]
pub(crate) struct RuntimeLifecycleGuard;

#[cfg(not(feature = "std"))]
pub(crate) struct RuntimeInvocationGuard;

pub(crate) fn runtime_lifecycle_guard() -> RuntimeLifecycleGuard {
    #[cfg(feature = "std")]
    {
        RuntimeLifecycleGuard {
            _guard: RUNTIME_LOCK
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }

    #[cfg(not(feature = "std"))]
    {
        RuntimeLifecycleGuard
    }
}

pub(crate) fn runtime_invocation_guard() -> RuntimeInvocationGuard {
    #[cfg(feature = "std")]
    {
        RuntimeInvocationGuard {
            _guard: RUNTIME_LOCK
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }

    #[cfg(not(feature = "std"))]
    {
        RuntimeInvocationGuard
    }
}

/// A wrapper for a constant byte span
pub(crate) struct ConstByteSpan<'a> {
    pub(crate) ctx: sys::iree_const_byte_span_t,
    marker: PhantomData<&'a [u8]>,
}

impl<'a> From<&'a [u8]> for ConstByteSpan<'a> {
    fn from(slice: &'a [u8]) -> Self {
        let byte_span = sys::iree_const_byte_span_t {
            data: slice.as_ptr(),
            data_length: slice.len(),
        };
        Self {
            ctx: byte_span,
            marker: PhantomData,
        }
    }
}

/// A wrapper for a string view
pub(crate) struct StringView<'a> {
    pub(crate) ctx: sys::iree_string_view_t,
    marker: PhantomData<&'a str>,
}

impl Display for StringView<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let bytes =
            unsafe { core::slice::from_raw_parts(self.ctx.data as *const u8, self.ctx.size) };
        let value = core::str::from_utf8(bytes).map_err(|_| core::fmt::Error)?;
        write!(f, "{value}")
    }
}

impl<'a> From<&'a str> for StringView<'a> {
    fn from(s: &'a str) -> Self {
        let string_view = sys::iree_string_view_t {
            data: s.as_ptr() as *const core::ffi::c_char,
            size: s.len(),
        };
        Self {
            ctx: string_view,
            marker: core::marker::PhantomData,
        }
    }
}

pub(crate) struct Allocator {
    pub(crate) ctx: sys::iree_allocator_t,
}

impl Allocator {
    pub(crate) fn get_global() -> Self {
        let allocator = sys::iree_allocator_t {
            self_: core::ptr::null_mut(),
            ctl: Some(rust_allocator_ctl),
        };
        Self { ctx: allocator }
    }

    pub(crate) fn null_allocator() -> Self {
        let allocator = sys::iree_allocator_t {
            self_: core::ptr::null_mut(),
            ctl: None,
        };
        Self { ctx: allocator }
    }
}

const ALIGNMENT: usize = 16;

unsafe extern "C" fn rust_allocator_ctl(
    _self_: *mut c_void,
    command: sys::iree_allocator_command_e,
    params: *const c_void,
    inout_ptr: *mut *mut c_void,
) -> sys::iree_status_t {
    // use Rust Global Allocator
    match command {
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_MALLOC => {
            let size = (*(params as *const sys::iree_allocator_alloc_params_t)).byte_length;
            let Some(alloc_size) = size.checked_add(ALIGNMENT) else {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            };
            if alloc_size > isize::MAX as usize {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            }
            let ptr = alloc::alloc::alloc(Layout::from_size_align_unchecked(alloc_size, ALIGNMENT));
            if ptr.is_null() {
                return Status::from_code(StatusErrorKind::ResourceExhausted).ctx;
            }
            *(ptr as *mut usize) = size;
            *inout_ptr = ptr.wrapping_add(ALIGNMENT) as *mut c_void;
            trace!(
                "rust_allocator_ctl: IREE_ALLOCATOR_COMMAND_MALLOC: size: {} -> {:?}",
                size,
                *inout_ptr
            );
            core::ptr::null_mut() as sys::iree_status_t
        }
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_CALLOC => {
            let size = (*(params as *const sys::iree_allocator_alloc_params_t)).byte_length;
            let Some(alloc_size) = size.checked_add(ALIGNMENT) else {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            };
            if alloc_size > isize::MAX as usize {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            }
            let ptr = alloc::alloc::alloc_zeroed(Layout::from_size_align_unchecked(
                alloc_size, ALIGNMENT,
            ));
            if ptr.is_null() {
                return Status::from_code(StatusErrorKind::ResourceExhausted).ctx;
            }
            *(ptr as *mut usize) = size;
            *inout_ptr = ptr.wrapping_add(ALIGNMENT) as *mut c_void;
            trace!(
                "rust_allocator_ctl: IREE_ALLOCATOR_COMMAND_CALLOC: size: {} -> {:?}",
                size,
                *inout_ptr
            );
            core::ptr::null_mut() as sys::iree_status_t
        }
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_REALLOC => {
            if (*inout_ptr).is_null() {
                // realloc of null is malloc
                return rust_allocator_ctl(
                    _self_,
                    sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_MALLOC,
                    params,
                    inout_ptr,
                );
            }
            let ptr = (*inout_ptr).wrapping_sub(ALIGNMENT);
            let old_size = unsafe { *(ptr as *mut usize) };
            let new_size = (*(params as *const sys::iree_allocator_alloc_params_t)).byte_length;
            trace!(
                "rust_allocator_ctl: IREE_ALLOCATOR_COMMAND_REALLOC: {} -> {}",
                old_size,
                new_size
            );
            let Some(new_alloc_size) = new_size.checked_add(ALIGNMENT) else {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            };
            if new_alloc_size > isize::MAX as usize {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            }
            let Some(old_alloc_size) = old_size.checked_add(ALIGNMENT) else {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            };
            let ptr = alloc::alloc::realloc(
                ptr as *mut u8,
                Layout::from_size_align_unchecked(old_alloc_size, ALIGNMENT),
                new_alloc_size,
            );
            if ptr.is_null() {
                return Status::from_code(StatusErrorKind::ResourceExhausted).ctx;
            }
            unsafe {
                *(ptr as *mut usize) = new_size;
            }
            *inout_ptr = ptr.wrapping_add(ALIGNMENT) as *mut c_void;
            core::ptr::null_mut() as sys::iree_status_t
        }
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_FREE => {
            if (*inout_ptr).is_null() {
                return core::ptr::null_mut() as sys::iree_status_t;
            }
            let ptr = (*inout_ptr).wrapping_sub(ALIGNMENT);
            let size = unsafe { *(ptr as *mut usize) };
            trace!(
                "rust_allocator_ctl: IREE_ALLOCATOR_COMMAND_FREE: size: {}->{:p}",
                size,
                *inout_ptr
            );
            let Some(alloc_size) = size.checked_add(ALIGNMENT) else {
                return Status::from_code(StatusErrorKind::OutOfRange).ctx;
            };
            alloc::alloc::dealloc(
                ptr as *mut u8,
                Layout::from_size_align_unchecked(alloc_size, ALIGNMENT),
            );
            core::ptr::null_mut() as sys::iree_status_t
        }
        _ => Status::from_code(StatusErrorKind::Unimplemented).ctx,
    }
}

/// IREE runtime status
pub(crate) struct Status {
    ctx: sys::iree_status_t,
}

impl Status {
    pub(crate) fn from_raw(ctx: sys::iree_status_t) -> Self {
        Self { ctx }
    }

    pub(crate) fn from_code(status_kind: StatusErrorKind) -> Self {
        let status: sys::iree_status_code_e = status_kind.into();
        Status {
            ctx: status as usize as sys::iree_status_t,
        }
    }

    pub(crate) fn is_ok(&self) -> bool {
        self.ctx as usize == 0
    }

    pub(crate) fn into_result(self) -> Result<(), StatusError> {
        if self.is_ok() {
            Ok(())
        } else {
            Err(StatusError { status: self })
        }
    }
}

impl core::fmt::Debug for StatusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for StatusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut bufptr = core::ptr::null_mut();
        let allocator = Allocator::get_global();
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

impl core::error::Error for StatusError {}

impl Drop for Status {
    fn drop(&mut self) {
        unsafe {
            if !self.is_ok() {
                sys::iree_status_ignore(self.ctx);
            }
        }
    }
}

/// IREE runtime status error
pub(crate) enum StatusErrorKind {
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
    Incompatible,
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
            sys::iree_status_code_e_IREE_STATUS_INCOMPATIBLE => Self::Incompatible,
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
            Incompatible => sys::iree_status_code_e_IREE_STATUS_INCOMPATIBLE,
            UnknownStatus => panic!("Unknown status"),
        }
    }
}
