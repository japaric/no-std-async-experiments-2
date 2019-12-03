//! Newtype over `japaric/tlsf::Tlsf` to implement the `alloc_trait::Alloc` trait
//!
//! Documentation: https://github.com/japaric/tlsf

#![deny(warnings)]
#![no_std]

use core::{alloc::Layout, ops, ptr::NonNull};

pub struct Tlsf {
    inner: tlsf::Tlsf,
}

impl Tlsf {
    pub const fn new() -> Self {
        Self {
            inner: tlsf::Tlsf::new(),
        }
    }
}

impl ops::Deref for Tlsf {
    type Target = tlsf::Tlsf;

    fn deref(&self) -> &tlsf::Tlsf {
        &self.inner
    }
}

impl ops::DerefMut for Tlsf {
    fn deref_mut(&mut self) -> &mut tlsf::Tlsf {
        &mut self.inner
    }
}

impl alloc_trait::Alloc for Tlsf {
    unsafe fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        self.inner.alloc(layout)
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, _layout: core::alloc::Layout) {
        self.inner.dealloc(ptr)
    }

    unsafe fn grow_in_place(
        &mut self,
        ptr: NonNull<u8>,
        _layout: Layout,
        new_size: usize,
    ) -> Result<(), ()> {
        self.inner.grow_in_place(ptr, new_size)
    }

    unsafe fn shrink_in_place(
        &mut self,
        ptr: NonNull<u8>,
        _layout: Layout,
        new_size: usize,
    ) -> Result<(), ()> {
        self.inner.shrink_in_place(ptr, new_size)
    }

    unsafe fn realloc(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, ()> {
        self.inner.realloc(ptr, layout, new_size)
    }
}
