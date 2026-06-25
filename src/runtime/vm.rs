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
    hal::{BufferElement, BufferView, Device, Value},
};

mod private {
    pub trait Sealed {}
}

#[cfg(feature = "std")]
// Owns the root VM instance ref for the program lifetime. HAL type adapters are
// process-global in IREE, so the root is intentionally not torn down/recreated.
static GLOBAL_INSTANCE: Mutex<Option<usize>> = Mutex::new(None);

#[cfg(not(feature = "std"))]
// Owns the root VM instance ref for the program lifetime. HAL type adapters are
// process-global in IREE, so the root is intentionally not torn down/recreated.
static mut GLOBAL_INSTANCE: usize = 0;

fn create_registered_instance() -> Result<*mut sys::iree_vm_instance_t, RuntimeError> {
    let _guard = base::runtime_lifecycle_guard();
    let allocator = base::Allocator::get_global();
    let mut ctx = core::ptr::null_mut();
    base::Status::from_raw(unsafe {
        sys::iree_vm_instance_create(
            sys::IREE_VM_TYPE_CAPACITY_DEFAULT as usize,
            allocator.ctx,
            &mut ctx,
        )
    })
    .into_result()?;

    let status = base::Status::from_raw(unsafe { sys::iree_hal_module_register_all_types(ctx) });
    if let Err(err) = status.into_result() {
        unsafe {
            sys::iree_vm_instance_release(ctx);
        }
        return Err(err.into());
    }

    Ok(ctx)
}

#[cfg(feature = "std")]
fn global_instance() -> Result<Instance, RuntimeError> {
    let mut global = GLOBAL_INSTANCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let ctx = match *global {
        Some(ctx) => ctx as *mut sys::iree_vm_instance_t,
        None => {
            let ctx = create_registered_instance()?;
            *global = Some(ctx as usize);
            ctx
        }
    };
    Ok(Instance::retain_raw(ctx))
}

#[cfg(not(feature = "std"))]
fn global_instance() -> Result<Instance, RuntimeError> {
    critical_section::with(|_| unsafe {
        if GLOBAL_INSTANCE == 0 {
            GLOBAL_INSTANCE = create_registered_instance()? as usize;
        }
        Ok(Instance::retain_raw(
            GLOBAL_INSTANCE as *mut sys::iree_vm_instance_t,
        ))
    })
}

/// A VM instance owns registered VM ref types and VM-level host allocation.
pub(crate) struct Instance {
    pub(crate) ctx: *mut sys::iree_vm_instance_t,
    _not_send_sync: base::NotSendSync,
}

impl Instance {
    /// Returns a retained handle to the process-wide VM instance.
    ///
    /// This does not create an independent IREE VM instance.
    /// IREE expects hosting applications to share VM instances across contexts.
    /// HAL type registration uses process-global adapter slots, so the safe
    /// runtime API exposes one shared instance instead of creating independent
    /// HAL-registered instances.
    ///
    /// The root VM instance is retained for the lifetime of the process or
    /// embedded program. Dropping an `Instance` releases the returned handle,
    /// but does not tear down the shared runtime root.
    pub(crate) fn global() -> Result<Self, RuntimeError> {
        global_instance()
    }

    fn retain_raw(ctx: *mut sys::iree_vm_instance_t) -> Self {
        unsafe {
            sys::iree_vm_instance_retain(ctx);
        }
        Self {
            ctx,
            _not_send_sync: base::not_send_sync(),
        }
    }

    pub(crate) fn allocator(&self) -> base::Allocator {
        base::Allocator {
            ctx: unsafe { sys::iree_vm_instance_allocator(self.ctx) },
        }
    }
}

