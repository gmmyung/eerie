use core::ffi::{c_char, c_int, c_long, c_longlong, c_void};

unsafe extern "C" {
    fn memcmp(lhs: *const c_void, rhs: *const c_void, n: usize) -> c_int;
    fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memset(dest: *mut c_void, value: c_int, n: usize) -> *mut c_void;
}

// rustc links libcompiler_builtins before native IREE archives. These references
// make the C memory symbols unresolved early enough for the linker to extract
// compiler_builtins' weak implementations.
#[used]
static FORCE_MEMCMP: unsafe extern "C" fn(*const c_void, *const c_void, usize) -> c_int = memcmp;
#[used]
static FORCE_MEMCPY: unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void =
    memcpy;
#[used]
static FORCE_MEMMOVE: unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void =
    memmove;
#[used]
static FORCE_MEMSET: unsafe extern "C" fn(*mut c_void, c_int, usize) -> *mut c_void = memset;

// tinyrlibc covers the string/ctype/parse functions we enable in Cargo.toml,
// but IREE also needs this small libc/system ABI surface.
#[no_mangle]
pub unsafe extern "C" fn strcpy(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let mut offset = 0;
    loop {
        let byte = unsafe { *src.add(offset) };
        unsafe {
            *dest.add(offset) = byte;
        }
        offset += 1;
        if byte == 0 {
            return dest;
        }
    }
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[no_mangle]
pub extern "C" fn raise(_signal: c_int) -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn _getpid() -> c_int {
    1
}

#[no_mangle]
pub extern "C" fn _kill(_pid: c_int, _signal: c_int) -> c_int {
    -1
}

#[no_mangle]
pub extern "C" fn _exit(_status: c_int) -> ! {
    abort()
}

#[no_mangle]
pub extern "C" fn __errno() -> *mut c_int {
    core::ptr::addr_of_mut!(ERRNO)
}

static mut ERRNO: c_int = 0;

// The libm crate provides Rust functions, not exported C ABI symbols, so expose
// the math names IREE's C runtime references.
macro_rules! unary_f32 {
    ($name:ident, $libm:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(value: f32) -> f32 {
            libm::$libm(value)
        }
    };
}

macro_rules! unary_f64 {
    ($name:ident, $libm:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(value: f64) -> f64 {
            libm::$libm(value)
        }
    };
}

macro_rules! binary_f32 {
    ($name:ident, $libm:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(lhs: f32, rhs: f32) -> f32 {
            libm::$libm(lhs, rhs)
        }
    };
}

macro_rules! binary_f64 {
    ($name:ident, $libm:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(lhs: f64, rhs: f64) -> f64 {
            libm::$libm(lhs, rhs)
        }
    };
}

unary_f32!(atanf, atanf);
unary_f32!(ceilf, ceilf);
unary_f32!(cosf, cosf);
unary_f32!(erff, erff);
unary_f32!(exp2f, exp2f);
unary_f32!(expf, expf);
unary_f32!(expm1f, expm1f);
unary_f32!(fabsf, fabsf);
unary_f32!(floorf, floorf);
unary_f32!(log10f, log10f);
unary_f32!(log1pf, log1pf);
unary_f32!(log2f, log2f);
unary_f32!(logf, logf);
unary_f32!(roundf, roundf);
unary_f32!(sinf, sinf);
unary_f32!(sqrtf, sqrtf);
unary_f32!(tanhf, tanhf);

binary_f32!(atan2f, atan2f);
binary_f32!(fmodf, fmodf);
binary_f32!(powf, powf);
binary_f32!(remainderf, remainderf);

unary_f64!(atan, atan);
unary_f64!(ceil, ceil);
unary_f64!(cos, cos);
unary_f64!(erf, erf);
unary_f64!(exp, exp);
unary_f64!(exp2, exp2);
unary_f64!(expm1, expm1);
unary_f64!(fabs, fabs);
unary_f64!(floor, floor);
unary_f64!(log, log);
unary_f64!(log10, log10);
unary_f64!(log1p, log1p);
unary_f64!(log2, log2);
unary_f64!(round, round);
unary_f64!(sin, sin);
unary_f64!(sqrt, sqrt);
unary_f64!(tanh, tanh);

binary_f64!(atan2, atan2);
binary_f64!(fmod, fmod);
binary_f64!(pow, pow);
binary_f64!(remainder, remainder);

#[no_mangle]
pub extern "C" fn lround(value: f64) -> c_long {
    libm::round(value) as c_long
}

#[no_mangle]
pub extern "C" fn lroundf(value: f32) -> c_long {
    libm::roundf(value) as c_long
}

#[no_mangle]
pub extern "C" fn llround(value: f64) -> c_longlong {
    libm::round(value) as c_longlong
}

#[no_mangle]
pub extern "C" fn llroundf(value: f32) -> c_longlong {
    libm::roundf(value) as c_longlong
}
