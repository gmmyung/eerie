use crate::runtime::{base, error::RuntimeError};
use eerie_sys::runtime as sys;

use super::{instance::Instance, module::Module};

/// An isolated execution context.
/// Effectively a sandbox where modules can be loaded and run with restricted
/// visibility and where they can maintain state.
///
/// Modules have imports resolved automatically when registered by searching
/// existing modules registered within the context and load order is used for
/// resolution. Functions are resolved from the most recently registered module
/// back to the first, such that modules can override implementations of
/// functions in previously registered modules.
pub struct Context {
    ctx: *mut sys::iree_vm_context_t,
}

/// Context is thread-compatible
unsafe impl Send for Context {}

impl Context {
    /// Creates a new context that uses the given `instance` for device management.
    pub fn new(instance: &Instance) -> Result<Self, base::status::StatusError> {
        let mut out = core::mem::MaybeUninit::uninit();
        Ok(Context {
            ctx: unsafe {
                base::status::Status::from(sys::iree_vm_context_create(
                    instance.ctx,
                    // TODO: Condsider option to configure context flags
                    sys::iree_vm_context_flag_bits_t_IREE_VM_CONTEXT_FLAG_CONCURRENT
                        | sys::iree_vm_context_flag_bits_t_IREE_VM_CONTEXT_FLAG_TRACE_EXECUTION,
                    instance.get_allocator().ctx,
                    out.as_mut_ptr(),
                ))
                .to_result()?;
                out.assume_init()
            },
        })
    }

    /// Creates a new context with the given static set of modules. This is
    /// equivalent to `Context::new`+`Context::register_modules` but may be
    /// more efficient to allocate. Contexts created in this way cannot have
    /// additional modules registered after creation.
    pub fn create_with_modules(
        instance: &Instance,
        modules: &mut [Module],
    ) -> Result<Self, base::status::StatusError> {
        let mut out = core::mem::MaybeUninit::uninit();
        Ok(Context {
            ctx: unsafe {
                base::status::Status::from(sys::iree_vm_context_create_with_modules(
                    instance.ctx,
                    sys::iree_vm_context_flag_bits_t_IREE_VM_CONTEXT_FLAG_CONCURRENT
                        | sys::iree_vm_context_flag_bits_t_IREE_VM_CONTEXT_FLAG_TRACE_EXECUTION,
                    modules.len(),
                    // NOTE: *mut Module as *mut *mut iree_vm_module_t
                    // Which should be correct
                    modules.as_mut_ptr() as _,
                    instance.get_allocator().ctx,
                    out.as_mut_ptr(),
                ))
                .to_result()?;
                out.assume_init()
            },
        })
    }

    /// Returns the instance this context was created within.
    pub fn instance(&self) -> Instance {
        Instance {
            ctx: unsafe { sys::iree_vm_context_instance(self.ctx) },
        }
    }

    /// Returns the total number of modules registered.
    pub fn module_count(&self) -> usize {
        unsafe { sys::iree_vm_context_module_count(self.ctx) }
    }

    /// Returns the module registered at `index`.
    pub fn module_at(&self, index: usize) -> Result<Module, RuntimeError> {
        unsafe {
            let ctx = sys::iree_vm_context_module_at(self.ctx, index);
            match ctx.is_null() {
                true => Err(RuntimeError::OutOfBounds(index)),
                false => Ok(Module { ctx }),
            }
        }
    }

    /// Registers a list of modules with the context and resolves imports in the
    /// order provided.
    pub fn register_modules(&self, modules: &[Module]) -> Result<(), RuntimeError> {
        base::status::Status::from(unsafe {
            sys::iree_vm_context_register_modules(self.ctx, modules.len(), modules.as_ptr() as _)
        })
        .to_result()?;
        Ok(())
    }

    /// Freezes a context such that no more modules can be registered.
    /// This can be used to ensure that context contents cannot be modified by other
    /// code as the context is made available to other parts of the program.
    /// No-op if already frozen.
    pub fn freeze(&self) -> Result<(), RuntimeError> {
        base::status::Status::from(unsafe { sys::iree_vm_context_freeze(self.ctx) }).to_result()?;
        Ok(())
    }

    // TODO: expose iree_vm_context_id
    // TODO: expose iree_vm_context_flags
    // TODO: expose iree_context_state_resolver
    // TODO: expose iree_vm_context_resolve_module_state
    // TODO: expose iree_vm_context_resolve_function
    // TODO: expose iree_vm_context_notify
}

impl Clone for Context {
    fn clone(&self) -> Self {
        unsafe { sys::iree_vm_context_retain(self.ctx) };
        Context { ctx: self.ctx }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { sys::iree_vm_context_release(self.ctx) }
    }
}

#[cfg(test)]
mod test {
    use crate::runtime::vm::instance::Instance;

    use super::Context;
    #[test]
    fn new_context() {
        let instance = Instance::default();
        let context = Context::new(&instance).unwrap();
        let context2 = context.clone();
        drop(context);
        drop(context2);
    }
}
