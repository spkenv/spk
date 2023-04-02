// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs_encoding::{Digest, Encodable, Hasher};

/// A digest calculation strategy that uses `Encodable::encode`.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct DigestFromEncode {}

/// Types that can calculate a digest by implementing `Encodable`.
pub trait EncodeDigest {
    /// The flavor of error returned by digest.
    type Error;

    /// Compute the digest for the object.
    fn digest<T>(object: &T) -> std::result::Result<Digest, Self::Error>
    where
        T: Encodable<Error = Self::Error>;
}

impl EncodeDigest for DigestFromEncode {
    type Error = crate::Error;

    fn digest<T>(object: &T) -> std::result::Result<Digest, Self::Error>
    where
        T: Encodable<Error = Self::Error>,
    {
        let mut hasher = Hasher::new_sync();
        object.encode(&mut hasher)?;
        Ok(hasher.digest())
    }
}
