// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;

use ring::digest;
use rstest::rstest;

use crate::Digest;

#[rstest]
fn test_empty_digest_bytes() {
    use crate::{DIGEST_SIZE, EMPTY_DIGEST};
    let empty_digest: [u8; DIGEST_SIZE] = digest::digest(&digest::SHA256, b"")
        .as_ref()
        .try_into()
        .unwrap();
    assert_eq!(empty_digest, EMPTY_DIGEST);
}

/// Verify that the debug representation of a `Digest` shows the bytes in hex
/// format, meaning that the change was injected into the generated flatbuffers
/// code as intended.
///
/// The default generated debug representation of a `Digest` would show the
/// bytes as a byte array, with one byte per line.
#[rstest]
fn digest_debug_shows_base32_string() {
    let digest = Digest::default();
    assert_eq!(
        format!("{digest:?}"),
        "Digest(\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA====\")"
    );
}
