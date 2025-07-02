// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#![allow(unused_imports)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::all)]

use std::borrow::Cow;

include!(concat!(env!("OUT_DIR"), "/spfs_generated.rs"));

pub mod digest;

pub use digest::{DIGEST_SIZE, EMPTY_DIGEST, NULL_DIGEST, parse_digest};

impl From<Digest> for Cow<'static, Digest> {
    fn from(value: Digest) -> Self {
        Cow::Owned(value)
    }
}

impl<'a> From<&'a Digest> for Cow<'a, Digest> {
    fn from(value: &'a Digest) -> Self {
        Cow::Borrowed(value)
    }
}
