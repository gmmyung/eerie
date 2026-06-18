extern crate alloc;

use alloc::{rc::Rc, string::String, vec::Vec};
use core::marker::PhantomData;
#[cfg(feature = "std")]
use std::sync::Mutex;

use eerie_sys::runtime as sys;
use log::trace;

use super::{
    base::{self, ConstByteSpan, StringView},
    error::RuntimeError,
    hal::{BufferElement, BufferView, Device},
};

#[cfg(feature = "std")]
static HAL_TYPE_ADAPTER_LOCK: Mutex<()> = Mutex::new(());

fn string_view_to_string(value: sys::iree_string_view_t) -> String {
    if value.data.is_null() || value.size == 0 {
        return String::new();
    }
    let bytes = unsafe { core::slice::from_raw_parts(value.data as *const u8, value.size) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn resolve_hal_types(instance: &Instance) -> Result<(), RuntimeError> {
    base::Status::from_raw(unsafe { sys::iree_hal_module_resolve_all_types(instance.ctx) })
        .to_result()
        .map_err(Into::into)
}

#[cfg(feature = "std")]
fn with_hal_type_adapter_lock<T>(
    f: impl FnOnce() -> Result<T, RuntimeError>,
) -> Result<T, RuntimeError> {
    let _guard = HAL_TYPE_ADAPTER_LOCK.lock().unwrap();
    f()
}

#[cfg(not(feature = "std"))]
fn with_hal_type_adapter_lock<T>(
    f: impl FnOnce() -> Result<T, RuntimeError>,
) -> Result<T, RuntimeError> {
    critical_section::with(|_| f())
}

fn with_hal_type_adapters<T>(
    instance: &Instance,
    f: impl FnOnce() -> Result<T, RuntimeError>,
) -> Result<T, RuntimeError> {
    with_hal_type_adapter_lock(|| {
        resolve_hal_types(instance)?;
        f()
    })
}

/// A VM instance owns registered VM ref types and VM-level host allocation.
pub struct Instance {
    pub(crate) ctx: *mut sys::iree_vm_instance_t,
}

impl Instance {
    pub fn new() -> Result<Self, RuntimeError> {
        let allocator = base::Allocator::get_global();
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_vm_instance_create(
                sys::IREE_VM_TYPE_CAPACITY_DEFAULT as usize,
                allocator.ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        let instance = Self { ctx };
        with_hal_type_adapter_lock(|| {
            base::Status::from_raw(unsafe { sys::iree_hal_module_register_all_types(ctx) })
                .to_result()?;
            resolve_hal_types(&instance)
        })?;
        Ok(instance)
    }

    pub(crate) fn allocator(&self) -> base::Allocator {
        base::Allocator {
            ctx: unsafe { sys::iree_vm_instance_allocator(self.ctx) },
        }
    }

    pub(crate) fn lookup_type(&self, name: &str) -> sys::iree_vm_ref_type_t {
        unsafe { sys::iree_vm_instance_lookup_type(self.ctx, StringView::from(name).ctx) }
    }
}

impl Clone for Instance {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_vm_instance_retain(self.ctx);
        }
        Self { ctx: self.ctx }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_instance_release(self.ctx);
        }
    }
}

/// A VM module retained by Rust.
pub struct Module {
    pub(crate) ctx: *mut sys::iree_vm_module_t,
    instance: Instance,
    archive: Option<Rc<[u8]>>,
}

impl Module {
    /// Creates the IREE HAL VM module bound to `device`.
    pub fn hal(instance: &Instance, device: &Device) -> Result<Self, RuntimeError> {
        with_hal_type_adapters(instance, || {
            let mut ctx = core::ptr::null_mut();
            let mut device_group = core::ptr::null_mut();
            base::Status::from_raw(unsafe {
                sys::iree_hal_device_group_create_from_device(
                    device.ctx,
                    instance.allocator().ctx,
                    &mut device_group,
                )
            })
            .to_result()?;
            let status = base::Status::from_raw(unsafe {
                sys::iree_hal_module_create(
                    instance.ctx,
                    sys::iree_hal_module_device_policy_default(),
                    device_group,
                    sys::iree_hal_module_flag_bits_t_IREE_HAL_MODULE_FLAG_SYNCHRONOUS,
                    sys::iree_hal_module_debug_sink_null(),
                    instance.allocator().ctx,
                    &mut ctx,
                )
            });
            unsafe {
                sys::iree_hal_device_group_release(device_group);
            }
            status.to_result()?;
            Ok(Self {
                ctx,
                instance: instance.clone(),
                archive: None,
            })
        })
    }

