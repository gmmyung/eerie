use eerie_sys::runtime as sys;

use crate::runtime::error;

pub struct Value<T: ToValue + Sized> {
    pub(super) ctx: sys::iree_vm_value_t,
    marker: core::marker::PhantomData<T>,
}

impl<T: ToValue> Value<T> {
    pub fn get(&self) -> T {
        T::get_value(self)
    }
}

impl<T: ToValue> From<T> for Value<T> {
    fn from(value: T) -> Self {
        value.to_value()
    }
}

impl<T: ToValue> Value<T> {
    pub fn from_raw(ctx: sys::iree_vm_value_t) -> Result<Self, error::RuntimeError> {
        if ctx.type_ == T::to_value_type() {
            Ok(Value {
                ctx,
                marker: core::marker::PhantomData,
            })
        } else {
            Err(error::RuntimeError::ValueTypeMismatch)
        }
    }
}

pub trait ToValue: Sized {
    fn to_value(&self) -> Value<Self>;
    fn get_value(value: &Value<Self>) -> Self;
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
                    marker: core::marker::PhantomData,
                }
            }

            fn get_value(value: &Value<Self>) -> Self {
                unsafe { value.ctx.__bindgen_anon_1.$variant }
            }

            fn to_value_type() -> sys::iree_vm_value_type_e {
                sys::$enum
            }
        }
    };
}

fn value_type_to_string(value_type: sys::iree_vm_value_type_e) -> &'static str {
    match value_type {
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I8 => "i8",
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I16 => "i16",
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I32 => "i32",
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I64 => "i64",
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F32 => "f32",
        sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F64 => "f64",
        _ => "unknown",
    }
}

impl_to_value!(i8, i8_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I8);
impl_to_value!(i16, i16_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I16);
impl_to_value!(i32, i32_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I32);
impl_to_value!(i64, i64_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_I64);
impl_to_value!(f32, f32_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F32);
impl_to_value!(f64, f64_, iree_vm_value_type_e_IREE_VM_VALUE_TYPE_F64);
