use super::vm::{DynamicList, List, Undefined};
use super::{base, hal::DriverRegistry};
use super::{
    base::StringView,
    error::RuntimeError,
    hal::{BufferView, ToElementType},
    vm,
};
use iree_sys::runtime as sys;
use std::{ffi::CString, marker::PhantomData, path::Path};
use tracing::trace;

/// Options used to configure an instance.
pub struct InstanceOptions<'a> {
    ctx: sys::iree_runtime_instance_options_t,
    marker: std::marker::PhantomData<&'a mut DriverRegistry>,
}

impl<'a> InstanceOptions<'a> {
    /// Creates a new instance options struct.
    pub fn new(driver_registry: &'a mut DriverRegistry) -> Self {
        let mut options = sys::iree_runtime_instance_options_t {
            driver_registry: driver_registry.ctx,
        };
        unsafe {
            trace!("iree_runtime_instance_options_initialize");
            sys::iree_runtime_instance_options_initialize(&mut options);
        }
        Self {
            ctx: options,
            marker: std::marker::PhantomData,
        }
    }

    /// Sets the instance to use all available registered in the current bindary. Sessions may
    /// query for the driver listing and select one(s) that are appropriate for their use.
    pub fn use_all_available_drivers(mut self) -> Self {
        unsafe {
            trace!("iree_runtime_instance_options_use_all_available_drivers");
            sys::iree_runtime_instance_options_use_all_available_drivers(&mut self.ctx);
        }
        self
    }
}

/// A runtime instance.
///
/// Shared runtime instance responsible for isolating runtime usage, enumerating and creating
/// hardware device interfaces, and managing device resource pools.
///
/// A single runtime instance can service multiple sessions and hosting applications should try to
/// reuse instances as much as possible. This ensures that resource allocation across contexts is
/// handled and extraneous device interaction is avoided. For devices that may have exclusive
/// access restrictions it is mandatory to share instances, so plan accordingly.
///
/// In multi-tenant systems separate instances can be used to isolate each tenant in cases where
/// the underlying devices do not cleanly support isolation themselves and otherwise multiple
/// tenants can share the same instance. Consider an instance as isolating IREE from itself rather
/// than being the only mechanism for isolation.
///
/// Caches and allocator pools are associated with an instance and resources may be reused among
/// any sessions sharing the same instance. In multi-tenant environments where all tenants are
/// trusted (and here "tenant" may just mean "a single session" where there are many sessions) then
/// they can often receive large benifits in terms of peak memory consumption, startup time, and
/// interoperation by sharing an instance. If two tenants must never share any data (PII) then they
/// should be placed in different instances.
pub struct Instance {
    ctx: *mut sys::iree_runtime_instance_t,
}

