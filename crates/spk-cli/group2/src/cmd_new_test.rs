// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema::{SpecTemplate, TemplateExt};

#[rstest]
fn test_template_is_valid() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spk-cli-test")
        .tempdir()
        .unwrap();
    let raw_spec = super::get_stub(&"my-package".parse().unwrap());
    let spec_file = tmpdir.path().join("file");
    std::fs::write(&spec_file, raw_spec).unwrap();
    let _spec = SpecTemplate::from_file(&spec_file).unwrap();
}
