use core::{
    alloc::Layout,
    cell::Cell,
    intrinsics::abort,
    marker::PhantomData,
    ops,
    ptr::{self, NonNull},
};

use alloc_trait::Alloc;

use crate::unique::Unique;

struct RcBox<T>
where
    T: ?Sized,
{
    strong: Cell<usize>,
    // weak: Cell<usize>,
    value: T,
}

pub struct Rc<T, A>
where
    A: Alloc,
    T: ?Sized,
{
    // NOTE alternatively the `allocator` could be stored in `RcBox`
    allocator: A,
    ptr: NonNull<RcBox<T>>,
    phantom: PhantomData<RcBox<T>>,
}

impl<T, A> Rc<T, A>
where
    A: Alloc,
    T: ?Sized,
{
    pub fn new(value: T, mut allocator: A) -> Rc<T, A>
    where
        T: Sized,
    {
        let u = Unique::alloc(
            RcBox {
                strong: Cell::new(1),
                // weak: Cell::new(1),
                value,
            },
            &mut allocator,
        );

        Self::from_inner(*u, allocator)
    }

    pub fn strong_count(this: &Self) -> usize {
        this.strong()
    }

    fn from_inner(ptr: NonNull<RcBox<T>>, allocator: A) -> Self {
        Self {
            allocator,
            ptr,
            phantom: PhantomData,
        }
    }

    fn inner(&self) -> &RcBox<T> {
        unsafe { self.ptr.as_ref() }
    }

    fn strong(&self) -> usize {
        self.inner().strong.get()
    }

    fn inc_strong(&self) {
        let strong = self.strong();

        if strong == 0 || strong == usize::max_value() {
            unsafe {
                abort();
            }
        }
        self.inner().strong.set(strong + 1);
    }

    fn dec_strong(&self) {
        self.inner().strong.set(self.strong() - 1);
    }

    // fn weak(&self) -> usize {
    //     self.inner().weak.get()
    // }

    // fn dec_weak(&self) {
    //     self.inner().weak.set(self.weak() - 1);
    // }
}

impl<A, T> Clone for Rc<T, A>
where
    T: ?Sized,
    A: Alloc + Copy,
{
    fn clone(&self) -> Self {
        self.inc_strong();
        Self::from_inner(self.ptr, self.allocator)
    }
}

unsafe impl<A, #[may_dangle] T> Drop for Rc<T, A>
where
    T: ?Sized,
    A: Alloc,
{
    fn drop(&mut self) {
        unsafe {
            self.dec_strong();
            if self.strong() == 0 {
                ptr::drop_in_place(self.ptr.as_mut());

                // self.dec_weak();

                // if self.weak() == 0 {
                self.allocator
                    .dealloc(self.ptr.cast(), Layout::for_value(self.ptr.as_ref()));
                // }
            }
        }
    }
}

impl<T, A> ops::Deref for Rc<T, A>
where
    T: ?Sized,
    A: Alloc,
{
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner().value
    }
}
