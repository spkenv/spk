// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::LitStr;

/// Derive macro for generating boilerplate [`Default`] and [`Drop`] impls
/// for a struct with [`indicatif::ProgressBar`] fields.
///
/// The struct is required to have one or more fields of type [`indicatif::ProgressBar`].
/// Each progress bar field requires a `#[progress_bar]` attribute with a `message`
/// argument. A `template` argument is also required either at the struct level or
/// the field level.
///
/// # Example
///
/// ```
/// use progress_bar_derive_macro::ProgressBar;
/// #[derive(ProgressBar)]
/// struct MyStruct {
///     #[progress_bar(
///         message = "processing widgets",
///         template = "      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}"
///     )]
///     widgets: indicatif::ProgressBar,
/// }
/// ```
#[proc_macro_derive(ProgressBar, attributes(progress_bar))]
pub fn proc_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_proc_macro_derive(&ast)
}

fn impl_proc_macro_derive(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;

    let mut progress_bar_field_names = Vec::new();
    let mut bars = Vec::new();

    if let syn::Data::Struct(s) = &ast.data {
        let mut template = None;

        for attr in &ast.attrs {
            if !attr.path().is_ident("progress_bar") {
                continue;
            }

            if let Err(err) = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("template") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse()?;
                    template = Some(s.value());
                    return Ok(());
                }
                Ok(())
            }) {
                return err.to_compile_error().into();
            }
        }

        for field in &s.fields {
            let Some(ident) = &field.ident else {
                continue;
            };
            if let syn::Type::Path(p) = &field.ty {
                if let Some(field_type) = p.path.segments.last().map(|s| &s.ident) {
                    if field_type != "ProgressBar" {
                        continue;
                    }

                    let mut message = None;

                    for attr in &field.attrs {
                        if !attr.path().is_ident("progress_bar") {
                            continue;
                        }

                        if let Err(err) = attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("message") {
                                let value = meta.value()?;
                                let s: LitStr = value.parse()?;
                                message = Some(s.value());
                                return Ok(());
                            }
                            if meta.path.is_ident("template") {
                                let value = meta.value()?;
                                let s: LitStr = value.parse()?;
                                template = Some(s.value());
                                return Ok(());
                            }
                            Ok(())
                        }) {
                            return err.to_compile_error().into();
                        }
                    }

                    let Some(message) = message else {
                        return syn::Error::new_spanned(
                            field,
                            "Missing #[progress_bar(message = \"...\")] attribute",
                        )
                        .to_compile_error()
                        .into();
                    };

                    let Some(template) = &template else {
                        return syn::Error::new_spanned(
                            field,
                            "Missing #[progress_bar(template = \"...\")] attribute",
                        )
                        .to_compile_error()
                        .into();
                    };

                    let ident_style = format_ident!("{ident}_style");

                    bars.push(quote! {
                        let #ident_style = indicatif::ProgressStyle::default_bar()
                            .template(#template)
                            .expect("Invalid progress bar template")
                            .tick_strings(TICK_STRINGS)
                            .progress_chars(PROGRESS_CHARS);
                        let #ident = bars.add(
                            indicatif::ProgressBar::new(0)
                                .with_style(#ident_style)
                                .with_message(#message),
                        );
                    });

                    progress_bar_field_names.push(quote! { #ident });
                }
            }
        }
    };

    let gen = quote! {
        impl Default for #name {
            fn default() -> Self {
                static TICK_STRINGS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                static PROGRESS_CHARS: &str = "=>-";

                let bars = indicatif::MultiProgress::new();
                #(#bars)*
                #(#progress_bar_field_names.enable_steady_tick(std::time::Duration::from_millis(100));)*
                Self {
                    #(#progress_bar_field_names,)*
                }
            }
        }
    };
    gen.into()
}
