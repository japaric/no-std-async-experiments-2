//! Expected output:
//!
//! ```
//! before A::get()
//! PendSV(None)
//! after A::get()
//! PendSV(None)
//! [0]
//! [0, 1]
//! [0, 1, 2]
//! [0, 1, 2, 3]
//! oom(Layout { size_: 32, align_: 4 })
//! ```

#![deny(warnings)]
#![no_main]
#![no_std]

use core::alloc::Layout;

use collections::Vec;
use cortex_m::peripheral::SCB;
use cortex_m_tm_alloc::allocator;
use cortex_m_rt::{entry, exception};
use cortex_m_semihosting::{debug, hprintln};
use panic_semihosting as _;
use tlsf::Tlsf;

#[allocator(lazy)]
static mut A: Tlsf = {
    static mut MEMORY: [u8; 64] = [0; 64];

    let mut tlsf = Tlsf::new();
    tlsf.extend(MEMORY);
    tlsf
};

#[entry]
fn main() -> ! {
    hprintln!("before A::get()").ok();
    SCB::set_pendsv();

    if let Some(a) = A::get() {
        hprintln!("after A::get()").ok();
        SCB::set_pendsv();

        let mut xs: Vec<i32, A> = Vec::new(a);

        for i in 0.. {
            xs.push(i);
            hprintln!("{:?}", &*xs).ok();
        }
    } else {
        // UNREACHABLE
    }

    loop {}
}

#[exception]
fn PendSV() {
    hprintln!("PendSV({:?})", A::get()).ok();
}

#[alloc_oom::oom]
fn oom(layout: Layout) -> ! {
    hprintln!("oom({:?})", layout).ok();
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}
