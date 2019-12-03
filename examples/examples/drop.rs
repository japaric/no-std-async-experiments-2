//! Expected output (it's implementation defined):
//!
//! A: I'll destroy the Rc!
//!
//! OR
//!
//! B: I'll destroy the Rc!

#![deny(warnings)]
#![feature(generator_trait)]
#![feature(generators)]
#![no_main]
#![no_std]

use core::alloc::Layout;

use alloc_oom::oom;
use collections::rc::Rc;
use cortex_m_tm_alloc::allocator;
use cortex_m_tm_executor::executor;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use panic_semihosting as _;
use tlsf::Tlsf;

#[allocator(lazy)]
static mut A: Tlsf = {
    static mut MEMORY: [u8; 128] = [0; 128];

    let mut tlsf = Tlsf::new();
    tlsf.extend(MEMORY);
    tlsf
};

executor!(name = X, allocator = A);

#[entry]
fn main() -> ! {
    if let Some((x, a)) = X::get() {
        let a = Rc::new(0, a);
        let b = a.clone();

        x.spawn(move || {
            if Rc::strong_count(&a) == 1 {
                hprintln!("A: I'll destroy the Rc!").ok();
            }
            drop(a);
            yield;
        });

        x.spawn(move || {
            if Rc::strong_count(&b) == 1 {
                hprintln!("B: I'll destroy the Rc!").ok();
            }
            drop(b);
            yield;
        });

        x.block_on(move || {
            yield;
            yield;
        });
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
