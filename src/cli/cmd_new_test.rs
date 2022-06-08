// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use spk::api::Template;

#[rstest]
fn test_template_is_valid() {
    let tmpdir = tempdir::TempDir::new("spk-cli-test").unwrap();
    let raw_spec = super::get_stub(&"my-package".parse().unwrap());
    let spec_file = tmpdir.path().join("file");
    std::fs::write(&spec_file, raw_spec).unwrap();
    let _spec = spk::api::SpecTemplate::from_file(&spec_file).unwrap();
}
