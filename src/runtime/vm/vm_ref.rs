// `iree_vm_ref_t` is renamed to `vmref` cause ref is a Rust keyword
use crate::runtime::base;
use eerie_sys::runtime as sys;

/// Type-erased reference counted type descriptor.
pub struct VmRefType {
    ctx: sys::iree_vm_ref_type_t,
}

impl VmRefType {
    pub fn name(&self) -> &str {
        base::string_view::StringView {
            ctx: unsafe { sys::iree_vm_ref_type_name(self.ctx) },
            marker: core::marker::PhantomData,
        }
        .into()
    }
}

/// A pointer reference to a reference-counted object.
pub struct VmRef<T: ToVmRef> {
    ctx: sys::iree_vm_ref_t,
    marker: core::marker::PhantomData<T>,
}

impl<T: ToVmRef> Clone for VmRef<T> {
    fn clone(&self) -> Self {
        let mut out: core::mem::MaybeUninit<sys::iree_vm_ref_t> = core::mem::MaybeUninit::uninit();
        unsafe {
            sys::iree_vm_ref_retain(&self.ctx as *const _ as *mut _, out.as_mut_ptr());
            Self {
                ctx: out.assume_init(),
                marker: core::marker::PhantomData,
            }
        }
    }
}

impl<T: ToVmRef> Drop for VmRef<T> {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_ref_release(&mut self.ctx);
        }
    }
}

// TODO: iree_vm_ref_retain_inplace
// TODO: iree_vm_ref_retain_or_move
// TODO: iree_vm_ref_assign
// TODO: iree_vm_ref_move
// TODO: iree_vm_ref_is_null
// TODO: iree_vm_ref_equal

pub trait ToVmRef: Sized {
    fn to_ref() -> VmRef<Self>;
    fn to_ref_type() -> sys::iree_vm_ref_type_t;
}
