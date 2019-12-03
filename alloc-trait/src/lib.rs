//! `core::alloc::Alloc` on stable

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]

use core::{
    alloc::Layout,
    cmp,
    ptr::{self, NonNull},
};

/// See [`core::alloc::Alloc`][0]
///
/// [0]: https://doc.rust-lang.org/core/alloc/trait.Alloc.html
pub trait Alloc {
    /// See [`core::alloc::Alloc.alloc`][0]
    ///
    /// [0]: https://doc.rust-lang.org/core/alloc/trait.Alloc.html#tymethod.alloc
    unsafe fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()>;

    /// See [`core::alloc::Alloc.dealloc`][0]
    ///
    /// [0]: https://doc.rust-lang.org/core/alloc/trait.Alloc.html#tymethod.dealloc
    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: core::alloc::Layout);

    /// See [`core::alloc::Alloc.grow_in_place`][0]
    ///
    /// [0]: https://doc.rust-lang.org/core/alloc/trait.Alloc.html#tymethod.grow_in_place
    unsafe fn grow_in_place(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<(), ()>;

    /// See [`core::alloc::Alloc.shrink_in_place`][0]
    ///
    /// [0]: https://doc.rust-lang.org/core/alloc/trait.Alloc.html#tymethod.shrink_in_place
    unsafe fn shrink_in_place(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<(), ()>;

    /// See [`core::alloc::Alloc.realloc`][0]
    ///
    /// [0]: https://doc.rust-lang.org/core/alloc/trait.Alloc.html#tymethod.realloc
    unsafe fn realloc(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, ()> {
        let old_size = layout.size();

        if new_size >= old_size {
            if self.grow_in_place(ptr, layout, new_size).is_ok() {
                return Ok(ptr);
            }
        } else if new_size < old_size {
            if self.shrink_in_place(ptr, layout, new_size).is_ok() {
                return Ok(ptr);
            }
        }

        // otherwise, fall back on alloc + copy + dealloc.
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let result = self.alloc(new_layout);
        if let Ok(new_ptr) = result {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), cmp::min(old_size, new_size));
            self.dealloc(ptr, layout);
        }
        result
    }
}
