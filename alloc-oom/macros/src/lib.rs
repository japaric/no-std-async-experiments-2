//! The `#[oom]` attribute

#![deny(missing_docs)]
#![deny(warnings)]

extern crate proc_macro;

use core::mem;
use proc_macro::TokenStream;

use proc_macro2::Span;
use quote::quote;
use syn::{parse, parse_macro_input, spanned::Spanned, FnArg, ItemFn, ReturnType, Type};

/// Declares the Out-Of-Memory handler
///
/// If there's any user of the `oom` function then an Out-Of-Memory handler must be declared exactly
/// once somewhere in the dependency graph
///
/// Usage
///
/// ```
/// use core::alloc::Layout;
///
/// #[oom]
/// fn oom(layout: Layout) -> ! {
///     // ..
/// }
/// ```
#[proc_macro_attribute]
pub fn oom(args: TokenStream, input: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return parse::Error::new(Span::call_site(), "`#[oom]` takes no arguments")
            .to_compile_error()
            .into();
    }

    let mut item = parse_macro_input!(input as ItemFn);

    let sig = &item.sig;
    let is_valid = sig.constness.is_none()
        && sig.asyncness.is_none()
        && sig.abi.is_none()
        && sig.generics.params.is_empty()
        && sig.generics.where_clause.is_none()
        && sig.inputs.len() == 1
        && match &sig.inputs[0] {
            FnArg::Receiver(_) => false,
            FnArg::Typed(arg) => match &*arg.ty {
                Type::Path(ty) => ty
                    .path
                    .segments
                    .last()
                    .map(|seg| seg.ident == "Layout")
                    .unwrap_or(false),
                _ => false,
            },
        }
        && is_divergent(&sig.output)
        && sig.variadic.is_none();

    if !is_valid {
        return parse::Error::new(
            sig.span(),
            "function must have signature `fn(core::alloc::Layout) -> !`",
        )
        .to_compile_error()
        .into();
    }

    let attrs = mem::replace(&mut item.attrs, vec![]);
    let vis = &item.vis;
    let ident = &sig.ident;
    quote!(
        #(#attrs)*
        #[export_name = "oom"]
        #vis fn #ident(layout: core::alloc::Layout) {
            #[inline(always)]
            #item

            #ident(layout)
        }
    )
    .into()
}

fn is_divergent(rt: &ReturnType) -> bool {
    match rt {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => match **ty {
            Type::Never(_) => true,
            _ => false,
        },
    }
}
