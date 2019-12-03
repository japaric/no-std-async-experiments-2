# `no-std-async-experiments-2`

(For historical reasons the name says `no-std` but this is specifically about
*embedded* `no_std` programs that target microcontrollers like ARM Cortex-M
based ones)

(Part I, about (unergonomic and limited) heapless executors and waking
mechanisms, can be found [here]; there's hardly any explanation text there
though)

[here]: https://github.com/japaric/no-std-async-experiments

Status: 🔬 **Proof of Concept** 🧪

The crates is this repository will not be published on crates.io; please do not
depend on them as they'll *not* be maintained.

## Goal

The goal of this experiment was to develop a cooperative scheduler that can work
within a [Real Time For the Masses][https://rtfm.rs] (RTFM) application without
reducing its suitability for building real time applications, that is the
cooperative scheduler should not make WCET (Worst-Case Execution Time) analysis
of the overall application harder to perform.

NB: Although the goal is literally "suitable for use in RTFM"; the scheduler can
be used outside RTFM applications, that is in pure `cortex-m-rt` applications as
shown in the examples contained in this document.

## Background

### Asynchronous code

As of Rust 1.39 the `async fn` / `.await` feature has been stabilized. This
language feature provides the ability to write cooperative code. `async fn` is
used to declare asynchronous functions; within an `async fn` one can use the
`.await` operator to drive an asynchronous operation (e.g. another `async fn`)
to completion in a non-blocking fashion.

``` rust
// toolchain: 1.39.0

// `std::fs::File::open`
async fn open_file(path: &Path) -> File {
    // ..
}

// `std::io::Write for std::fs::File`
async fn write_to_file(file: &mut File, bytes: &[u8]) {
    // ..
}

// `std::fs::write`
async fn write_file(path: &Path, contents: &[u8]) {
    let mut f = open_file(path).await;
    write_to_file(&mut f, bytes).await;
}
```

Asynchronous code is meant to be executed by a (task) *executor*; no executor is
provided in the standard library but third party crates like [`async-std`] and
[`tokio`] provide multi-threaded executors. Asynchronous code to be executed by
the executor is logically split into *tasks*; a task is basically is an
*instance* of an `async fn` that has been scheduled to run on the executor.

[`async-std`]: https://crates.io/crates/async-std
[`tokio`]: https://crates.io/crates/tokio

``` rust
// toolchain: 1.39.0
// async-std = "1.2.0"

use async_std::task;

fn main() {
    // schedule one instance of task `foo` -- nothing is printed at this point
    task::spawn(foo());

    println!("start");
    // start task `bar` and drive it to completion
    // this puts the executor to work
    // it's implementation defined whether `foo` or `bar` runs first or
    // whether `foo` gets to run at all
    task::block_on(bar());
}

async fn foo() {
    println!("foo");
}

async fn bar() {
    println!("bar");
}
```

``` console
$ cargo run
start
foo
bar
```

The executor executes tasks cooperatively; in its simplest form the executor
will run a task until it reaches an `.await` operation; if that operation would
need to block (e.g. because it's waiting on a socket, etc.) then
executor suspends that task and moves to, *resumes*, another one. Thus, `.await`
operators are *potential* suspension points within asynchronous code. An
efficient executor will only resume tasks that it knows will be able to make
progress (that is, they will not immediately yield again); this minimizes the
amount of context switching between tasks.

### Some implementation details

In Rust, the core building block for cooperative multitasking (AKA asynchronous
code) are generators. Syntactically, a generator looks like a closure (`|| { ..
}` ) with suspension points (`yield`). Semantically, a generator is a state
machine where each state consists of the execution of arbitrary code (between
`yield` points) and transitions are controlled externally (using `resume`).

``` rust
// toolchain: nightly-2019-12-02

use core::{pin::Pin, ops::Generator};

fn main() {
    let mut g = || {
        println!("A");
        yield;
        println!("B");
        yield;
        println!("C");
    };

    let mut state = Pin::new(&mut g).resume();
    println!("{:?}", state);
    state = Pin::new(&mut g).resume();
    println!("{:?}", state);
    state = Pin::new(&mut g).resume();
    println!("{:?}", state);
}
```

In its simplest form (ignoring fairness and efficiency), an executor needs to
keep a list of these state machines and continuously `resume` them until they
complete. As each of these state machines may have a different size and runs
different code when `resume`-d some form of indirection is required to store
them in a list. So each element in the list will be a trait object, e.g.
`Box<dyn Generator>`, instead of a concrete generator, i.e. `impl Generator`.

``` rust
fn executor(mut tasks: Vec<Pin<Box<dyn Generator<Yield = (), Return = ()>>>>) {
    let mut n = tasks.len();
    while n != 0 {
        for i in (0..n).rev() {
            let state = tasks[i].as_mut().resume();
            if let GeneratorState::Complete(()) = state {
                tasks.swap_remove(i); // done; remove
            }
        }

        n = tasks.len();
    }
}
```

## Idea

The approach that will be explored here will consist of isolating all
cooperative multitasking ("asynchronous code") to the `#[idle]` context (or `fn
main` in non-RTFM applications).

There are few reasons for this:

- I expect that some of cooperative tasks that users will write will be
  never-ending and some others will be short-lived. Thus it's sensible to run
  the executor in `#[idle]`, which is the never-ending background context.

- All the dynamic memory allocations required by the executor can be constrained
  to `#[idle]`. Meaning that we can use a non-real-time allocator in `#[idle]`
  and leave regular tasks completely free of dynamic memory allocation (\*). As
  the allocator will be exclusively used in `#[idle]` we don't need any form of
  mutex to protect it -- `#[idle]` effectively owns the allocator.

(There's an hypothetical third reason: hyper-tuning the allocator; this will be
explored later on)

(\*) Or least make dynamic allocation in tasks opt-in. We can give tasks a
resource-locked [TLSF] allocator; this gives them the ability to `alloc` and
`dealloc` (but not `realloc`) in bounded constant time regardless of the size of
the allocation.

[TLSF]: https://github.com/japaric/tlsf

## Implementation

The implementation has two main components: a "thread-mode" allocator and a
"thread-mode" executor.

The "thread-mode" moniker is a bit unfortunate but it refers to the fact that
the allocator / executor will be constrained to what ARM calls "Thread mode".
That is the allocator / executor can not be accessed / used from "Handler mode"
(another ARM term). "Handler mode" is basically interrupt / exception context,
whereas "Thread mode" is non-interrupt context. All code executed by the Reset
handler (e.g. after booting) runs in "Thread mode"; in RTFM apps `#[init]` and
`#[idle]` run in "Thread mode"; in `cortex-m-rt` apps `#[entry]` runs in "Thread
mode".

### TM (Thread-Mode) allocator

(NB: a complete version of the snippets presented here can be found in the
`examples` directory. You can run the examples (`cargo run`) if you have QEMU
and the `thumb7m-none-eabi` installed; you can find installation instructions in
the [rust-embedded] book)

The TM allocator is a separate allocator, independent of the global allocator
one can be define with `#[global_allocator]`. Ideally, we would use the `Alloc`
trait and allocator-generic collections specified (the later loosely specified)
in [RFC #1398] to implement this allocator but the former, the `Alloc` trait, is
unstable and the later, the collections, don't exist -- all `alloc` collections
are hard-coded to use the `#[global_allocator]`.

On the bright side, we can implement the TM allocator on stable but we can't
implement all the collections because some of them depend on unstable features
(e.g. `core::intrinsics::abort` in `Rc`). Also, on stable we can't implement
coercion for these collections so you can't coerce `Box<impl Generator>`
(concrete type) into `Box<dyn Generator>` (trait object) or `Box<[u8; 64]>`
(array) into `Box<[u8]>` (slice).

[RFC #1398]: https://github.com/rust-lang/rfcs/blob/master/text/1398-kinds-of-allocators.md

The API devised for the TM allocator looks like this:

``` rust
// toolchain: 1.39.0

use cortex_m_alloc::allocator;
use tlsf::Tlsf;

#[allocator(lazy)]
static mut A: Tlsf = {
    // `MEMORY` is transformed into `&'static mut [u8; 64]`
    static mut MEMORY: [u8; 64] = [0; 64];

    let mut tlsf = Tlsf::new();
    tlsf.extend(MEMORY);
    tlsf
};
```

This defines a TM allocator named `A`. `A` is a `lazy`-ly (runtime) initialized
[TLSF] allocator. The block expression on the right hand side of the static item
is executed at runtime exactly once, so the usual `static mut X: T` -> `X:
&'static mut T` transformation applies. Also, since this code is executed at
runtime rather than at compile time the allocator constructor doesn't need to be
a `const fn`.

[TLSF]: https://github.com/japaric/tlsf

You can get a handle to the `A` allocator using the `get` constructor. The
constructor returns `Option<A>`; when called in "thread-mode" it always returns
the `Some` variant; on the other hand it always returns `None` when called in
"handler-mode". `A` is a zero sized type (ZST) that implements the `Copy` and
`Alloc` traits but doesn't implement the `Send` and `Sync` traits. Not
implementing `Send` ensures an instance is never sent into an interrupt /
exception handler.

``` rust
#[entry]
fn main() -> ! {
    hprintln!("before A::get()").ok();
    SCB::set_pendsv();

    if let Some(a) = A::get() {
        hprintln!("after A::get()").ok();
        SCB::set_pendsv();

        // ..
    } else {
        // UNREACHABLE
    }

    // ..
}

#[exception]
fn PendSV() {
    hprintln!("PendSV({:?})", A::get()).ok();
}
```

``` console
$ cargo run
before A::get()
PendSV(None)
after A::get()
PendSV(None)
```

Once you get an instance of `A` you can use it initialize to a collection by
passing a copy of it to its constructor. (The collection stores a copy of the
allocator handle; thanks to `A` being a ZST this doesn't increase the stack-size
of the collection)

``` rust
if let Some(a) = A::get() {
    // ..

    let mut xs: Vec<i32, A> = Vec::new(a);

    for i in 0.. {
        xs.push(i);
        hprintln!("{:?}", xs).ok();
    }
}
```

As with the global allocator, a "thread-mode" allocator may run out of memory.
In that case, the Out-Of-Memory handler defined using the `#[oom]` attribute
will get called.

``` rust
#[alloc_oom::oom]
fn oom(layout: Layout) -> ! {
    hprintln!("oom({:?})", layout).ok();
    debug::exit(debug::EXIT_FAILURE);
    loop {}
}
```

``` console
$ cargo run
[0]
[0, 1]
[0, 1, 2]
[0, 1, 2, 3]
oom(Layout { size_: 32, align_: 4 })

$ echo $?
1
```

It should be noted that "thread-mode" allocators never mask interrupts; they
don't internally use `RefCell` either so no runtime checks or panicking branches
there; the fast path of their `get` constructor compiles down to 3 instructions
(load, shift left, conditional branch); and their lazy initialization uses a
single extra byte of static memory and doesn't require atomics.

### TM (Thread-Mode) executor

The other component is the TM (task) executor. This executor depends on a TM
allocator and it's declared like this:

``` rust
// toolchain: nightly-2019-12-02

use cortex_m_alloc::allocator;
use cortex_m_executor::executor;

#[allocator(lazy)]
static mut A: Tlsf = { /* .. */ };

executor!(name = X, allocator = A);
```

Like the TM allocator you can only get a handle to this TM executor in
thread-mode context using the `get` constructor. This functions returns a handle
to the TM executor *and* a handle to its TM allocator. The handle to the TM
executor is `Copy` but not `Send` or `Sync`.

The executor handle can be used to `spawn` tasks. `spawn` takes a *concrete*
generator, boxes it and stores it in an internal queue; `spawn` doesn't execute
any of the generator / task code! To start executing tasks you use the
`block_on` API. This function takes a generator that will driven to completion
(without boxing it) while making progress on other previously spawned tasks.

Here's an example:

``` rust
#[entry]
fn main() -> ! {
    if let Some((x, _a)) = X::get() {
        x.spawn(move || {
            hprintln!(" A0").ok();
            yield;

            hprintln!(" A1").ok();
            // but of course you can `spawn` a task from a spawned task
            x.spawn(|| {
                hprintln!("  C0").ok();
                yield;

                hprintln!("  C1").ok();
            });
            yield;

            hprintln!(" A2").ok();
            // NOTE return value will be discarded
            42
        });

        let ans = x.block_on(|| {
            hprintln!("B0").ok();
            yield;

            hprintln!("B1").ok();
            yield;

            hprintln!("B2").ok();
            yield;

            hprintln!("B3").ok();

            42
        });

        hprintln!("the answer is {}", ans).ok();
    }

    debug::exit(debug::EXIT_SUCCESS);

    loop {}
}
```

Given that the executor handle is `Copy` one can move the handle into a spawned
task and spawn another task from it. This can be seen in the example: `main`
spawns task `A` and task `A` spawns task `C`.

`block_on` does *not* guarantee that *all* previously spawned tasks will be
driven to completion; it unless drives its argument generator to completion.
This can be seen in the example: task `C` is not completed by the time
`block_on` returns.

#### Deadlocks (not?)

`block_on` seems better than `spawn` because it doesn't box its generator and
it's able to preserve the return value of the generator. However, nesting
`block_on` calls can lead to deadlocks. Here's an example using `async-std`.

``` rust
// toolchain: 1.39.0
// async-std = "1.2.0"

use async_std::{sync, task};

fn main() {
    task::block_on(async {
        let (s, r) = sync::channel::<i32>(1);

        task::block_on(async move {
            let x = r.recv().await;
            println!("got {:?}", x);
        });

        s.send(0).await;
        println!("send");
    });
}
```

This program hangs and nothing is printed to the console. If we replace the
inner `block_on` with `spawn` then we get the intended output of:

``` console
$ cargo run
send
got Some(0)
```

(I know, I know. "Nobody writes code like this". I agree this is unlikely if the
program is short enough to fit in a single file but I think the chances of
running into are non-negligible once your program spans several files, or worst
crates)

For this reason and to simplify the implementation nesting `block_on` calls will
panic the TM executor. (I *think* it's not possible to deadlock the executor
with this restriction but have no way to prove it)

#### `#[r#async]` / `r#await!`

If you can't nest `block_on` calls then how do we drive generators to
completion (in a non-blocking fashion)? We use the `r#await!` macro. There's
also an `#[r#async]` attribute that save typing time when writing functions that
return generators.

As an example, let's say you want to asynchronous receive items send from an
interrupt handler. You would write your asynchronous function using `#[r#async]`
like this:

``` rust
use core::ops::Generator;

use heapless::{spsc::Consumer, ArrayLength};
use gen_async_await::r#async;

#[r#async]
fn dequeue<T, N>(mut c: Consumer<'static, T, N>) -> (T, Consumer<'static, T, N>)
where
    N: ArrayLength<T>,
{
    loop {
        if let Some(x) = c.dequeue() {
            break (x, c);
        }
        yield
    }
}

// OR you could have written; both are equivalent
fn dequeue2<T, N>(
    mut c: Consumer<'static, T, N>,
) -> impl Generator<Yield = (), Return = (T, Consumer<'static, T, N>)>
where
    N: ArrayLength<T>,
{
    || loop {
        if let Some(x) = c.dequeue() {
            break (x, c);
        }
        yield
    }
}
```

(If you are wondering why I'm passing the `Consumer` by value rather than by
reference: it's to work around the lack of support for self-referential borrows
in generators; I'll get back to this later on)

Then in the application you would write something like this:

``` rust
#[entry]
fn main() -> ! {
    static mut Q: Queue<i32, consts::U4> = Queue(i::Queue::new());

    let (p, mut c) = Q.split();

    // send the producer to an interrupt handler
    send(p);

    if let Some((x, _a)) = X::get() {
        // task that asynchronously processes items produced by
        // the interrupt handler
        x.spawn(move || loop {
            let ret = r#await!(dequeue(c)); // <-
            let item = ret.0;
            c = ret.1;
            // do stuff with `item`
        });

        x.block_on(|| {
            // .. do something else ..
        });
    }

    debug::exit(debug::EXIT_SUCCESS);

    loop {}
}
```

The infinite task will `r#await!` the `#[r#async]` function we wrote before.

#### Fixed capacity queue

The TM executor uses a variable capacity queue by default (e.g. `alloc::Vec`)
but it's possible to switch to a fixed capacity queue (e.g. [`heapless::Vec`]).
If you can upper bound the number of spawned tasks in your program it may be
advantageous to use a fixed capacity queue. With a fixed capacity queue, the
queue is allocated once, and could even be allocated on the stack. Plus, if you
are using the allocator only for the task executor then the compiler can
optimize away the `realloc` routine (and the `grow_in_place` and
`shrink_in_place` routines called by it) as only `alloc` and `dealloc` are
required to box generators and destroy them.

[`heapless::Vec`]: https://docs.rs/heapless/0.5.1/heapless/struct.Vec.html

The syntax to switch to the fixed capacity queue is shown below:

``` rust
use executor::executor;

// fixed-capacity = 4 tasks
executor!(name = X, allocator = A, max_spawned_tasks = U4);
```

### `std_async::sync::Mutex`?

With `async-std` if you want to share memory between two tasks you need to reach
out for its `Mutex` or its `RwLock` abstraction because the `task::spawn` API
requires that its argument generator implements the `Send` trait. This is
required because the executor is multi-threaded so tasks can run in parallel.

``` rust
use async_std::{sync::Mutex, task};

fn main() {
    let shared: &'static _ = Box::leak(Box::new(Mutex::new(0u128)));

    task::spawn(async move {
        let x = shared.lock().await;
        println!("{}", x);
    });

    task::block_on(async move {
        *shared.lock().await += 1;
    });
}
```

In our case, the TM executor runs everything on the same context so tasks will
always be resumed serially (one after the one). Thus no `Send` bound is
required on the generator passed to `spawn`; therefore instead of a `Mutex` you
use a plain `RefCell` (or a `Cell`) to share data between tasks.

``` rust
use core::cell::RefCell;

#[entry]
fn main() -> ! {
    static mut SHARED: RefCell<u64> = RefCell::new(0);

    if let Some((x, _a)) = X::get() {
        let shared: &'static _ = SHARED;

        x.spawn(move || loop {
            hprintln!("{}", shared.borrow()).ok();
            yield;
        });

        x.block_on(move || {
            *shared.borrow_mut() += 1;
            yield;
            *shared.borrow_mut() += 1;
            yield;
        });
    }

    debug::exit(debug::EXIT_SUCCESS);

    loop {}
}
```

Note that it's *not* necessary to `r#await!` to access the shared data.

### `std::sync::Arc`?

Sometimes you want the shared data to eventually be freed up. In `async-std` you
would reach out for a `Send`-able `Arc` to delegate the destruction of the data
to the last user.

``` rust
use async_std::{sync::Arc, task};

fn main() {
    let a = Arc::new(0);
    let b = a.clone();
    task::spawn(async move {
        if Arc::strong_count(&a) == 1 {
            println!("A: I'll destroy the Arc!")
        }
        drop(a);
    });

    task::spawn(async move {
        if Arc::strong_count(&b) == 1 {
            println!("B: I'll destroy the Arc!")
        }
        drop(b);
    });

    task::block_on(async move {
        // do something else
    });
}
```

With the TM executor you can make do with a non-atomic `Rc`.

``` rust
#[entry]
fn main() -> ! {
    if let Some((x, a)) = X::get() {
        let a = Rc::new(0, a);
        let b = a.clone();

        x.spawn(move || {
            if Rc::strong_count(&a) == 1 {
                hprintln!("A: I'll destroy the Rc!").ok();
            }
            drop(a);
            yield;
        });

        x.spawn(move || {
            if Rc::strong_count(&b) == 1 {
                hprintln!("B: I'll destroy the Rc!").ok();
            }
            drop(b);
            yield;
        });

        x.block_on(move || {
            yield;
            yield;
        });
    }

    debug::exit(debug::EXIT_SUCCESS);

    loop {}
}
```

## Limitations

### Self-referential generators

AFAICT, as of nightly-2019-12-02, all generators created using the `|| { yield
}` syntax are marked `Unpin` (they are *movable*) and self-referential borrows
are forbidden inside them. See below:

``` rust
fn main() {
    let g = || {
        let x = 0;
        let y = &x; //~ ERROR: borrow may still be in use when generator yields
        yield; //~ INFO: possible yield occurs here
        drop(y);
    };
}
```

This limits what one can do with `#[r#async]` / `r#await!` implemented directly
on top of generators. In contrast, in the built-in `async fn` / `.await`
language feature the future returned by an `async fn` function is always marked
`!Unpin` (the future is *immovable*) and self-referential borrows are allowed.
See below:

``` rust
use core::{future::Future, ops::Generator};

fn main() {
    let g = || yield;
    is_future(&g); // always false
    is_generator(&g); // always true
    is_unpin(&g); // (currently?) always true

    let f = foo();
    is_future(&f); // always true
    is_generator(&f); // always false
    is_unpin(&f); // (currently?) always false
}

async fn foo() {}

fn is_future(_: &impl Future) {}
fn is_generator(_: &impl Generator) {}
fn is_unpin(_: &impl Unpin) {}
```

We saw an example of what can't be written due to the lack self-referential
generators in the `dequeue` function (section `#[r#async]` and `r#await!`). Here
show it can be written using futures and `async fn` / `.await`.

`#[r#async]` / `r#await!` version

``` rust
// so, actually you can write this
#[r#async]
fn dequeue<'a, T, N>(c: &'a mut Consumer<'static, T, N>) -> T
where
    N: ArrayLength<T>,
{
    loop {
        if let Some(x) = c.dequeue() {
            break x;
        }
        yield
    }
}

// but then you cannot use it
#[r#async]
fn task(mut c: Consumer<'static, i32, U4>) {
    loop {
        //~ ERROR: borrow may still be in use when generator yields
        let item = r#await!(dequeue(&mut c));
        //~ INFO:                   ^^^^^^ possible yield occurs here
        // do stuff
    }
}
```

`async fn` / `.await` version

``` rust
use core::{future::Future, ops::Generator, task::Poll};

use futures::future;
use heapless::{spsc::Consumer, ArrayLength, consts::U4};

fn dequeue<'a, T, N>(c: &'a mut Consumer<'static, T, N>) -> impl Future<Output = T> + 'a
where
    N: ArrayLength<T>,
{
    future::poll_fn(move |cx| {
        if let Some(item) = c.dequeue() {
            return Poll::Ready(item);
        }
        // NOTE: dumb but without this, awaiting this value may hang
        cx.waker().wake_by_ref();
        Poll::Pending
    })
}

async fn task(mut c: Consumer<'static, i32, U4>) {
    loop {
        let item = dequeue(&mut c).await;
    }
}
```

#### What's even this `Unpin` stuff?

The `Pin` abstraction and the `Unpin` marker trait enable the memory-safe
creation and use of structs with *self-referential fields*. Self-referential
fields mean that one of the field of the struct can be a reference to a another
field of the same struct.

"Wait, what do structs have to do with generators?" The generator syntax is
sugar for creating a struct that implements the `Generator` trait; the state of
the generator is stored in the fields of this struct.

For example the following generator:

(which as I mentioned is not accepted today but I hope it'll be accepted in the
future and marked `!Unpin`)

``` rust
let g = || {
    let x = 0;
    let y = &x;
    yield;
    use(y);
};
```

could be represented by the following struct:

``` rust
struct G {
    x: i32,
    // (`'self` is made up syntax)
    y: &'self i32, // points to `x`

    state: i32, // keeps track of which `yield` where are currently at
}
```

`x` is part of the state of the generator so it's a field of the struct. `y` is
a reference to this variable but also part of the state so it needs to be a
field too. Thus the struct contains a self-referential reference: `y` points to
the field `x`.

It would be problematic if we could `resume` this generator *and* then move it.
Here's an example:

``` rust
fn foo() {
   let mut g = bar();
   Pin::new(&mut g).resume();
}

fn bar() -> impl Generator {
    let mut g = || {
        let x = 0;
        let y = &x;
        yield;
        use(y);
    };

    Pin::new(&mut g).resume();
    g
}
```

In `bar` we create the generator and resume it once. At that point, the state of
the generator contains initialized `x` and `y` fields. However, both `x` and `y`
live on the stack frame of function `bar` and `y` points into the stack frame of
`bar`. When the generator is returned from `bar` to `foo` the stack frame where
the generator was created is destroyed. By the time we call the second `resume`
in `foo`, `x` is valid but `y` is not; it's still pointing into the destroyed
stack frame. This second `resume` call is UB.

The way Rust prevents this misuse of generators is the `Unpin` marker trait. One
can only call `Pin::new(&mut g)` if `g` implements the `Unpin` trait. However,
in this case the generator is `!Unpin` because of the self-referential borrow.
Thus the compiler rejects this code at compile time.

Does this means we can't never `resume` generators that contain self-referential
borrows? It's possible to `resume` them but they need to be properly pinned.
This updated example compiles and its sound:

``` rust
fn foo() {
   let mut bg = bar();
   bg.resume();
}

fn bar() -> Box<Pin<impl Generator>> {
    let g = || {
        let x = 0;
        let y = &x;
        yield;
        use(y);
    };
    let mut bg = Box::pin(g);

    bg.resume();
    bg
}
```

In the updated example we first box-pin the generator. The boxed generator can
be safely resumed in `bar` and in `foo`. The reason this operation is now safe
is that `bg.x` is stored in the heap and has a stable address. Moving the boxed
generator from `bar` to `foo` doesn't change the address of `bg.x` so the
self-reference `bg.y` is not invalidated by the move.

## Potential improvements

### Hyper-tuning the allocator

As I mentioned before if one goes down the route of using the TM allocator only
in the TM executor *and* bounds the maximum number of spawned tasks then the TM
allocator only needs to `alloc` and `dealloc` generators (tasks). As all these
generators are `: Sized` we can extract their size information from the output
of the compiler (e.g. LLVM IR or machine code). With this information, we could
optimize the allocator for this particular payload; this could either mean using
just a few single-linked lists of free blocks as the allocator (fastest, even
constant time, but not memory efficient) or configuring a more general allocator
to have size classes that closely match the expected allocation requests.

### Less dumb executor

The executor in this proof of concept is extremely dumb: it resumes
all tasks non-stop (without ever sleeping) in a round-robin-ish fashion  until
they complete.

In contrast, `async fn` / `.await` executors only resume tasks that can make
progress (that have been "woken"). This maximizes throughput in multi-threaded
systems but I'm not sure the complex book-keeping required to achieve this
"perfect resumption" is worth the effort in constrained systems that will only
handle a few concurrent tasks. The CPU time spend in book-keeping could be
comparable to the amount of work spent in application logic (some embedded
systems do little work and sleep most of the time). Plus, the memory used for
book-keeping could be a sizable part of the limited amount found in these
systems. More worrying though is that HAL authors will likely be writing this
complex book-keeping code to provide `async` APIs in their libraries; the more
complex the code to integrate the higher chance to also introduce bugs.

Any improvement on this front needs to take all these factors into account:
being able to get some sleep is better than perfectly avoiding any unnecessary
resumptions; book-keeping needs to be lightweight, both in terms of memory usage
and CPU time; the solution needs to be easy to integrate by HAL authors.

## Other observations

### Panicking branches

Resuming even the simplest generator `|| { yield }` produces machine code that
contains panicking branches that will never be reached at runtime. This may be
solved by improving the MIR / LLVM-IR codegen pass in `rustc`. More details can
be found in [rust-lang/rust#66100].

[rust-lang/rust#66100]: https://github.com/rust-lang/rust/issues/66100

### Benchmarks

The context switching performance of the proof of concept was measured
using the following snippet:

``` rust
x.spawn(move || {
    asm::bkpt();
    yield;
});

x.spawn(|| {
    asm::bkpt();
    yield;
});

x.block_on(|| {
    asm::bkpt();
    yield;

    asm::bkpt();
    yield;
});
```

The CYCcle CouNTer (CYCCNT) was read at each breakpoint and the difference
was found to be 19-36 cycles.

As a reference the context switching in an RTFM application was also measured
using the following snippets.

- Software tasks

``` rust
#[task(priority = 2)]
fn a(cx: a::Context) {
    asm::bkpt();
}

#[task(priority = 2)]
fn b(cx: b::Context) {
    asm::bkpt();
}

#[task(priority = 1)]
fn c(cx: c::Context) {
    asm::bkpt();
}
```

`a` -> `b` (same priority) took 24 cycles; `b -> c` (to lower priority) took 35 cycles.

- Hardware tasks

``` rust
#[task(binds = EXTI0, priority = 2)]
fn a(cx: a::Context) {
    asm::bkpt();
}

#[task(binds = EXTI1, priority = 2)]
fn b(cx: b::Context) {
    asm::bkpt();
}

#[task(binds = EXTI2, priority = 1)]
fn c(cx: c::Context) {
    asm::bkpt();
}
```

Both `a` -> `b` and `b` -> `c` took 8 cycles.

## Conclusions

We have presented a proof of concept implementation of async-await that can be
used today in embedded no-std code (though it requires nightly).

The cooperative scheduler (AKA executor) is completely isolated from interrupt
handlers. Thus the cooperative code can run without compromising the
predictability of real-time / time-sensitive code running in interrupt handlers.
This has been achieved by having the executor use a dedicated memory allocator,
separate from the `#[global_allocator]`. Furthermore, in some configurations one
can extract information from the compiler to optimize this allocator for the
target application.

Lastly, the memory allocator API developed during this experiment can be used in
no-std binary crates on stable, unlike the `#[global_allocator]` API which
depends on the unstable `#[alloc_error_handler]` feature. Unfortunately, this
allocator API does not inter-operate with the collections in `alloc` so these
collections need to be re-implemented to use the alternative allocator API.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
