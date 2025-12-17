// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;

use rstest::rstest;
use spk_schema_foundation::ident::PinnedRequest;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map;
use spk_schema_foundation::version_range::VersionFilter;

use crate::foundation::fixtures::*;
use crate::foundation::option_map::OptionMap;
use crate::option::PkgOpt;
use crate::spec::SpecTemplate;
use crate::v0::{PackageSpec, RecipeSpec};
use crate::{BuildEnv, Opt, Recipe, SourceSpec, Template, TemplateExt, Variant, VariantExt};

#[rstest]
fn test_spec_is_invalid_with_only_name() {
    serde_yaml::from_str::<PackageSpec>("{pkg: test-pkg}")
        .expect_err("package specs require a build id");
}

#[rstest]
fn test_sources_relative_to_spec_file(tmpdir: tempfile::TempDir) {
    let spec_dir = dunce::canonicalize(tmpdir.path()).unwrap().join("dir");
    std::fs::create_dir(&spec_dir).unwrap();
    let spec_file = spec_dir.join("package.spk.yaml");
    let mut file = std::fs::File::create(&spec_file).unwrap();
    file.write_all(b"{pkg: test-pkg}").unwrap();
    drop(file);

    let spec = SpecTemplate::from_file(&spec_file)
        .unwrap()
        .render(&OptionMap::default())
        .unwrap();
    let crate::Spec::V0Package(recipe) = spec
        .into_recipe()
        .unwrap()
        .generate_source_build(&spec_dir)
        .unwrap();
    if let Some(SourceSpec::Local(local)) = recipe.sources.first() {
        assert_eq!(local.path, spec_dir);
    } else {
        panic!("expected spec to have one local source spec");
    }
}

#[rstest]
fn test_strong_inheritance_injection() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![
                serde_yaml::from_str(
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
                .unwrap(),
            ]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::default()
        }
    }

    let build_env = TestBuildEnv();

    let spec: RecipeSpec = serde_yaml::from_str(
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
                spk_schema_foundation::ident::PinnedRequest::Pkg(_) => false,
                spk_schema_foundation::ident::PinnedRequest::Var(var) =>
                    var.var == "base.inherit-me" && var.value == "1.2.3".into(),
            }),
        "didn't find inherited install requirement"
    );
}

#[rstest]
fn test_strong_inheritance_injection_transitivity() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![
                serde_yaml::from_str(
                    r#"
                api: v0/package
                pkg: base/1.0.0/3TCOOP2W
                build:
                  options:
                    - var: inherit-me/1.2.3
                      static: 1.2.3
                      inheritance: Strong
            "#,
                )
                .unwrap(),
            ]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::default()
        }
    }

    let build_env = TestBuildEnv();

    // Unlike `test_strong_inheritance_injection`, this spec does not have a
    // build dependency on "base".
    let spec: RecipeSpec = serde_yaml::from_str(
        r#"
        api: v0/package
        pkg: test-pkg/1.0.0
    "#,
    )
    .unwrap();

    let built_package = spec
        .generate_binary_build(&option_map! {}, &build_env)
        .unwrap();

    // Check that the built_package has inherited a build option on "inherit-me"
    // as well as an install requirement, even though "test-pkg" does not depend
    // on "base".
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
                spk_schema_foundation::ident::PinnedRequest::Pkg(_) => false,
                spk_schema_foundation::ident::PinnedRequest::Var(var) =>
                    var.var == "base.inherit-me" && var.value == "1.2.3".into(),
            }),
        "didn't find inherited install requirement"
    );
}

#[rstest]
fn test_variants_can_introduce_components() {
    let spec: RecipeSpec = serde_yaml::from_str(
        r#"
        pkg: test-pkg
        build:
          variants:
            - { "dep-pkg:{comp1,comp2}": "1.2.3" }
    "#,
    )
    .unwrap();

    let comp1 = Component::Named("comp1".to_owned());
    let comp2 = Component::Named("comp2".to_owned());
    let ver = VersionFilter::from_str("1.2.3").unwrap();

    let mut found = false;
    for variant in spec.build.variants {
        let mut found_opt = false;
        let mut found_pkg = false;

        for (opt_name, value) in variant.options().iter() {
            if opt_name == "dep-pkg" && value == "1.2.3" {
                found_opt = true;
                break;
            }
        }

        for requirement in variant.additional_requirements().iter() {
            if let PinnedRequest::Pkg(pkg) = requirement
                && pkg.pkg.name == "dep-pkg"
                && pkg.pkg.components.contains(&comp1)
                && pkg.pkg.components.contains(&comp2)
                && pkg.pkg.version == ver
            {
                found_pkg = true;
                break;
            }
        }

        if found_opt && found_pkg {
            found = true;
            break;
        }
    }

    assert!(
        found,
        "dep-pkg adds option and package dependency with comp1 and comp2 enabled"
    )
}

