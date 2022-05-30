// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Spec;

#[rstest]
fn test_sources_relative_to_spec_file(tmpdir: tempdir::TempDir) {
    let spec_dir = tmpdir.path().canonicalize().unwrap().join("dir");
    std::fs::create_dir(&spec_dir).unwrap();
    let spec_file = spec_dir.join("package.spk.yaml");
    let mut file = std::fs::File::create(&spec_file).unwrap();
    file.write_all(b"{pkg: test-pkg}").unwrap();
    drop(file);

    let spec = super::read_spec_file(&spec_file).unwrap();
    if let Some(super::SourceSpec::Local(local)) = spec.sources.get(0) {
        assert_eq!(local.path, spec_dir);
    } else {
        panic!("expected spec to have one local source spec");
    }
}
