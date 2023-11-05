use std::ops::Deref;

use iree_sys::runtime as sys;
use tracing::debug;

use super::{
    api::{Instance, self},
    base::{self, ByteSpan},
    error::RuntimeError, hal::{BufferView, ToElementType},
};

pub struct Function<'a> {
    pub(crate) ctx: sys::iree_vm_function_t,
    pub(crate) session: &'a api::Session<'a>,
}


impl<'a> Function<'a> {
    pub fn invoke<T1, T2>(&self, input_list: &impl List<T1>, output_list: &impl List<T2>) 
        -> Result<(), RuntimeError>
        where T1: Type, T2: Type
    {
        base::Status::from_raw(unsafe {
            sys::iree_vm_invoke(
                self.session.context(),
                self.ctx,
                sys::iree_vm_invocation_flag_bits_t_IREE_VM_INVOCATION_FLAG_NONE,
                std::ptr::null_mut(),
                input_list.to_raw(),
                output_list.to_raw(),
                self.session.get_allocator().ctx
            )
        }).to_result()?;
        Ok(())
    }
}

pub trait Type {
    fn to_raw(instance: &Instance) -> sys::iree_vm_type_def_t;
}

pub trait ToValue: Sized {
    fn to_value(&self) -> Value<Self>;

    fn to_value_type() -> sys::iree_vm_value_type_e;
}

impl ToValue for i8 {
    fn to_value(&self) -> Value<Self> {
        let mut out = sys::iree_vm_value_t::default();
        out.type_ = Self::to_value_type();
        out.__bindgen_anon_1.i8_ = *self;
        Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_value_type() -> sys::iree_vm_value_type_e {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I8
    }
}

impl ToValue for i16 {
    fn to_value(&self) -> Value<Self> {
        let mut out = sys::iree_vm_value_t::default();
        out.type_ = Self::to_value_type();
        out.__bindgen_anon_1.i16_ = *self;
        Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_value_type() -> sys::iree_vm_value_type_e {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I16
    }
}

impl ToValue for i32 {
    fn to_value(&self) -> Value<Self> {
        let mut out = sys::iree_vm_value_t::default();
        out.type_ = Self::to_value_type();
        out.__bindgen_anon_1.i32_ = *self;
        Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_value_type() -> sys::iree_vm_value_type_e {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I32
    }
}

impl ToValue for i64 {
    fn to_value(&self) -> Value<Self> {
        let mut out = sys::iree_vm_value_t::default();
        out.type_ = Self::to_value_type();
        out.__bindgen_anon_1.i64_ = *self;
        Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_value_type() -> sys::iree_vm_value_type_e {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I64
    }
}

impl ToValue for f32 {
    fn to_value(&self) -> Value<Self> {
        let mut out = sys::iree_vm_value_t::default();
        out.type_ = Self::to_value_type();
        out.__bindgen_anon_1.f32_ = *self;
        Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_value_type() -> sys::iree_vm_value_type_e {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F32
    }
}

impl ToValue for f64 {
    fn to_value(&self) -> Value<Self> {
        let mut out = sys::iree_vm_value_t::default();
        out.type_ = Self::to_value_type();
        out.__bindgen_anon_1.f64_ = *self;
        Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        }
    }

    fn to_value_type() -> sys::iree_vm_value_type_e {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F64
    }
}

pub struct Value<T: ToValue> {
    pub(crate) ctx: sys::iree_vm_value_t,
    _marker: std::marker::PhantomData<T>,
}

impl<T: ToValue> Type for Value<T> {
    fn to_raw(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(T::to_value_type() as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        debug!("out: {:?}", out);
        out
    }
}

impl Value<i8> {
    pub fn from_value(&self) -> i8 {
        unsafe { self.ctx.__bindgen_anon_1.i8_ }
    }
}

impl Value<i16> {
    pub fn from_value(&self) -> i16 {
        unsafe { self.ctx.__bindgen_anon_1.i16_ }
    }
}

impl Value<i32> {
    pub fn from_value(&self) -> i32 {
        unsafe { self.ctx.__bindgen_anon_1.i32_ }
    }
}

impl Value<i64> {
    pub fn from_value(&self) -> i64 {
        unsafe { self.ctx.__bindgen_anon_1.i64_ }
    }
}

impl Value<f32> {
    pub fn from_value(&self) -> f32 {
        unsafe { self.ctx.__bindgen_anon_1.f32_ }
    }
}

impl Value<f64> {
    pub fn from_value(&self) -> f64 {
        unsafe { self.ctx.__bindgen_anon_1.f64_ }
    }
}


pub struct Ref<'a, T: ToRef> {
    pub(crate) ctx: sys::iree_vm_ref_t,
    pub(crate) _marker: std::marker::PhantomData<&'a T>,
}

impl<T: ToRef> Type for Ref<'_, T> {
    fn to_raw(instance: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(T::to_ref_type(instance) >> sys::IREE_VM_REF_TYPE_TAG_BITS as usize);
        out
    }
}

impl<T: ToRef> Drop for Ref<'_, T> {
    fn drop(&mut self) {
        unsafe {
            debug!("Dropping ref: {:?}", self.ctx); 
            sys::iree_vm_ref_release(&mut self.ctx);
        }
    }
}

impl<'a, T: ToElementType> Ref<'a, BufferView<'a, T>> {
    pub fn to_buffer_view(&self) -> BufferView<'a, T> {
        BufferView {
            ctx: self.ctx.ptr as *mut sys::iree_hal_buffer_view_t,
            marker: std::marker::PhantomData,
        }
    } 
}

