// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
use std::sync::Arc;

use futures::TryStreamExt;
use itertools::zip_eq;
use spfs::RemoteAddress;
use spfs::config::Remote;
use spk_schema::foundation::ident_component::Component;
use spk_schema::ident::parse_version_ident;
use spk_schema::ident_build::Build;
use spk_schema::name::PkgNameBuf;
use spk_schema::{DeprecateMut, Spec, recipe, spec};

use super::RepoWalkerBuilder;
use crate::fixtures::{empty_layer_digest, spfs_runtime, spfsrepo};
use crate::walker::{
    DeprecationState,
    RepoWalkerFilter,
    WalkedBuild,
    WalkedComponent,
    WalkedPackage,
    WalkedRepo,
    WalkedVersion,
};
use crate::{RepoWalkerItem, RepositoryHandle, remote_repository};

#[tokio::test]
async fn test_walker_builder_with_calls() {
    // Set up a test repo in the runtime
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    // Setup the list of repos to walk
    let repo_name = Arc::new("origin".to_string());
    let repo = remote_repository(&*repo_name).await.unwrap();
    let repos = vec![((*repo_name).clone(), RepositoryHandle::SPFS(repo))];

    // Make and test the walker
    let package = Some("test/1.0.0".to_string());
    let mut builder = RepoWalkerBuilder::new(&repos);

    let _walker = builder
        .with_package_name_substring_matching("test".to_string())
        .try_with_package_equals(&package)
        .unwrap()
        .with_file_path(Some("lib/python/site-packages".to_string()))
        .with_package_filter(RepoWalkerFilter::no_package_filter)
        .with_build_ident_filter(RepoWalkerFilter::no_build_ident_filter)
        .with_build_spec_filter(RepoWalkerFilter::no_build_spec_filter)
        .with_component_filter(RepoWalkerFilter::no_component_filter)
        .with_file_filter(RepoWalkerFilter::no_file_filter)
        .with_report_on_versions(true)
        .with_report_on_builds(true)
        .with_report_src_builds(true)
        .with_report_deprecated_builds(true)
        .with_report_embedded_builds(true)
        .with_build_options_matching(None)
        .with_report_on_components(true)
        .with_report_on_files(true)
        .with_end_of_markers(true)
        .with_continue_on_error(true)
        .with_sort_objects(true)
        .with_highest_version_only(false)
        .with_calculate_deprecated_versions(false)
        .build();
}

#[tokio::test]
async fn test_walker_builder_walker_walk() {
    // Set up a test repo in the runtime
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Setup the list of repos to walk
    let repo_name: &str = "origin";
    let repo = remote_repository(repo_name).await.unwrap();
    let repos = vec![(repo_name.to_string(), RepositoryHandle::SPFS(repo))];

    // Set up expected items in the order they should be walked.
    let pkg_name = Arc::new(unsafe { PkgNameBuf::from_string("my-pkg".to_string()) });
    let ident = Arc::new(parse_version_ident("my-pkg/1.0.0").unwrap());
    let build_ident = ident.to_build_ident(Build::Source);
    let expected = vec![
        RepoWalkerItem::Repo(WalkedRepo { name: repo_name }),
        RepoWalkerItem::Package(WalkedPackage {
            repo_name,
            name: pkg_name.clone(),
        }),
        RepoWalkerItem::Version(WalkedVersion {
            repo_name,
            ident: ident.clone(),
            deprecation_state: DeprecationState::NotCalculated,
        }),
        RepoWalkerItem::Build(WalkedBuild {
            repo_name,
            spec: Arc::new(Spec::V0Package(spk_schema::v0::Spec::new(
                build_ident.clone(),
            ))),
        }),
        RepoWalkerItem::Component(WalkedComponent {
            repo_name,
            build: Arc::new(build_ident.clone()),
            name: Component::Run,
            digest: Arc::new(empty_layer_digest()),
        }),
        // Note: this doesn't check files
        RepoWalkerItem::EndOfBuild(WalkedBuild {
            repo_name,
            spec: Arc::new(Spec::V0Package(spk_schema::v0::Spec::new(build_ident))),
        }),
        RepoWalkerItem::EndOfVersion(WalkedVersion {
            repo_name,
            ident: ident.clone(),
            deprecation_state: DeprecationState::NotCalculated,
        }),
        RepoWalkerItem::EndOfPackage(WalkedPackage {
            repo_name,
            name: pkg_name,
        }),
        RepoWalkerItem::EndOfRepo(WalkedRepo { name: repo_name }),
    ];

    // Make and test the walker
    let mut builder = RepoWalkerBuilder::new(&repos);
    let walker = builder
        .with_end_of_markers(true)
        .with_report_on_components(true)
        .build();
    let mut traversal = walker.walk();

    let mut count = 0;
    while let Some(item) = traversal.try_next().await.unwrap() {
        println!("walked: {item:?}");
        // Should encounter the same kinds of items in the same order.
        // This doesn't check for exact matches, it just verifies the
        // walk order.
        println!("Comparing: {:?} == {:?}", item, expected[count]);
        assert_eq!(
            std::mem::discriminant(&item),
            std::mem::discriminant(&expected[count]),
        );

        count += 1;
    }

    assert_eq!(count, expected.len());
}

