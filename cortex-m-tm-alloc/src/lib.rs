//! "Thread-mode" allocators for the ARM Cortex-M architecture
//!
//! **DANGER**: Ironically, this crate is *NOT* compatible with threads. Using this crate in a
//! thread environment will result in an unsound program.
//!
//! A "thread-mode" allocator is an allocator that can only be used in what ARM defines as the
//! Thread context; this context corresponds to the `#[cortex_m_rt::entry]` function (or the
//! `#[init]` and `#[idle]` functions in RTFM applications) and all functions called from it. In
//! other words, these allocators can *not* be used in interrupt / exception context (or in RTFM
//! tasks).
//!
//! # Example
//!
//! ```
//! use cortex_m_allocator::allocator;
//! use cortex_m_rt::{entry, exception};
//!
//! // `SomeAllocator` must implement the `Alloc` trait
//! #[allocator(lazy)]
//! static mut A: SomeAllocator = {
//!     // `lazy` means this allocator will be initialized at runtime the first time is `get`-ed
//!     // thus noting in this block needs to be a `const fn`
//!
//!     // memory to be managed by the allocator
//!     static mut MEMORY: [u8; 1024] = [0; 1024];
//!
//!     // `MEMORY` becomes `&'static mut [u8; 1024]`
//!     SomeAllocator::new(MEMORY)
//! };
//!
//! #[entry]
//! fn main() {
//!     if let Some(a) = A::get() {
//!         // `a` has type `A`; `A` implements the `Alloc` and `Copy` traits but NOT the
//!         // `Send` or `Sync` traits
//!     }
//! }
//!
//! #[exception]
//! fn SysTick() {
//!     // the allocator cannot be accessed / used from interrupt handlers
//!     assert!(A::get().is_none());
//! }
//! ```

#![no_std]

use core::marker::PhantomData;

/// IMPLEMENTATION DETAIL
#[doc(hidden)]
pub use alloc_trait::Alloc;
pub use cortex_m_tm_alloc_macros::allocator;

/// IMPLEMENTATION DETAIL
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Private {
    _not_send_or_sync: PhantomData<*mut ()>,
}

impl Private {
    pub unsafe fn get() -> Option<Self> {
        if cfg!(not(cortex_m)) {
            return None;
        }

        const SCB_ICSR: *const u32 = 0xE000_ED04 as *const u32;

        if SCB_ICSR.read_volatile() as u8 == 0 {
            // Thread mode (i.e. not within an interrupt or exception handler)
            Some(Private {
                _not_send_or_sync: PhantomData,
            })
        } else {
            None
        }
    }
}
