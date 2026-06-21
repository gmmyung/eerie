extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
};

use eerie_sys::runtime as sys;
use half::{bf16, f16};
use log::debug;

use super::{
    base::{self, ConstByteSpan, StringView},
    error::RuntimeError,
};

mod private {
    pub trait Sealed {}
}

fn string_view_to_string(value: sys::iree_string_view_t) -> String {
    if value.data.is_null() || value.size == 0 {
        return String::new();
    }
    let bytes = unsafe { core::slice::from_raw_parts(value.data as *const u8, value.size) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn infinite_timeout() -> sys::iree_timeout_t {
    sys::iree_timeout_t {
        type_: sys::iree_timeout_type_e_IREE_TIMEOUT_ABSOLUTE,
        nanos: i64::MAX as sys::iree_time_t,
    }
}

/// A HAL driver registry populated with the drivers linked into this binary.
pub struct DriverRegistry {
    pub(crate) ctx: *mut sys::iree_hal_driver_registry_t,
    host_allocator: base::Allocator,
}

impl DriverRegistry {
    /// Creates a driver registry and registers all available IREE HAL drivers.
    pub fn with_available_drivers() -> Result<Self, RuntimeError> {
        let host_allocator = base::Allocator::get_global();
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_registry_allocate(host_allocator.ctx, &mut ctx)
        })
        .to_result()?;
        base::Status::from_raw(unsafe { sys::iree_hal_register_all_available_drivers(ctx) })
            .to_result()?;
        Ok(Self {
            ctx,
            host_allocator,
        })
    }

    pub fn create_driver(&self, name: &str) -> Result<Driver, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_registry_try_create(
                self.ctx,
                StringView::from(name).ctx,
                self.host_allocator.ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Driver {
            ctx,
            host_allocator: base::Allocator::get_global(),
        })
    }
}

impl Drop for DriverRegistry {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_driver_registry_free(self.ctx);
        }
    }
}

/// A HAL driver.
pub struct Driver {
    pub(crate) ctx: *mut sys::iree_hal_driver_t,
    host_allocator: base::Allocator,
}

impl Driver {
    /// Queries devices currently available through this driver.
    pub fn available_devices(&self) -> Result<Vec<DeviceInfo>, RuntimeError> {
        let mut count = 0usize;
        let mut infos = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_query_available_devices(
                self.ctx,
                self.host_allocator.ctx,
                &mut count,
                &mut infos,
            )
        })
        .to_result()?;

        if infos.is_null() || count == 0 {
            return Ok(Vec::new());
        }

        let result = unsafe { core::slice::from_raw_parts(infos, count) }
            .iter()
            .enumerate()
            .map(|(ordinal, info)| DeviceInfo {
                ordinal,
                id: info.device_id,
                path: string_view_to_string(info.path),
                name: string_view_to_string(info.name),
            })
            .collect();

        unsafe {
            sys::iree_allocator_free(self.host_allocator.ctx, infos as *mut core::ffi::c_void);
        }

        Ok(result)
    }

    /// Creates the driver's default device.
    pub fn create_default_device(&self) -> Result<Device, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_create_default_device(self.ctx, self.host_allocator.ctx, &mut ctx)
        })
        .to_result()?;
        Ok(Device { ctx })
    }

    /// Creates a device by ordinal as returned by `available_devices`.
    pub fn create_device_by_ordinal(&self, ordinal: usize) -> Result<Device, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_create_device_by_ordinal(
                self.ctx,
                ordinal,
                0,
                core::ptr::null(),
                self.host_allocator.ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Device { ctx })
    }

    /// Creates a device by driver-specific path.
    pub fn create_device_by_path(
        &self,
        driver_name: &str,
        path: &str,
    ) -> Result<Device, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_create_device_by_path(
                self.ctx,
                StringView::from(driver_name).ctx,
                StringView::from(path).ctx,
                0,
                core::ptr::null(),
                self.host_allocator.ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Device { ctx })
    }
}

/// A device enumerated by a HAL driver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub ordinal: usize,
    pub id: sys::iree_hal_device_id_t,
    pub path: String,
    pub name: String,
}

