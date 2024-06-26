use super::status;
use eerie_sys::runtime as sys;
extern crate alloc;

/// A span of mutable bytes.
pub(crate) struct ByteSpan<'a> {
    pub(crate) ctx: sys::iree_byte_span_t,
    pub(crate) marker: core::marker::PhantomData<&'a mut [u8]>,
}

impl<'a> From<&'a mut [u8]> for ByteSpan<'a> {
    fn from(slice: &'a mut [u8]) -> Self {
        let byte_span = sys::iree_byte_span_t {
            data: slice.as_ptr() as *mut u8,
            data_length: slice.len(),
        };
        Self {
            ctx: byte_span,
            marker: core::marker::PhantomData,
        }
    }
}

impl<'a> From<ByteSpan<'a>> for &'a mut [u8] {
    fn from(byte_span: ByteSpan<'a>) -> Self {
        unsafe { core::slice::from_raw_parts_mut(byte_span.ctx.data, byte_span.ctx.data_length) }
    }
}

/// A span of constant bytes.
pub(crate) struct ConstByteSpan<'a> {
    pub(crate) ctx: sys::iree_const_byte_span_t,
    pub(crate) marker: core::marker::PhantomData<&'a [u8]>,
}

impl<'a> From<&'a [u8]> for ConstByteSpan<'a> {
    fn from(slice: &'a [u8]) -> Self {
        let byte_span = sys::iree_const_byte_span_t {
            data: slice.as_ptr(),
            data_length: slice.len(),
        };
        Self {
            ctx: byte_span,
            marker: core::marker::PhantomData,
        }
    }
}

impl<'a> From<ConstByteSpan<'a>> for &'a [u8] {
    fn from(byte_span: ConstByteSpan<'a>) -> Self {
        unsafe { core::slice::from_raw_parts(byte_span.ctx.data, byte_span.ctx.data_length) }
    }
}

/// An allocator for host-memory allocations.
/// IREE will attempt to use this in place of the system malloc and free.
pub(crate) struct Allocator {
    pub(crate) ctx: sys::iree_allocator_t,
}

impl Allocator {
    pub fn get_global() -> Self {
        let allocator = sys::iree_allocator_t {
            self_: core::ptr::null_mut(),
            ctl: Some(rust_allocator_ctl),
        };
        Self { ctx: allocator }
    }

    pub fn null_allocator() -> Self {
        let allocator = sys::iree_allocator_t {
            self_: core::ptr::null_mut(),
            ctl: Some(null_allocator_ctl),
        };
        Self { ctx: allocator }
    }
}

const ALIGNMENT: usize = 16;

unsafe extern "C" fn null_allocator_ctl(
    _self_: *mut core::ffi::c_void,
    _command: sys::iree_allocator_command_e,
    _params: *const core::ffi::c_void,
    _inout_ptr: *mut *mut core::ffi::c_void,
) -> sys::iree_status_t {
    core::ptr::null_mut() as sys::iree_status_t
}

unsafe extern "C" fn rust_allocator_ctl(
    _self_: *mut core::ffi::c_void,
    command: sys::iree_allocator_command_e,
    params: *const core::ffi::c_void,
    inout_ptr: *mut *mut core::ffi::c_void,
) -> sys::iree_status_t {
    // use Rust Global Allocator
    match command {
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_MALLOC => {
            let size = (*(params as *const sys::iree_allocator_alloc_params_t)).byte_length;
            if size > core::isize::MAX as usize {
                return status::Status::from_code(status::StatusErrorKind::OutOfRange).ctx;
            }
            let ptr = alloc::alloc::alloc(core::alloc::Layout::from_size_align_unchecked(
                size + ALIGNMENT,
                ALIGNMENT,
            ));
            *(ptr as *mut usize) = size;
            *inout_ptr = ptr.wrapping_add(ALIGNMENT) as *mut core::ffi::c_void;
            core::ptr::null_mut() as sys::iree_status_t
        }
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_CALLOC => {
            let size = (*(params as *const sys::iree_allocator_alloc_params_t)).byte_length;
            if size > core::isize::MAX as usize {
                return status::Status::from_code(status::StatusErrorKind::OutOfRange).ctx;
            }
            let ptr = alloc::alloc::alloc_zeroed(core::alloc::Layout::from_size_align_unchecked(
                size + ALIGNMENT,
                ALIGNMENT,
            ));
            *(ptr as *mut usize) = size;
            *inout_ptr = ptr.wrapping_add(ALIGNMENT) as *mut core::ffi::c_void;
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
            if new_size > core::isize::MAX as usize {
                return status::Status::from_code(status::StatusErrorKind::OutOfRange).ctx;
            }
            let ptr = alloc::alloc::realloc(
                ptr as *mut u8,
                core::alloc::Layout::from_size_align_unchecked(old_size + ALIGNMENT, ALIGNMENT),
                new_size + ALIGNMENT,
            );
            unsafe {
                *(ptr as *mut usize) = new_size;
            }
            *inout_ptr = ptr.wrapping_add(ALIGNMENT) as *mut core::ffi::c_void;
            core::ptr::null_mut() as sys::iree_status_t
        }
        sys::iree_allocator_command_e_IREE_ALLOCATOR_COMMAND_FREE => {
            let ptr = (*inout_ptr).wrapping_sub(ALIGNMENT);
            let size = unsafe { *(ptr as *mut usize) };
            alloc::alloc::dealloc(
                ptr as *mut u8,
                core::alloc::Layout::from_size_align_unchecked(size + ALIGNMENT, ALIGNMENT),
            );
            core::ptr::null_mut() as sys::iree_status_t
        }
        _ => status::Status::from_code(status::StatusErrorKind::Unimplemented).ctx,
    }
}
