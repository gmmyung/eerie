#![cfg(all(feature = "std", feature = "runtime"))]

use eerie::runtime::{BufferView, DeviceSpec, Runtime};

// Keep this wrapper inspectable after linking. Its C callee is defined in
// IREE's buffer_view.c, so a load-only body demonstrates cross-language LTO.
#[no_mangle]
#[inline(never)]
pub fn eerie_cross_lang_lto_rank(view: &BufferView<f32>) -> usize {
    view.rank()
}

#[test]
fn buffer_rank_crosses_the_ffi_boundary() {
    let runtime = Runtime::new(DeviceSpec::local_sync()).unwrap();
    let view = runtime.buffer_view(&[2, 2], &[1.0_f32; 4]).unwrap();
    assert_eq!(eerie_cross_lang_lto_rank(std::hint::black_box(&view)), 2);
}