#[tokio::test]
async fn test_walker_builder_walker_walk_with_calc_deprecated_versions() {
    // Set up a test repo in the runtime
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    // An active package version, none of its builds are deprecated
    let recipe = recipe!({"pkg": "my-pkg/3.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let active_spec_three = spec!({"pkg": "my-pkg/3.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &active_spec_three,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // A deprecated package version, because all its builds are deprecated
    let recipe = recipe!({"pkg": "my-pkg/2.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let mut deprecated_spec_two = spec!({"pkg": "my-pkg/2.0.0/BGSHW3CN"});
    deprecated_spec_two.deprecate().unwrap();
    remote_repo
        .publish_package(
            &deprecated_spec_two,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // A partially deprecated package version, because some of its
    // builds are deprecated.
    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let mut deprecated_spec_one = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    deprecated_spec_one.deprecate().unwrap();
    remote_repo
        .publish_package(
            &deprecated_spec_one,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();
    let active_spec_one = spec!({"pkg": "my-pkg/1.0.0/DWHSVGP2"});
    remote_repo
        .publish_package(
            &active_spec_one,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Setup the list of repos to walk
    let repo_name: &str = "origin";
    let repo = remote_repository(repo_name).await.unwrap();
    let repos = vec![(repo_name.to_string(), RepositoryHandle::SPFS(repo))];

    // Set up expected items in the order they should be walked.
    let pkg_name = Arc::new(unsafe { PkgNameBuf::from_string("my-pkg".to_string()) });

    let ident_one = Arc::new(parse_version_ident("my-pkg/1.0.0").unwrap());
    let ident_two = Arc::new(parse_version_ident("my-pkg/2.0.0").unwrap());
    let ident_three = Arc::new(parse_version_ident("my-pkg/3.0.0").unwrap());

    let expected_items = vec![
        RepoWalkerItem::Repo(WalkedRepo { name: repo_name }),
        RepoWalkerItem::Package(WalkedPackage {
            repo_name,
            name: pkg_name.clone(),
        }),
        // Version 3.0.0 is Active
        RepoWalkerItem::Version(WalkedVersion {
            repo_name,
            ident: ident_three.clone(),
            deprecation_state: DeprecationState::Active,
        }),
        RepoWalkerItem::Build(WalkedBuild {
            repo_name,
            spec: Arc::new(active_spec_three),
        }),
        RepoWalkerItem::EndOfVersion(WalkedVersion {
            repo_name,
            ident: ident_three.clone(),
            deprecation_state: DeprecationState::Active,
        }),
        // Version 2.0.0 is deprecated
        RepoWalkerItem::Version(WalkedVersion {
            repo_name,
            ident: ident_two.clone(),
            deprecation_state: DeprecationState::Deprecated,
        }),
        RepoWalkerItem::Build(WalkedBuild {
            repo_name,
            spec: Arc::new(deprecated_spec_two),
        }),
        RepoWalkerItem::EndOfVersion(WalkedVersion {
            repo_name,
            ident: ident_two.clone(),
            deprecation_state: DeprecationState::Deprecated,
        }),
        // Version 1.0.0 is partially-deprecated
        RepoWalkerItem::Version(WalkedVersion {
            repo_name,
            ident: ident_one.clone(),
            deprecation_state: DeprecationState::PartiallyDeprecated,
        }),
        RepoWalkerItem::Build(WalkedBuild {
            repo_name,
            spec: Arc::new(deprecated_spec_one),
        }),
        RepoWalkerItem::Build(WalkedBuild {
            repo_name,
            spec: Arc::new(active_spec_one),
        }),
        RepoWalkerItem::EndOfVersion(WalkedVersion {
            repo_name,
            ident: ident_one.clone(),
            deprecation_state: DeprecationState::PartiallyDeprecated,
        }),
        RepoWalkerItem::EndOfPackage(WalkedPackage {
            repo_name,
            name: pkg_name,
        }),
        RepoWalkerItem::EndOfRepo(WalkedRepo { name: repo_name }),
    ];

    // Walk the repo to get the items
    let mut builder = RepoWalkerBuilder::new(&repos);
    let walker = builder
        .with_end_of_markers(true)
        .with_report_on_builds(true)
        .with_report_deprecated_builds(true)
        .with_calculate_deprecated_versions(true)
        .build();
    let mut traversal = walker.walk();

    let mut walked_items = Vec::new();
    while let Some(item) = traversal.try_next().await.unwrap() {
        walked_items.push(item);
    }

    // Test the walked items are what was expected
    for (item, expected) in zip_eq(walked_items.iter(), expected_items.iter()) {
        println!("Comparing: {:?}\n     with: {:?}", item, expected);
        println!("   equal?: {}\n", item == expected);
        assert_eq!(
            item, expected,
            "Walked item did not match the expected item"
        );
    }
}
