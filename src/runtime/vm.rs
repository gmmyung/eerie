use eerie_sys::runtime as sys;
use log::trace;

use super::{
    api::{self, Instance},
    base::{self, ByteSpan},
    error::RuntimeError,
    hal::{BufferView, ToElementType},
};

/// An IREE function reference.
pub struct Function<'a> {
    pub(crate) ctx: sys::iree_vm_function_t,
    pub(crate) session: &'a api::Session<'a>,
}

impl<'a> Function<'a> {
    /// Synchronously invokes the function with the given arguments.
    /// The function will be run to completion and may block on external resources.
    pub fn invoke<'b, T1, T2>(
        &self,
        input_list: &impl List<'b, T1>,
        output_list: &impl List<'b, T2>,
    ) -> Result<(), RuntimeError>
    where
        T1: Type,
        T2: Type,
    {
        base::Status::from_raw(unsafe {
            trace!("iree_vm_invoke");
            sys::iree_vm_invoke(
                self.session.context(),
                self.ctx,
                sys::iree_vm_invocation_flag_bits_t_IREE_VM_INVOCATION_FLAG_NONE,
                core::ptr::null_mut(),
                input_list.to_raw(),
                output_list.to_raw(),
                self.session.get_allocator().ctx,
            )
        })
        .to_result()?;
        Ok(())
    }
}

/// Trait for types that can be used as a List type.
pub trait Type {
    fn to_raw(instance: &Instance) -> sys::iree_vm_type_def_t;
}

/// Trait for types that can be used as VM values.
pub trait ToValue: Sized {
    fn to_value(&self) -> Value<Self>;

    fn to_value_type() -> sys::iree_vm_value_type_e;
}

// Macro to implement ToValue for primitive types.
macro_rules! impl_to_value {
    ($type:ty, $variant:ident, $enum:ident) => {
        impl ToValue for $type {
            fn to_value(&self) -> Value<Self> {
                let mut out = sys::iree_vm_value_t::default();
                out.type_ = Self::to_value_type();
                out.__bindgen_anon_1.$variant = *self;
                Value {
                    ctx: out,
                    _marker: core::marker::PhantomData,
                }
            }

            fn to_value_type() -> sys::iree_vm_value_type_e {
                sys::$enum
            }
        }
    };
}

impl_to_value!(i8, i8_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I8);
impl_to_value!(i16, i16_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I16);
impl_to_value!(i32, i32_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I32);
impl_to_value!(i64, i64_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I64);
impl_to_value!(f32, f32_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F32);
impl_to_value!(f64, f64_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F64);

/// VM value type, used for passing values to functions. Value is a reference counted type.
pub struct Value<T: ToValue> {
    pub(crate) ctx: sys::iree_vm_value_t,
    _marker: core::marker::PhantomData<T>,
}

// This means that Value can be inserted into a List.
impl<T: ToValue> Type for Value<T> {
    fn to_raw(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(T::to_value_type() as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        out
    }
}

// Macro to implement Value for primitive types.
macro_rules! impl_value {
    ($type:ty, $variant:ident) => {
        impl Value<$type> {
            pub fn from_value(&self) -> $type {
                unsafe { self.ctx.__bindgen_anon_1.$variant }
            }
        }
    };
}

impl_value!(i8, i8_);
impl_value!(i16, i16_);
impl_value!(i32, i32_);
impl_value!(i64, i64_);
impl_value!(f32, f32_);
impl_value!(f64, f64_);

/// VM Ref type, used for passing reference to things like HAL buffers. Ref is a reference counted type.
pub struct Ref<'a, T: ToRef<'a>> {
    pub(crate) ctx: sys::iree_vm_ref_t,
    pub(crate) _instance: &'a Instance,
    pub(crate) _marker: core::marker::PhantomData<T>,
}

// This means that Ref can be inserted into a List.
impl<'a, T: ToRef<'a>> Type for Ref<'a, T> {
    fn to_raw(instance: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(T::to_ref_type(instance) >> sys::IREE_VM_REF_TYPE_TAG_BITS as usize);
        out
    }
}