// Instance is thread-safe.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Instance {
    /// Creates a new instance with the given options.
    pub fn new(options: &InstanceOptions) -> Result<Self, RuntimeError> {
        let mut out_ptr = std::ptr::null_mut();
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_instance_create");
            sys::iree_runtime_instance_create(
                &options.ctx,
                base::Allocator::get_global().ctx,
                &mut out_ptr as *mut *mut sys::iree_runtime_instance_t,
            )
        })
        .to_result()?;
        Ok(Self { ctx: out_ptr })
    }

    pub(crate) fn get_host_allocator(&self) -> base::Allocator {
        let out_ptr = unsafe {
            trace!("iree_runtime_instance_host_allocator");
            sys::iree_runtime_instance_host_allocator(self.ctx)
        };
        base::Allocator {
            ctx: sys::iree_allocator_t {
                self_: std::ptr::null_mut(),
                ctl: out_ptr.ctl,
            },
        }
    }

    pub(crate) fn get_vm_instance(&self) -> *mut sys::iree_vm_instance_t {
        let out_ptr = unsafe {
            trace!("iree_runtime_instance_vm_instance");
            sys::iree_runtime_instance_vm_instance(self.ctx)
        };
        out_ptr
    }

    pub(crate) fn lookup_type(&self, full_name: StringView) -> sys::iree_vm_ref_type_t {
        let vm_instance = self.get_vm_instance();
        unsafe {
            trace!("iree_vm_instance_lookup_type, full_name: {}", full_name);
            sys::iree_vm_instance_lookup_type(vm_instance, full_name.ctx)
        }
    }

    /// Creates a Device with the given name.
    pub fn try_create_default_device(
        &self,
        name: &str,
    ) -> Result<super::hal::Device, RuntimeError> {
        let mut out_ptr = std::ptr::null_mut();
        let status = unsafe {
            trace!(
                "iree_runtime_instance_try_create_default_device, name: {}",
                name
            );
            sys::iree_runtime_instance_try_create_default_device(
                self.ctx,
                StringView::from(name).ctx,
                &mut out_ptr as *mut *mut sys::iree_hal_device_t,
            )
        };
        base::Status::from_raw(status)
            .to_result()
            .map_err(|e| RuntimeError::StatusError(e))?;
        Ok(super::hal::Device {
            ctx: out_ptr,
            marker: PhantomData,
        })
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_runtime_instance_release");
            sys::iree_runtime_instance_release(self.ctx);
        }
    }
}

/// Options used to configure a Session.
#[repr(C)]
pub struct SessionOptions {
    ctx: sys::iree_runtime_session_options_t,
}

impl Default for SessionOptions {
    fn default() -> Self {
        let mut options = Self {
            ctx: sys::iree_runtime_session_options_t {
                context_flags: 0,
                builtin_modules: 0,
            },
        };
        unsafe {
            trace!("iree_runtime_session_options_initialize");
            sys::iree_runtime_session_options_initialize(&mut options.ctx);
        }
        options
    }
}

/// A runtime session.
///
/// A session containing a set of loaded VM modules and their runtime state. Each session has its
/// own isolated module state and though multiple sessions may share the same device they will all
/// see their own individual timelines. Think of a session like a process in an operating system:
/// able to communicate and share syscalls but with a strict separation.
///
/// Only sessions that share an instance may directly share resources as different instances may
/// have different HAL devices and have incomparible memory. Import and export APIs must be used to
/// transfer the resources across instances or incompatible devices within the same instance.
///
/// Sessions are thread-compatible and may be used from any thread so long as the caller ensures
/// synchronization.
pub struct Session<'a> {
    pub(crate) ctx: *mut sys::iree_runtime_session_t,
    pub(crate) instance: &'a Instance,
}

// Session is thread-compatible.
unsafe impl Send for Session<'_> {}

impl<'a> Session<'a> {
    /// Creates a new session with the given options and device.
    pub fn create_with_device(
        instance: &'a Instance,
        options: &SessionOptions,
        device: &'a super::hal::Device,
    ) -> Result<Self, RuntimeError> {
        let mut out_ptr = std::ptr::null_mut();
        let allocator = instance.get_host_allocator();
        let status = unsafe {
            trace!("iree_runtime_session_create_with_device");
            sys::iree_runtime_session_create_with_device(
                instance.ctx,
                &options.ctx,
                device.ctx,
                allocator.ctx,
                &mut out_ptr as *mut *mut sys::iree_runtime_session_t,
            )
        };
        base::Status::from_raw(status)
            .to_result()
            .map_err(|e| RuntimeError::StatusError(e))?;
        Ok(Self {
            ctx: out_ptr,
            instance,
        })
    }

    pub(crate) fn get_allocator(&self) -> base::Allocator {
        let out = unsafe {
            trace!("iree_runtime_session_host_allocator");
            sys::iree_runtime_session_host_allocator(self.ctx)
        };
        base::Allocator { ctx: out }
    }

