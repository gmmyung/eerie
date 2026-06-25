extern crate alloc;

use alloc::{borrow::Cow, format, vec::Vec};
use core::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
};

use eerie_sys::runtime as sys;
#[cfg(feature = "half")]
use half::{bf16, f16};
use log::debug;

use super::{
    base::{self, ConstByteSpan, StringView},
    error::RuntimeError,
};

mod private {
    use super::{Cow, RuntimeError, Vec};

    pub trait Element: Copy {
        const IS_RAW_STORAGE_COMPATIBLE: bool;
        const IREE_ELEMENT_TYPE: u32;

        fn element_size() -> usize {
            core::mem::size_of::<Self>()
        }

        fn as_bytes(data: &[Self]) -> Cow<'_, [u8]>;

        fn decode_slice(bytes: &[u8]) -> Result<Vec<Self>, RuntimeError>;
    }
}

fn infinite_timeout() -> sys::iree_timeout_t {
    sys::iree_timeout_t {
        type_: sys::iree_timeout_type_e_IREE_TIMEOUT_ABSOLUTE,
        nanos: i64::MAX as sys::iree_time_t,
    }
}

/// A HAL driver registry populated with the drivers linked into this binary.
pub(crate) struct DriverRegistry {
    pub(crate) ctx: *mut sys::iree_hal_driver_registry_t,
    host_allocator: base::Allocator,
    _not_send_sync: base::NotSendSync,
}

impl DriverRegistry {
    /// Creates a driver registry and registers all available IREE HAL drivers.
    pub(crate) fn with_available_drivers() -> Result<Self, RuntimeError> {
        let _guard = base::runtime_lifecycle_guard();
        let host_allocator = base::Allocator::get_global();
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_registry_allocate(host_allocator.ctx, &mut ctx)
        })
        .into_result()?;
        base::Status::from_raw(unsafe { sys::iree_hal_register_all_available_drivers(ctx) })
            .into_result()?;
        Ok(Self {
            ctx,
            host_allocator,
            _not_send_sync: base::not_send_sync(),
        })
    }

    pub(crate) fn create_driver(&self, name: &str) -> Result<Driver, RuntimeError> {
        let _guard = base::runtime_lifecycle_guard();
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_registry_try_create(
                self.ctx,
                StringView::from(name).ctx,
                self.host_allocator.ctx,
                &mut ctx,
            )
        })
        .into_result()?;
        Ok(Driver {
            ctx,
            host_allocator: base::Allocator::get_global(),
            _not_send_sync: base::not_send_sync(),
        })
    }
}

impl Drop for DriverRegistry {
    fn drop(&mut self) {
        let _guard = base::runtime_lifecycle_guard();
        unsafe {
            sys::iree_hal_driver_registry_free(self.ctx);
        }
    }
}

/// A HAL driver.
pub(crate) struct Driver {
    pub(crate) ctx: *mut sys::iree_hal_driver_t,
    host_allocator: base::Allocator,
    _not_send_sync: base::NotSendSync,
}

impl Driver {
    /// Creates the driver's default device.
    pub(crate) fn create_default_device(&self) -> Result<Device, RuntimeError> {
        let _guard = base::runtime_lifecycle_guard();
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_driver_create_default_device(self.ctx, self.host_allocator.ctx, &mut ctx)
        })
        .into_result()?;
        Ok(Device {
            ctx,
            _not_send_sync: base::not_send_sync(),
        })
    }
}

impl Clone for Driver {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_hal_driver_retain(self.ctx);
        }
        Self {
            ctx: self.ctx,
            host_allocator: base::Allocator::get_global(),
            _not_send_sync: base::not_send_sync(),
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        let _guard = base::runtime_lifecycle_guard();
        unsafe {
            sys::iree_hal_driver_release(self.ctx);
        }
    }
}

/// A HAL device.
pub(crate) struct Device {
    pub(crate) ctx: *mut sys::iree_hal_device_t,
    _not_send_sync: base::NotSendSync,
}

impl Device {}

impl Clone for Device {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_hal_device_retain(self.ctx);
        }
        Self {
            ctx: self.ctx,
            _not_send_sync: base::not_send_sync(),
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _guard = base::runtime_lifecycle_guard();
        unsafe {
            sys::iree_hal_device_release(self.ctx);
        }
    }
}

