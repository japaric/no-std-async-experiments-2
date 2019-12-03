#![deny(warnings)]
#![feature(generator_trait)]
#![feature(generators)]
#![no_main]
#![no_std]

use core::{alloc::Layout, ops::Generator};

use alloc_oom::oom;
use cortex_m_tm_alloc::allocator;
use cortex_m_tm_executor::executor;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use gen_async_await::{r#async, r#await};
use heapless::{
    consts, i,
    spsc::{Consumer, Queue},
    ArrayLength,
};
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
    static mut Q: Queue<i32, consts::U4> = Queue(i::Queue::new());

    let (p, mut c) = Q.split();

    // send this to an interrupt handler
    send(p);

    if let Some((x, _a)) = X::get() {
        x.spawn(move || loop {
            let ret = r#await!(dequeue(c));
            let _item = ret.0;
            c = ret.1;
            // do stuff with `item`
        });

        x.block_on(|| {
            // .. do something else ..
            yield
        });
    }

    debug::exit(debug::EXIT_SUCCESS);

    loop {}
}

#[r#async]
fn dequeue<T, N>(mut c: Consumer<'static, T, N>) -> (T, Consumer<'static, T, N>)
where
    N: ArrayLength<T>,
{
    loop {
        if let Some(x) = c.dequeue() {
            break (x, c);
        }
        yield
    }
}

#[allow(dead_code)]
fn dequeue2<T, N>(
    mut c: Consumer<'static, T, N>,
) -> impl Generator<Yield = (), Return = (T, Consumer<'static, T, N>)>
where
    N: ArrayLength<T>,
{
    || loop {
        if let Some(x) = c.dequeue() {
            break (x, c);
        }
        yield
    }
}

fn send<T>(_: T) {}

#[oom]
fn on_oom(_layout: Layout) -> ! {
    hprintln!("OOM").ok();
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}
