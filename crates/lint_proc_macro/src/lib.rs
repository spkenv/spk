// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(Lint)]
pub fn derive_macro_lints(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_proc_macro_derive(&ast)
}

fn impl_proc_macro_derive(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let (impl_generics, type_generics, _) = ast.generics.split_for_impl();
    let mut field_names = Vec::new();

    if let syn::Data::Struct(s) = &ast.data {
        for field in &s.fields {
            let Some(ident) = &field.ident else {
                continue;
            };
            field_names.push(ident.to_string());
        }
    } else if let syn::Data::Enum(e) = &ast.data {
        for variant in &e.variants {
            field_names.push(variant.ident.to_string());
        }
    }

    let lint = quote! {
        impl #impl_generics #name #type_generics {
            pub fn generate_lints(&self, unknown_key: &str) -> String {
                let name = String::from(stringify!(#name));
                let mut message = format!("Unrecognized {name} key: {unknown_key}. ");

                let mut corpus = CorpusBuilder::new().finish();
                let fields = vec![#(#field_names),*];
                for field in fields.iter() {
                    if field == &"lints" {
                        continue;
                    }

                    corpus.add_text(field);
                }

                match corpus.search(unknown_key, 0.6).first() {
                    Some(s) => message.push_str(&format!("(Did you mean: '{}'?)", s.text)),
                    None => message.push_str(&format!("(No similar keys found for: {}.)", unknown_key)),
                };
                message
            }
        }
    };
    lint.into()
}
