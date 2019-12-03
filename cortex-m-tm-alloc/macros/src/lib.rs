extern crate proc_macro;

use proc_macro::TokenStream;
use std::collections::HashSet;

use proc_macro2::Span;
use quote::quote;
use syn::{parse, parse_macro_input, Expr, Ident, Item, ItemStatic, Stmt, Visibility};

/// Declares an allocator that can only be used in "thread-mode" (AKA `#[entry]`, `#[init]` or
/// `#[idle]`)
#[proc_macro_attribute]
pub fn allocator(args: TokenStream, item: TokenStream) -> TokenStream {
    let lazy = if args.is_empty() {
        false
    } else {
        let args = args.to_string();

        if args == "lazy" {
            true
        } else {
            return parse::Error::new(
                Span::call_site(),
                &format!("expected `lazy`, found `{}`", args),
            )
            .to_compile_error()
            .into();
        }
    };

    let item = parse_macro_input!(item as ItemStatic);

    if item.mutability.is_none() {
        return parse::Error::new(
            Span::call_site(),
            "expected `static mut` variable, found `static` variable",
        )
        .to_compile_error()
        .into();
    }

    if item.vis != Visibility::Inherited {
        return parse::Error::new(
            Span::call_site(),
            "expected `static mut` variable, found `PUB static mut` variable",
        )
        .to_compile_error()
        .into();
    }

    let krate = Ident::new("cortex_m_tm_alloc", Span::call_site());
    let ident = &item.ident;
    let ty = &item.ty;
    let expr = item.expr;
    let fns = if lazy {
        let expr = if let Expr::Block(e) = *expr {
            let (statics, stmts) = match extract_statics(e.block.stmts) {
                Ok(x) => x,
                Err(e) => return e.to_compile_error().into(),
            };

            let statics = statics
                .into_iter()
                .map(|statik| {
                    let ident = &statik.ident;
                    let expr = &statik.expr;
                    let ty = &statik.ty;

                    quote!(
                        #[allow(non_snake_case)]
                        let #ident: &'static mut #ty = {
                            static mut #ident: #ty = #expr;
                            unsafe { &mut #ident }
                        };
                    )
                })
                .collect::<Vec<_>>();

            quote!({
                #(#statics)*
                #(#stmts)*
            })
        } else {
            quote!(#expr)
        };

        quote!(
            #[inline(always)]
            pub fn get() -> Option<Self> {
                static mut INITIALIZED: bool = false;

                let p = unsafe { #krate::Private::get() };
                p.map(|_private| {
                    // NOTE this closure will never be reentered
                    if unsafe { !INITIALIZED } {
                        // we wrap the expression in a closure so the compiler is able to produce a
                        // separate subroutine instead of potentially turning `get` into a long
                        // subroutine
                        (|| {
                            // NOTE this section of code runs exactly once
                            let e = #expr;
                            unsafe { Self::_ptr().write(e) }
                        })()
                    }

                    Self { _private }
                })
            }

            /// IMPLEMENTATION DETAIL -- DO NOT USE
            #[doc(hidden)]
            #[inline(always)]
            fn _ptr() -> *mut #ty {
                static mut #ident: core::mem::MaybeUninit<#ty> = core::mem::MaybeUninit::uninit();

                unsafe { #ident.as_mut_ptr() }
            }
        )
    } else {
        quote!(
            pub fn get() -> Option<Self> {
                unsafe {
                    #krate::Private::get().map(|_private| {
                        Self { _private }
                    })
                }
            }

            /// IMPLEMENTATION DETAIL -- DO NOT USE
            #[doc(hidden)]
            #[inline(always)]
            fn _ptr() -> *mut #ty {
                static mut #ident: #ty = #expr;

                unsafe { &mut #ident }
            }
        )
    };
    quote!(
        #[derive(Clone)]
        pub struct #ident {
            _private: #krate::Private,
        }

        impl Copy for #ident {}

        impl #ident {
            #fns
        }

        impl core::fmt::Debug for #ident {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.write_str(stringify!(#ident))
            }
        }

        impl #krate::Alloc for #ident {
            unsafe fn alloc(
                &mut self,
                layout: core::alloc::Layout,
            ) -> Result<core::ptr::NonNull<u8>, ()> {
                <#ty as #krate::Alloc>::alloc(&mut *Self::_ptr(), layout)
            }

            unsafe fn dealloc(
                &mut self,
                ptr: core::ptr::NonNull<u8>,
                layout: core::alloc::Layout,
            ) {
                <#ty as #krate::Alloc>::dealloc(&mut *Self::_ptr(), ptr, layout)
            }

            unsafe fn grow_in_place(
                &mut self,
                ptr: core::ptr::NonNull<u8>,
                layout: core::alloc::Layout,
                new_size: usize,
            ) -> Result<(), ()> {
                <#ty as #krate::Alloc>::grow_in_place(
                    &mut *Self::_ptr(),
                    ptr,
                    layout,
                    new_size,
                )
            }

            unsafe fn shrink_in_place(
                &mut self,
                ptr: core::ptr::NonNull<u8>,
                layout: core::alloc::Layout,
                new_size: usize,
            ) -> Result<(), ()> {
                <#ty as #krate::Alloc>::shrink_in_place(
                    &mut *Self::_ptr(),
                    ptr,
                    layout,
                    new_size,
                )
            }

            unsafe fn realloc(
                &mut self,
                ptr: core::ptr::NonNull<u8>,
                layout: core::alloc::Layout,
                new_size: usize,
            ) -> Result<core::ptr::NonNull<u8>, ()> {
                <#ty as #krate::Alloc>::realloc(
                    &mut *Self::_ptr(),
                    ptr,
                    layout,
                    new_size,
                )
            }
        }
    )
    .into()
}