impl<'a, T: ToRef<'a>> Drop for Ref<'a, T> {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_vm_ref_release");
            sys::iree_vm_ref_release(&mut self.ctx);
        }
    }
}

impl<'a, T: ToElementType> Ref<'a, BufferView<'a, T>> {
    /// Returns Ref to the BufferView
    pub fn to_buffer_view(&self, session: &'a api::Session) -> BufferView<'a, T> {
        BufferView {
            ctx: self.ctx.ptr as *mut sys::iree_hal_buffer_view_t,
            session,
            marker: core::marker::PhantomData,
        }
    }
}

/// Trait for types that can be used as VM references.
pub trait ToRef<'a>: Sized {
    fn to_ref(&'a self, instance: &'a Instance) -> Result<Ref<Self>, RuntimeError>;
    fn to_ref_type(instance: &Instance) -> sys::iree_vm_ref_type_t;
}

/// Undefined type, any type can be inserted into it.
pub struct Undefined;

impl Type for Undefined {
    fn to_raw(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        out
    }
}

pub(crate) trait IsList<'a, T: Type> {
    fn to_raw(&self) -> *mut sys::iree_vm_list_t;

    fn instance(&self) -> &super::api::Instance;

    fn from_raw(instance: &'a super::api::Instance, raw: *mut sys::iree_vm_list_t) -> Self;
}