    /// Trims transient/cached resources used by the session.
    /// Upon resuming these resources may be expensive to rematerialize/reload and as such this
    /// should only be called when it is known the resources will not be needed soon.
    pub fn trim(&self) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_session_trim");
            sys::iree_runtime_session_trim(self.ctx)
        })
        .to_result()
        .map_err(|e| RuntimeError::StatusError(e))
    }

    // pub fn append_module(&self, module: &Module) -> Result<(), RuntimeError> {
    // TODO: implement this

    /// Appends a bytecode module to the context loaded from the given memory blob.
    /// If the module exists as a file, prefer instead to use append_module_from_file to use memory
    /// mapped I/O and reduce total memory consumption.
    /// # Safety
    /// The runtime does not perform strict validation on the module data and assumes it is correct.
    /// Make sure that the bytecode data is valid and trusted before use.
    pub unsafe fn append_module_from_memory(
        &self,
        flatbuffer_data: &'a [u8],
    ) -> Result<(), RuntimeError> {
        let const_byte_span = base::ConstByteSpan::from(flatbuffer_data);
        base::Status::from_raw(unsafe {
            trace!(
                "iree_runtime_session_append_bytecode_module_from_memory, bytecode length: {}",
                flatbuffer_data.len()
            );
            sys::iree_runtime_session_append_bytecode_module_from_memory(
                self.ctx,
                const_byte_span.ctx,
                base::Allocator::null_allocator().ctx,
            )
        })
        .to_result()
        .map_err(|e| RuntimeError::StatusError(e))
    }

    /// Appends a bytecode module to the context loaded from the given file.
    /// # Safety
    /// The runtime does not perform strict validation on the module data and assumes it is correct.
    /// Make sure that the bytecode data is valid and trusted before use.
    pub unsafe fn append_module_from_file(&self, path: &Path) -> Result<(), RuntimeError> {
        let cstr = CString::new(path.to_str().unwrap()).unwrap();
        base::Status::from_raw(unsafe {
            trace!(
                "iree_runtime_session_append_bytecode_module_from_file, path: {:?}",
                path
            );
            sys::iree_runtime_session_append_bytecode_module_from_file(self.ctx, cstr.as_ptr())
        })
        .to_result()
        .map_err(|e| RuntimeError::StatusError(e))
    }

    pub fn lookup_function<'f>(&'f self, name: &str) -> Result<vm::Function<'f>, RuntimeError> {
        let mut out = std::mem::MaybeUninit::<sys::iree_vm_function_t>::uninit();
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_session_lookup_function, name: {:?}", name);
            sys::iree_runtime_session_lookup_function(
                self.ctx,
                StringView::from(name).ctx,
                out.as_mut_ptr(),
            )
        })
        .to_result()?;

        Ok(vm::Function {
            ctx: unsafe { out.assume_init() },
            session: self,
        })
    }

    pub(crate) fn context(&self) -> *mut sys::iree_vm_context_t {
        unsafe {
            trace!("iree_runtime_session_context");
            sys::iree_runtime_session_context(self.ctx)
        }
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_runtime_session_release");
            sys::iree_runtime_session_release(self.ctx);
        }
    }
}

/// A stateful VM function call builder.
///
/// Application that will be calling the same function repeatedly can reuse the call to avoid
/// having to construct the inputs lists each time. Outputs of prior calls will be retained unless
/// iree_runtime_call_reset is used and will be provided to the VM on subsequent calls to reuse (if
/// able): when reusing a call like this callers are required to either reset the call, copy their
/// data out, or reset the particular output they are consuming.
///
/// Calls are thread-compatible and may be used from any thread so long as the caller ensures
/// synchronization.
pub struct Call<'a> {
    ctx: sys::iree_runtime_call_t,
    session: &'a Session<'a>,
}

unsafe impl Send for Call<'_> {}

