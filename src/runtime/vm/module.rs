use core::mem::MaybeUninit;

use crate::runtime::base;
use eerie_sys::runtime as sys;

pub struct Module {
    pub(super) ctx: *mut sys::iree_vm_module_t,
}

unsafe impl Send for Module {}
unsafe impl Sync for Module {}

impl Clone for Module {
    fn clone(&self) -> Self {
        unsafe {
            sys::iree_vm_module_retain(self.ctx);
        }
        Self { ctx: self.ctx }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            sys::iree_vm_module_release(self.ctx);
        }
    }
}

impl Module {
    /// Returns the name of the module (used during resolution).
    pub fn name(&self) -> &str {
        base::string_view::StringView {
            ctx: unsafe { sys::iree_vm_module_name(self.ctx) },
            marker: core::marker::PhantomData,
        }
        .into()
    }

    /// Returns the signature of the module describing the contents.
    pub fn signature(&self) -> ModuleSignature {
        ModuleSignature {
            ctx: unsafe { sys::iree_vm_module_signature(self.ctx) },
            marker: core::marker::PhantomData,
        }
    }

    /// Returns a value for the given reflection attribute |key|, if found.
    pub fn lookup_attr_by_name<'a>(&'a self, key: &'_ str) -> Option<&'a str> {
        let str: &str = base::string_view::StringView {
            ctx: unsafe {
                sys::iree_vm_module_lookup_attr_by_name(
                    self.ctx,
                    base::string_view::StringView::from(key).ctx,
                )
            },
            marker: core::marker::PhantomData,
        }
        .into();
        if str.len() == 0 {
            None
        } else {
            Some(str)
        }
    }

    // TODO: iree_vm_module_get_attr
    // TODO: iree_vm_module_enumerate_dependencies
    // TODO: iree_vm_module_lookup_function_by_name
    // TODO: iree_vm_module_lookup_function_by_ordinal
    // TODO: iree_vm_module_resolve_source_location
}

/// Describes the imports, exports, and capabilities of a module.
pub struct ModuleSignature<'a> {
    ctx: sys::iree_vm_module_signature_t,
    marker: core::marker::PhantomData<&'a Module>,
}

impl ModuleSignature<'_> {
    /// Module version.
    pub fn get_version(&self) -> u32 {
        self.ctx.version
    }

    /// Total number of module-level attributes.
    pub fn attr_count(&self) -> usize {
        self.ctx.attr_count
    }

    /// Total number of imported functions.
    pub fn import_function_count(&self) -> usize {
        self.ctx.import_function_count
    }

    /// Total number of exported functions.
    pub fn export_function_count(&self) -> usize {
        self.ctx.export_function_count
    }

    // Total number of internal functions, if debugging info is present and they
    // can be queried.
    pub fn internal_function_count(&self) -> usize {
        self.ctx.internal_function_count
    }
}

/// A function reference.
/// These should be treated as opaque and the accessor functions should be used
/// instead.
///
/// The register counts specify required internal storage used for VM for stack
/// frame management and debugging. They must at least be able to contain all
/// entry arguments for the function. The counts may be omitted if the function
/// will not be referenced by a VM stack frame.
pub struct Function<'a> {
    ctx: sys::iree_vm_function_t,
    marker: core::marker::PhantomData<&'a Module>,
}

