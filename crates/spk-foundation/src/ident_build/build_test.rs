// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{parse_build, SRC};
use spk_option_map::OptionMap;

#[rstest]
fn test_parse_build_src() {
    // should allow non-digest if it's a special token
    assert!(parse_build(SRC).is_ok());
}

#[rstest]
fn test_parse_build() {
    assert!(parse_build(OptionMap::default().digest_str()).is_ok());
    assert!(parse_build("not eight characters").is_err());
    assert!(parse_build("invalid.").is_err())
}