impl<'a> Call<'a> {
    /// Creates a new call to the given function.
    pub fn new(session: &'a Session, func: &'a vm::Function) -> Result<Self, RuntimeError> {
        let mut call = Self {
            ctx: sys::iree_runtime_call_t::default(),
            session,
        };
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_call_initialize");
            sys::iree_runtime_call_initialize(
                session.ctx,
                func.ctx,
                &mut call.ctx as *mut sys::iree_runtime_call_t,
            )
        })
        .to_result()?;
        Ok(call)
    }

    /// Creates a new call to the given function by name.
    pub fn from_func_name(session: &'a Session, name: &str) -> Result<Self, RuntimeError> {
        let mut out = std::mem::MaybeUninit::uninit();
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_call_initialize_by_name, name: {:?}", name);
            sys::iree_runtime_call_initialize_by_name(
                session.ctx,
                StringView::from(name).ctx,
                out.as_mut_ptr(),
            )
        })
        .to_result()?;
        Ok(Self {
            ctx: unsafe { out.assume_init() },
            session,
        })
    }

    /// Invokes the call
    pub fn invoke(&mut self) -> Result<(), RuntimeError> {
        // TODO: Call flags interface, not fixed to 0
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_call_invoke");
            sys::iree_runtime_call_invoke(&mut self.ctx, 0)
        })
        .to_result()
        .map_err(|e| RuntimeError::StatusError(e))
    }

    /// Pushes a buffer view to the call input list.
    ///
    /// The buffer view must originate from the same instance of the runtime as the call.
    pub fn inputs_push_back_buffer_view<T: ToElementType>(
        &mut self,
        buffer_view: &BufferView<'a, T>,
    ) -> Result<(), RuntimeError> {
        (self.session.instance.ctx == buffer_view.session.instance.ctx)
            .then(|| ())
            .ok_or(RuntimeError::InstanceMismatch(
                "The buffer view must originate from the same instance of the runtime as the call.".to_string(),
            ))?;
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_call_inputs_push_back_buffer_view");
            sys::iree_runtime_call_inputs_push_back_buffer_view(&mut self.ctx, buffer_view.ctx)
        })
        .to_result()?;
        Ok(())
    }

    /// Pops a buffer view from the call output list.
    pub fn outputs_pop_front_buffer_view<T: ToElementType>(
        &mut self,
    ) -> Result<BufferView<'_, T>, RuntimeError> {
        let mut out = std::mem::MaybeUninit::uninit();
        base::Status::from_raw(unsafe {
            trace!("iree_runtime_call_outputs_pop_front_buffer_view");
            sys::iree_runtime_call_outputs_pop_front_buffer_view(&mut self.ctx, out.as_mut_ptr())
        })
        .to_result()?;
        Ok(unsafe { BufferView::from_ptr(out.assume_init(), self.session) })
    }

    /// Resets the input and output lists back to 0-length in preparation for construction of
    /// another call.
    pub fn reset(&mut self) {
        unsafe {
            trace!("iree_runtime_call_reset");
            sys::iree_runtime_call_reset(&mut self.ctx);
        }
    }

    /// Returns the mutable input list.
    pub fn input_list<'l>(&'l mut self) -> DynamicList<'l, Undefined> {
        unsafe {
            trace!("iree_runtime_call_inputs");
            List::from_raw(
                self.session.instance,
                sys::iree_runtime_call_inputs(&mut self.ctx),
            )
        }
    }

    /// Returns the mutable output list.
    pub fn output_list<'l>(&'l mut self) -> DynamicList<'l, Undefined> {
        unsafe {
            trace!("iree_runtime_call_outputs");
            List::from_raw(
                self.session.instance,
                sys::iree_runtime_call_outputs(&mut self.ctx),
            )
        }
    }
}

impl Drop for Call<'_> {
    fn drop(&mut self) {
        unsafe {
            trace!("iree_runtime_call_deinitialize");
            sys::iree_runtime_call_deinitialize(&mut self.ctx);
        }
    }
}
