// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(IsDefault)]
pub fn derive_is_default(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Get the name of the struct
    let name = input.ident;
    let generics = input.generics;
    let generic_idents: Vec<_> = generics.type_params().map(|tp| tp.ident.clone()).collect();

    // Generate the trait implementation based on the struct's data (fields)
    let expanded = match input.data {
        Data::Struct(data_struct) => {
            match data_struct.fields {
                Fields::Named(fields_named) => {
                    // Generate code that calls `IsDefault::is_default` on each field
                    let field_checks = fields_named.named.iter().map(|field| {
                        let field_name = &field.ident;
                        quote! {
                            spk_schema_foundation::IsDefault::is_default(&self.#field_name)
                        }
                    });

                    quote! {
                        impl #generics spk_schema_foundation::IsDefault for #name <#(#generic_idents),*> {
                            fn is_default(&self) -> bool {
                                true #(&& #field_checks)*
                            }
                        }
                    }
                }
                Fields::Unnamed(fields_unnamed) => {
                    let field_checks = fields_unnamed.unnamed.iter().enumerate().map(|(i, _)| {
                        let index = syn::Index::from(i);
                        quote! {
                            spk_schema_foundation::IsDefault::is_default(&self.#index)
                        }
                    });

                    quote! {
                        impl spk_schema_foundation::IsDefault for #name {
                            fn is_default(&self) -> bool {
                                true #(&& #field_checks)*
                            }
                        }
                    }
                }
                Fields::Unit => {
                    quote! {
                        impl spk_schema_foundation::IsDefault for #name {
                            fn is_default(&self) -> bool {
                                true
                            }
                        }
                    }
                }
            }
        }
        _ => panic!("IsDefault can only be derived for structs"),
    };

    // Return the generated code
    TokenStream::from(expanded)
}
