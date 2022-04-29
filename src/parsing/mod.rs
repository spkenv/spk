// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
mod component;
mod ident;
mod name;
mod request;
mod version;
mod version_range;

pub(crate) use ident::ident;
pub(crate) use request::range_ident;

#[cfg(test)]
#[path = "./parsing_test.rs"]
mod parsing_test;
