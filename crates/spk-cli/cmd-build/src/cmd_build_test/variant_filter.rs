// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::rstest;
use spk_cli_common::{BuildArtifact, Run};
use spk_schema::foundation::fixtures::*;
use spk_schema::opt_name;
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
async fn test_build_with_opts_acts_as_variant_filter(tmpdir: tempfile::TempDir) {
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
        // By saying --opt color=green, we are asking for the second variant
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
            BuildArtifact::Binary(_, 1, options) if matches!(options.get(opt_name_color), Some(color) if color == "green")
        ),
        "Expected the second variant to be built, and color=green"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_acts_as_variant_filter_no_match(tmpdir: tempfile::TempDir) {
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
        // By saying --opt color=purple, we are asking for a variant that
        // doesn't exist.
        "--opt",
        "color=purple",
    );

    result.expect_err("Expected build to fail");
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_on_recipe_with_no_variants(tmpdir: tempfile::TempDir) {
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
        // By saying --opt color=green, we are asking for a bespoke variant
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
            BuildArtifact::Binary(_, 0, options) if matches!(options.get(opt_name_color), Some(color) if color == "green")
        ),
        "Expected the first variant to be built, and color=green"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_acts_as_variant_filter_two_opts(tmpdir: tempfile::TempDir) {
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
        // By saying --opt color=green, we are asking for the second variant
        "--opt",
        "color=green",
        // Our choice of fruit has to match the same variant
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
            BuildArtifact::Binary(_, 1, options) if
            matches!(options.get(opt_name_color), Some(color) if color == "green")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "apple")
        ),
        "Expected the second variant to be built, with color=green and fruit=apple"
    );
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_acts_as_variant_filter_two_opts_no_match(tmpdir: tempfile::TempDir) {
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
        "--opt",
        "color=green",
        "--opt",
        "fruit=orange",
    );

    result.expect_err("Expected build to fail");
}

#[rstest]
#[tokio::test]
async fn test_build_with_opts_acts_as_variant_filter_and_override(tmpdir: tempfile::TempDir) {
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
        // By saying --opt color=green, we are asking for the second variant
        "--opt",
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
            BuildArtifact::Binary(_, 1, options) if
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
async fn test_build_with_opts_and_variant_index_overrules_filter(tmpdir: tempfile::TempDir) {
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
            BuildArtifact::Binary(_, 0, options) if
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
        "--variant",
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
                // 3 is the next index after the last variant found in the recipe
                3,
                options) if matches!(options.get(opt_name_color), Some(color) if color == "brown")
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
        "--variant",
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
                // 3 is the next index after the last variant found in the recipe
                3,
                options) if matches!(options.get(opt_name_color), Some(color) if color == "green")
            && matches!(options.get(opt_name_fruit), Some(fruit) if fruit == "kiwi")
        ),
        "Expected the first extra-variant to be built, with color=green and fruit=kiwi"
    );
}