impl Clone for Driver {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_hal_driver_retain(self.ctx);
        }
        Self {
            ctx: self.ctx,
            host_allocator: base::Allocator::get_global(),
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_driver_release(self.ctx);
        }
    }
}

/// A HAL device.
pub struct Device {
    pub(crate) ctx: *mut sys::iree_hal_device_t,
}

impl Device {
    pub fn id(&self) -> String {
        string_view_to_string(unsafe { sys::iree_hal_device_id(self.ctx) })
    }

    pub fn query_i64(&self, category: &str, key: &str) -> Result<i64, RuntimeError> {
        let mut value = 0;
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_query_i64(
                self.ctx,
                StringView::from(category).ctx,
                StringView::from(key).ctx,
                &mut value,
            )
        })
        .to_result()?;
        Ok(value)
    }

    pub fn capabilities(&self) -> Result<DeviceCapabilities, RuntimeError> {
        let mut caps = sys::iree_hal_device_capabilities_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_query_capabilities(self.ctx, &mut caps)
        })
        .to_result()?;
        Ok(DeviceCapabilities {
            flags: caps.flags,
            semaphore_export_types: caps.semaphore_export_types,
            semaphore_import_types: caps.semaphore_import_types,
            buffer_export_types: caps.buffer_export_types,
            buffer_import_types: caps.buffer_import_types,
            numa_node: caps.numa_node,
            physical_device_uuid: caps
                .has_physical_device_uuid
                .then_some(caps.physical_device_uuid),
            device_group_index: caps.has_device_group.then_some(caps.device_group_index),
        })
    }

    pub fn trim(&self) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_hal_device_trim(self.ctx) })
            .to_result()
            .map_err(Into::into)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceCapabilities {
    pub flags: sys::iree_hal_device_capability_bits_t,
    pub semaphore_export_types: sys::iree_hal_topology_handle_type_t,
    pub semaphore_import_types: sys::iree_hal_topology_handle_type_t,
    pub buffer_export_types: sys::iree_hal_topology_handle_type_t,
    pub buffer_import_types: sys::iree_hal_topology_handle_type_t,
    pub numa_node: u8,
    pub physical_device_uuid: Option<[u8; 16]>,
    pub device_group_index: Option<u32>,
}

impl Clone for Device {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_hal_device_retain(self.ctx);
        }
        Self { ctx: self.ctx }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_device_release(self.ctx);
        }
    }
}

/// A buffer view encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    Opaque,
    DenseRowMajor,
}

impl From<Encoding> for sys::iree_hal_encoding_types_t {
    fn from(encoding: Encoding) -> Self {
        match encoding {
            Encoding::Opaque => sys::iree_hal_encoding_types_t_IREE_HAL_ENCODING_TYPE_OPAQUE,
            Encoding::DenseRowMajor => {
                sys::iree_hal_encoding_types_t_IREE_HAL_ENCODING_TYPE_DENSE_ROW_MAJOR
            }
        }
    }
}

impl From<sys::iree_hal_encoding_type_t> for Encoding {
    fn from(encoding: sys::iree_hal_encoding_type_t) -> Self {
        match encoding {
            sys::iree_hal_encoding_types_t_IREE_HAL_ENCODING_TYPE_DENSE_ROW_MAJOR => {
                Self::DenseRowMajor
            }
            _ => Self::Opaque,
        }
    }
}

/// A HAL buffer view element type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    Other(sys::iree_hal_element_type_t),
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
            ElementType::Other(element_type) => element_type,
        }
    }
}

