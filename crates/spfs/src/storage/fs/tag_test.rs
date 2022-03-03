// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::os::unix::fs::MetadataExt;

use rstest::rstest;

use super::FSRepository;
use crate::{encoding, storage::TagStorage, tracking};

use crate::fixtures::*;

#[rstest]
fn test_tag_permissions(tmpdir: tempdir::TempDir) {
    let mut storage = FSRepository::create(tmpdir.path().join("repo")).unwrap();
    let spec = tracking::TagSpec::parse("hello").unwrap();
    storage
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .unwrap();
    assert_eq!(
        tmpdir
            .path()
            .join("repo/tags/hello.tag")
            .metadata()
            .unwrap()
            .mode()
            & 0o777,
        0o777
    );
}