impl Function<'_> {
    /// Returns the name of the module (used during resolution).
    pub fn name(&self) -> &str {
        base::string_view::StringView {
            ctx: unsafe { sys::iree_vm_function_name(&self.ctx) },
            marker: core::marker::PhantomData,
        }
        .into()
    }

    /// Returns the signature of the module describing the contents.
    pub fn signature(&self) -> FunctionSignature {
        FunctionSignature {
            ctx: unsafe { sys::iree_vm_function_signature(&self.ctx) },
            marker: core::marker::PhantomData,
        }
    }

    /// Returns a value for the given reflection attribute `key`, if found.
    /// Returns the empty string if the reflection data in general or the specific
    /// key is not found.
    pub fn lookup_attr_by_name(&self, key: &str) -> Option<&str> {
        let str: &str = base::string_view::StringView {
            ctx: unsafe {
                sys::iree_vm_function_lookup_attr_by_name(
                    &self.ctx,
                    base::string_view::StringView::from(key).ctx,
                )
            },
            marker: core::marker::PhantomData,
        }
        .into();
        if str.len() == 0 {
            None
        } else {
            Some(str)
        }
    }

    /// Gets a reflection attribute for a function by index into the attribute list.
    /// The returned key and value strings are guaranteed valid for the life
    /// of the module. Note that not all functions have reflection attributes.
    ///
    /// For more information on the function ABI and its reflection metadata see:
    /// https://iree.dev/developers/design-docs/function-abi/.
    ///
    /// Returns IREE_STATUS_OUT_OF_RANGE if index >= the number of attributes for
    /// the function.
    ///
    /// NOTE: always prefer to use `vm::Function::lookup_attr_by_name`; this should
    /// only be used when exporting attributes into a generic data structure (JSON
    /// or python dicts, etc).
    pub fn get_attr(&self, index: usize) -> Result<(&str, &str), base::status::StatusError> {
        let mut string_pair: MaybeUninit<sys::iree_string_pair_t> =
            core::mem::MaybeUninit::uninit();
        unsafe {
            base::status::Status::from(sys::iree_vm_function_get_attr(
                self.ctx,
                index,
                string_pair.as_mut_ptr(),
            ))
            .to_result()?;
            let out = base::string_view::StringPair {
                ctx: string_pair.assume_init(),
                marker: core::marker::PhantomData,
            };
            Ok(out.into())
        }
    }
}

pub struct FunctionSignature<'a> {
    ctx: sys::iree_vm_function_signature_t,
    marker: core::marker::PhantomData<&'a Function<'a>>,
}

impl FunctionSignature<'_> {
    /// Returns the arguments and results fragments from the function signature.
    /// Either may be empty if they have no values.
    ///
    /// Example:
    ///  ``          -> arguments = ``, results = ``
    ///  `0`         -> arguments = ``, results = ``
    ///  `0v`        -> arguments = ``, results = ``
    ///  `0ri`       -> arguments = `ri`, results = ``
    ///  `0_ir`      -> arguments = ``, results = `ir`
    ///  `0v_ir`     -> arguments = ``, results = `ir`
    ///  `0iCiD_rr`  -> arguments = `iCiD`, results = `rr`
    fn get_cconv_fragments(&self) -> Result<(&str, &str), base::status::StatusError> {
        let mut args = core::mem::MaybeUninit::uninit();
        let mut results = core::mem::MaybeUninit::uninit();
        unsafe {
            base::status::Status::from(sys::iree_vm_function_call_get_cconv_fragments(
                &self.ctx,
                args.as_mut_ptr(),
                results.as_mut_ptr(),
            ))
            .to_result()?;
            Ok((
                base::string_view::StringView {
                    ctx: args.assume_init(),
                    marker: core::marker::PhantomData,
                }
                .into(),
                base::string_view::StringView {
                    ctx: results.assume_init(),
                    marker: core::marker::PhantomData,
                }
                .into(),
            ))
        }
    }

    fn is_variadic_cconv(&self) -> bool {
        unsafe { sys::iree_vm_function_call_is_variadic_cconv(self.ctx.calling_convention) }
    }

    // TODO: iree_vm_function_call_compute_cconv_fragment_size
}

// TODO: iree_vm_stack_t
// TODO: iree_vm_stack_frame_t
// TODO: iree_vm_module_dependency_t
// TODO: iree_vm_module_state_t
// TODO: iree_vm_register_list_t
// TODO: iree_vm_source_location_t