pub trait ToRef: Sized {
    fn to_ref(&self, instance: &Instance) -> Result<Ref<Self>, RuntimeError>;
    fn to_ref_type(instance: &Instance) -> sys::iree_vm_ref_type_t;
}

pub struct Undefined;

impl Type for Undefined {
    fn to_raw(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        out
    }
}

pub trait List<T: Type> {
    fn to_raw(&self) -> *mut sys::iree_vm_list_t;

    fn instance(&self) -> &super::api::Instance;

    fn get_value<A: ToValue>(&self, idx: usize) -> Result<Value<A>, RuntimeError> {
        let mut out = sys::iree_vm_value_t::default();
        let status = unsafe { sys::iree_vm_list_get_value(self.to_raw(), idx, &mut out) };
        base::Status::from_raw(status).to_result()?;
        Ok(Value {
            ctx: out,
            _marker: std::marker::PhantomData,
        })
    }

    fn set_value<A: ToValue>(&self, idx: usize, value: Value<A>) -> Result<(), RuntimeError> {
        let status = unsafe { sys::iree_vm_list_set_value(self.to_raw(), idx, &value.ctx) };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    fn push_value<A: ToValue>(&self, value: Value<A>) -> Result<(), RuntimeError> {
        debug!("pushing value");
        let status = unsafe { sys::iree_vm_list_push_value(self.to_raw(), &value.ctx) };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    fn push_ref<A: ToRef>(&self, value: &Ref<A>) -> Result<(), RuntimeError> {
        let status = unsafe { sys::iree_vm_list_push_ref_retain(self.to_raw(), &value.ctx) };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    fn get_ref<A: ToRef>(&self, idx: usize) -> Result<Ref<A>, RuntimeError> {
        let mut out = sys::iree_vm_ref_t::default();
        let status = unsafe { sys::iree_vm_list_get_ref_retain(self.to_raw(), idx, &mut out) };
        base::Status::from_raw(status).to_result()?;
        Ok(Ref {
            ctx: out,
            _marker: std::marker::PhantomData,
        })
    }
}

pub struct StaticList<'a, T: Type> {
    pub(crate) ctx: *mut sys::iree_vm_list_t,
    instance: &'a super::api::Instance,
    _marker: std::marker::PhantomData<(ByteSpan<'a>, T)>,
}

impl<'a, T: Type> StaticList<'a, T> {
    pub fn new(
        storage: ByteSpan<'a>,
        capacity: usize,
        instance: &'a super::api::Instance,
    ) -> Result<Self, RuntimeError> {
        let mut out = std::ptr::null_mut();
        let status = unsafe {
            let size = sys::iree_vm_list_storage_size(&T::to_raw(instance), capacity);
            sys::iree_vm_list_initialize(storage.ctx, &T::to_raw(instance), size, &mut out)
        };
        base::Status::from_raw(status).to_result()?;
        Ok(Self {
            ctx: out,
            instance,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<'a, T: Type> List<T> for StaticList<'a, T> {
    fn to_raw(&self) -> *mut sys::iree_vm_list_t {
        self.ctx
    }

    fn instance(&self) -> &super::api::Instance {
        self.instance
    }
}

impl<T: Type> Drop for StaticList<'_, T> {
    fn drop(&mut self) {
        unsafe {
            debug!("dropping list");
            sys::iree_vm_list_deinitialize(self.ctx);
        }
    }
}

pub struct DynamicList<'a, T: Type> {
    pub(crate) ctx: *mut sys::iree_vm_list_t,
    _instance: &'a super::api::Instance,
    _marker: std::marker::PhantomData<T>,
}

impl<'a, T: Type> DynamicList<'a, T> {
    pub fn new(
        initial_capacity: usize,
        instance: &'a super::api::Instance,
    ) -> Result<Self, RuntimeError> {
        let mut out = std::ptr::null_mut();
        let status = unsafe {
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
            _marker: std::marker::PhantomData,
        })
    }

    pub fn capacity(&self) -> usize {
        unsafe { sys::iree_vm_list_capacity(self.ctx) }
    }

    pub fn reserve(&mut self, minimum_capacity: usize) -> Result<(), RuntimeError> {
        let status = unsafe { sys::iree_vm_list_reserve(self.ctx, minimum_capacity) };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    pub fn resize(&mut self, new_size: usize) -> Result<(), RuntimeError> {
        let status = unsafe { sys::iree_vm_list_resize(self.ctx, new_size) };
        base::Status::from_raw(status).to_result()?;
        Ok(())
    }

    pub fn clear(&mut self) {
        unsafe {
            sys::iree_vm_list_clear(self.ctx);
        }
    }
}

impl<'a, T: Type> List<T> for DynamicList<'a, T> {
    fn to_raw(&self) -> *mut sys::iree_vm_list_t {
        self.ctx
    }

    fn instance(&self) -> &'a super::api::Instance {
        self._instance
    }
}

impl<T: Type> Drop for DynamicList<'_, T> {
    fn drop(&mut self) {
        unsafe {
            debug!("dropping list");
            sys::iree_vm_list_release(self.ctx);
        }
    }
}
