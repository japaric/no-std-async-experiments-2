//! `async fn` and `.await` re-implemented as macros on top of `Generator`s

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]

pub use gen_async_await_macros::r#async;

/// `e.await` -> `r#await(e)`
// expansion is equivalent to the desugaring of `($g).await` -- see
// rust-lang/rust/src/librustc/hir/lowering/expr.rs (Rust 1.39)
// XXX Does `$g` need to satisfy the `: Unpin` bound? -- I think not because `$g` is drived to
// completion so any self-referential borrow will be over by the time this macro returns control
// back to the caller. This is unlike `futures::select!` which partially polls its input futures.
// Those input futures may be moved around and then passed to a different `select!` call; the move
// can invalidate self-referential borrows so the input future must satisfy `Unpin`
#[macro_export]
macro_rules! r#await {
    ($g:expr) => {
        match $g {
            mut pinned => {
                use core::ops::Generator;
                loop {
                    match unsafe { core::pin::Pin::new_unchecked(&mut pinned).resume() } {
                        core::ops::GeneratorState::Yielded(()) => {}
                        core::ops::GeneratorState::Complete(x) => break x,
                    }
                    yield ()
                }
            }
        }
    };
}