#[rstest]
fn test_variants_can_append_components() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![
                serde_yaml::from_str(
                    r#"
                api: v0/package
                pkg: dep-pkg/1.2.3/3TCOOP2W
            "#,
                )
                .unwrap(),
            ]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::default()
        }
    }

    let build_env = TestBuildEnv();

    let spec: RecipeSpec = serde_yaml::from_str(
        r#"
        pkg: test-pkg
        build:
          options:
            - pkg: dep-pkg:comp1/1.2.3
          variants:
            - { "dep-pkg:comp2": "1.2.3" }
    "#,
    )
    .unwrap();

    let variants = spec.default_variants(&OptionMap::default());

    let variant = variants[0].clone().with_overrides(option_map! {});

    let built_package = spec.generate_binary_build(&variant, &build_env).unwrap();

    // Verify that after building the first variant, the built package has
    // requests for both comp1 and comp2 (the requests were merged).

    let comp1 = Component::Named("comp1".to_owned());
    let comp2 = Component::Named("comp2".to_owned());

    let mut found = false;
    for option in built_package.build.options.iter() {
        match option {
            Opt::Pkg(PkgOpt {
                pkg,
                components,
                default,
                ..
            }) if pkg == "dep-pkg"
                && components.contains(&comp1)
                && components.contains(&comp2)
                && default == "1.2.3" =>
            {
                found = true;
                break;
            }
            _ => (),
        };
    }

    assert!(
        found,
        "dep-pkg adds package dependency with comp1 and comp2 enabled"
    )
}

#[rstest]
fn test_variants_can_append_components_and_modify_version() {
    struct TestBuildEnv();

    impl BuildEnv for TestBuildEnv {
        type Package = PackageSpec;

        fn build_env(&self) -> Vec<Self::Package> {
            vec![
                serde_yaml::from_str(
                    r#"
                api: v0/package
                pkg: dep-pkg/1.2.3/3TCOOP2W
            "#,
                )
                .unwrap(),
                serde_yaml::from_str(
                    r#"
                api: v0/package
                pkg: dep-pkg/1.2.4/3TCOOP2W
            "#,
                )
                .unwrap(),
            ]
        }

        fn env_vars(&self) -> HashMap<String, String> {
            HashMap::default()
        }
    }

    let build_env = TestBuildEnv();

    let spec: RecipeSpec = serde_yaml::from_str(
        r#"
        pkg: test-pkg
        build:
          options:
            # base option asks for 1.2.3
            - pkg: dep-pkg:comp1/1.2.3
          variants:
            # variant asks for 1.2.4
            - { "dep-pkg:comp2": "1.2.4" }
    "#,
    )
    .unwrap();

    let variants = spec.default_variants(&OptionMap::default());

    let variant = variants[0].clone().with_overrides(option_map! {});

    let built_package = spec.generate_binary_build(&variant, &build_env).unwrap();

    // Verify that after building the first variant, the built package has
    // requests for both comp1 and comp2 (the requests were merged).

    let comp1 = Component::Named("comp1".to_owned());
    let comp2 = Component::Named("comp2".to_owned());

    let mut found = false;
    for option in built_package.build.options.iter() {
        match option {
            Opt::Pkg(PkgOpt {
                pkg,
                components,
                default,
                ..
            }) if pkg == "dep-pkg"
                && components.contains(&comp1)
                && components.contains(&comp2)
                && default == "1.2.4" =>
            {
                found = true;
                break;
            }
            x => dbg!(x),
        };
    }

    assert!(
        found,
        "dep-pkg adds package dependency with comp1 and comp2 enabled and expected version"
    )
}