impl From<sys::iree_hal_element_type_t> for ElementType {
    fn from(element_type: sys::iree_hal_element_type_t) -> Self {
        match element_type {
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_NONE => Self::None,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_8 => Self::Opaque8,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_16 => Self::Opaque16,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_32 => Self::Opaque32,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_OPAQUE_64 => Self::Opaque64,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BOOL_8 => Self::Bool8,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_4 => Self::Int4,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_4 => Self::Sint4,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_4 => Self::Uint4,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_8 => Self::Int8,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_8 => Self::Sint8,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_8 => Self::Uint8,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_16 => Self::Int16,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_16 => Self::Sint16,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_16 => Self::Uint16,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_32 => Self::Int32,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_32 => Self::Sint32,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_32 => Self::Uint32,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_64 => Self::Int64,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_SINT_64 => Self::Sint64,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_64 => Self::Uint64,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_16 => Self::Float16,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_32 => Self::Float32,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_64 => Self::Float64,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BFLOAT_16 => Self::BFloat16,
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_COMPLEX_FLOAT_64 => {
                Self::ComplexFloat64
            }
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_COMPLEX_FLOAT_128 => {
                Self::ComplexFloat128
            }
            _ => Self::Other(element_type),
        }
    }
}

/// A scalar element type that can be copied to and from raw HAL buffer storage.
///
/// This trait is sealed because `BufferView` reads device bytes directly into
/// `Vec<T>`. Only types with plain byte representations should implement it.
pub trait BufferElement: Copy + private::Sealed {
    fn element_type() -> ElementType;
}

macro_rules! impl_buffer_element {
    ($type:ty, $variant:ident) => {
        impl private::Sealed for $type {}

        impl BufferElement for $type {
            fn element_type() -> ElementType {
                ElementType::$variant
            }
        }
    };
}

impl_buffer_element!(u8, Uint8);
impl_buffer_element!(u16, Uint16);
impl_buffer_element!(u32, Uint32);
impl_buffer_element!(u64, Uint64);
impl_buffer_element!(i8, Int8);
impl_buffer_element!(i16, Int16);
impl_buffer_element!(i32, Int32);
impl_buffer_element!(i64, Int64);
impl_buffer_element!(f32, Float32);
impl_buffer_element!(f64, Float64);
impl_buffer_element!(f16, Float16);
impl_buffer_element!(bf16, BFloat16);

pub type MemoryType = sys::iree_hal_memory_type_t;
pub type MemoryAccess = sys::iree_hal_memory_access_t;
pub type BufferUsage = sys::iree_hal_buffer_usage_t;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferParams {
    pub memory_type: MemoryType,
    pub access: MemoryAccess,
    pub usage: BufferUsage,
    pub queue_affinity: u64,
    pub min_alignment: usize,
}

impl BufferParams {
    pub fn device_local() -> Self {
        Self {
            memory_type: sys::iree_hal_memory_type_bits_t_IREE_HAL_MEMORY_TYPE_DEVICE_LOCAL,
            access: sys::iree_hal_memory_access_bits_t_IREE_HAL_MEMORY_ACCESS_ALL as u16,
            usage: sys::iree_hal_buffer_usage_bits_t_IREE_HAL_BUFFER_USAGE_DEFAULT,
            queue_affinity: 0,
            min_alignment: 0,
        }
    }

    fn into_raw(self) -> sys::iree_hal_buffer_params_t {
        sys::iree_hal_buffer_params_t {
            usage: self.usage,
            access: self.access,
            type_: self.memory_type,
            queue_affinity: self.queue_affinity,
            min_alignment: self.min_alignment,
        }
    }
}

impl Default for BufferParams {
    fn default() -> Self {
        Self::device_local()
    }
}

/// A first-class HAL buffer allocation or subspan.
pub struct Buffer {
    pub(crate) ctx: *mut sys::iree_hal_buffer_t,
}

