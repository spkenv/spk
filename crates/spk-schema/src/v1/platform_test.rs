// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;

use rstest::rstest;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map;
use spk_schema_foundation::option_map::HOST_OPTIONS;

use super::Platform;
use crate::Opt::Var;
use crate::v0::PackageSpec;
use crate::{BuildEnv, Recipe};

/// Simplifies the validation of component requirements in our tests
macro_rules! assert_requirements {
    ($spec:ident:$cmpt:ident len $count:literal) => {
        let count = $spec
            .install
            .components
            .get(Component::$cmpt)
            .expect("run component")
            .requirements
            .len();
        assert_eq!(
            count,
            $count,
            "{} component has the wrong number of requirements",
            Component::$cmpt
        );
    };
    ($spec:ident:$cmpt:ident contains $request:literal) => {{
        let cmpt = $spec
            .install
            .components
            .get(Component::$cmpt)
            .expect("expected a run component");
        let name_and_val: crate::ident::NameAndValue = $request
            .parse()
            .expect("could not fetch name from provided request");
        let Some(request) = cmpt.requirements.get(&name_and_val.0) else {
            panic!(
                "expected a {} request for {}",
                Component::$cmpt,
                name_and_val.0
            );
        };
        assert_eq!(
            request.to_string(),
            $request,
            "{} request did not have the expected value",
            Component::$cmpt
        );
    }};
    ($spec:ident:$cmpt:ident excludes $request:literal) => {{
        let cmpt = $spec
            .install
            .components
            .get(Component::$cmpt)
            .expect("expected a run component");
        let name_and_val: crate::ident::NameAndValue = $request
            .parse()
            .expect("could not fetch name from provided request");
        if let Some(found) = cmpt.requirements.get(&name_and_val.0) {
            panic!(
                "expected no {} request for {}, found {found}",
                Component::$cmpt,
                name_and_val.0
            );
        };
    }};
}

#[rstest]
fn test_platform_is_valid_with_only_api_and_name() {
    let _spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         api: v1/platform
         "#,
    )
    .unwrap();
}

#[rstest]
fn test_platform_no_runtime_no_build() {
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

    let spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         requirements:
           - pkg: test-requirement
         "#,
    )
    .unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    let host_options = HOST_OPTIONS.get().unwrap();
    let build_options = build
        .build
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

    // when not specified, a requirement is only for workspace builds
    assert_requirements!(build:Build len 0);
    assert_requirements!(build:Run len 0);
}

#[rstest]
fn test_platform_single_inheritance() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            let base: Platform = serde_yaml::from_str(
                r#"
                        platform: base/1.0.0
                        requirements:
                        - pkg: inherit-me
                          atRuntime: 1.0.0
                    "#,
            )
            .unwrap();
            let build = base.generate_binary_build(&option_map! {}, &()).unwrap();
            vec![build]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    let build_env = TestBuildEnv();

    let spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         base: [base]
         requirements:
           - pkg: test-requirement
             atRuntime: 1.0.0
         "#,
    )
    .unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    println!("{:#?}", build.install);

    assert_requirements!(build:Run contains "inherit-me/1.0.0");
    assert_requirements!(build:Run contains "test-requirement/1.0.0");
    assert_requirements!(build:Run len 2);
}

#[rstest]
fn test_platform_stack_inheritance() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            let base1: Platform = serde_yaml::from_str(
                r#"
                platform: base1/1.0.0
                requirements:
                  - pkg: base
                    atRuntime: 1.0.0
                  - pkg: base1
                    atRuntime: 1.0.0
            "#,
            )
            .unwrap();
            let base1 = base1.generate_binary_build(&option_map! {}, &()).unwrap();
            let base2: Platform = serde_yaml::from_str(
                r#"
                platform: base2/1.0.0
                requirements:
                  - pkg: base2
                    atRuntime: 2.0.0
                  - pkg: base
                    atRuntime: 2.0.0
            "#,
            )
            .unwrap();
            let base2 = base2.generate_binary_build(&option_map! {}, &()).unwrap();
            vec![base1, base2]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    let build_env = TestBuildEnv();

    let spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         base: [base1, base2]
         requirements:
           - pkg: test-requirement
             atRuntime: 1.0.0
         "#,
    )
    .unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    assert_requirements!(build:Run contains "base/2.0.0");
    assert_requirements!(build:Run contains "base1/1.0.0");
    assert_requirements!(build:Run contains "base2/2.0.0");
    assert_requirements!(build:Run contains "test-requirement/1.0.0");
    assert_requirements!(build:Run len 4);
}

#[rstest]
fn test_platform_inheritance_with_override_and_removal() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            let base: Platform = serde_yaml::from_str(
                r#"
                platform: base/1.0.0
                requirements:
                - pkg: inherit-me1
                  atBuild: =1.0.0
                  atRuntime: 1.0.0
                - pkg: inherit-me2
                  atBuild: =1.0.0
                  atRuntime: 1.0.0
                - pkg: inherit-me3
                  atBuild: =1.0.0
                  atRuntime: 1.0.0
            "#,
            )
            .unwrap();
            let base = base.generate_binary_build(&option_map! {}, &()).unwrap();
            vec![base]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::new()
        }
    }

    let build_env = TestBuildEnv();

    let spec: Platform = serde_yaml::from_str(
        r#"
         platform: test-platform
         base: [base]
         requirements:
           - pkg: inherit-me1
             atRuntime: 2.0.0
           - pkg: inherit-me2
             atRuntime: false
         "#,
    )
    .unwrap();

    let build = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    assert_requirements!(build:Run excludes "inherit-me2");
    assert_requirements!(build:Run contains "inherit-me1/2.0.0");
    assert_requirements!(build:Run contains "inherit-me3/1.0.0");
    assert_requirements!(build:Run len 2);
    assert_requirements!(build:Build len 3);
}
