use iree_sys::runtime as sys;

pub struct DriverRegistry {
    pub(crate) ctx: *mut sys::iree_hal_driver_registry_t,
}

impl Drop for DriverRegistry {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_driver_registry_free(self.ctx);
        }
    }
}

impl DriverRegistry {
    pub fn new() -> Self {
        let out_ptr;
        unsafe {
            out_ptr = sys::iree_hal_driver_registry_default();
        }
        Self { ctx: out_ptr }
    }
}

pub struct Device {
    pub(crate) ctx: *mut sys::iree_hal_device_t,
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            sys::iree_hal_device_release(self.ctx);
        }
    }
}