impl Buffer {
    pub fn allocate(
        device: &Device,
        byte_length: usize,
        params: BufferParams,
    ) -> Result<Self, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_allocator_allocate_buffer(
                sys::iree_hal_device_allocator(device.ctx),
                params.into_raw(),
                byte_length,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self { ctx })
    }

    pub(crate) unsafe fn from_raw_retained(ctx: *mut sys::iree_hal_buffer_t) -> Self {
        unsafe {
            sys::iree_hal_buffer_retain(ctx);
        }
        Self { ctx }
    }

    pub fn subspan(&self, byte_offset: usize, byte_length: usize) -> Result<Self, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_subspan(
                self.ctx,
                byte_offset,
                byte_length,
                base::Allocator::get_global().ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self { ctx })
    }

    pub fn byte_offset(&self) -> usize {
        unsafe { sys::iree_hal_buffer_byte_offset(self.ctx) }
    }

    pub fn byte_length(&self) -> usize {
        unsafe { sys::iree_hal_buffer_byte_length(self.ctx) }
    }

    pub fn allocation_size(&self) -> usize {
        unsafe { sys::iree_hal_buffer_allocation_size(self.ctx) }
    }

    pub fn memory_type(&self) -> MemoryType {
        unsafe { sys::iree_hal_buffer_memory_type(self.ctx) }
    }

    pub fn allowed_access(&self) -> MemoryAccess {
        unsafe { sys::iree_hal_buffer_allowed_access(self.ctx) }
    }

    pub fn allowed_usage(&self) -> BufferUsage {
        unsafe { sys::iree_hal_buffer_allowed_usage(self.ctx) }
    }
}

impl Clone for Buffer {
    fn clone(&self) -> Self {
        unsafe { Self::from_raw_retained(self.ctx) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_buffer_release(self.ctx);
        }
    }
}

/// A shaped and typed view into a HAL buffer.
pub struct BufferView<T: BufferElement> {
    pub(crate) ctx: *mut sys::iree_hal_buffer_view_t,
    marker: PhantomData<T>,
}

