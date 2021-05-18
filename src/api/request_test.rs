// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use serde_yaml;

use super::{InclusionPolicy, PkgRequest, PreReleasePolicy};

#[rstest]
fn test_prerelease_policy() {
    let mut a: PkgRequest =
        serde_yaml::from_str("{pkg: something, prereleasePolicy: IncludeAll}").unwrap();
    let b: PkgRequest =
        serde_yaml::from_str("{pkg: something, prereleasePolicy: ExcludeAll}").unwrap();

    a.restrict(&b).unwrap();
    match a.prerelease_policy {
        PreReleasePolicy::ExcludeAll => (),
        _ => panic!("expected restricted prerelease policy"),
    }
}

#[rstest]
fn test_inclusion_policy() {
    let mut a: PkgRequest =
        serde_yaml::from_str("{pkg: something, include: IfAlreadyPresent}").unwrap();
    let b: PkgRequest = serde_yaml::from_str("{pkg: something, include: Always}").unwrap();

    a.restrict(&b).unwrap();
    match a.inclusion_policy {
        InclusionPolicy::Always => (),
        _ => panic!("expected restricted inclusion policy"),
    }
}
