use core::fmt::{Debug, Formatter};

use iree_sys::runtime as sys;
use log::debug;

use super::{
    api::{self, Instance},
    base::{self, ConstByteSpan},
    error::RuntimeError,
    vm::{Ref, ToRef},
};

pub struct DriverRegistry {
    pub(crate) ctx: *mut sys::iree_hal_driver_registry_t,
}

impl Drop for DriverRegistry {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_driver_registry_free(self.ctx);
        }
    }
}

impl DriverRegistry {
    pub fn new() -> Self {
        let out_ptr;
        unsafe {
            out_ptr = sys::iree_hal_driver_registry_default();
        }
        Self { ctx: out_ptr }
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Device<'a> {
    pub(crate) ctx: *mut sys::iree_hal_device_t,
    pub(crate) marker: core::marker::PhantomData<&'a api::Session<'a>>,
}

impl Drop for Device<'_> {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_device_release(self.ctx);
        }
    }
}

pub enum EncodingType {
    Opaque,
    DenseRowMajor,
}

impl From<EncodingType> for sys::iree_hal_encoding_types_t {
    fn from(encoding_type: EncodingType) -> Self {
        match encoding_type {
            EncodingType::Opaque => sys::iree_hal_encoding_types_t_IREE_HAL_ENCODING_TYPE_OPAQUE,
            EncodingType::DenseRowMajor => {
                sys::iree_hal_encoding_types_t_IREE_HAL_ENCODING_TYPE_DENSE_ROW_MAJOR
            }
        }
    }
}

pub enum ElementType {
    None,
    Opaque8,
    Opaque16,
    Opaque32,
    Opaque64,
    Bool8,
    Int4,
    Sint4,
    Uint4,
    Int8,
    Sint8,
    Uint8,
    Int16,
    Sint16,
    Uint16,
    Int32,
    Sint32,
    Uint32,
    Int64,
    Sint64,
    Uint64,
    Float16,
    Float32,
    Float64,
    BFloat16,
    ComplexFloat64,
    ComplexFloat128,
}

impl From<ElementType> for sys::iree_hal_element_type_t {
    fn from(element_type: ElementType) -> Self {
        match element_type {
            ElementType::None => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_NONE,
            ElementType::Opaque8 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_8,
            ElementType::Opaque16 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_16,
            ElementType::Opaque32 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_32,
            ElementType::Opaque64 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_64,
            ElementType::Bool8 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BOOL_8,
            ElementType::Int4 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_4,
            ElementType::Sint4 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_4,
            ElementType::Uint4 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_4,
            ElementType::Int8 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_8,
            ElementType::Sint8 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_8,
            ElementType::Uint8 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_8,
            ElementType::Int16 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_16,
            ElementType::Sint16 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_16,
            ElementType::Uint16 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_16,
            ElementType::Int32 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_32,
            ElementType::Sint32 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_32,
            ElementType::Uint32 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_32,
            ElementType::Int64 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_64,
            ElementType::Sint64 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_64,
            ElementType::Uint64 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_64,
            ElementType::Float16 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_16,
            ElementType::Float32 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_32,
            ElementType::Float64 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_64,
            ElementType::BFloat16 => sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BFLOAT_16,
            ElementType::ComplexFloat64 => {
                sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_COMPLEX_FLOAT_64
            }
            ElementType::ComplexFloat128 => {
                sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_COMPLEX_FLOAT_128
            }
        }
    }
}

pub trait ToElementType {
    fn to_element_type() -> ElementType;
}

macro_rules! impl_to_element_type {
    ($type:ty, $variant:ident) => {
        impl ToElementType for $type {
            fn to_element_type() -> ElementType {
                ElementType::$variant
            }
        }
    };
}

impl_to_element_type!(u8, Uint8);
impl_to_element_type!(u16, Uint16);
impl_to_element_type!(u32, Uint32);
impl_to_element_type!(u64, Uint64);
impl_to_element_type!(i8, Sint8);
impl_to_element_type!(i16, Sint16);
impl_to_element_type!(i32, Sint32);
impl_to_element_type!(i64, Sint64);
impl_to_element_type!(f32, Float32);
impl_to_element_type!(f64, Float64);
impl_to_element_type!(bool, Bool8);

pub struct BufferView<'a, T: ToElementType> {
    pub(crate) ctx: *mut sys::iree_hal_buffer_view_t,
    pub(crate) session: &'a api::Session<'a>,
    pub(crate) marker: core::marker::PhantomData<T>,
}