    /// Creates a VM bytecode module from a VMFB archive in memory.
    ///
    /// IREE does not retain or copy the archive bytes, so this method copies the
    /// input into ref-counted Rust-owned storage and keeps it alive with the
    /// retained module handle.
    pub fn bytecode(instance: &Instance, data: &[u8]) -> Result<Self, RuntimeError> {
        let archive = Rc::<[u8]>::from(data);
        let bytes: ConstByteSpan = archive.as_ref().into();
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_vm_bytecode_module_create(
                instance.ctx,
                sys::iree_vm_bytecode_module_flags_e_IREE_VM_BYTECODE_MODULE_FLAG_NONE,
                bytes.ctx,
                base::Allocator::null_allocator().ctx,
                instance.allocator().ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            archive: Some(archive),
        })
    }

    pub fn name(&self) -> String {
        string_view_to_string(unsafe { sys::iree_vm_module_name(self.ctx) })
    }

    pub fn signature(&self) -> ModuleSignature {
        let ctx = unsafe { sys::iree_vm_module_signature(self.ctx) };
        ModuleSignature {
            version: ctx.version,
            attr_count: ctx.attr_count,
            import_function_count: ctx.import_function_count,
            export_function_count: ctx.export_function_count,
            internal_function_count: ctx.internal_function_count,
        }
    }

    pub fn lookup_attr(&self, key: &str) -> Option<String> {
        let value = string_view_to_string(unsafe {
            sys::iree_vm_module_lookup_attr_by_name(self.ctx, StringView::from(key).ctx)
        });
        (!value.is_empty()).then_some(value)
    }

    pub fn attr(&self, index: usize) -> Result<Option<StringPair>, RuntimeError> {
        let mut pair = sys::iree_string_pair_t::default();
        let status = base::Status::from_raw(unsafe {
            sys::iree_vm_module_get_attr(self.ctx, index, &mut pair)
        });
        match status.to_result() {
            Ok(()) => Ok(Some(string_pair_to_owned(pair))),
            Err(err) => {
                let _ = err;
                Ok(None)
            }
        }
    }

    pub fn lookup_export_function(&self, name: &str) -> Result<FunctionRef, RuntimeError> {
        self.lookup_function(name, FunctionLinkage::Export)
    }

    pub fn lookup_function(
        &self,
        name: &str,
        linkage: FunctionLinkage,
    ) -> Result<FunctionRef, RuntimeError> {
        let mut ctx = sys::iree_vm_function_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_vm_module_lookup_function_by_name(
                self.ctx,
                linkage.into(),
                StringView::from(name).ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(FunctionRef { ctx })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSignature {
    pub version: u32,
    pub attr_count: usize,
    pub import_function_count: usize,
    pub export_function_count: usize,
    pub internal_function_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringPair {
    pub key: String,
    pub value: String,
}

fn string_pair_to_owned(pair: sys::iree_string_pair_t) -> StringPair {
    unsafe {
        StringPair {
            key: string_view_to_string(pair.__bindgen_anon_1.key),
            value: string_view_to_string(pair.__bindgen_anon_2.value),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionLinkage {
    Internal,
    Import,
    Export,
    ImportOptional,
    ExportOptional,
}

impl From<FunctionLinkage> for sys::iree_vm_function_linkage_t {
    fn from(value: FunctionLinkage) -> Self {
        match value {
            FunctionLinkage::Internal => {
                sys::iree_vm_function_linkage_e_IREE_VM_FUNCTION_LINKAGE_INTERNAL
            }
            FunctionLinkage::Import => {
                sys::iree_vm_function_linkage_e_IREE_VM_FUNCTION_LINKAGE_IMPORT
            }
            FunctionLinkage::Export => {
                sys::iree_vm_function_linkage_e_IREE_VM_FUNCTION_LINKAGE_EXPORT
            }
            FunctionLinkage::ImportOptional => {
                sys::iree_vm_function_linkage_e_IREE_VM_FUNCTION_LINKAGE_IMPORT_OPTIONAL
            }
            FunctionLinkage::ExportOptional => {
                sys::iree_vm_function_linkage_e_IREE_VM_FUNCTION_LINKAGE_EXPORT_OPTIONAL
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct FunctionRef {
    ctx: sys::iree_vm_function_t,
}

impl FunctionRef {
    pub fn name(&self) -> String {
        string_view_to_string(unsafe { sys::iree_vm_function_name(&self.ctx) })
    }

    pub fn signature(&self) -> FunctionSignature {
        Function::signature_from_raw(&self.ctx)
    }

    pub fn lookup_attr(&self, key: &str) -> Option<String> {
        let value = string_view_to_string(unsafe {
            sys::iree_vm_function_lookup_attr_by_name(&self.ctx, StringView::from(key).ctx)
        });
        (!value.is_empty()).then_some(value)
    }

    pub fn attr(&self, index: usize) -> Result<Option<StringPair>, RuntimeError> {
        let mut pair = sys::iree_string_pair_t::default();
        let status = base::Status::from_raw(unsafe {
            sys::iree_vm_function_get_attr(self.ctx, index, &mut pair)
        });
        match status.to_result() {
            Ok(()) => Ok(Some(string_pair_to_owned(pair))),
            Err(err) => {
                let _ = err;
                Ok(None)
            }
        }
    }
}

impl Clone for Module {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_vm_module_retain(self.ctx);
        }
        Self {
            ctx: self.ctx,
            instance: self.instance.clone(),
            archive: self.archive.clone(),
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_module_release(self.ctx);
        }
    }
}

/// A VM context with a fixed module set.
pub struct Context {
    pub(crate) ctx: *mut sys::iree_vm_context_t,
    instance: Instance,
    modules: Vec<Module>,
}

impl Context {
    pub fn with_modules(instance: &Instance, modules: &[&Module]) -> Result<Self, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        let mut raw_modules: Vec<*mut sys::iree_vm_module_t> =
            modules.iter().map(|module| module.ctx).collect();
        base::Status::from_raw(unsafe {
            sys::iree_vm_context_create_with_modules(
                instance.ctx,
                sys::iree_vm_context_flag_bits_t_IREE_VM_CONTEXT_FLAG_NONE,
                raw_modules.len(),
                raw_modules.as_mut_ptr(),
                instance.allocator().ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            modules: modules.iter().map(|module| (*module).clone()).collect(),
        })
    }

    pub fn resolve_function(&self, name: &str) -> Result<Function, RuntimeError> {
        let mut ctx = sys::iree_vm_function_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_vm_context_resolve_function(self.ctx, StringView::from(name).ctx, &mut ctx)
        })
        .to_result()?;
        Ok(Function {
            ctx,
            context: self.clone(),
        })
    }

    pub fn instance(&self) -> &Instance {
        &self.instance
    }
}

impl Clone for Context {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_vm_context_retain(self.ctx);
        }
        Self {
            ctx: self.ctx,
            instance: self.instance.clone(),
            modules: self.modules.clone(),
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_context_release(self.ctx);
        }
    }
}

/// An exported VM function resolved against a context.
pub struct Function {
    pub(crate) ctx: sys::iree_vm_function_t,
    context: Context,
}

impl Function {
    pub fn name(&self) -> String {
        string_view_to_string(unsafe { sys::iree_vm_function_name(&self.ctx) })
    }

    pub fn signature(&self) -> FunctionSignature {
        Self::signature_from_raw(&self.ctx)
    }

    fn signature_from_raw(function: &sys::iree_vm_function_t) -> FunctionSignature {
        let ctx = unsafe { sys::iree_vm_function_signature(function) };
        let calling_convention = string_view_to_string(ctx.calling_convention);
        let (argument_count, result_count) =
            Self::count_arguments_and_results(&ctx).unwrap_or((0, 0));
        FunctionSignature {
            calling_convention,
            argument_count,
            result_count,
        }
    }

    pub fn lookup_attr(&self, key: &str) -> Option<String> {
        let value = string_view_to_string(unsafe {
            sys::iree_vm_function_lookup_attr_by_name(&self.ctx, StringView::from(key).ctx)
        });
        (!value.is_empty()).then_some(value)
    }

    pub fn attr(&self, index: usize) -> Result<Option<StringPair>, RuntimeError> {
        let mut pair = sys::iree_string_pair_t::default();
        let status = base::Status::from_raw(unsafe {
            sys::iree_vm_function_get_attr(self.ctx, index, &mut pair)
        });
        match status.to_result() {
            Ok(()) => Ok(Some(string_pair_to_owned(pair))),
            Err(err) => {
                let _ = err;
                Ok(None)
            }
        }
    }

    fn count_arguments_and_results(
        signature: &sys::iree_vm_function_signature_t,
    ) -> Result<(usize, usize), RuntimeError> {
        let mut argument_count = 0usize;
        let mut result_count = 0usize;
        base::Status::from_raw(unsafe {
            sys::iree_vm_function_call_count_arguments_and_results(
                signature,
                &mut argument_count,
                &mut result_count,
            )
        })
        .to_result()?;
        Ok((argument_count, result_count))
    }

    pub fn invoke<I, O>(&self, inputs: &List<I>, outputs: &mut List<O>) -> Result<(), RuntimeError>
    where
        I: Type,
        O: Type,
    {
        with_hal_type_adapters(&self.context.instance, || {
            base::Status::from_raw(unsafe {
                trace!("iree_vm_invoke");
                sys::iree_vm_invoke(
                    self.context.ctx,
                    self.ctx,
                    sys::iree_vm_invocation_flag_bits_t_IREE_VM_INVOCATION_FLAG_NONE,
                    core::ptr::null(),
                    inputs.ctx,
                    outputs.ctx,
                    self.context.instance.allocator().ctx,
                )
            })
            .to_result()
            .map_err(Into::into)
        })
    }
}

/// Reflected VM function signature metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionSignature {
    pub calling_convention: String,
    pub argument_count: usize,
    pub result_count: usize,
}

impl Clone for Function {
    fn clone(&self) -> Self {
        Self {
            ctx: self.ctx,
            context: self.context.clone(),
        }
    }
}

/// Trait for VM list element types.
pub trait Type {
    fn type_def(instance: &Instance) -> sys::iree_vm_type_def_t;
}

/// Trait for values that can be stored in VM lists.
pub trait ToValue: Sized {
    fn to_value(&self) -> Value<Self>;
    fn value_type() -> sys::iree_vm_value_type_e;
}

macro_rules! impl_to_value {
    ($type:ty, $variant:ident, $enum:ident) => {
        impl ToValue for $type {
            fn to_value(&self) -> Value<Self> {
                let mut out = sys::iree_vm_value_t::default();
                out.type_ = Self::value_type();
                out.__bindgen_anon_1.$variant = *self;
                Value {
                    ctx: out,
                    _marker: PhantomData,
                }
            }

            fn value_type() -> sys::iree_vm_value_type_e {
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

/// A VM scalar value.
pub struct Value<T: ToValue> {
    pub(crate) ctx: sys::iree_vm_value_t,
    _marker: PhantomData<T>,
}

impl<T: ToValue> Type for Value<T> {
    fn type_def(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(T::value_type() as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        out
    }
}

macro_rules! impl_value {
    ($type:ty, $variant:ident) => {
        impl Value<$type> {
            pub fn get(&self) -> $type {
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

pub trait RefObject {
    fn ref_type(instance: &Instance) -> sys::iree_vm_ref_type_t;
}

/// Trait for objects that can be converted to VM refs.
pub trait ToRef: RefObject {
    fn to_ref(&self, instance: &Instance) -> Result<Ref<Self>, RuntimeError>
    where
        Self: Sized;
}

/// A VM reference.
pub struct Ref<T: RefObject> {
    pub(crate) ctx: sys::iree_vm_ref_t,
    instance: Instance,
    _marker: PhantomData<T>,
}

impl<T: RefObject> Type for Ref<T> {
    fn type_def(instance: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(
            T::ref_type(instance) / (1usize << sys::IREE_VM_REF_TYPE_TAG_BITS as usize),
        );
        out
    }
}

impl<T: RefObject> Clone for Ref<T> {
    fn clone(&self) -> Self {
        let mut source = self.ctx;
        let mut ctx = sys::iree_vm_ref_t::default();
        unsafe {
            sys::iree_vm_ref_retain(&mut source, &mut ctx);
        }
        Self {
            ctx,
            instance: self.instance.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T: RefObject> Drop for Ref<T> {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_vm_ref_release");
            sys::iree_vm_ref_release(&mut self.ctx);
        }
    }
}

impl<T: BufferElement> RefObject for BufferView<T> {
    fn ref_type(instance: &Instance) -> sys::iree_vm_ref_type_t {
        instance.lookup_type("hal.buffer_view")
    }
}

impl<T: BufferElement> ToRef for BufferView<T> {
    fn to_ref(&self, instance: &Instance) -> Result<Ref<Self>, RuntimeError> {
        let mut ctx = sys::iree_vm_ref_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_vm_ref_wrap_retain(
                self.ctx as *mut core::ffi::c_void,
                Self::ref_type(instance),
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Ref {
            ctx,
            instance: instance.clone(),
            _marker: PhantomData,
        })
    }
}

impl<T: BufferElement> Ref<BufferView<T>> {
    pub fn to_buffer_view(&self) -> Result<BufferView<T>, RuntimeError> {
        if self.ctx.type_ != BufferView::<T>::ref_type(&self.instance) {
            return Err(RuntimeError::InvalidArgument(String::from(
                "VM ref type mismatch: expected hal.buffer_view",
            )));
        }
        let buffer = unsafe {
            BufferView::from_raw_retained(self.ctx.ptr as *mut sys::iree_hal_buffer_view_t)
        };
        if buffer.element_type() != T::element_type() {
            return Err(RuntimeError::InvalidArgument(alloc::format!(
                "buffer element type mismatch: expected {:?}, got {:?}",
                T::element_type(),
                buffer.element_type()
            )));
        }
        Ok(buffer)
    }
}

/// Undefined list element type. Use for heterogenous VM argument/result lists.
pub struct Undefined;

impl Type for Undefined {
    fn type_def(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        out
    }
}

/// Any VM ref list element type.
pub struct AnyRef;

impl Type for AnyRef {
    fn type_def(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(
            sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_ANY as usize
                / (1usize << sys::IREE_VM_REF_TYPE_TAG_BITS as usize),
        );
        out
    }
}

/// An owned dynamic VM list.
pub struct List<T: Type> {
    pub(crate) ctx: *mut sys::iree_vm_list_t,
    instance: Instance,
    _marker: PhantomData<T>,
}

impl<T: Type> List<T> {
    pub fn new(initial_capacity: usize, instance: &Instance) -> Result<Self, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_vm_list_create(
                T::type_def(instance),
                initial_capacity,
                instance.allocator().ctx,
                &mut ctx,
            )
        })
        .to_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            _marker: PhantomData,
        })
    }

    pub fn capacity(&self) -> usize {
        unsafe { sys::iree_vm_list_capacity(self.ctx) }
    }

    pub fn reserve(&mut self, minimum_capacity: usize) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_vm_list_reserve(self.ctx, minimum_capacity) })
            .to_result()
            .map_err(Into::into)
    }

    pub fn resize(&mut self, new_size: usize) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_vm_list_resize(self.ctx, new_size) })
            .to_result()
            .map_err(Into::into)
    }

    pub fn clear(&mut self) {
        unsafe {
            sys::iree_vm_list_clear(self.ctx);
        }
    }

    pub fn push_value<A: ToValue>(&mut self, value: Value<A>) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_vm_list_push_value(self.ctx, &value.ctx) })
            .to_result()
            .map_err(Into::into)
    }

    pub fn set_value<A: ToValue>(
        &mut self,
        idx: usize,
        value: Value<A>,
    ) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_vm_list_set_value(self.ctx, idx, &value.ctx) })
            .to_result()
            .map_err(Into::into)
    }

    pub fn get_value<A: ToValue>(&self, idx: usize) -> Result<Value<A>, RuntimeError> {
        let mut ctx = sys::iree_vm_value_t::default();
        base::Status::from_raw(unsafe { sys::iree_vm_list_get_value(self.ctx, idx, &mut ctx) })
            .to_result()?;
        if ctx.type_ != A::value_type() {
            return Err(RuntimeError::InvalidArgument(String::from(
                "VM value type mismatch",
            )));
        }
        Ok(Value {
            ctx,
            _marker: PhantomData,
        })
    }

    pub fn push_ref<A: RefObject>(&mut self, value: &Ref<A>) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_vm_list_push_ref_retain(self.ctx, &value.ctx) })
            .to_result()
            .map_err(Into::into)
    }

    pub fn get_ref<A: RefObject>(&self, idx: usize) -> Result<Ref<A>, RuntimeError> {
        let mut ctx = sys::iree_vm_ref_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_vm_list_get_ref_retain(self.ctx, idx, &mut ctx)
        })
        .to_result()?;
        if ctx.type_ != A::ref_type(&self.instance) {
            unsafe {
                sys::iree_vm_ref_release(&mut ctx);
            }
            return Err(RuntimeError::InvalidArgument(String::from(
                "VM ref type mismatch",
            )));
        }
        Ok(Ref {
            ctx,
            instance: self.instance.clone(),
            _marker: PhantomData,
        })
    }
}

impl<T: Type> Clone for List<T> {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_vm_list_retain(self.ctx);
        }
        Self {
            ctx: self.ctx,
            instance: self.instance.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T: Type> Drop for List<T> {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_list_release(self.ctx);
        }
    }
}
