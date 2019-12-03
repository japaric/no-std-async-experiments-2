//! Expected output:
//!
//! 1
//! 2

#![deny(warnings)]
#![feature(generator_trait)]
#![feature(generators)]
#![no_main]
#![no_std]

use core::{alloc::Layout, cell::RefCell};

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
    static mut SHARED: RefCell<u64> = RefCell::new(0);

    if let Some((x, _a)) = X::get() {
        let shared: &'static _ = SHARED;

        x.spawn(move || loop {
            hprintln!("{}", shared.borrow()).ok();
            yield;
        });

        x.block_on(move || {
            *shared.borrow_mut() += 1;
            yield;
            *shared.borrow_mut() += 1;
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
