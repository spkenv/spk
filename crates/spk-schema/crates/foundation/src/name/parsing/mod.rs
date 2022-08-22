// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod name;

pub use name::{
    is_legal_package_name_chr, known_repository_name, package_name, repository_name, tag_name,
};
