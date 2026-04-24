// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

/// Attribute macro that enables a test to run without an external `spfs run`
/// wrapper. When the test process is not already inside an spfs runtime, the
/// macro re-execs the test binary inside `spfs run - --` with the original
/// command-line arguments forwarded.
///
/// Must be placed before `#[rstest]` and `#[tokio::test]`:
///
/// ```ignore
/// #[spfstest]
/// #[rstest]
/// #[tokio::test]
/// async fn test_something(tmpdir: tempfile::TempDir) {
///     let rt = spfs_runtime().await;
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn spfstest(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut func = parse_macro_input!(item as syn::ItemFn);
    let fn_name = func.sig.ident.to_string();

    let guard = if func.sig.asyncness.is_some() {
        quote! {
            spfstest::maybe_reexec_in_spfs_async(
                &spfstest::current_test_name(module_path!(), #fn_name)
            ).await;
        }
    } else {
        quote! {
            spfstest::maybe_reexec_in_spfs(
                &spfstest::current_test_name(module_path!(), #fn_name)
            );
        }
    };

    let original_stmts = &func.block.stmts;
    func.block = syn::parse2(quote! {
        {
            #guard
            #(#original_stmts)*
        }
    })
    .expect("spfstest: failed to construct modified function body");

    TokenStream::from(quote! { #func })
}