impl Clone for Instance {
    fn clone(&self) -> Self {
        Self::retain_raw(self.ctx)
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
pub(crate) struct Module {
    pub(crate) ctx: *mut sys::iree_vm_module_t,
    instance: Instance,
    archive: Option<Rc<[u8]>>,
    _not_send_sync: base::NotSendSync,
}

impl Module {
    /// Creates the IREE HAL VM module bound to `device`.
    pub(crate) fn hal(instance: &Instance, device: &Device) -> Result<Self, RuntimeError> {
        let _guard = base::runtime_lifecycle_guard();

        let mut ctx = core::ptr::null_mut();
        let mut device_group = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_hal_device_group_create_from_device(
                device.ctx,
                instance.allocator().ctx,
                &mut device_group,
            )
        })
        .into_result()?;
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
        status.into_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            archive: None,
            _not_send_sync: base::not_send_sync(),
        })
    }

    /// Creates a VM bytecode module from a VMFB archive in memory.
    ///
    /// IREE does not retain or copy the archive bytes, so this method copies the
    /// input into ref-counted Rust-owned storage and keeps it alive with the
    /// retained module handle.
    pub(crate) fn bytecode(instance: &Instance, data: &[u8]) -> Result<Self, RuntimeError> {
        let _guard = base::runtime_lifecycle_guard();

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
        .into_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            archive: Some(archive),
            _not_send_sync: base::not_send_sync(),
        })
    }

    fn release_unlocked(&mut self) {
        if self.ctx.is_null() {
            return;
        }
        unsafe {
            sys::iree_vm_module_release(self.ctx);
        }
        self.ctx = core::ptr::null_mut();
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
            _not_send_sync: base::not_send_sync(),
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        if self.ctx.is_null() {
            return;
        }

        let _guard = base::runtime_lifecycle_guard();

        self.release_unlocked();
    }
}

/// A VM context with a fixed module set.
pub(crate) struct Context {
    pub(crate) ctx: *mut sys::iree_vm_context_t,
    instance: Instance,
    modules: Vec<Module>,
    _not_send_sync: base::NotSendSync,
}

impl Context {
    pub(crate) fn with_modules(
        instance: &Instance,
        modules: &[&Module],
    ) -> Result<Self, RuntimeError> {
        let _guard = base::runtime_lifecycle_guard();

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
        .into_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            modules: modules.iter().map(|module| (*module).clone()).collect(),
            _not_send_sync: base::not_send_sync(),
        })
    }

    pub(crate) fn resolve_function(&self, name: &str) -> Result<Function, RuntimeError> {
        let mut ctx = sys::iree_vm_function_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_vm_context_resolve_function(self.ctx, StringView::from(name).ctx, &mut ctx)
        })
        .into_result()?;
        Ok(Function {
            ctx,
            context: self.clone(),
            _not_send_sync: base::not_send_sync(),
        })
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
            _not_send_sync: base::not_send_sync(),
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        let _guard = base::runtime_lifecycle_guard();

        unsafe {
            sys::iree_vm_context_release(self.ctx);
        }
        self.ctx = core::ptr::null_mut();

        for mut module in self.modules.drain(..) {
            module.release_unlocked();
        }
    }
}

/// An exported VM function resolved against a context.
pub(crate) struct Function {
    pub(crate) ctx: sys::iree_vm_function_t,
    context: Context,
    _not_send_sync: base::NotSendSync,
}

impl Function {
    pub(crate) fn result_count(&self) -> Result<usize, RuntimeError> {
        let signature = unsafe { sys::iree_vm_function_signature(&self.ctx) };
        let mut argument_count = 0usize;
        let mut result_count = 0usize;
        base::Status::from_raw(unsafe {
            sys::iree_vm_function_call_count_arguments_and_results(
                &signature,
                &mut argument_count,
                &mut result_count,
            )
        })
        .into_result()?;
        Ok(result_count)
    }

    pub(crate) fn invoke<I, O>(
        &self,
        inputs: &List<I>,
        outputs: &mut List<O>,
    ) -> Result<(), RuntimeError>
    where
        I: Type,
        O: Type,
    {
        let _guard = base::runtime_invocation_guard();
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
        .into_result()
        .map_err(Into::into)
    }
}

impl Clone for Function {
    fn clone(&self) -> Self {
        Self {
            ctx: self.ctx,
            context: self.context.clone(),
            _not_send_sync: base::not_send_sync(),
        }
    }
}

/// Trait for VM list element types.
pub(crate) trait Type: private::Sealed {
    fn type_def(instance: &Instance) -> sys::iree_vm_type_def_t;
}