/// List type, used for passing lists of values to functions.
#[allow(private_bounds)]
// Private bounds are needed because the IsList cannot be implemented for other types.
pub trait List<'a, T: Type>: IsList<'a, T> {
    /// Returns value at the given index. The caller must specify the type of the value, which
    /// must match the type of the value at the given index.
    fn get_value<A: ToValue>(&self, idx: usize) -> Result<Value<A>, RuntimeError> {
        let mut out = sys::iree_vm_value_t::default();
        let status = unsafe {
            trace!("iree_vm_list_get_value, idx: {}", idx);
            sys::iree_vm_list_get_value(self.to_raw(), idx, &mut out)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(Value {
            ctx: out,
            _marker: core::marker::PhantomData,
        })
    }

    /// Sets value at the given index. The caller must specify the type of the value, which
    /// must match the type of the value at the given index.
    fn set_value<A: ToValue>(&self, idx: usize, value: Value<A>) -> Result<(), RuntimeError> {
        let status = unsafe {
            trace!("iree_vm_list_set_value, idx: {}", idx);
            sys::iree_vm_list_set_value(self.to_raw(), idx, &value.ctx)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    /// Push value to the end of the list. Value is a reference counted object, so it will be
    /// retained.
    fn push_value<A: ToValue>(&self, value: Value<A>) -> Result<(), RuntimeError> {
        let status = unsafe {
            trace!("iree_vm_list_push_value");
            sys::iree_vm_list_push_value(self.to_raw(), &value.ctx)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    /// Push a Ref to the end of the list. Ref is a reference counted object, so it will be
    /// retained.
    fn push_ref<A: ToRef<'a>>(&self, value: &Ref<'a, A>) -> Result<(), RuntimeError> {
        let status = unsafe {
            trace!("iree_vm_list_push_ref_retain");
            sys::iree_vm_list_push_ref_retain(self.to_raw(), &value.ctx)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    /// Get a Ref from the list. The caller must specify the type of the value, which
    /// must match the type of the Ref at the given index.
    fn get_ref<A: ToRef<'a>>(&'a self, idx: usize) -> Result<Ref<'a, A>, RuntimeError> {
        let mut out = sys::iree_vm_ref_t::default();
        let status = unsafe {
            trace!("iree_vm_list_get_ref_retain, idx: {}", idx);
            sys::iree_vm_list_get_ref_retain(self.to_raw(), idx, &mut out)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(Ref {
            ctx: out,
            _instance: self.instance(),
            _marker: core::marker::PhantomData,
        })
    }
}

/// Static list type, used for passing lists of values to functions. Use when the size of the list
/// is known at compile time.
pub struct StaticList<'a, T: Type> {
    pub(crate) ctx: *mut sys::iree_vm_list_t,
    instance: &'a super::api::Instance,
    _marker: core::marker::PhantomData<(ByteSpan<'a>, T)>,
}

impl<'a, T: Type> StaticList<'a, T> {
    /// Creates a new static list with the given capacity at the given buffer.
    pub fn new(
        storage: ByteSpan<'a>,
        capacity: usize,
        instance: &'a super::api::Instance,
    ) -> Result<Self, RuntimeError> {
        let mut out = core::ptr::null_mut();
        let status = unsafe {
            trace!("iree_vm_list_storage_size");
            let size = sys::iree_vm_list_storage_size(&T::to_raw(instance), capacity);
            trace!("iree_vm_list_initialize, size: {}", size);
            sys::iree_vm_list_initialize(storage.ctx, &T::to_raw(instance), size, &mut out)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(Self {
            ctx: out,
            instance,
            _marker: core::marker::PhantomData,
        })
    }
}

impl<'a, T: Type> List<'a, T> for StaticList<'a, T> {}

impl<'a, T: Type> IsList<'a, T> for StaticList<'a, T> {
    fn to_raw(&self) -> *mut sys::iree_vm_list_t {
        self.ctx
    }

    fn instance(&self) -> &super::api::Instance {
        self.instance
    }

    fn from_raw(instance: &'a super::api::Instance, raw: *mut sys::iree_vm_list_t) -> Self {
        Self {
            ctx: raw,
            instance,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<T: Type> Drop for StaticList<'_, T> {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_vm_list_deinitialize");
            sys::iree_vm_list_deinitialize(self.ctx);
        }
    }
}

pub struct DynamicList<'a, T: Type> {
    pub(crate) ctx: *mut sys::iree_vm_list_t,
    _instance: &'a super::api::Instance,
    _marker: core::marker::PhantomData<T>,
}

impl<'a, T: Type> DynamicList<'a, T> {
    /// Creates a new dynamic list with the given capacity.
    pub fn new(
        initial_capacity: usize,
        instance: &'a super::api::Instance,
    ) -> Result<Self, RuntimeError> {
        let mut out = core::ptr::null_mut();
        let status = unsafe {
            trace!("iree_vm_list_create");
            sys::iree_vm_list_create(
                T::to_raw(instance),
                initial_capacity,
                instance.get_host_allocator().ctx,
                &mut out,
            )
        };
        base::Status::from_raw(status).to_result()?;
        Ok(Self {
            ctx: out,
            _instance: instance,
            _marker: core::marker::PhantomData,
        })
    }

    pub fn capacity(&self) -> usize {
        unsafe {
            trace!("iree_vm_list_capacity");
            sys::iree_vm_list_capacity(self.ctx)
        }
    }

    pub fn reserve(&mut self, minimum_capacity: usize) -> Result<(), RuntimeError> {
        let status = unsafe {
            trace!(
                "iree_vm_list_reserve, minimum_capacity: {}",
                minimum_capacity
            );
            sys::iree_vm_list_reserve(self.ctx, minimum_capacity)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    pub fn resize(&mut self, new_size: usize) -> Result<(), RuntimeError> {
        let status = unsafe {
            trace!("iree_vm_list_resize, new_size: {}", new_size);
            sys::iree_vm_list_resize(self.ctx, new_size)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    pub fn clear(&mut self) {
        unsafe {
            trace!("iree_vm_list_clear");
            sys::iree_vm_list_clear(self.ctx);
        }
    }
}

impl<'a, T: Type> List<'a, T> for DynamicList<'a, T> {}

impl<'a, T: Type> IsList<'a, T> for DynamicList<'a, T> {
    fn to_raw(&self) -> *mut sys::iree_vm_list_t {
        self.ctx
    }

    fn from_raw(instance: &'a super::api::Instance, raw: *mut sys::iree_vm_list_t) -> Self {
        Self {
            ctx: raw,
            _instance: instance,
            _marker: core::marker::PhantomData,
        }
    }

    fn instance(&self) -> &'a super::api::Instance {
        self._instance
    }
}

impl<T: Type> Drop for DynamicList<'_, T> {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_vm_list_release");
            sys::iree_vm_list_release(self.ctx);
        }
    }
}
