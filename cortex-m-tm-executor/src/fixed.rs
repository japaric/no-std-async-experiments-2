use core::{
    alloc::Layout,
    cell::{Cell, UnsafeCell},
    ops::{Generator, GeneratorState},
    pin::Pin,
};

use alloc_trait::Alloc;
use collections::Box;
use heapless::Vec;
use pin_utils::pin_mut;

use crate::{GenDrop, Task, TaskMut};

type Tasks<A, N> = Vec<Pin<Task<A>>, N>;

/// Executor variant that only supports a fixed maximum number of tasks
pub struct Executor<A, N>
where
    A: Alloc,
    N: heapless::ArrayLength<Pin<Task<A>>>,
{
    allocator: A,
    /// Spawned tasks
    tasks: UnsafeCell<Tasks<A, N>>,
    running: Cell<bool>,
}

impl<A, N> Executor<A, N>
where
    A: Alloc + Copy,
    N: heapless::ArrayLength<Pin<Task<A>>>,
{
    /// IMPLEMENTATION DETAIL
    #[doc(hidden)]
    pub fn new(allocator: A) -> Self {
        Self {
            tasks: UnsafeCell::new(Vec::new()),
            allocator,
            running: Cell::new(false),
        }
    }

    pub fn block_on<T>(&self, g: impl Generator<Yield = (), Return = T>) -> T {
        assert!(!self.running.get());

        self.running.set(true);

        pin_mut!(g);

        loop {
            // move forward the main task `g`
            if let GeneratorState::Complete(x) = g.as_mut().resume() {
                self.running.set(false);
                break x;
            }

            // since the queue has a fixed capacity and it's stored in a static variable we don't
            // have to worry about pointer invalidation caused by insertion of new tasks *if* we
            // iterator from end to start
            let n = unsafe { (*self.tasks.get()).len() };
            for i in (0..n).rev() {
                let s = {
                    // this is a (pinned) pointer into the trait object (see `TaskMut` alias above)
                    // `spawn` calls performed by `task.resume()` won't invalidate this pointer or
                    // its contents, nor will they alias this reference
                    let task: TaskMut =
                        unsafe { (*self.tasks.get()).get_unchecked_mut(i).as_mut() };
                    task.resume()
                };

                if let GeneratorState::Complete(()) = s {
                    // task completed -- release memory
                    let task = unsafe { (*self.tasks.get()).swap_remove(i) };
                    drop(task);
                }
            }
        }
    }

    pub fn spawn<T>(&self, g: impl Generator<Yield = (), Return = T> + 'static) {
        let task: Task<A> = Box::new(GenDrop { g }, self.allocator);
        unsafe {
            (*self.tasks.get())
                .push(task.into())
                .unwrap_or_else(|_| alloc_oom::oom(Layout::new::<Tasks<A, N>>()));
        }
    }
}