fn element_type_name(element_type: sys::iree_hal_element_type_t) -> &'static str {
    match element_type {
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BOOL_8 => "bool",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_8 => "u8",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_16 => "u16",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_32 => "u32",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_64 => "u64",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_8 => "i8",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_16 => "i16",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_32 => "i32",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_64 => "i64",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_16 => "f16",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_32 => "f32",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_64 => "f64",
        sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BFLOAT_16 => "bf16",
        _ => "unsupported",
    }
}

/// A scalar element type that can be copied to and from HAL buffer storage.
///
/// This trait is sealed so the runtime wrapper can preserve Rust validity
/// invariants while decoding raw device bytes.
pub trait BufferElement: private::Element {}

macro_rules! impl_buffer_element {
    ($type:ty, $element_type:expr) => {
        impl private::Element for $type {
            const IS_RAW_STORAGE_COMPATIBLE: bool = true;
            const IREE_ELEMENT_TYPE: u32 = $element_type;

            fn as_bytes(data: &[Self]) -> Cow<'_, [u8]> {
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        data.as_ptr() as *const u8,
                        core::mem::size_of_val(data),
                    )
                };
                Cow::Borrowed(bytes)
            }

            fn decode_slice(bytes: &[u8]) -> Result<Vec<Self>, RuntimeError> {
                let element_size = Self::element_size();
                debug_assert_ne!(element_size, 0);
                if !bytes.len().is_multiple_of(element_size) {
                    return Err(RuntimeError::InvalidArgument(format!(
                        "buffer byte length {} is not divisible by element size {}",
                        bytes.len(),
                        element_size
                    )));
                }

                let element_len = bytes.len() / element_size;
                let mut data = Vec::<Self>::with_capacity(element_len);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        data.as_mut_ptr() as *mut u8,
                        bytes.len(),
                    );
                    data.set_len(element_len);
                }
                Ok(data)
            }
        }

        impl BufferElement for $type {}
    };
}

impl_buffer_element!(
    u8,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_8
);
impl_buffer_element!(
    u16,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_16
);
impl_buffer_element!(
    u32,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_32
);
impl_buffer_element!(
    u64,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_64
);
impl_buffer_element!(
    i8,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_8
);
impl_buffer_element!(
    i16,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_16
);
impl_buffer_element!(
    i32,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_32
);
impl_buffer_element!(
    i64,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_64
);
impl_buffer_element!(
    f32,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_32
);
impl_buffer_element!(
    f64,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_64
);
#[cfg(feature = "half")]
impl_buffer_element!(
    f16,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_16
);
#[cfg(feature = "half")]
impl_buffer_element!(
    bf16,
    sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BFLOAT_16
);

impl private::Element for bool {
    const IS_RAW_STORAGE_COMPATIBLE: bool = false;
    const IREE_ELEMENT_TYPE: u32 = sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BOOL_8;

    fn as_bytes(data: &[Self]) -> Cow<'_, [u8]> {
        Cow::Owned(data.iter().map(|&value| u8::from(value)).collect())
    }

    fn decode_slice(bytes: &[u8]) -> Result<Vec<Self>, RuntimeError> {
        bytes
            .iter()
            .enumerate()
            .map(|(index, &value)| match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(RuntimeError::InvalidArgument(format!(
                    "invalid Bool8 value {value} at element {index}; expected 0 or 1"
                ))),
            })
            .collect()
    }
}

impl BufferElement for bool {}

/// A shaped and typed view into a HAL buffer.
pub struct BufferView<T: BufferElement> {
    pub(crate) ctx: *mut sys::iree_hal_buffer_view_t,
    device: Device,
    marker: PhantomData<T>,
    _not_send_sync: base::NotSendSync,
}

impl<T: BufferElement> BufferView<T> {
    /// Allocates a buffer on `device` and copies `data` into it.
    pub(crate) fn from_host(
        device: &Device,
        shape: &[usize],
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
        let encoded = T::as_bytes(data);
        let bytes: ConstByteSpan = encoded.as_ref().into();
        debug!("shape: {:?}", shape);
        debug!("data len: {}", encoded.len());
        base::Status::from_raw(unsafe {
            sys::iree_hal_buffer_view_allocate_buffer_copy(
                device.ctx,
                sys::iree_hal_device_allocator(device.ctx),
                shape.len(),
                shape.as_ptr(),
                T::IREE_ELEMENT_TYPE as sys::iree_hal_element_type_t,
                sys::iree_hal_encoding_types_t_IREE_HAL_ENCODING_TYPE_DENSE_ROW_MAJOR,
                sys::iree_hal_buffer_params_t {
                    usage: sys::iree_hal_buffer_usage_bits_t_IREE_HAL_BUFFER_USAGE_DEFAULT,
                    access: sys::iree_hal_memory_access_bits_t_IREE_HAL_MEMORY_ACCESS_ALL as _,
                    type_: sys::iree_hal_memory_type_bits_t_IREE_HAL_MEMORY_TYPE_DEVICE_LOCAL,
                    queue_affinity: 0,
                    min_alignment: 0,
                },
                bytes.ctx,
                &mut ctx,
            )
        })
        .into_result()?;
        Ok(Self {
            ctx,
            device: device.clone(),
            marker: PhantomData,
            _not_send_sync: base::not_send_sync(),
        })
    }

