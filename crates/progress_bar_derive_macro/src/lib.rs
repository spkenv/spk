// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(ProgressBar)]
pub fn proc_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_proc_macro_derive(&ast)
}

fn impl_proc_macro_derive(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;

    let mut progress_bar_field_names = Vec::new();

    if let syn::Data::Struct(s) = &ast.data {
        for field in &s.fields {
            let Some(ident) = &field.ident else { continue; };
            if let syn::Type::Path(p) = &field.ty {
                if let Some(field_type) = p.path.segments.last().map(|s| &s.ident) {
                    if field_type != "ProgressBar" {
                        continue;
                    }

                    progress_bar_field_names.push(quote! { #ident });
                }
            }
        }
    };

    let gen = quote! {
        impl Drop for #name {
            fn drop(&mut self) {
                #(self.#progress_bar_field_names.finish_and_clear();)*
                if let Some(r) = self.renderer.take() {
                    let _ = r.join();
                }
            }
        }
    };
    gen.into()
}
