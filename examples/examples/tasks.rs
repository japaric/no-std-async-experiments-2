//! Expected output:
//!
//! B0
//!  A0
//! B1
//!  A1
//! B2
//!  A2
//!   C0
//! B3
//! the answer is 42

#![deny(warnings)]
#![feature(generator_trait)]
#![feature(generators)]
#![no_main]
#![no_std]

use core::alloc::Layout;

use alloc_oom::oom;
use cortex_m_tm_alloc::allocator;
use cortex_m_tm_executor::executor;
use cortex_m_rt::entry;
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

executor!(name = X, allocator = A);

#[entry]
fn main() -> ! {
    if let Some((x, _a)) = X::get() {
        x.spawn(move || {
            hprintln!(" A0").ok();
            yield;

            hprintln!(" A1").ok();
            // but of course you can `spawn` a task from a spawned task
            x.spawn(|| {
                hprintln!("  C0").ok();
                yield;

                hprintln!("  C1").ok();
            });
            yield;

            hprintln!(" A2").ok();
            // NOTE return value will be discarded
            42
        });

        let ans = x.block_on(|| {
            hprintln!("B0").ok();
            yield;

            hprintln!("B1").ok();
            yield;

            hprintln!("B2").ok();
            yield;

            hprintln!("B3").ok();

            42
        });

        hprintln!("the answer is {}", ans).ok();
    }

    debug::exit(debug::EXIT_SUCCESS);

    loop {}
}

#[oom]
fn on_oom(_layout: Layout) -> ! {
    hprintln!("OOM").ok();
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}
