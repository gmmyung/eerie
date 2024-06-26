use crate::runtime::base;
use eerie_sys::runtime as sys;

/// Shared runtime instance responsible for routing `vm::Context` events,
/// enumerating and creating hardware device interfaces, and managing device
/// resource pools.
///
/// Instance is reference counted, so cloning this does not copy.
///
/// A single runtime instance can service multiple contexts and hosting
/// applications should try to reuse instances as much as possible. This ensures
/// that resource allocation across contexts is handled and extraneous device
/// interaction is avoided. For devices that may have exclusive access
/// restrictions it is mandatory to share instances, so plan accordingly.
pub struct Instance {
    pub(super) ctx: *mut sys::iree_vm_instance_t,
}

/// Instance is Thread-safe.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Default for Instance {
    fn default() -> Self {
        let mut out = core::mem::MaybeUninit::uninit();
        Instance {
            ctx: unsafe {
                sys::iree_vm_instance_create(
                    sys::IREE_VM_TYPE_CAPACITY_DEFAULT as usize,
                    base::allocator::Allocator::get_global().ctx,
                    out.as_mut_ptr(),
                );
                out.assume_init()
            },
        }
    }
}

impl Clone for Instance {
    fn clone(&self) -> Self {
        unsafe { sys::iree_vm_instance_retain(self.ctx) };
        Instance { ctx: self.ctx }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { sys::iree_vm_instance_release(self.ctx) }
    }
}

impl Instance {
    pub(super) fn get_allocator(&self) -> base::allocator::Allocator {
        unsafe {
            base::allocator::Allocator {
                ctx: sys::iree_vm_instance_allocator(self.ctx),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn new_instance() {
        let a = Instance::default();
        let b = a.clone();
        drop(a);
        drop(b);
    }
}
