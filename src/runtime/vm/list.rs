use super::{instance, value, vm_type};
use crate::runtime::{base, error};
use eerie_sys::runtime as sys;

pub struct List {
    ctx: *mut sys::iree_vm_list_t,
}

impl Clone for List {
    /// List is referenced counted, which means this creates another reference to
    /// the same list, increasing its reference count.
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_vm_list_retain(self.ctx);
        }
        Self { ctx: self.ctx }
    }
}

impl List {
    // Statically allocated lists are not supported due to lifetime issues.
    // There is no way to control the lifetime of the list properly without
    // refcounting.

    /// Creates a growable list containing the given `element_type`, which may either
    /// be a primitive value (like i32) or a ref type. When
    /// storing ref types the list may either store a specific `VmRefType`
    /// and ensure that all elements set match the type or IREE_VM_REF_TYPE_ANY to
    /// indicate that any ref type is allowed.
    ///
    /// `element_type` can be set by `undefined_type_def()` to indicate that
    /// the list stores variants (each element can differ in type).
    pub fn create(
        element_type: &vm_type::VmTypeDef,
        initial_capacity: usize,
        // NOTE: Instance is not required, but VM instance creation registers list
        // types, which is required before using the list APIs. Type is registered
        // even after instance release.
        #[allow(unused_variables)] instance: &instance::Instance,
    ) -> Result<Self, base::status::StatusError> {
        let mut out = core::mem::MaybeUninit::uninit();
        unsafe {
            base::status::Status::from(sys::iree_vm_list_create(
                element_type.ctx,
                initial_capacity,
                instance.get_allocator().ctx,
                out.as_mut_ptr(),
            ))
            .to_result()?;
            Ok(Self {
                ctx: out.assume_init(),
            })
        }
    }

    /// Actually shallow clones `source` into `out_target`.
    /// The resulting list will be have its capacity set to the `self` size.
    pub fn shallow_clone(&self, instance: &instance::Instance) -> Self {
        let mut out = core::mem::MaybeUninit::uninit();
        unsafe {
            sys::iree_vm_list_clone(self.ctx, instance.get_allocator().ctx, out.as_mut_ptr());
            Self {
                ctx: out.assume_init(),
            }
        }
    }

    /// Returns the element type stored in the list.
    pub fn element_type(&self) -> vm_type::VmTypeDef {
        unsafe {
            vm_type::VmTypeDef {
                ctx: sys::iree_vm_list_element_type(self.ctx),
            }
        }
    }

    /// Returns the capacity of the list in elements.
    pub fn capacity(&self) -> usize {
        unsafe { sys::iree_vm_list_capacity(self.ctx) }
    }

    /// Reserves storage for at least minimum_capacity elements. If the list already
    /// has at least the specified capacity the operation is ignored.
    pub fn reserve(&self, minimum_capacity: usize) {
        unsafe {
            sys::iree_vm_list_reserve(self.ctx, minimum_capacity);
        }
    }

    /// Returns the number of elements in the list.
    pub fn size(&self) -> usize {
        unsafe { sys::iree_vm_list_size(self.ctx) }
    }

    /// Resizes the list to contain new_size elements. This will either truncate
    /// the list if the existing size is greater than new_size or extend the list
    /// with the default list value of 0 if storing primitives, null if refs, or
    /// empty if variants.
    pub fn resize(&self, new_size: usize) {
        unsafe {
            sys::iree_vm_list_resize(self.ctx, new_size);
        }
    }

    /// Clears the list contents. Equivalent to resizing to 0.
    pub fn clear(&self) {
        unsafe {
            sys::iree_vm_list_clear(self.ctx);
        }
    }

    /// Swaps the storage of `self` and `other`. The list references remain the
    /// same but the count, capacity, and underlying storage will be swapped. This
    /// can be used to treat lists as persistent stable references to dynamically
    /// mutated storage such as when emulating structs or dicts.
    pub fn swap_storage(&self, other: &Self) {
        unsafe {
            sys::iree_vm_list_swap_storage(self.ctx, other.ctx);
        }
    }

