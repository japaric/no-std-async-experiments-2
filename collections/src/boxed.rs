use core::{alloc::Layout, cmp, fmt, ops, pin::Pin, ptr};

use alloc_trait::Alloc;

use crate::unique::Unique;

pub struct Box<T, A>
where
    A: Alloc,
    T: ?Sized,
{
    allocator: A,
    ptr: Unique<T>,
}

impl<A, T> Box<T, A>
where
    A: Alloc,
{
    /// Allocates memory on the allocator `A` and then places `x` into it.
    pub fn new(value: T, mut allocator: A) -> Self {
        let ptr = Unique::alloc(value, &mut allocator);
        Box { allocator, ptr }
    }
}

#[cfg(feature = "coerce")]
impl<A, T, U> ops::CoerceUnsized<Box<U, A>> for Box<T, A>
where
    A: Alloc,
    T: ?Sized + core::marker::Unsize<U>,
    U: ?Sized,
{
}

impl<A, T> ops::Deref for Box<T, A>
where
    T: ?Sized,
    A: Alloc,
{
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.ptr.as_ptr() }
    }
}

impl<A, T> ops::DerefMut for Box<T, A>
where
    T: ?Sized,
    A: Alloc,
{
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr.as_ptr() }
    }
}

impl<A, T> Drop for Box<T, A>
where
    A: Alloc,
    T: ?Sized,
{
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::for_value(self.ptr.as_ref());
            ptr::drop_in_place(self.ptr.as_ptr());
            self.allocator.dealloc((*self.ptr).cast(), layout)
        }
    }
}

impl<A, T> fmt::Debug for Box<T, A>
where
    T: ?Sized + fmt::Debug,
    A: Alloc,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as fmt::Debug>::fmt(self, f)
    }
}

impl<A, T> fmt::Display for Box<T, A>
where
    T: ?Sized + fmt::Display,
    A: Alloc,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as fmt::Display>::fmt(self, f)
    }
}

impl<A, T> Eq for Box<T, A>
where
    T: ?Sized + Eq,
    A: Alloc,
{
}

impl<A, T> Unpin for Box<T, A>
where
    A: Alloc,
    T: ?Sized,
{
}

#[cfg(feature = "generator")]
impl<A, G> ops::Generator for Box<G, A>
where
    A: Alloc,
    G: ops::Generator + Unpin + ?Sized,
{
    type Yield = G::Yield;
    type Return = G::Return;

    fn resume(mut self: Pin<&mut Self>) -> ops::GeneratorState<G::Yield, G::Return> {
        G::resume(Pin::new(&mut *self))
    }
}

#[cfg(feature = "generator")]
impl<A, G> ops::Generator for Pin<Box<G, A>>
where
    A: Alloc,
    G: ops::Generator + ?Sized,
{
    type Yield = G::Yield;
    type Return = G::Return;

    fn resume(mut self: Pin<&mut Self>) -> ops::GeneratorState<G::Yield, G::Return> {
        G::resume((*self).as_mut())
    }
}

impl<A, B, T> PartialEq<Box<T, B>> for Box<T, A>
where
    T: ?Sized + PartialEq,
    A: Alloc,
    B: Alloc,
{
    fn eq(&self, other: &Box<T, B>) -> bool {
        <T as PartialEq>::eq(self, other)
    }
}

impl<A, B, T> PartialOrd<Box<T, B>> for Box<T, A>
where
    T: ?Sized + PartialOrd,
    A: Alloc,
    B: Alloc,
{
    fn partial_cmp(&self, other: &Box<T, B>) -> Option<cmp::Ordering> {
        <T as PartialOrd>::partial_cmp(self, other)
    }
}

impl<A, T> From<Box<T, A>> for Pin<Box<T, A>>
where
    A: Alloc,
    T: ?Sized,
{
    fn from(boxed: Box<T, A>) -> Self {
        unsafe { Pin::new_unchecked(boxed) }
    }
}
