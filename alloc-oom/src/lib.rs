//! Stable alternative to `#[alloc_error_handler]`

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]

use core::alloc::Layout;

pub use alloc_oom_macros::oom;

/// Calls the Out-Of-Memory handler
///
/// If there's any user of the `oom` function then an Out-Of-Memory handler must be declared (using
/// the `#[oom]` attribute) exactly once somewhere in the dependency graph
pub fn oom(layout: Layout) -> ! {
    extern "Rust" {
        fn oom(layout: Layout) -> !;
    }

    unsafe { oom(layout) }
}
