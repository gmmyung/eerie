use super::{value, vm_ref};
use eerie_sys::runtime as sys;

/// Describes a type in the type table, mapping from a local module type ID to
/// either a primitive value type or registered ref type.
///
/// * ?: variant (value_type/ref_type == 0)
/// * i8: primitive value (value_type != 0)
/// * !vm.ref<?>: any ref value (ref_type == IREE_VM_REF_TYPE_ANY)
/// * !vm.ref<!foo>: ref value of type !foo (ref_type > 0)
///
/// Implementation note: since type defs are used frequently and live in tables
/// and on the stack we pack the value and ref types together into a single
/// native machine word. This exploits the fact that iree_vm_ref_type_t is a
/// pointer to a struct that should always be aligned to some multiple of the
/// native machine word and we'll have low bits to spare.
pub struct VmTypeDef {
    pub(super) ctx: sys::iree_vm_type_def_t,
}

impl VmTypeDef {
    pub fn undefined_type_def() -> VmTypeDef {
        let mut result = sys::iree_vm_type_def_t::default();
        result.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        result.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        VmTypeDef { ctx: result }
    }

    pub fn value_type_def<T: value::ToValue>() -> VmTypeDef {
        let mut result = sys::iree_vm_type_def_t::default();
        result.set_value_type_bits(T::to_value_type() as usize);
        result.set_ref_type_bits(sys::iree_vm_ref_type_bits_t_IREE_VM_REF_TYPE_NULL as usize);
        VmTypeDef { ctx: result }
    }

    pub fn ref_type_def<T: vm_ref::ToVmRef>() -> VmTypeDef {
        let mut result = sys::iree_vm_type_def_t::default();
        result.set_value_type_bits(sys::iree_vm_value_type_e_IREE_VM_VALUE_TYPE_NONE as usize);
        result.set_ref_type_bits(T::to_ref_type() as usize);
        VmTypeDef { ctx: result }
    }
}
