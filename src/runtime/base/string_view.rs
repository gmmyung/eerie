extern crate alloc;
use eerie_sys::runtime as sys;
/// A string view into a non-NUL-terminated string.
pub(crate) struct StringView<'a> {
    pub(crate) ctx: sys::iree_string_view_t,
    pub(crate) marker: core::marker::PhantomData<&'a mut str>,
}

impl core::fmt::Display for StringView<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", unsafe {
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                self.ctx.data as *const u8,
                self.ctx.size,
            ))
        })
    }
}

impl<'a> From<&'a str> for StringView<'a> {
    fn from(s: &'a str) -> Self {
        let string_view = sys::iree_string_view_t {
            data: s.as_ptr() as *const core::ffi::c_char,
            size: s.len(),
        };
        Self {
            ctx: string_view,
            marker: core::marker::PhantomData,
        }
    }
}

impl<'a> From<StringView<'a>> for &'a str {
    fn from(string_view: StringView<'a>) -> Self {
        unsafe {
            core::str::from_utf8_unchecked_mut(core::slice::from_raw_parts_mut(
                string_view.ctx.data as *mut u8,
                string_view.ctx.size,
            ))
        }
    }
}

/// A pair of strings.
pub(crate) struct StringPair<'a> {
    pub(crate) ctx: sys::iree_string_pair_t,
    pub(crate) marker: core::marker::PhantomData<&'a mut str>,
}

impl<'a> From<(&'a str, &'a str)> for StringPair<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        StringPair {
            ctx: unsafe {
                sys::iree_string_pair_t {
                    __bindgen_anon_1: core::mem::transmute(StringView::from(value.0)),
                    __bindgen_anon_2: core::mem::transmute(StringView::from(value.1)),
                }
            },
            marker: core::marker::PhantomData,
        }
    }
}

impl<'a> From<StringPair<'a>> for (&'a str, &'a str) {
    fn from(value: StringPair<'a>) -> Self {
        unsafe {
            let stringview1: StringView = core::mem::transmute(value.ctx.__bindgen_anon_1);
            let stringview2: StringView = core::mem::transmute(value.ctx.__bindgen_anon_2);
            return (stringview1.into(), stringview2.into());
        }
    }
}
