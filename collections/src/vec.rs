use core::{alloc::Layout, cmp, mem, ops, ptr, slice};

use alloc_trait::Alloc;

use crate::unique::Unique;

pub struct Vec<T, A>
where
    A: Alloc,
{
    allocator: A,
    cap: usize,
    len: usize,
    ptr: Unique<T>,
}

impl<A, T> Vec<T, A>
where
    A: Alloc,
{
    pub fn new(allocator: A) -> Self {
        let cap = if mem::size_of::<T>() == 0 {
            usize::max_value()
        } else {
            0
        };

        Self {
            allocator,
            cap,
            len: 0,
            ptr: Unique::empty(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn push(&mut self, elem: T) {
        if self.len == self.cap {
            self.reserve(1);
        }

        unsafe {
            self.as_mut_ptr().add(self.len).write(elem);
            self.len += 1;
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        if self.cap.wrapping_sub(self.len) >= additional {
            return;
        }

        unsafe {
            let (new_cap, new_layout) = amortized_new_capacity(self.len, additional)
                .and_then(|new_cap| layout_array::<T>(new_cap).map(|layout| (new_cap, layout)))
                .unwrap_or_else(|| capacity_overflow());

            let res = match self.current_layout() {
                None => self.allocator.alloc(new_layout),
                Some(layout) => self
                    .allocator
                    .realloc(self.ptr.cast(), layout, new_layout.size()),
            };

            self.ptr = Unique::new_unchecked(
                res.unwrap_or_else(|_| alloc_oom::oom(new_layout))
                    .as_ptr()
                    .cast(),
            );
            self.cap = new_cap;
        }
    }

    pub fn swap_remove(&mut self, index: usize) -> T {
        unsafe {
            // We replace self[index] with the last element. Note that if the
            // bounds check on hole succeeds there must be a last element (which
            // can be self[index] itself).
            let hole: *mut T = &mut self[index];
            let last = ptr::read(self.get_unchecked(self.len - 1));
            self.len -= 1;
            ptr::replace(hole, last)
        }
    }

    fn current_layout(&self) -> Option<Layout> {
        if self.cap == 0 {
            None
        } else {
            unsafe {
                let align = mem::align_of::<T>();
                let size = mem::size_of::<T>() * self.cap;
                Some(Layout::from_size_align_unchecked(size, align))
            }
        }
    }
}

fn amortized_new_capacity(curr: usize, additional: usize) -> Option<usize> {
    let double_cap = curr.checked_mul(2)?;
    let required_cap = curr.checked_add(additional)?;

    Some(cmp::max(double_cap, required_cap))
}

fn capacity_overflow() -> ! {
    panic!("capacity overflow")
}

impl<A, T> ops::Deref for Vec<T, A>
where
    A: Alloc,
{
    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<A, T> ops::DerefMut for Vec<T, A>
where
    A: Alloc,
{
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

// unstable methods of `core::alloc::Layout`
fn layout_array<T>(n: usize) -> Option<Layout> {
    layout_repeat(&Layout::new::<T>(), n).map(|(k, _)| k)
}

fn layout_repeat(layout: &Layout, n: usize) -> Option<(Layout, usize)> {
    let padded_size = layout
        .size()
        .checked_add(padding_needed_for(layout, layout.align()))?;

    let alloc_size = padded_size.checked_mul(n)?;

    unsafe {
        // self.align is already known to be valid and alloc_size has been
        // padded already.
        Some((
            Layout::from_size_align_unchecked(alloc_size, layout.align()),
            padded_size,
        ))
    }
}

fn padding_needed_for(layout: &Layout, align: usize) -> usize {
    let len = layout.size();

    let len_rounded_up = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);

    len_rounded_up.wrapping_sub(len)
}
