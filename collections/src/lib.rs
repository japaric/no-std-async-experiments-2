//! Allocator-generic collections
//!
//! No API documentation is provided here but the API follows [`alloc`]'s API
//!
//! [`alloc`]: https://doc.rust-lang.org/nightly/alloc/index.html

#![allow(dead_code)]
#![cfg_attr(feature = "coerce", feature(coerce_unsized))]
#![cfg_attr(feature = "coerce", feature(unsize))]
#![cfg_attr(feature = "generator", feature(generator_trait))]
#![cfg_attr(feature = "rc", feature(core_intrinsics))]
#![cfg_attr(feature = "rc", feature(dropck_eyepatch))]
#![deny(rust_2018_compatibility)]
#![deny(rust_2018_idioms)]
#![no_std]

pub use boxed::Box;
pub use vec::Vec;

pub mod boxed;
#[cfg(feature = "rc")]
pub mod rc;
mod unique;
pub mod vec;