pub(crate) trait RefObject: private::Sealed {
    fn ref_type(instance: &Instance) -> sys::iree_vm_ref_type_t;
}

/// Trait for objects that can be converted to VM refs.
pub(crate) trait ToRef: RefObject {
    fn to_ref(&self, instance: &Instance) -> Result<Ref<Self>, RuntimeError>
    where
        Self: Sized;
}

/// A VM reference.
pub(crate) struct Ref<T: RefObject> {
    pub(crate) ctx: sys::iree_vm_ref_t,
    instance: Instance,
    _marker: PhantomData<T>,
    _not_send_sync: base::NotSendSync,
}

impl<T: RefObject> private::Sealed for Ref<T> {}

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
            _not_send_sync: base::not_send_sync(),
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
    fn ref_type(_instance: &Instance) -> sys::iree_vm_ref_type_t {
        unsafe { sys::iree_hal_buffer_view_registration }
    }
}

impl<T: BufferElement> private::Sealed for BufferView<T> {}

impl<T: BufferElement> ToRef for BufferView<T> {
    fn to_ref(&self, instance: &Instance) -> Result<Ref<Self>, RuntimeError> {
        let ctx = unsafe { sys::iree_hal_buffer_view_retain_ref(self.ctx) };
        Ok(Ref {
            ctx,
            instance: instance.clone(),
            _marker: PhantomData,
            _not_send_sync: base::not_send_sync(),
        })
    }
}

/// Undefined list element type. Use for heterogenous VM argument/result lists.
pub(crate) struct Undefined;

impl private::Sealed for Undefined {}

impl Type for Undefined {
    fn type_def(_: &Instance) -> sys::iree_vm_type_def_t {
        let mut out = sys::iree_vm_type_def_t::default();
        out.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        out.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        out
    }
}

/// An owned dynamic VM list.
pub(crate) struct List<T: Type> {
    pub(crate) ctx: *mut sys::iree_vm_list_t,
    instance: Instance,
    _marker: PhantomData<T>,
    _not_send_sync: base::NotSendSync,
}

impl<T: Type> List<T> {
    pub(crate) fn new(initial_capacity: usize, instance: &Instance) -> Result<Self, RuntimeError> {
        let mut ctx = core::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_vm_list_create(
                T::type_def(instance),
                initial_capacity,
                instance.allocator().ctx,
                &mut ctx,
            )
        })
        .into_result()?;
        Ok(Self {
            ctx,
            instance: instance.clone(),
            _marker: PhantomData,
            _not_send_sync: base::not_send_sync(),
        })
    }

    pub(crate) fn push_ref<A: RefObject>(&mut self, value: &Ref<A>) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe { sys::iree_vm_list_push_ref_retain(self.ctx, &value.ctx) })
            .into_result()
            .map_err(Into::into)
    }

    fn get_ref(&self, idx: usize) -> Result<sys::iree_vm_ref_t, RuntimeError> {
        let mut ctx = sys::iree_vm_ref_t::default();
        base::Status::from_raw(unsafe {
            sys::iree_vm_list_get_ref_retain(self.ctx, idx, &mut ctx)
        })
        .into_result()?;
        Ok(ctx)
    }
}

impl List<Undefined> {
    pub(crate) fn get_buffer_view_value(
        &self,
        idx: usize,
        device: &Device,
    ) -> Result<Value, RuntimeError> {
        let mut ctx = self.get_ref(idx)?;
        let result = (|| {
            if ctx.type_ != unsafe { sys::iree_hal_buffer_view_registration } {
                return Err(RuntimeError::InvalidArgument(String::from(
                    "VM ref type mismatch",
                )));
            }

            let mut ptr = core::ptr::null_mut();
            base::Status::from_raw(unsafe { sys::iree_hal_buffer_view_check_deref(ctx, &mut ptr) })
                .into_result()?;
            unsafe { Value::from_raw_retained(ptr, device) }
        })();

        unsafe {
            sys::iree_vm_ref_release(&mut ctx);
        }

        result
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
            _not_send_sync: base::not_send_sync(),
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
