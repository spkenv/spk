// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;

use rstest::rstest;
use spk_schema_foundation::ident::{InclusionPolicy, PinnedRequest};
use spk_schema_foundation::option_map;
use spk_schema_foundation::option_map::HOST_OPTIONS;

use super::Platform;
use crate::Opt::Var;
use crate::v0::PackageSpec;
use crate::{BuildEnv, Recipe};

#[rstest]
fn test_platform_is_valid_with_only_api_and_name() {
    let _spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         api: v0/platform
         "#,
    )
    .unwrap();
}

#[rstest]
#[case::add_form(
    r#"
         platform: test-platform
         api: v0/platform
         requirements:
           - pkg: test-requirement
         "#
)]
#[case::patch_form(
    r#"
         platform: test-platform
         api: v0/platform
         requirements:
           add:
             - pkg: test-requirement
         "#
)]
fn test_platform_add_pkg_requirement(#[case] spec: &str) {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            Vec::new()
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    let build_env = TestBuildEnv();

    let spec: Platform = serde_yaml::from_str(spec).unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    let host_options = HOST_OPTIONS.get().unwrap();
    let build_options = build
        .build()
        .options
        .iter()
        .filter_map(|o| match o {
            Var(var_opt) => Some(var_opt.var.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (name, _value) in host_options.iter() {
        assert!(build_options.contains(name));
    }

    assert_eq!(build.install().requirements.len(), 1);
    assert!(
        matches!(&build.install().requirements[0], PinnedRequest::Pkg(pkg) if pkg.pkg.name() == "test-requirement")
    );
    assert!(
        matches!(&build.install().requirements[0], PinnedRequest::Pkg(pkg) if pkg.inclusion_policy == InclusionPolicy::IfAlreadyPresent)
    );
}

#[rstest]
fn test_platform_inheritance() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![
                serde_yaml::from_str(
                    r#"
                api: package/v0
                pkg: base/1.0.0/3TCOOP2W
                install:
                  requirements:
                    - pkg: inherit-me
            "#,
                )
                .unwrap(),
            ]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    let build_env = TestBuildEnv();

    let spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         base: base
         api: v0/platform
         requirements:
           - pkg: test-requirement
         "#,
    )
    .unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    assert_eq!(build.install().requirements.len(), 2);
    assert!(
        matches!(&build.install().requirements[0], PinnedRequest::Pkg(pkg) if pkg.pkg.name() == "inherit-me")
    );
    assert!(
        matches!(&build.install().requirements[1], PinnedRequest::Pkg(pkg) if pkg.pkg.name() == "test-requirement")
    );
}

#[rstest]
fn test_platform_inheritance_with_override_and_removal() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![
                serde_yaml::from_str(
                    r#"
                api: package/v0
                pkg: base/1.0.0/3TCOOP2W
                install:
                  requirements:
                    - pkg: inherit-me1/1.0.0
                    - pkg: inherit-me2/1.0.0
                    - pkg: inherit-me3/1.0.0
            "#,
                )
                .unwrap(),
            ]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    let build_env = TestBuildEnv();

    let spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         base: base
         api: v0/platform
         requirements:
           add:
             - pkg: inherit-me1/2.0.0
           remove:
             - pkg: inherit-me2
         "#,
    )
    .unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    assert_eq!(build.install().requirements.len(), 2);
    assert!(
        matches!(&build.install().requirements[0], PinnedRequest::Pkg(pkg) if pkg.pkg.name() == "inherit-me1" && pkg.pkg.version.to_string() == "2.0.0")
    );
    assert!(
        matches!(&build.install().requirements[1], PinnedRequest::Pkg(pkg) if pkg.pkg.name() == "inherit-me3")
    );
}