impl<'a, T: ToElementType> BufferView<'a, T> {
    pub fn new(
        session: &'a api::Session,
        shape: &[usize],
        encoding_type: EncodingType,
        data: &[T],
    ) -> Result<Self, RuntimeError> {
        let mut out_ptr = core::ptr::null_mut();
        let bytespan: ConstByteSpan = unsafe {
            core::slice::from_raw_parts(data.as_ptr() as *const u8, core::mem::size_of_val(data))
        }
        .into();
        debug!("shape: {:?}", shape);
        debug!("data len: {}", core::mem::size_of_val(data));
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_view_allocate_buffer_copy(
                sys::iree_runtime_session_device(session.ctx),
                sys::iree_runtime_session_device_allocator(session.ctx),
                shape.len(),
                shape.as_ptr(),
                T::to_element_type().into(),
                encoding_type.into(),
                sys::iree_hal_buffer_params_t {
                    usage: sys::iree_hal_buffer_usage_bits_t_IREE_HAL_BUFFER_USAGE_DEFAULT,
                    access: 0,
                    type_: sys::iree_hal_memory_type_bits_t_IREE_HAL_MEMORY_TYPE_DEVICE_LOCAL,
                    queue_affinity: 0,
                    min_alignment: 0,
                },
                bytespan.ctx,
                &mut out_ptr as *mut *mut sys::iree_hal_buffer_view_t,
            )
        })
        .to_result()?;
        Ok(Self {
            ctx: out_ptr,
            session,
            marker: core::marker::PhantomData,
        })
    }

    pub(crate) unsafe fn from_ptr(
        ctx: *mut sys::iree_hal_buffer_view_t,
        session: &'a api::Session,
    ) -> Self {
        Self {
            ctx,
            session,
            marker: core::marker::PhantomData,
        }
    }

    pub(crate) fn get_buffer(&self) -> *mut sys::iree_hal_buffer_t {
        unsafe { sys::iree_hal_buffer_view_buffer(self.ctx) }
    }

    pub fn byte_length(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_byte_length(self.ctx) }
    }
}

impl<T: ToElementType> Debug for BufferView<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(unsafe {
            let buf = &mut [0u8; 1024];
            let mut len: usize = 0;
            sys::iree_hal_buffer_view_format(
                self.ctx,
                6,
                buf.len(),
                buf.as_mut_ptr(),
                &mut len as *mut usize,
            );
            core::ffi::CStr::from_ptr(buf.as_ptr()).to_str().unwrap()
        })
    }
}

impl<T: ToElementType> Drop for BufferView<'_, T> {
    fn drop(&mut self) {
        unsafe {
            debug!("Releasing BufferView...");
            sys::iree_hal_buffer_view_release(self.ctx);
        }
    }
}

impl<'a, T: ToElementType> ToRef<'a> for BufferView<'a, T> {
    fn to_ref(&'a self, instance: &'a Instance) -> Result<Ref<'a, Self>, RuntimeError> {
        let mut out = core::mem::MaybeUninit::<sys::iree_vm_ref_t>::zeroed();
        base::Status::from_raw(unsafe {
            sys::iree_vm_ref_wrap_retain(
                self.ctx as *mut core::ffi::c_void,
                Self::to_ref_type(instance),
                out.as_mut_ptr(),
            )
        })
        .to_result()?;
        debug!("BufferView ref: {:?}", unsafe { out.assume_init() });
        Ok(Ref {
            ctx: unsafe { out.assume_init() },
            _instance: instance,
            _marker: core::marker::PhantomData,
        })
    }

    fn to_ref_type(instance: &Instance) -> sys::iree_vm_ref_type_t {
        instance.lookup_type("hal.buffer_view".into())
    }
}

pub struct BufferMapping<'a, T: ToElementType> {
    ctx: sys::iree_hal_buffer_mapping_t,
    marker: core::marker::PhantomData<&'a T>,
}

impl<'a, T: ToElementType> BufferMapping<'a, T> {
    pub fn new(buffer_view: BufferView<'a, T>) -> Result<Self, RuntimeError> {
        let mut out = core::mem::MaybeUninit::<sys::iree_hal_buffer_mapping_t>::uninit();
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_map_range(
                buffer_view.get_buffer(),
                sys::iree_hal_mapping_mode_bits_t_IREE_HAL_MAPPING_MODE_SCOPED,
                sys::iree_hal_memory_access_bits_t_IREE_HAL_MEMORY_ACCESS_READ as u16,
                0,
                buffer_view.byte_length(),
                out.as_mut_ptr(),
            )
        })
        .to_result()?;
        Ok(Self {
            ctx: unsafe { out.assume_init() },
            marker: core::marker::PhantomData,
        })
    }

    pub fn data(&self) -> &'a [T] {
        unsafe {
            core::slice::from_raw_parts(
                self.ctx.contents.data as *const T,
                self.ctx.contents.data_length / core::mem::size_of::<T>(),
            )
        }
    }
}

impl<T: ToElementType> Drop for BufferMapping<'_, T> {
    fn drop(&mut self) {
        unsafe {
            debug!("Releasing BufferMapping...");
            sys::iree_hal_buffer_unmap_range(&mut self.ctx);
        }
    }
}
