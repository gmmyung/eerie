use iree_sys::runtime as sys;

pub struct Function<'a> {
    pub(crate) ctx: sys::iree_vm_function_t,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl Default for Function<'_> {
    fn default() -> Self {
        Self {
            ctx: sys::iree_vm_function_t {
                module: std::ptr::null_mut(),
                linkage: 0,
                ordinal: 0,
            },
            _marker: std::marker::PhantomData,
        }
    }
}