    /// Copies `count` elements from `src_list` starting at `src_i` to `dst_list`
    /// starting at `dst_i`. The ranges specified must be valid in both lists.
    ///
    /// Supported list types:
    ///   any type -> variant list
    ///   variant list -> compatible element types only
    ///   same value type -> same value type
    ///   same ref type -> same ref type
    pub fn copy(
        src_list: &Self,
        src_i: usize,
        dst_list: &Self,
        dst_i: usize,
        count: usize,
    ) -> Result<(), base::status::StatusError> {
        unsafe {
            base::status::Status::from(sys::iree_vm_list_copy(
                src_list.ctx,
                src_i,
                dst_list.ctx,
                dst_i,
                count,
            ))
            .to_result()
        }
    }

    /// Returns the value of the element at the given index.
    /// Note that the value type may vary from element to element in variant lists,
    /// so the value type should be specified.
    pub fn get_value<T: value::ToValue>(&self, index: usize) -> Result<T, error::RuntimeError> {
        unsafe {
            let mut out = core::mem::MaybeUninit::uninit();
            base::status::Status::from(sys::iree_vm_list_get_value(
                self.ctx,
                index,
                out.as_mut_ptr(),
            ))
            .to_result()?;

            Ok(value::Value::from_raw(out.assume_init())?.get())
        }
    }

    /// Returns the value of the element at the given index. If the specified
    /// `value_type` differs from the list storage type the value will be converted
    /// using the value type semantics (such as sign/zero extend, etc).
    pub fn get_as<T: value::ToValue>(&self, index: usize) -> Result<T, base::status::StatusError> {
        unsafe {
            let mut out = core::mem::MaybeUninit::uninit();
            base::status::Status::from(sys::iree_vm_list_get_value_as(
                self.ctx,
                index,
                T::to_value_type(),
                out.as_mut_ptr(),
            ))
            .to_result()?;
            Ok(value::Value::from_raw(out.assume_init()).unwrap().get())
        }
    }

    /// Sets the value of the element at the given index. If the specified `value`
    /// type differs from the list storage type the value will be converted using the
    /// value type semantics (such as sign/zero extend, etc).
    pub fn set_value<T: value::ToValue>(
        &self,
        index: usize,
        value: T,
    ) -> Result<(), base::status::StatusError> {
        unsafe {
            base::status::Status::from(sys::iree_vm_list_set_value(
                self.ctx,
                index,
                &value.to_value().ctx,
            ))
            .to_result()
        }
    }

    /// Pushes the value of the element to the end of the list.
    /// If the specified `value` type differs from the list storage type the value
    /// will be converted using the value type semantics (such as sign/zero extend,
    /// etc).
    pub fn push_value<T: value::ToValue>(&self, value: T) -> Result<(), base::status::StatusError> {
        unsafe {
            base::status::Status::from(sys::iree_vm_list_push_value(
                self.ctx,
                &value.to_value().ctx,
            ))
            .to_result()
        }
    }
}

impl Drop for List {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_list_release(self.ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::vm::instance;

    #[test]
    fn new_list() {
        let a = List::create(
            &vm_type::VmTypeDef::value_type_def::<i32>(),
            1,
            &instance::Instance::default(),
        )
        .unwrap();
        let b = a.clone();
        drop(a);
        drop(b);
    }

    #[test]
    fn list_push() {
        let a = List::create(
            &vm_type::VmTypeDef::value_type_def::<i32>(),
            1,
            &instance::Instance::default(),
        )
        .unwrap();
        a.push_value::<i32>(1).unwrap();
        a.push_value::<i32>(2).unwrap();
        a.push_value::<i32>(3).unwrap();
        assert_eq!(a.get_value::<i32>(0).unwrap(), 1);
        assert_eq!(a.get_value::<i32>(1).unwrap(), 2);
        assert_eq!(a.get_value::<i32>(2).unwrap(), 3);
    }

    #[test]
    fn list_set() {
        let a = List::create(
            &vm_type::VmTypeDef::value_type_def::<i32>(),
            1,
            &instance::Instance::default(),
        )
        .unwrap();
        a.push_value(1).unwrap();
        a.set_value(0, 2).unwrap();
        assert_eq!(a.get_value::<i32>(0).unwrap(), 2);
    }

    #[test]
    fn type_error() {
        let a = List::create(
            &vm_type::VmTypeDef::value_type_def::<i32>(),
            1,
            &instance::Instance::default(),
        )
        .unwrap();
        a.push_value::<i32>(1).unwrap();
        assert!(a.get_value::<i64>(0).is_err());
    }
}
