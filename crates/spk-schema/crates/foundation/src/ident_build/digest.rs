// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

// given option digests are namespaced by the package itself,
// there is a slim likelihood of collision, so we roll the dice
// also must be a multiple of 8 to be decodable which is generally
// a nice way to handle validation / and 16 is a lot
pub const DIGEST_SIZE: usize = 8;

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Digest([char; DIGEST_SIZE]);

impl Digest {
    pub const fn new(chars: [char; DIGEST_SIZE]) -> Self {
        Self(chars)
    }

    pub fn new_from_bytes(bytes: &[u8]) -> Self {
        let encoded = data_encoding::BASE32.encode(bytes);
        Self(
            encoded
                .chars()
                .chain(std::iter::repeat('0'))
                .take(DIGEST_SIZE)
                .collect::<Vec<_>>()
                .try_into()
                .expect("always enough bytes available"),
        )
    }
}

impl Default for Digest {
    fn default() -> Self {
        // For legacy reasons, the default digest is what one would get
        // previously from OptionMap::digest() on a default OptionMap.
        let hasher = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
        // An empty OptionMap would not update the hasher with anything.
        let digest = hasher.finish();
        Self::new_from_bytes(digest.as_ref())
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in &self.0 {
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

impl From<[char; DIGEST_SIZE]> for Digest {
    fn from(chars: [char; DIGEST_SIZE]) -> Self {
        Self::new(chars)
    }
}

impl TryFrom<Vec<char>> for Digest {
    type Error = ();

    fn try_from(value: Vec<char>) -> Result<Self, Self::Error> {
        if value.len() != DIGEST_SIZE {
            Err(())
        } else {
            let mut chars = [0 as char; DIGEST_SIZE];
            chars.copy_from_slice(&value);
            Ok(Self::new(chars))
        }
    }
}
