#![no_std]
#![no_main]

extern crate alloc;

use core::mem::MaybeUninit;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use eerie::runtime::{BufferView, DeviceSpec, Runtime, RuntimeError};
use panic_semihosting as _;

#[global_allocator]
static HEAP: embedded_alloc::LlffHeap = embedded_alloc::LlffHeap::empty();

#[entry]
fn main() -> ! {
    const HEAP_SIZE: usize = 1024 * 1024;
    static mut MEMORY: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(core::ptr::addr_of_mut!(MEMORY) as usize, HEAP_SIZE) };

    match run() {
        Ok(()) => {
            hprintln!("PASS: eerie bare-metal inline VMVX");
            debug::exit(debug::EXIT_SUCCESS);
        }
        Err(error) => {
            hprintln!("FAIL: {:?}", error);
            debug::exit(debug::EXIT_FAILURE);
        }
    }
    loop {
        cortex_m::asm::wfi();
    }
}

fn run() -> Result<(), RuntimeError> {
    // Keep the existing embedded local-sync selector working through inline HAL.
    let runtime = Runtime::new(DeviceSpec::local_sync())?;
    let program = runtime.load_vmfb(include_bytes!("../../mul_vmvx_inline.vmfb"))?;
    let function = program.function("arithmetic.simple_mul")?;
    let data = alloc::vec::Vec::from_iter((0..100).map(|i| i as f32));
    let lhs = runtime.buffer_view(&[100], &data)?;
    let rhs = runtime.buffer_view(&[100], &data)?;
    drop(program);
    drop(runtime);
    let mut outputs = function.invoke([&lhs, &rhs])?;
    let output = BufferView::<f32>::try_from(outputs.remove(0))?;
    drop(function);
    let values = output.read()?;
    assert_eq!(values[0], 0.0);
    assert_eq!(values[7], 49.0);
    assert_eq!(values[99], 9801.0);
    Ok(())
}
