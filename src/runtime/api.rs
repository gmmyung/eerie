use std::{ffi::CString, marker::PhantomData, path::Path};
use super::{base::StringView, error::RuntimeError, vm, hal::{BufferView, ToElementType}};
use iree_sys::runtime as sys;
use tracing::debug;
use super::{base, hal::DriverRegistry};

pub struct InstanceOptions<'a> {
    ctx: sys::iree_runtime_instance_options_t,
    marker: std::marker::PhantomData<&'a mut DriverRegistry>,
}

impl<'a> InstanceOptions<'a> {
    pub fn new(driver_registry: &'a mut DriverRegistry) -> Self {
        let mut options = sys::iree_runtime_instance_options_t {
            driver_registry: driver_registry.ctx,
        };
        unsafe {
            sys::iree_runtime_instance_options_initialize(&mut options);
        }
        Self {
            ctx: options,
            marker: std::marker::PhantomData,
        }
    }

    pub fn use_all_available_drivers(mut self) -> Self {
        unsafe {
            sys::iree_runtime_instance_options_use_all_available_drivers(&mut self.ctx);
        }
        self
    }
}

pub struct Instance {
    ctx: *mut sys::iree_runtime_instance_t,
}

// Instance is thread-safe.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Instance {
    pub fn new(options: &InstanceOptions) -> Result<Self, RuntimeError> {
        debug!("Creating instance...");
        let mut out_ptr = std::ptr::null_mut();
        base::Status::from_raw(unsafe {
            sys::iree_runtime_instance_create(
                &options.ctx,
                base::Allocator::get_global().ctx,
                //Allocator::from_system().ctx,
                &mut out_ptr as *mut *mut sys::iree_runtime_instance_t,
            )
        })
        .to_result()?;
        debug!("Instance created!, out_ptr: {:p}", out_ptr);
        Ok(Self { ctx: out_ptr })
    }

    pub(crate) fn get_host_allocator(&self) -> base::Allocator {
        let out_ptr = unsafe { sys::iree_runtime_instance_host_allocator(self.ctx) };
        base::Allocator {
            ctx: sys::iree_allocator_t {
                self_: std::ptr::null_mut(),
                ctl: out_ptr.ctl,
            },
        }
    }

    // pub fn get_vm_instance(&self) -> vm::Instance {
    // TODO: implement this
    
    pub(crate) fn get_vm_instance(&self) -> *mut sys::iree_vm_instance_t {
        let out_ptr = unsafe { sys::iree_runtime_instance_vm_instance(self.ctx) };
        out_ptr
    }

    pub(crate) fn lookup_type(&self, full_name: StringView) -> sys::iree_vm_ref_type_t {
        let vm_instance = self.get_vm_instance();
        unsafe { sys::iree_vm_instance_lookup_type(vm_instance, full_name.ctx) }
    }

    fn get_driver_registry(&self) -> DriverRegistry {
        let out_ptr = unsafe { sys::iree_runtime_instance_driver_registry(self.ctx) };
        DriverRegistry { ctx: out_ptr }
    }

    pub fn try_create_default_device(
        &self,
        name: &str,
    ) -> Result<super::hal::Device, RuntimeError> {
        let mut out_ptr = std::ptr::null_mut();
        let status = unsafe {
            sys::iree_runtime_instance_try_create_default_device(
                self.ctx,
                StringView::from(name).ctx,
                &mut out_ptr as *mut *mut sys::iree_hal_device_t,
            )
        };
        debug!("Device created!");
        base::Status::from_raw(status)
            .to_result()
            .map_err(|e| RuntimeError::StatusError(e))?;
        Ok(super::hal::Device { ctx: out_ptr, marker: PhantomData })
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        debug!("Instance freed!");
        unsafe {
            sys::iree_runtime_instance_release(self.ctx);
        }
    }
}

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
            sys::iree_runtime_session_options_initialize(&mut options.ctx);
        }
        options
    }
}

pub struct Session<'a> {
    pub(crate) ctx: *mut sys::iree_runtime_session_t,
    _instance: &'a Instance,
}

// Session is thread-compatible.
unsafe impl Send for Session<'_> {}