fn extract_statics(stmts: Vec<Stmt>) -> parse::Result<(Vec<ItemStatic>, Vec<Stmt>)> {
    let mut istmts = stmts.into_iter();

    let mut seen = HashSet::new();
    let mut locals = vec![];
    let mut stmts = vec![];
    while let Some(stmt) = istmts.next() {
        match stmt {
            Stmt::Item(Item::Static(static_)) => {
                if static_.mutability.is_some() {
                    if seen.contains(&static_.ident) {
                        return Err(parse::Error::new(
                            static_.ident.span(),
                            "this local `static` appears more than once",
                        ));
                    }

                    seen.insert(static_.ident.clone());
                    locals.push(static_);
                } else {
                    stmts.push(Stmt::Item(Item::Static(static_)));
                    break;
                }
            }

            _ => {
                stmts.push(stmt);
                break;
            }
        }
    }

    stmts.extend(istmts);

    Ok((locals, stmts))
}

// fn extract_fns(stmts: Vec<Stmt>) -> parse::Result<(Vec<ItemFn>, Vec<Stmt>)> {
//     let mut istmts = stmts.into_iter();

//     let mut seen = HashSet::new();
//     let mut fns = vec![];
//     let mut stmts = vec![];
//     while let Some(stmt) = istmts.next() {
//         match stmt {
//             Stmt::Item(Item::Function(f)) => {
//                 if static_.mutability.is_some() {
//                     if seen.contains(&static_.ident) {
//                         return Err(parse::Error::new(
//                             static_.ident.span(),
//                             "this local `static` appears more than once",
//                         ));
//                     }

//                     seen.insert(static_.ident.clone());
//                     locals.push(static_);
//                 } else {
//                     stmts.push(Stmt::Item(Item::Static(static_)));
//                     break;
//                 }
//             }

//             _ => {
//                 stmts.push(stmt);
//                 break;
//             }
//         }
//     }

//     stmts.extend(istmts);

//     Ok((locals, stmts))
// }
