// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::Write;

use rstest::rstest;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map;
use spk_schema_ident::{AnyIdent, BuildIdent, VersionIdent};

use super::Spec;
use crate::foundation::fixtures::*;
use crate::foundation::option_map::OptionMap;
use crate::foundation::FromYaml;
use crate::spec::SpecTemplate;
use crate::{BuildEnv, Opt, Recipe, Template, TemplateExt};

#[rstest]
fn test_spec_is_valid_with_only_name() {
    let _spec: Spec<VersionIdent> = serde_yaml::from_str("{pkg: test-pkg}").unwrap();
}

#[rstest]
fn test_explicit_no_sources() {
    let spec: Spec<VersionIdent> = serde_yaml::from_str("{pkg: test-pkg, sources: []}").unwrap();
    assert!(spec.sources.is_empty());
}

#[rstest]
fn test_sources_relative_to_spec_file(tmpdir: tempfile::TempDir) {
    let spec_dir = tmpdir.path().canonicalize().unwrap().join("dir");
    std::fs::create_dir(&spec_dir).unwrap();
    let spec_file = spec_dir.join("package.spk.yaml");
    let mut file = std::fs::File::create(&spec_file).unwrap();
    file.write_all(b"{pkg: test-pkg}").unwrap();
    drop(file);

    let crate::Spec::V0Package(spec) = SpecTemplate::from_file(&spec_file)
        .unwrap()
        .render(&OptionMap::default())
        .unwrap()
        .generate_source_build(&spec_dir)
        .unwrap();
    if let Some(super::SourceSpec::Local(local)) = spec.sources.get(0) {
        assert_eq!(local.path, spec_dir);
    } else {
        panic!("expected spec to have one local source spec");
    }
}

#[rstest]
#[case(
    r#"pkg: python
install:
  components:
    - name: Component
"#,
    r#"
   | pkg: python
   | install:
   |   components:
 4 |     - name: Component
   |             ^ install.components[0].name: Invalid name: Invalid package name at pos 0:  > C < omponent at line 4 column 13
"#
)]
#[case(
    r#"pkg: python
install:
  components:
    - name: run
      files:
        - {'***'}
"#,
    r#"
   | components:
   |   - name: run
   |     files:
 6 |       - {'***'}
   |         ^ install.components[0].files[0]: invalid type: map, expected a string at line 6 column 11
"#
)]
#[case(
    r#"pkg: python
tests:
  - stage: other"#,
    r#"
   | pkg: python
   | tests:
 3 |   - stage: other
   |            ^ tests[0].stage: unknown variant `other`, expected one of `build`, `install`, `sources` at line 3 column 12
"#
)]
#[case(
    r#"pkg: python
build:
  options:
    - pkg: python/3.4
    - var  arch
    - var: os
"#,
    r#"
   | build:
   |   options:
   |     - pkg: python/3.4
 5 |     - var  arch
   |       ^ build.options[1]: invalid type: string "var  arch", expected a pkg or var option at line 5 column 7
   |     - var: os
"#
)]
fn test_yaml_error_context(#[case] yaml: &str, #[case] expected: &str) {
    // validate that some common and/or deep(ish) errors in the spec format
    // still show errors that are well placed and reasonably worded

    format_serde_error::never_color();
    let err = Spec::<VersionIdent>::from_yaml(yaml).expect_err("expected yaml parsing to fail");
    let message = err.to_string();
    assert_eq!(
        message, expected,
        "error message does not match expected
    ERROR:{message}EXPECTED:{expected}
    "
    );
}

#[rstest]
fn test_build_options_respect_components() {
    let spec: Spec<AnyIdent> = serde_yaml::from_str(
        r#"
        pkg: test-pkg
        build:
          options:
            # This request has a component specified
            - pkg: base:run
    "#,
    )
    .unwrap();

    let mut found = false;
    for opt in spec.build.options {
        match opt {
            Opt::Pkg(pkg)
                if pkg.pkg == "base"
                    && pkg.components.len() == 1
                    && pkg.components.contains(&Component::Run) =>
            {
                found = true;
            }
            _ => continue,
        }
    }

    assert!(found, "build pkg requirement base has run component")
}

#[rstest]
fn test_strong_inheritance_injection() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = Spec<BuildIdent>;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![serde_yaml::from_str(
                r#"
                api: package/v0
                pkg: base/1.0.0/3TCOOP2W
                build:
                  options:
                    - var: inherit-me/1.2.3
                      static: 1.2.3
                      inheritance: Strong
            "#,
            )
            .unwrap()]
        }
    }

    let build_env = TestBuildEnv();

    let spec: Spec<VersionIdent> = serde_yaml::from_str(
        r#"
        api: recipe/v0
        pkg: test-pkg/1.0.0
        build:
          options:
            - pkg: base
    "#,
    )
    .unwrap();

    let built_package = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    // Check that the built_package has inherited a build option on "inherit-me"
    // as well as an install requirement.
    assert!(
        built_package.build.options.iter().any(|opt| match opt {
            Opt::Pkg(_) => false,
            Opt::Var(var) =>
                var.var == "base.inherit-me" && var.get_value(None) == Some("1.2.3".into()),
        }),
        "didn't find inherited build option"
    );
    assert!(
        built_package
            .install
            .requirements
            .iter()
            .any(|request| match request {
                spk_schema_ident::Request::Pkg(_) => false,
                spk_schema_ident::Request::Var(var) =>
                    var.var == "base.inherit-me" && var.value == "1.2.3".into(),
            }),
        "didn't find inherited install requirement"
    );
}