impl<T: BufferElement> BufferView<T> {
    /// Allocates a buffer on `device` and copies `data` into it.
    pub fn from_host(
        device: &Device,
        shape: &[usize],
        encoding: Encoding,
        data: &[T],
    ) -> Result<Self, RuntimeError> {
        let expected_len = shape.iter().try_fold(1usize, |product, dim| {
            product.checked_mul(*dim).ok_or_else(|| {
                RuntimeError::InvalidArgument(format!(
                    "buffer shape {:?} overflows usize element count",
                    shape
                ))
            })
        })?;
        if expected_len != data.len() {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer shape {:?} requires {} elements, got {}",
                shape,
                expected_len,
                data.len()
            )));
        }

        let mut ctx = core::ptr::null_mut();
        let bytes: ConstByteSpan = unsafe {
            core::slice::from_raw_parts(data.as_ptr() as *const u8, core::mem::size_of_val(data))
        }
        .into();
        debug!("shape: {:?}", shape);
        debug!("data len: {}", core::mem::size_of_val(data));
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_view_allocate_buffer_copy(
                device.ctx,
                sys::iree_hal_device_allocator(device.ctx),
                shape.len(),
                shape.as_ptr(),
                T::element_type().into(),
                encoding.into(),
                sys::iree_hal_buffer_params_t {
                    usage: sys::iree_hal_buffer_usage_bits_t_IREE_HAL_BUFFER_USAGE_DEFAULT,
                    access: 0,
                    type_: sys::iree_hal_memory_type_bits_t_IREE_HAL_MEMORY_TYPE_DEVICE_LOCAL,
                    queue_affinity: 0,
                    min_alignment: 0,
                },
                bytes.ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self {
            ctx,
            marker: PhantomData,
        })
    }

    pub(crate) unsafe fn from_raw_retained(ctx: *mut sys::iree_hal_buffer_view_t) -> Self {
        unsafe {
            sys::iree_hal_buffer_view_retain(ctx);
        }
        Self {
            ctx,
            marker: PhantomData,
        }
    }

    pub(crate) fn buffer(&self) -> *mut sys::iree_hal_buffer_t {
        unsafe { sys::iree_hal_buffer_view_buffer(self.ctx) }
    }

    pub fn raw_buffer(&self) -> Buffer {
        unsafe { Buffer::from_raw_retained(self.buffer()) }
    }

    pub fn from_buffer(
        buffer: &Buffer,
        shape: &[usize],
        encoding: Encoding,
    ) -> Result<Self, RuntimeError> {
        let expected_byte_length =
            shape
                .iter()
                .try_fold(core::mem::size_of::<T>(), |product, dim| {
                    product.checked_mul(*dim).ok_or_else(|| {
                        RuntimeError::InvalidArgument(format!(
                            "buffer shape {:?} overflows usize byte count",
                            shape
                        ))
                    })
                })?;
        if expected_byte_length > buffer.byte_length() {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer view requires {} bytes, buffer has {} bytes",
                expected_byte_length,
                buffer.byte_length()
            )));
        }
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_view_create(
                buffer.ctx,
                shape.len(),
                shape.as_ptr(),
                T::element_type().into(),
                encoding.into(),
                base::Allocator::get_global().ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self {
            ctx,
            marker: PhantomData,
        })
    }

    pub fn byte_length(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_byte_length(self.ctx) }
    }

    pub fn rank(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_shape_rank(self.ctx) }
    }

    pub fn shape(&self) -> Vec<usize> {
        let rank = self.rank();
        let dims = unsafe { sys::iree_hal_buffer_view_shape_dims(self.ctx) };
        unsafe { core::slice::from_raw_parts(dims, rank) }.to_vec()
    }

    pub fn dim(&self, index: usize) -> usize {
        unsafe { sys::iree_hal_buffer_view_shape_dim(self.ctx, index) }
    }

    pub fn element_count(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_element_count(self.ctx) }
    }

    pub fn element_size(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_element_size(self.ctx) }
    }

    pub fn element_type(&self) -> ElementType {
        unsafe { sys::iree_hal_buffer_view_element_type(self.ctx) }.into()
    }

    pub fn encoding(&self) -> Encoding {
        unsafe { sys::iree_hal_buffer_view_encoding_type(self.ctx) }.into()
    }

    /// Synchronously overwrites the buffer contents from host memory.
    pub fn write_from_slice(&self, device: &Device, data: &[T]) -> Result<(), RuntimeError> {
        if self.element_type() != T::element_type() {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer element type mismatch: expected {:?}, got {:?}",
                T::element_type(),
                self.element_type()
            )));
        }
        if data.len() != self.element_count() {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer requires {} elements, got {}",
                self.element_count(),
                data.len()
            )));
        }
        let byte_length = core::mem::size_of_val(data);
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_transfer_h2d(
                device.ctx,
                data.as_ptr() as *const core::ffi::c_void,
                self.buffer(),
                0,
                byte_length,
                sys::iree_hal_transfer_buffer_flag_bits_t_IREE_HAL_TRANSFER_BUFFER_FLAG_DEFAULT,
                infinite_timeout(),
            )
        })
        .to_result()
        .map_err(Into::into)
    }

    /// Synchronously copies this buffer view into another buffer view.
    pub fn copy_to(&self, device: &Device, target: &BufferView<T>) -> Result<(), RuntimeError> {
        if self.byte_length() != target.byte_length() {
            return Err(RuntimeError::InvalidArgument(format!(
                "source byte length {} does not match target byte length {}",
                self.byte_length(),
                target.byte_length()
            )));
        }
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_transfer_d2d(
                device.ctx,
                self.buffer(),
                0,
                target.buffer(),
                0,
                self.byte_length(),
                sys::iree_hal_transfer_buffer_flag_bits_t_IREE_HAL_TRANSFER_BUFFER_FLAG_DEFAULT,
                infinite_timeout(),
            )
        })
        .to_result()
        .map_err(Into::into)
    }

    /// Synchronously reads the buffer view contents back to host memory.
    pub fn read_to_vec(&self, device: &Device) -> Result<Vec<T>, RuntimeError> {
        let element_type = self.element_type();
        if element_type != T::element_type() {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer element type mismatch: expected {:?}, got {:?}",
                T::element_type(),
                element_type
            )));
        }
        let byte_length = self.byte_length();
        let element_size = core::mem::size_of::<T>();
        debug_assert_ne!(element_size, 0);
        if !byte_length.is_multiple_of(element_size) {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer byte length {} is not divisible by element size {}",
                byte_length, element_size
            )));
        }

        let mut data = Vec::<T>::with_capacity(byte_length / element_size);
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_transfer_d2h(
                device.ctx,
                self.buffer(),
                0,
                data.as_mut_ptr() as *mut core::ffi::c_void,
                byte_length,
                sys::iree_hal_transfer_buffer_flag_bits_t_IREE_HAL_TRANSFER_BUFFER_FLAG_DEFAULT,
                infinite_timeout(),
            )
        })
        .to_result()?;
        unsafe {
            data.set_len(byte_length / element_size);
        }
        Ok(data)
    }
}

