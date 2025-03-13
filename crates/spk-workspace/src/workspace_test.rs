// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::vec;

use rstest::{fixture, rstest};
use spk_schema::Template;

use super::Workspace;

#[fixture]
pub fn tmpdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("spk-test-")
        .tempdir()
        .expect("create a temp directory for test files")
}

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter("spk_workspace=trace,debug")
        .without_time()
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

#[rstest]
fn test_config_specialization(tmpdir: tempfile::TempDir) {
    // test that when a workspace is created, the templates
    // are loaded in order and can add specific configs to
    // templates that were previously loaded as a glob

    init_logging();

    for name in &["pkg-a", "pkg-b", "pkg-c"] {
        let template_path = tmpdir.path().join(format!("{name}.spk.yaml"));
        std::fs::write(template_path, format!("pkg: {name}/1.0.0")).unwrap();
    }
    let v1: spk_schema::version::Version = "1.0.0".parse().unwrap();
    let workspace = Workspace::builder()
        .with_root(tmpdir.path())
        .load_from_file(crate::file::WorkspaceFile {
            recipes: vec![
                crate::file::RecipesItem {
                    path: "*.spk.yaml".parse().unwrap(),
                    config: Default::default(),
                },
                crate::file::RecipesItem {
                    path: "pkg-a.spk.yaml".parse().unwrap(),
                    config: crate::file::TemplateConfig {
                        versions: vec![v1.clone()].into_iter().collect(),
                    },
                },
            ],
        })
        .unwrap()
        .build()
        .unwrap();

    let found = workspace.find_package_template("pkg-a").unwrap();
    assert!(
        found.config.versions.contains(&v1),
        "config specialization should apply based on workspace file order, got: {:?}",
        found.config
    )
}

#[rstest]
#[case::default_request_with_one_spec(
    &[("my-package.spk.yaml", "my-package/1.0.0")],
    "",
    "my-package.spk.yaml"
)]
#[should_panic]
#[case::default_request_fails_with_multiple_specs(
    &[
        ("my-package.spk.yaml", "my-package/1.0.0"),
        ("my-other-package.spk.yaml", "my-other-package/1.0.0"),
    ],
    "",
    ""
)]
#[case::request_by_name(
    &[
        ("my-package.spk.yaml", "my-package/1.0.0"),
        ("my-other-package.spk.yaml", "my-other-package/1.0.0"),
    ],
    "my-package",
    "my-package.spk.yaml"
)]
#[should_panic]
#[case::request_by_name_fails_with_multiple_specs(
    &[
        ("my-package1.spk.yaml", "my-package/1.0.0"),
        ("my-package2.spk.yaml", "my-package/2.0.0"),
    ],
    "my-package",
    ""
)]
fn test_workspace_find_template(
    tmpdir: tempfile::TempDir,
    #[case] templates: &[(&str, &str)],
    #[case] request: &str,
    #[case] expected: &str,
) {
    // ensure that the workspace can find templates using all
    // of the expected syntaxes

    init_logging();

    for (file_name, pkg) in templates {
        let template_path = tmpdir.path().join(file_name);
        std::fs::write(&template_path, format!("pkg: {pkg}")).unwrap();
    }

    let workspace = Workspace::builder()
        .with_root(tmpdir.path())
        .with_glob_pattern("*.spk.yaml")
        .unwrap()
        .build()
        .unwrap();

    let result = if request.is_empty() {
        workspace.default_package_template()
    } else {
        workspace.find_package_template(request)
    };

    let result = result.expect("should be found");
    assert_eq!(
        result
            .template
            .file_path()
            .file_name()
            .expect("template has file name")
            .to_str()
            .expect("file name is valid string"),
        expected
    );
}

#[rstest]
fn test_workspace_find_by_version(tmpdir: tempfile::TempDir) {
    // test that when a workspace is created with multiple
    // templates of the same package, that one can still be
    // appropriately selected if they are for different versions
    // and the version is given as a request

    init_logging();

    for version in 1..3 {
        let template_path = tmpdir.path().join(format!("{version}.spk.yaml"));
        std::fs::write(template_path, format!("pkg: my-package/{version}.0.0")).unwrap();
    }
    let v1: spk_schema::version::Version = "1.0.0".parse().unwrap();
    let workspace = Workspace::builder()
        .with_root(tmpdir.path())
        .load_from_file(crate::file::WorkspaceFile {
            recipes: vec![
                crate::file::RecipesItem {
                    path: "*.spk.yaml".parse().unwrap(),
                    config: Default::default(),
                },
                crate::file::RecipesItem {
                    path: "1.spk.yaml".parse().unwrap(),
                    config: crate::file::TemplateConfig {
                        versions: vec![v1.clone()].into_iter().collect(),
                    },
                },
                crate::file::RecipesItem {
                    path: "2.spk.yaml".parse().unwrap(),
                    config: crate::file::TemplateConfig {
                        versions: vec!["2.0.0".parse().unwrap()].into_iter().collect(),
                    },
                },
            ],
        })
        .unwrap()
        .build()
        .unwrap();

    let res = workspace.find_package_template("my-package");
    if !matches!(
        res,
        Err(super::FindPackageTemplateError::MultipleTemplates(_))
    ) {
        panic!(
            "should fail to find template when there was no version given and multiple exist in the workspace, got {res:#?}"
        );
    };

    let found = workspace
        .find_package_template("my-package/1")
        .expect("should find template when multiple exist but an unambiguous version is given");
    assert!(
        found.config.versions.contains(&v1),
        "should select the requested version, got: {:?}",
        found.config
    )
}
