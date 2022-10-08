// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema_foundation::version::Compatibility;

/// An item that can satisfy requests of a specific type
pub trait Satisfy<Request> {
    /// Check is the provided request is satisfied by this item
    fn check_satisfies_request(&self, request: &Request) -> Compatibility;
}

impl<R, T> Satisfy<R> for &T
where
    T: Satisfy<R>,
{
    fn check_satisfies_request(&self, request: &R) -> Compatibility {
        (**self).check_satisfies_request(request)
    }
}

impl<R, T> Satisfy<R> for std::sync::Arc<T>
where
    T: Satisfy<R>,
{
    fn check_satisfies_request(&self, request: &R) -> Compatibility {
        (**self).check_satisfies_request(request)
    }
}
