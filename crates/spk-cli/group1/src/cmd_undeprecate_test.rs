// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_ident::parse_ident;
use spk_solve::make_repo;
use spk_spec::Deprecate;

use super::{change_deprecation_state, ChangeAction};

#[rstest]
#[tokio::test]
async fn test_undeprecate_without_prompt() {
    // Set up a repo with three package versions, with one build each,
    // two of which are deprecated
    let name1 = "my-pkg/1.0.0";
    let name2 = "my-pkg/1.0.1";
    let name3 = "my-pkg/1.0.2";

    let repo = make_repo!([
        {"pkg": name1, "deprecated": true},
        {"pkg": name2, "deprecated": false},
        {"pkg": name3, "deprecated": true}
    ]);

    let repos = vec![("test".to_string(), repo)];

    // Test undeprecating all the package versions and their builds
    // with the '--yes' flag to prevent it prompting.
    let packages = vec![name1.to_string(), name2.to_string(), name3.to_string()];
    let yes = true;
    let result = change_deprecation_state(ChangeAction::Undeprecate, &repos, &packages, yes).await;

    match result {
        Ok(r) => assert_eq!(r, 0),
        Err(e) => {
            // This should not happen
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }

    // None of the packages should be deprecated anymore, although one
    // was already not deprecated (undeprecated) before the test.
    for name in &[name1, name2, name3] {
        let ident = parse_ident(name).unwrap();
        let (_, r) = &repos[0];
        let recipe = r.read_recipe(&ident).await.unwrap();
        println!("checking: {}", ident);
        assert!(!recipe.is_deprecated());

        for b in r.list_package_builds(&ident).await.unwrap() {
            let spec = r.read_package(&b).await.unwrap();
            println!("checking: {}", b);
            assert!(!spec.is_deprecated());
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_undeprecate_no_repos() {
    let name = "my-pkg/1.0.0";
    let repos = Vec::new();

    // Test undeprecating the package when there's no repos specified
    // at all. No packages should be found, this should a result of 1.
    let packages = vec![name.to_string()];
    let yes = true;
    let result = change_deprecation_state(ChangeAction::Undeprecate, &repos, &packages, yes).await;

    match result {
        Ok(r) => assert_eq!(r, 1),
        Err(e) => {
            // This should not happen
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_undeprecate_no_version() {
    // Set up a repo with one package that is already deprecated
    let name = "my-pkg";
    let repo = make_repo!([
        {"pkg": format!("{}/1.0.0", name), "deprecated": true}
    ]);
    let repos = vec![("test".to_string(), repo)];

    // Test undeprecating the package without specifying a version.
    // This should return a result of 2.
    let packages = vec![name.to_string()];
    let yes = true;
    let result = change_deprecation_state(ChangeAction::Undeprecate, &repos, &packages, yes).await;

    match result {
        Ok(r) => assert_eq!(r, 2),
        Err(e) => {
            // This should not happen
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_undeprecate_no_version_but_trailing_slash() {
    // Set up a repo with one package that is already deprecated
    let name = "my-pkg";
    let repo = make_repo!([
        {"pkg": format!("{}/1.0.0", name), "deprecated": true}
    ]);
    let repos = vec![("test".to_string(), repo)];

    // Test undeprecating the package without specifying a version but
    // putting in a trailing slash. This should return a result of 3.
    let packages = vec![format!("{}/", name)];
    let yes = true;
    let result = change_deprecation_state(ChangeAction::Undeprecate, &repos, &packages, yes).await;

    match result {
        Ok(r) => assert_eq!(r, 3),
        Err(e) => {
            // This should not happen
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_undeprecate_with_no_package_found() {
    // Set up a repo with two packages, both already deprecated
    let name1 = "my-pkg/1.0.0";
    let name2 = "my-pkg/1.0.1";
    let repo = make_repo!([
        {"pkg": "my-pkg/1.0.0", "deprecated": true},
        {"pkg": "my-pkg/1.0.1", "deprecated": true},
    ]);
    let repos = vec![("test".to_string(), repo)];

    // Test undeprecating a package, when there is no such package in
    // the repos. This should return a result of 4.
    let missing_pkg = "nosuchpackage/1.0.0";

    let packages = vec![missing_pkg.to_string()];
    let yes = true;
    let result = change_deprecation_state(ChangeAction::Undeprecate, &repos, &packages, yes).await;

    match result {
        Ok(r) => assert_eq!(r, 4),
        Err(e) => {
            // This should not happen
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }

    // This test succeeds if it reaches this point. The packages should
    // still be deprecated because the command should have exited
    // before it made any changes to them.
    for name in &[name1, name2] {
        let ident = parse_ident(name).unwrap();
        let repo = &repos[0].1;
        let spec = repo.read_recipe(&ident).await.unwrap();
        assert!(spec.is_deprecated());
    }
}