    pub(crate) unsafe fn from_raw_retained(
        ctx: *mut sys::iree_hal_buffer_view_t,
        device: &Device,
    ) -> Self {
        unsafe {
            sys::iree_hal_buffer_view_retain(ctx);
        }
        Self {
            ctx,
            device: device.clone(),
            marker: PhantomData,
            _not_send_sync: base::not_send_sync(),
        }
    }

    pub(crate) fn raw_buffer(&self) -> *mut sys::iree_hal_buffer_t {
        unsafe { sys::iree_hal_buffer_view_buffer(self.ctx) }
    }

    fn byte_length(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_byte_length(self.ctx) }
    }

    fn rank(&self) -> usize {
        unsafe { sys::iree_hal_buffer_view_shape_rank(self.ctx) }
    }

    pub fn shape(&self) -> Vec<usize> {
        let rank = self.rank();
        if rank == 0 {
            return Vec::new();
        }
        let dims = unsafe { sys::iree_hal_buffer_view_shape_dims(self.ctx) };
        unsafe { core::slice::from_raw_parts(dims, rank) }.to_vec()
    }

    /// Synchronously reads the buffer view contents back to host memory.
    pub fn read(&self) -> Result<Vec<T>, RuntimeError> {
        let element_type = unsafe { sys::iree_hal_buffer_view_element_type(self.ctx) };
        if element_type != T::IREE_ELEMENT_TYPE as sys::iree_hal_element_type_t {
            return Err(RuntimeError::InvalidArgument(format!(
                "buffer element type mismatch: expected {}, got {}",
                element_type_name(T::IREE_ELEMENT_TYPE as sys::iree_hal_element_type_t),
                element_type_name(element_type)
            )));
        }
        let byte_length = self.byte_length();
        if T::IS_RAW_STORAGE_COMPATIBLE {
            let element_size = T::element_size();
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
                    self.device.ctx,
                    self.raw_buffer(),
                    0,
                    data.as_mut_ptr() as *mut core::ffi::c_void,
                    byte_length,
                    sys::iree_hal_transfer_buffer_flag_bits_t_IREE_HAL_TRANSFER_BUFFER_FLAG_DEFAULT,
                    infinite_timeout(),
                )
            })
            .into_result()?;
            unsafe {
                data.set_len(byte_length / element_size);
            }
            return Ok(data);
        }

        let mut bytes = Vec::<u8>::with_capacity(byte_length);
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_transfer_d2h(
                self.device.ctx,
                self.raw_buffer(),
                0,
                bytes.as_mut_ptr() as *mut core::ffi::c_void,
                byte_length,
                sys::iree_hal_transfer_buffer_flag_bits_t_IREE_HAL_TRANSFER_BUFFER_FLAG_DEFAULT,
                infinite_timeout(),
            )
        })
        .into_result()?;
        unsafe {
            bytes.set_len(byte_length);
        }
        T::decode_slice(&bytes)
    }
}

impl<T: BufferElement> Clone for BufferView<T> {
    fn clone(&self) -> Self {
        unsafe { Self::from_raw_retained(self.ctx, &self.device) }
    }
}

