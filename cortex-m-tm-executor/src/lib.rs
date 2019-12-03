#![feature(generator_trait)]
#![feature(generators)]
#![no_std]

use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    ops::{self, Generator, GeneratorState},
    pin::Pin,
};

use alloc_trait::Alloc;
use collections::{Box, Vec};
use pin_utils::{pin_mut, unsafe_pinned};

#[doc(hidden)]
pub use heapless::consts;

pub mod fixed;

type Task<A> = Box<dyn Generator<Yield = (), Return = ()> + 'static, A>;
type TaskMut<'a> = Pin<&'a mut (dyn Generator<Yield = (), Return = ()> + 'static)>;

#[macro_export]
macro_rules! executor {
    (name = $name:ident, allocator = $alloc:ident) => {
        #[derive(Clone, Copy)]
        pub struct $name {
            _private: $crate::Private,
        }

        impl $name {
            pub fn get() -> Option<(Self, $alloc)> {
                $alloc::get().map(|a| {
                    // NOTE this closure will never be reentered
                    static mut INITIALIZED: bool = false;

                    if unsafe { !INITIALIZED } {
                        // NOTE this section of code runs exactly once
                        (|| unsafe {
                            Self::_ptr().write($crate::Executor::new(a));
                        })()
                    }

                    (
                        Self {
                            _private: unsafe { $crate::Private::new() },
                        },
                        a,
                    )
                })
            }

            pub fn block_on<T>(&self, g: impl core::ops::Generator<Yield = (), Return = T>) -> T {
                unsafe { (*Self::_ptr()).block_on(g) }
            }

            pub fn spawn<T>(&self, g: impl core::ops::Generator<Yield = (), Return = T> + 'static) {
                unsafe { (*Self::_ptr()).spawn(g) }
            }

            fn _ptr() -> *mut $crate::Executor<$alloc> {
                static mut $name: core::mem::MaybeUninit<$crate::Executor<$alloc>> =
                    core::mem::MaybeUninit::uninit();

                unsafe { $name.as_mut_ptr() }
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.write_str(stringify!($name))
            }
        }
    };

    (name = $name:ident, allocator = $alloc:ident, max_spawned_tasks = $N:ident) => {
        #[derive(Clone, Copy)]
        pub struct $name {
            _private: $crate::Private,
        }

        impl $name {
            pub fn get() -> Option<(Self, $alloc)> {
                $alloc::get().map(|a| {
                    // NOTE this closure will never be reentered
                    static mut INITIALIZED: bool = false;

                    if unsafe { !INITIALIZED } {
                        // NOTE this section of code runs exactly once
                        (|| unsafe {
                            Self::_ptr().write($crate::fixed::Executor::new(a));
                        })()
                    }

                    (
                        Self {
                            _private: unsafe { $crate::Private::new() },
                        },
                        a,
                    )
                })
            }

            pub fn block_on<T>(&self, g: impl core::ops::Generator<Yield = (), Return = T>) -> T {
                unsafe { (*Self::_ptr()).block_on(g) }
            }

            pub fn spawn<T>(&self, g: impl core::ops::Generator<Yield = (), Return = T> + 'static) {
                unsafe { (*Self::_ptr()).spawn(g) }
            }

            fn _ptr() -> *mut $crate::fixed::Executor<$alloc, $crate::consts::$N> {
                static mut $name: core::mem::MaybeUninit<
                    $crate::fixed::Executor<$alloc, $crate::consts::$N>,
                > = core::mem::MaybeUninit::uninit();

                unsafe { $name.as_mut_ptr() }
            }
        }
    };
}

pub struct Executor<A>
where
    A: Alloc + Copy,
{
    allocator: A,
    /// Spawned tasks
    tasks: UnsafeCell<Vec<Pin<Task<A>>, A>>,
    running: Cell<bool>,
}

impl<A> Executor<A>
where
    A: Alloc + Copy,
{
    /// IMPLEMENTATION DETAIL
    #[doc(hidden)]
    pub fn new(allocator: A) -> Self {
        Self {
            tasks: UnsafeCell::new(Vec::new(allocator)),
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

            // next we are going to resume previously spawned tasks; we have to be extra careful
            // about two things:
            //
            // these tasks can get a handle to *this* executor (`&Executor`). For soudness any
            // reference to `self.tasks` (as in `&[mut] Vec<_>`) must be destroyed before we call
            // into a task (otherwise we may mutably alias the `self.tasks` field)
            //
            // The other issue is that a task may append new tasks (using `spawn`). We provide no
            // guarantees about fairness but we'll resume each task *currently* in the list *once*
            // in every pass without any promise about the order in which tasks are resumed.
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
        // this alternative to `GenDrop` produces larger heap allocations
        // let g = || drop(r#await!(g));
        let task: Task<A> = Box::new(GenDrop { g }, self.allocator);
        unsafe {
            (*self.tasks.get()).push(task.into());
        }
    }
}

struct GenDrop<G> {
    g: G,
}

impl<G> GenDrop<G> {
    // NOTE trivial projection (I hope)
    unsafe_pinned!(g: G);
}

impl<G> ops::Deref for GenDrop<G> {
    type Target = G;

    fn deref(&self) -> &G {
        &self.g
    }
}

impl<G> ops::DerefMut for GenDrop<G> {
    fn deref_mut(&mut self) -> &mut G {
        &mut self.g
    }
}

impl<G> Generator for GenDrop<G>
where
    G: Generator<Yield = ()>,
{
    type Yield = ();
    type Return = ();

    fn resume(self: Pin<&mut Self>) -> GeneratorState<(), ()> {
        match G::resume(self.g()) {
            GeneratorState::Yielded(()) => GeneratorState::Yielded(()),
            GeneratorState::Complete(x) => {
                drop(x);
                GeneratorState::Complete(())
            }
        }
    }
}
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct Private {
    _not_send_or_sync: PhantomData<*mut ()>,
}

impl Private {
    pub unsafe fn new() -> Self {
        Self {
            _not_send_or_sync: PhantomData,
        }
    }
}
