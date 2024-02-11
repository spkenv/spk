// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::rstest;
use spk_cli_common::flags::VariantLocation;
use spk_cli_common::{BuildArtifact, Run};
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::option_map;
use spk_schema::opt_name;
use spk_schema::option_map::HOST_OPTIONS;
use spk_storage::fixtures::*;

use super::Build;
use crate::try_build_package;

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    build: Build,
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_acts_as_variant_filter(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red }
        - { color: green }
        - { color: blue }
        "#,
        // By saying --variant color=green, we are asking for the second variant
        "--variant",
        "color=green",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Index(index), options) if *index == 1 && matches!(options.get(opt_name_color), Some(color) if color == "green")
        ),
        "Expected the second variant to be built, and color=green"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_acts_as_override(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red }
        - { color: green }
        - { color: blue }
        "#,
        // By saying --opt color=green, we are asking to override the color in
        // all the variants (pruning duplicates).
        "--opt",
        "color=green",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Index(index), options) if *index == 0 && matches!(options.get(opt_name_color), Some(color) if color == "green")
        ),
        "Expected the first variant to be built, and color=green"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_acts_as_variant_filter_no_match(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red }
        - { color: green }
        - { color: blue }
        "#,
        // By saying --variant color=purple, we are asking for a variant that
        // doesn't exist.
        "--variant",
        "color=purple",
    );

    result.expect_err("Expected build to fail");
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_on_recipe_with_no_variants_match_default(
    tmpdir: tempfile::TempDir,
) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "no-variants.spk.yaml",
        br#"
pkg: no-variants/1.0.0
api: v0/package
build:
    options:
        - var: color/blue
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    "#,
        // By saying --variant color=blue, we are asking for the "default"
        // variant, because the default color is blue.
        "--variant",
        "color=blue",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Index(index), options) if *index == 0 && matches!(options.get(opt_name_color), Some(color) if color == "blue")
        ),
        "Expected the first variant to be built, and color=blue"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_on_recipe_with_no_variants_no_match(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "no-variants.spk.yaml",
        br#"
pkg: no-variants/1.0.0
api: v0/package
build:
    options:
        - var: color/blue
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    "#,
        // By saying --variant color=green, we are asking for a variant that
        // doesn't exist.
        "--variant",
        "color=green",
    );

    result.expect_err("Expected build to fail");
}