impl<T: BufferElement> Debug for BufferView<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let formatted = unsafe {
            let buf = &mut [b'\0' as core::ffi::c_char; 1024];
            let mut len: usize = 0;
            sys::iree_hal_buffer_view_format(self.ctx, 6, buf.len(), buf.as_mut_ptr(), &mut len);
            let len = len.min(buf.len());
            let bytes = core::slice::from_raw_parts(buf.as_ptr() as *const u8, len);
            let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
            alloc::string::String::from_utf8_lossy(bytes).into_owned()
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

/// A dynamically typed runtime buffer value.
///
/// `BufferView<T>` remains the typed tensor handle. `Value` is only used at the
/// function invocation boundary, where IREE functions may accept and return
/// different buffer element types.
#[derive(Clone, Debug)]
pub enum Value {
    Bool(BufferView<bool>),
    U8(BufferView<u8>),
    U16(BufferView<u16>),
    U32(BufferView<u32>),
    U64(BufferView<u64>),
    I8(BufferView<i8>),
    I16(BufferView<i16>),
    I32(BufferView<i32>),
    I64(BufferView<i64>),
    #[cfg(feature = "half")]
    F16(BufferView<f16>),
    F32(BufferView<f32>),
    F64(BufferView<f64>),
    #[cfg(feature = "half")]
    Bf16(BufferView<bf16>),
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::U8(_) => "u8",
            Value::U16(_) => "u16",
            Value::U32(_) => "u32",
            Value::U64(_) => "u64",
            Value::I8(_) => "i8",
            Value::I16(_) => "i16",
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            #[cfg(feature = "half")]
            Value::F16(_) => "f16",
            Value::F32(_) => "f32",
            Value::F64(_) => "f64",
            #[cfg(feature = "half")]
            Value::Bf16(_) => "bf16",
        }
    }

    pub(crate) unsafe fn from_raw_retained(
        ctx: *mut sys::iree_hal_buffer_view_t,
        device: &Device,
    ) -> Result<Self, RuntimeError> {
        let element_type = unsafe { sys::iree_hal_buffer_view_element_type(ctx) };
        match element_type {
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BOOL_8 => Ok(Value::Bool(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_8 => Ok(Value::U8(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_16 => Ok(Value::U16(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_32 => Ok(Value::U32(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_UINT_64 => Ok(Value::U64(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_8 => Ok(Value::I8(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_16 => Ok(Value::I16(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_32 => Ok(Value::I32(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_INT_64 => Ok(Value::I64(unsafe {
                BufferView::from_raw_retained(ctx, device)
            })),
            #[cfg(feature = "half")]
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_16 => {
                Ok(Value::F16(unsafe {
                    BufferView::from_raw_retained(ctx, device)
                }))
            }
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_32 => {
                Ok(Value::F32(unsafe {
                    BufferView::from_raw_retained(ctx, device)
                }))
            }
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_FLOAT_64 => {
                Ok(Value::F64(unsafe {
                    BufferView::from_raw_retained(ctx, device)
                }))
            }
            #[cfg(feature = "half")]
            sys::iree_hal_element_types_t_IREE_HAL_ELEMENT_TYPE_BFLOAT_16 => {
                Ok(Value::Bf16(unsafe {
                    BufferView::from_raw_retained(ctx, device)
                }))
            }
            _ => Err(RuntimeError::InvalidArgument(format!(
                "unsupported buffer element type {}",
                element_type_name(element_type)
            ))),
        }
    }
}

macro_rules! impl_value_conversion {
    ($variant:ident, $type:ty, $name:literal) => {
        impl From<BufferView<$type>> for Value {
            fn from(buffer: BufferView<$type>) -> Self {
                Value::$variant(buffer)
            }
        }

        impl From<&BufferView<$type>> for Value {
            fn from(buffer: &BufferView<$type>) -> Self {
                Value::$variant(buffer.clone())
            }
        }

        impl TryFrom<Value> for BufferView<$type> {
            type Error = RuntimeError;

            fn try_from(value: Value) -> Result<Self, Self::Error> {
                match value {
                    Value::$variant(buffer) => Ok(buffer),
                    other => Err(RuntimeError::InvalidArgument(format!(
                        "value type mismatch: expected {}, got {}",
                        $name,
                        other.type_name()
                    ))),
                }
            }
        }
    };
}

impl_value_conversion!(Bool, bool, "bool");
impl_value_conversion!(U8, u8, "u8");
impl_value_conversion!(U16, u16, "u16");
impl_value_conversion!(U32, u32, "u32");
impl_value_conversion!(U64, u64, "u64");
impl_value_conversion!(I8, i8, "i8");
impl_value_conversion!(I16, i16, "i16");
impl_value_conversion!(I32, i32, "i32");
impl_value_conversion!(I64, i64, "i64");
#[cfg(feature = "half")]
impl_value_conversion!(F16, f16, "f16");
impl_value_conversion!(F32, f32, "f32");
impl_value_conversion!(F64, f64, "f64");
#[cfg(feature = "half")]
impl_value_conversion!(Bf16, bf16, "bf16");
