// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

/// The address of a repository.
pub trait Address {
    /// Return the address of this repository.
    fn address(&self) -> Cow<'_, url::Url>;
}

impl<T: Address> Address for &T {
    fn address(&self) -> Cow<'_, url::Url> {
        T::address(&**self)
    }
}