#[rstest]
#[tokio::test]
async fn test_build_with_new_variant_on_recipe_with_no_variants(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "no-variants.spk.yaml",
        br#"
pkg: no-variants/1.0.0
api: v0/package
build:
    options:
        - var: color/blue
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    "#,
        // By saying --new-variant with color=green, we are asking for a bespoke
        // variant
        "--new-variant",
        r#"{ "color": "green" }"#,
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Bespoke(index), options) if *index == 0 && matches!(options.get(opt_name_color), Some(color) if color == "green")
        ),
        "Expected the first extra-variant to be built, and color=green"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_acts_as_variant_filter_two_opts(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red, fruit: banana }
        - { color: green, fruit: apple }
        - { color: blue, fruit: orange }
        "#,
        // By saying --variant color=green,fruit=apple we are asking for the
        // second variant
        "--variant",
        "color=green,fruit=apple",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");
    let opt_name_fruit = opt_name!("fruit");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Index(index), options) if
            *index == 1 &&
            matches!(options.get(opt_name_color), Some(color) if color == "green")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "apple")
        ),
        "Expected the second variant to be built, with color=green and fruit=apple"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_acts_as_variant_filter_two_opts_no_match(
    tmpdir: tempfile::TempDir,
) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red, fruit: banana }
        - { color: green, fruit: apple }
        - { color: blue, fruit: orange }
        "#,
        // The first option matches, but the second doesn't
        "--variant",
        "color=green,fruit=orange",
    );

    result.expect_err("Expected build to fail");
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_and_opts_acts_as_variant_filter_and_override(
    tmpdir: tempfile::TempDir,
) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit/banana
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red }
        - { color: green }
        - { color: blue }
        "#,
        // By saying --variant color=green, we are asking for the second variant
        "--variant",
        "color=green",
        // We are overriding the green variant's fruit to be apple
        "--opt",
        "fruit=apple",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");
    let opt_name_fruit = opt_name!("fruit");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Index(index), options) if
            *index == 1 &&
            matches!(options.get(opt_name_color), Some(color) if color == "green")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "apple")
        ),
        "Expected the second variant to be built, with color=green and fruit=apple"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_acts_as_an_override(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit/banana
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red }
        - { color: green }
        - { color: blue }
        "#,
        // Setting an option that doesn't appear in the variants will
        // not filter out any variants, but will override the default
        "--opt",
        "fruit=apple",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        3,
        "Expected three builds to be created"
    );

    let opt_name_fruit = opt_name!("fruit");

    assert!(
        result.created_builds.artifacts.iter().all(|(_, artifact)| {
            matches!(
                artifact,
                BuildArtifact::Binary(_, _, options) if matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "apple")
            )
        }),
        "Expected all variants to be built with fruit=apple"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_and_variant_index(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red, fruit: banana }
        - { color: green, fruit: apple }
        - { color: blue, fruit: orange }
        "#,
        // By saying --variant 0, we are explicitly asking for the first variant
        "--variant",
        "0",
        // By saying --opt color=green, we are overriding the first variant's
        // color
        "--opt",
        "color=green",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");
    let opt_name_fruit = opt_name!("fruit");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(_, VariantLocation::Index(index), options) if
            *index == 0 &&
            matches!(options.get(opt_name_color), Some(color) if color == "green")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "banana")
        ),
        "Expected the first variant to be built, with color=green and fruit=banana"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_spec(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red, fruit: banana }
        - { color: green, fruit: apple }
        - { color: blue, fruit: orange }
        "#,
        // By supplying a variant spec, we are asking for a bespoke variant
        "--new-variant",
        r#"{ "color": "brown", "fruit": "kiwi" }"#,
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");
    let opt_name_fruit = opt_name!("fruit");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(
                _,
                VariantLocation::Bespoke(index),
                options) if *index == 0 && matches!(options.get(opt_name_color), Some(color) if color == "brown")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "kiwi")
        ),
        "Expected the first extra-variant to be built, with color=brown and fruit=kiwi"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_variant_spec_and_override(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0
api: v0/package
build:
    options:
        - var: color
        - var: fruit
    script:
        - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
    variants:
        - { color: red, fruit: banana }
        - { color: green, fruit: apple }
        - { color: blue, fruit: orange }
        "#,
        // By supplying a variant spec, we are asking for a bespoke variant
        "--new-variant",
        r#"{ "color": "brown", "fruit": "kiwi" }"#,
        // But by also supplying --opt, we are overriding the bespoke variant
        "--opt",
        "color=green",
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    let opt_name_color = opt_name!("color");
    let opt_name_fruit = opt_name!("fruit");

    assert!(
        matches!(
            &result.created_builds.artifacts[0].1,
            BuildArtifact::Binary(
                _,
                VariantLocation::Bespoke(index),
                options) if *index == 0 && matches!(options.get(opt_name_color), Some(color) if color == "green")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "kiwi")
        ),
        "Expected the first extra-variant to be built, with color=green and fruit=kiwi"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_filters_variants_based_on_host_opts(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    // Force "distro" host option to "centos" to make this test pass on any OS.
    HOST_OPTIONS
        .scoped_options(Ok(option_map! { "distro" => "centos" }), async move {

        let (_, result) = try_build_package!(
            tmpdir,
            "three-variants.spk.yaml",
            br#"
    pkg: three-variants/1.0.0
    api: v0/package
    build:
        options:
            - var: color
        script:
            - 'echo "color: $SPK_OPT_color" > "$PREFIX/color.txt"'
        variants:
            - { distro: centos, color: red }
            - { distro: rocky, color: green }
            - { distro: centos, color: blue }
            "#,
        );

        let mut result = result.expect("Expected build to succeed");

        // Only care about binary builds (not source builds)
        result
            .created_builds
            .artifacts
            .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

        assert_eq!(
            result.created_builds.artifacts.len(),
            2,
            "Expected two builds to be created"
        );

        let opt_name_distro = opt_name!("distro");

        assert!(
            result.created_builds.artifacts.iter().all(|(_, artifact)| {
                matches!(
                    artifact,
                    BuildArtifact::Binary(_, _, options) if matches!(options.get(opt_name_distro), Some(distro) if distro == "centos")
                )
            }),
            "Expected all variants to be built with distro=centos"
        );

        Ok::<_, ()>(())
    }).await.unwrap();
}
