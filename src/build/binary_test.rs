// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use spfs::encoding::EMPTY_DIGEST;

#[rstest]
fn test_split_manifest_permissions() {
    use spfs::tracking::{Entry, EntryKind, Manifest};
    let mut manifest = Manifest::default();
    let dir = manifest.mkdir("bin").unwrap();
    dir.mode = 0o754;
    manifest
        .mknod(
            "bin/runme",
            Entry {
                kind: EntryKind::Blob,
                object: EMPTY_DIGEST.into(),
                mode: 0o555,
                size: 0,
                entries: Default::default(),
            },
        )
        .unwrap();
    let pkg = "mypkg".parse().unwrap();
    let spec = crate::api::ComponentSpecList::default();
    let components = super::split_manifest_by_component(&pkg, &manifest, &spec).unwrap();
    let run = components.get(&crate::api::Component::Run).unwrap();
    assert_eq!(run.get_path("bin").unwrap().mode, 0o754);
    assert_eq!(run.get_path("bin/runme").unwrap().mode, 0o555);
}

#[rstest]
fn test_empty_var_option_is_not_a_request() {
    let spec: crate::api::Spec = serde_yaml::from_str(
        r#"{
        pkg: mypackage/1.0.0,
        build: {
            options: [
                {var: something}
            ]
        }
    }"#,
    )
    .unwrap();
    let builder = super::BinaryPackageBuilder::from_spec(spec);
    let requirements = builder.get_build_requirements().unwrap();
    assert!(
        requirements.is_empty(),
        "a var option with empty value should not create a solver request"
    )
}