impl<T: BufferElement> Clone for BufferView<T> {
    fn clone(&self) -> Self {
        unsafe { Self::from_raw_retained(self.ctx) }
    }
}

impl<T: BufferElement> Debug for BufferView<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let formatted = unsafe {
            let buf = &mut [b'\0' as core::ffi::c_char; 1024];
            let mut len: usize = 0;
            sys::iree_hal_buffer_view_format(self.ctx, 6, buf.len(), buf.as_mut_ptr(), &mut len);
            alloc::string::String::from_utf8_lossy(
                core::ffi::CStr::from_ptr(buf.as_ptr()).to_bytes(),
            )
            .into_owned()
        };
        f.write_str(&formatted)
    }
}

impl<T: BufferElement> Drop for BufferView<T> {
    fn drop(&mut self) {
        unsafe {
            debug!("Releasing BufferView...");
            sys::iree_hal_buffer_view_release(self.ctx);
        }
    }
}

/// A scoped host mapping of a HAL buffer view.
pub struct BufferMapping<T: BufferElement> {
    ctx: sys::iree_hal_buffer_mapping_t,
    _buffer: BufferView<T>,
    element_len: usize,
}

impl<T: BufferElement> BufferMapping<T> {
    pub fn map_read(buffer_view: &BufferView<T>) -> Result<Self, RuntimeError> {
        let element_type = buffer_view.element_type();
        if element_type != T::element_type() {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer element type mismatch: expected {:?}, got {:?}",
                T::element_type(),
                element_type
            )));
        }
        let buffer = buffer_view.clone();
        let mut out = core::mem::MaybeUninit::<sys::iree_hal_buffer_mapping_t>::uninit();
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_map_range(
                buffer.buffer(),
                sys::iree_hal_mapping_mode_bits_t_IREE_HAL_MAPPING_MODE_SCOPED,
                sys::iree_hal_memory_access_bits_t_IREE_HAL_MEMORY_ACCESS_READ as u16,
                0,
                buffer.byte_length(),
                out.as_mut_ptr(),
            )
        })
        .to_result()?;
        let mut ctx = unsafe { out.assume_init() };
        let element_size = core::mem::size_of::<T>();
        debug_assert_ne!(element_size, 0);
        if !ctx.contents.data_length.is_multiple_of(element_size) {
            unsafe {
                sys::iree_hal_buffer_unmap_range(&mut ctx);
            }
            return Err(RuntimeError::InvalidArgument(format!(
                "mapped byte length {} is not divisible by element size {}",
                ctx.contents.data_length, element_size
            )));
        }
        let align = core::mem::align_of::<T>();
        let address = ctx.contents.data as usize;
        if ctx.contents.data_length != 0 && address == 0 {
            unsafe {
                sys::iree_hal_buffer_unmap_range(&mut ctx);
            }
            return Err(RuntimeError::InvalidArgument(String::from(
                "mapped buffer returned null data for a non-empty range",
            )));
        }
        if address != 0 && !address.is_multiple_of(align) {
            unsafe {
                sys::iree_hal_buffer_unmap_range(&mut ctx);
            }
            return Err(RuntimeError::InvalidArgument(format!(
                "mapped buffer address 0x{address:x} is not aligned to {align}"
            )));
        }
        let element_len = ctx.contents.data_length / element_size;
        Ok(Self {
            ctx,
            _buffer: buffer,
            element_len,
        })
    }

    pub fn data(&self) -> &[T] {
        if self.element_len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ctx.contents.data as *const T, self.element_len) }
    }
}

impl<T: BufferElement> Drop for BufferMapping<T> {
    fn drop(&mut self) {
        unsafe {
            debug!("Releasing BufferMapping...");
            sys::iree_hal_buffer_unmap_range(&mut self.ctx);
        }
    }
}
