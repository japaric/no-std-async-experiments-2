use core::{alloc::Layout, marker::PhantomData, mem, ops, ptr::NonNull};

use alloc_trait::Alloc;

pub struct Unique<T>
where
    T: ?Sized,
{
    ptr: NonNull<T>,
    _marker: PhantomData<T>,
}

impl<T> Unique<T>
where
    T: ?Sized,
{
    pub const fn empty() -> Self
    where
        T: Sized,
    {
        unsafe { Self::new_unchecked(mem::align_of::<T>() as *mut T) }
    }

    pub const unsafe fn new_unchecked(ptr: *mut T) -> Self {
        Self {
            ptr: NonNull::new_unchecked(ptr),
            _marker: PhantomData,
        }
    }

    pub fn new(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| Self {
            ptr,
            _marker: PhantomData,
        })
    }

    pub(crate) fn alloc<A>(value: T, allocator: &mut A) -> Self
    where
        A: Alloc,
        T: Sized,
    {
        unsafe {
            if mem::size_of::<T>() == 0 {
                Unique::new_unchecked(mem::align_of::<T>() as *mut _)
            } else {
                let layout = Layout::new::<T>();

                allocator
                    .alloc(layout)
                    .ok()
                    .map(|nn| {
                        let nn = nn.cast::<T>();
                        nn.as_ptr().write(value);

                        Unique::new_unchecked(nn.as_ptr())
                    })
                    .unwrap_or_else(|| alloc_oom::oom(layout))
            }
        }
    }
}

#[cfg(feature = "coerce")]
impl<T, U> ops::CoerceUnsized<Unique<U>> for Unique<T>
where
    T: ?Sized + core::marker::Unsize<U>,
    U: ?Sized,
{
}

impl<T> ops::Deref for Unique<T>
where
    T: ?Sized,
{
    type Target = NonNull<T>;

    fn deref(&self) -> &NonNull<T> {
        &self.ptr
    }
}

unsafe impl<T> Send for Unique<T> where T: Send + ?Sized {}

unsafe impl<T> Sync for Unique<T> where T: Sync + ?Sized {}
