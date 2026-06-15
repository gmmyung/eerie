use core::ffi::{c_int, c_long, c_uint, c_void};

const PTHREAD_BUSY: c_int = 16;
const PTHREAD_TIMED_OUT: c_int = 110;

#[repr(C)]
pub struct timespec {
    tv_sec: c_long,
    tv_nsec: c_long,
}

#[inline]
unsafe fn lock_mutex(mutex: *mut c_uint) {
    loop {
        if try_lock_mutex(mutex) {
            return;
        }
        core::hint::spin_loop();
    }
}

#[inline]
unsafe fn try_lock_mutex(mutex: *mut c_uint) -> bool {
    critical_section::with(|_| {
        if unsafe { *mutex } == 0 {
            unsafe { *mutex = 1 };
            true
        } else {
            false
        }
    })
}

#[inline]
unsafe fn unlock_mutex(mutex: *mut c_uint) {
    critical_section::with(|_| unsafe {
        *mutex = 0;
    });
}

#[no_mangle]
pub unsafe extern "C" fn call_once(flag: *mut c_uint, func: Option<unsafe extern "C" fn()>) {
    let should_call = critical_section::with(|_| {
        if unsafe { *flag } == 0 {
            unsafe { *flag = 1 };
            true
        } else {
            false
        }
    });

    if should_call {
        if let Some(func) = func {
            unsafe { func() };
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_once(
    flag: *mut c_uint,
    func: Option<unsafe extern "C" fn()>,
) -> c_int {
    unsafe { call_once(flag, func) };
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_init(mutex: *mut c_uint, _attr: *const c_void) -> c_int {
    unsafe {
        *mutex = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_destroy(_mutex: *mut c_uint) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_lock(mutex: *mut c_uint) -> c_int {
    unsafe { lock_mutex(mutex) };
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_trylock(mutex: *mut c_uint) -> c_int {
    if unsafe { try_lock_mutex(mutex) } {
        0
    } else {
        PTHREAD_BUSY
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_unlock(mutex: *mut c_uint) -> c_int {
    unsafe { unlock_mutex(mutex) };
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_init(cond: *mut c_uint, _attr: *const c_void) -> c_int {
    unsafe {
        *cond = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_destroy(_cond: *mut c_uint) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_broadcast(_cond: *mut c_uint) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_signal(_cond: *mut c_uint) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_wait(_cond: *mut c_uint, mutex: *mut c_uint) -> c_int {
    unsafe {
        unlock_mutex(mutex);
        lock_mutex(mutex);
    }
    // This bare-metal backend has no scheduler or blocking primitive. IREE's
    // no-thread runtime path should not rely on condvar waits; report timeout
    // instead of spinning forever if one is reached.
    PTHREAD_TIMED_OUT
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_timedwait(
    _cond: *mut c_uint,
    mutex: *mut c_uint,
    _deadline: *const timespec,
) -> c_int {
    unsafe {
        unlock_mutex(mutex);
        lock_mutex(mutex);
    }
    PTHREAD_TIMED_OUT
}

#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_init(attr: *mut c_uint) -> c_int {
    unsafe {
        *attr = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_setclock(_attr: *mut c_uint, _clock: c_int) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn pthread_condattr_destroy(_attr: *mut c_uint) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn clock_gettime(_clock_id: c_int, ts: *mut timespec) -> c_int {
    if !ts.is_null() {
        unsafe {
            (*ts).tv_sec = 0;
            (*ts).tv_nsec = 0;
        }
    }
    0
}