impl<'a> Session<'a> {
    pub fn create_with_device(
        instance: &'a Instance,
        options: &SessionOptions,
        device: &'a super::hal::Device,
    ) -> Result<Self, RuntimeError> {
        let mut out_ptr = std::ptr::null_mut();
        let allocator = instance.get_host_allocator();
        let status = unsafe {
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
            _instance: instance,
        })
    }

    pub(crate) fn get_allocator(&self) -> base::Allocator {
        let out = unsafe { sys::iree_runtime_session_host_allocator(self.ctx) };
        base::Allocator { ctx: out }
    }

    pub fn trim(&self) -> Result<(), RuntimeError> {
        debug!("Trimming session...");
        base::Status::from_raw(unsafe { sys::iree_runtime_session_trim(self.ctx) })
            .to_result()
            .map_err(|e| RuntimeError::StatusError(e))
    }

    // pub fn append_module(&self, module: &Module) -> Result<(), RuntimeError> {
    // TODO: implement this

    pub unsafe fn append_module_from_memory(
        &self,
        flatbuffer_data: &'a [u8],
    ) -> Result<(), RuntimeError> {
        debug!("Appending bytecode module from memory...");
        let const_byte_span = base::ConstByteSpan::from(flatbuffer_data);
        base::Status::from_raw(unsafe {
            sys::iree_runtime_session_append_bytecode_module_from_memory(
                self.ctx,
                const_byte_span.ctx,
                base::Allocator::null_allocator().ctx,
            )
        })
        .to_result()
        .map_err(|e| RuntimeError::StatusError(e))
    }

    pub unsafe fn append_module_from_file(&self, path: &Path) -> Result<(), RuntimeError> {
        debug!("Appending bytecode module from file...");
        let cstr = CString::new(path.to_str().unwrap()).unwrap();
        base::Status::from_raw(unsafe {
            sys::iree_runtime_session_append_bytecode_module_from_file(self.ctx, cstr.as_ptr())
        })
        .to_result()
        .map_err(|e| RuntimeError::StatusError(e))
    }

    pub fn lookup_function<'f>(&'f self, name: &str) -> Result<vm::Function<'f>, RuntimeError> {
        debug!("Looking up function...");
        let function = vm::Function::default();
        base::Status::from_raw(unsafe {
            sys::iree_runtime_session_lookup_function(
                self.ctx,
                StringView::from(name).ctx,
                &function.ctx as *const sys::iree_vm_function_t as *mut sys::iree_vm_function_t,
            )
        })
        .to_result()?;

        Ok(function)
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe {
            sys::iree_runtime_session_release(self.ctx);
        }
    }
}

pub struct Call<'a> {
    ctx: sys::iree_runtime_call_t,
    _marker: PhantomData<&'a Session<'a>>,
}

// Call is thread-compatible.
unsafe impl Send for Call<'_> {}

impl<'a> Call<'a> {
    pub fn new(session: &'a Session, func: &'a vm::Function) -> Result<Self, RuntimeError> {
        let mut call = Self {
            ctx: sys::iree_runtime_call_t::default(),
            _marker: PhantomData,
        };
        debug!("call.ctx: {:p}", &call.ctx);
        base::Status::from_raw(unsafe {
            debug!("Creating call...");
            sys::iree_runtime_call_initialize(
                session.ctx,
                func.ctx,
                &mut call.ctx as *mut sys::iree_runtime_call_t,
            )
        })
        .to_result()?;
        debug!("Call created!");
        Ok(call)
    }

    pub fn from_func_name(session: &'a Session, name: &str) -> Result<Self, RuntimeError> {
        let mut out = std::mem::MaybeUninit::uninit();
        base::Status::from_raw(unsafe {
            debug!("Creating call...");
            sys::iree_runtime_call_initialize_by_name(
                session.ctx,
                StringView::from(name).ctx,
                out.as_mut_ptr(),
            )
        })
        .to_result()?;
        debug!("Call created!");
        Ok(Self {
            ctx: unsafe { out.assume_init() },
            _marker: PhantomData,
        })
    }

    pub fn invoke(&mut self) -> Result<(), RuntimeError> {
        // TODO: Call flags interface, not fixed to 0
        base::Status::from_raw(unsafe { sys::iree_runtime_call_invoke(&mut self.ctx, 0) })
            .to_result()
            .map_err(|e| RuntimeError::StatusError(e))
    }

    pub fn inputs_push_back_buffer_view<T: ToElementType>(&mut self, buffer_view: &BufferView<'a, T>) -> Result<(), RuntimeError> {
        base::Status::from_raw(unsafe {
            sys::iree_runtime_call_inputs_push_back_buffer_view(
                &mut self.ctx,
                buffer_view.ctx,
            )
        })
        .to_result()?;
        Ok(())
    }

    pub fn outputs_pop_front_buffer_view<T: ToElementType>(&mut self) -> Result<BufferView<'_, T>, RuntimeError> {
        let mut out = std::mem::MaybeUninit::uninit();
        base::Status::from_raw(unsafe {
            sys::iree_runtime_call_outputs_pop_front_buffer_view(
                &mut self.ctx,
                out.as_mut_ptr(),
            )
        })
        .to_result()?;
        Ok(unsafe { BufferView::from_ptr(out.assume_init()) })
    }
}

impl Drop for Call<'_> {
    fn drop(&mut self) {
        unsafe {
            sys::iree_runtime_call_deinitialize(&mut self.ctx);
        }
    }
}
