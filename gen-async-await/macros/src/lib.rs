//! The `#[r#async]` attribute

#![deny(missing_docs)]
#![deny(warnings)]

extern crate proc_macro;

use proc_macro::TokenStream;

use proc_macro2::Span;
use quote::quote;
use syn::{parse, parse_macro_input, GenericParam, ItemFn, ReturnType};

/// `async fn foo() { .. }` -> `#[r#async] fn foo() { .. }`
// NOTE the built-in `async fn` desugars to a generator and wraps it in a newtype that makes it
// `!Unpin`; this is required because `async fn`s allow self-referential borrows (i.e. `let x = ..;
// let y = &x; f().await; use(y)`). AFAICT, self referential borrows are not possible in generators
// (as of 1.39) so I think we don't need the newtype
#[proc_macro_attribute]
pub fn r#async(args: TokenStream, item: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return parse::Error::new(Span::call_site(), "`#[async]` attribute takes no arguments")
            .to_compile_error()
            .into();
    }

    let item = parse_macro_input!(item as ItemFn);

    let is_valid =
        item.sig.constness.is_none() && item.sig.asyncness.is_none() && item.sig.abi.is_none();

    if !is_valid {
        return parse::Error::new(Span::call_site(), "function must not be `fn(..) -> ..`")
            .to_compile_error()
            .into();
    }

    let attrs = &item.attrs;
    let vis = &item.vis;
    let ident = &item.sig.ident;
    let generics = &item.sig.generics;
    let lts = generics
        .params
        .iter()
        .filter_map(|gp| {
            if let GenericParam::Lifetime(ld) = gp {
                let lt = &ld.lifetime;
                Some(quote!(#lt))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let where_clause = &generics.where_clause;
    let inputs = &item.sig.inputs;
    let output = match &item.sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let block = &item.block;
    quote!(
        #(#attrs)*
        #vis fn #ident #generics (
            #inputs
        ) -> impl core::ops::Generator<Yield = (), Return = #output> #(+ #lts)*
        #where_clause
        {
            move || #block
        }
    )
    .into()
}
