// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

use futures::TryStreamExt;
use itertools::Itertools;
use rstest::rstest;
use spfs::encoding::EMPTY_DIGEST;
use spk_schema::foundation::build_ident;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident_build::BuildId;
use spk_schema::ident_component::Component;
use spk_schema::name::OptNameBuf;
use spk_schema::prelude::{HasVersion, Named, Versioned};
use spk_schema::spec_ops::HasBuildIdent;
use spk_schema::{
    ComponentEmbeddedPackage,
    ComponentSpec,
    Components,
    Deprecate,
    OptionMap,
    OptionValues,
    Package,
    Spec,
    v0,
};
use spk_solve_macros::{
    make_build,
    //make_build_and_components,
    make_package,
    //pinned_request,
};

use crate::{IndexedRepository, RepoWalkerBuilder, RepoWalkerItem, RepositoryHandle};

// A copy of the one in spk-solve-macros because using it directly
// here cases an import loop
#[macro_export]
macro_rules! make_repo {
    ( [ $( $spec:tt ),+ $(,)? ] ) => {{
        make_repo!([ $( $spec ),* ], options={})
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options={ $($k:expr => $v:expr),* } ) => {{
        let options = spk_schema::foundation::option_map!{$($k => $v),*};
        make_repo!([ $( $spec ),* ], options=options)
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options=$options:expr ) => {{
        tracing::debug!("creating in-memory repository");
        // This line changed from the copy in spk_solve_macros
        let repo = RepositoryHandle::new_mem();
        let _opts = $options;
        $(
            // This line changed from the copy in spk_solve_macros
            let (s, cmpts) = make_package!(repo, $spec, &_opts);
            tracing::trace!(pkg=%spk_schema::Package::ident(&s), cmpts=?cmpts.keys(), "adding package to repo");
            repo.publish_package(&s, &cmpts).await.unwrap();
        )*
        repo
    }};
}

// A cutdown copy of the one used by solver_tests
#[macro_export]
macro_rules! option_map {
    ($($k:expr => $v:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut opts = OptionMap::default();
        $(opts.insert(
            OptNameBuf::try_from($k).expect("invalid option name"),
            $v.into()
        );)*
        opts
    }};
}

// Helper for making an IndexedRepository from a RepositoryHandle for testing with
async fn index_for_test(repo: RepositoryHandle) -> RepositoryHandle {
    let ir = match IndexedRepository::generate_from_repo(Arc::new(repo)).await {
        Ok(ir) => ir,
        Err(err) => {
            panic!(
                "Unable to make IndexedRepository: Failed to generate an in-mem index from a repo: {err}"
            )
        }
    };
    ir.into()
}

// Helper for comparing 2 Specs, from a repo and its index, to check
// the index one is equivalent to the repo one based on the data
// returned from the trait methods of each of them.
fn assert_packages_are_equivalent(build_from_repo: Arc<Spec>, build_from_index: Arc<Spec>) {
    // Compare the trait methods that both packages implement
    let pkg = build_from_repo.ident();

    // BuildOptions - not implemented by Spec

    // Deprecate trait
    assert_eq!(
        build_from_repo.is_deprecated(),
        build_from_index.is_deprecated(),
        "is_deprecated() methods don't match [{pkg}]"
    );

    // Satisfy<PkgRequestWithOptions>  - need a sample request

    // Satisfy<VarRequest<PinnedValue>> - need a sample request

    // HasVersion
    assert_eq!(
        build_from_repo.version(),
        build_from_index.version(),
        "version() methods don't match [{pkg}]"
    );

    // HasBuild - not implemented by spec

    // BuildIdent
    assert_eq!(
        build_from_repo.build_ident(),
        build_from_index.build_ident(),
        "build_ident() methods don't match [{pkg}]"
    );

    // Named
    assert_eq!(
        build_from_repo.name(),
        build_from_index.name(),
        "name() methods don't match [{pkg}]"
    );

    // RuntimeEnvironment - not implemented by SolverPackageSpec

    // Versioned
    assert_eq!(
        build_from_repo.compat(),
        build_from_index.compat(),
        "compat() methods don't match [{pkg}]"
    );

    // Components - compared in pieces to exclude the parts that are
    // not stored in the index.
    let repo_components = build_from_repo.components();
    assert_eq!(
        repo_components.len(),
        build_from_index.components().len(),
        "components() methods do not return the same number of components in index package: '{pkg}': [{}] vs [{}]",
        repo_components
            .iter()
            .map(|cs| cs.name.to_string())
            .join(", "),
        build_from_index
            .components()
            .iter()
            .map(|cs| cs.name.to_string())
            .join(", "),
    );

    let index_components: HashMap<Component, ComponentSpec> = build_from_index
        .components()
        .iter()
        .map(|cs| (cs.name.clone(), cs.clone()))
        .collect();

    for repo_comp in repo_components.iter() {
        let index_comp = index_components.get(&repo_comp.name).unwrap_or_else(|| {
            panic!(
                "'{}' component should exist in index package: '{pkg}'",
                repo_comp.name
            )
        });

        assert_eq!(
            repo_comp.uses, index_comp.uses,
            "components() methods don't match [{pkg}:{}] in .uses field",
            repo_comp.name,
        );
        assert_eq!(
            repo_comp.requirements(),
            index_comp.requirements(),
            "components() methods don't match [{pkg}:{}] in requirements() method",
            repo_comp.name,
        );

        let repo_comp_embedded: Vec<ComponentEmbeddedPackage> = repo_comp.embedded.to_vec();
        let index_comp_embedded: Vec<ComponentEmbeddedPackage> = index_comp.embedded.to_vec();

        assert_eq!(
            repo_comp_embedded, index_comp_embedded,
            "components() methods don't match [{pkg}:{}] in embedded field (ignoring fabricated)",
            repo_comp.name,
        );
        assert_eq!(
            repo_comp.requirements_with_options(),
            index_comp.requirements_with_options(),
            "components() methods don't match [{pkg}:{}] in requirements_with_options() method",
            repo_comp.name,
        );
    }

    // Package trait
    assert_eq!(
        build_from_repo.ident(),
        build_from_index.ident(),
        "ident() methods don't match [{pkg}]"
    );

    // metadata() - not implemented by SolverPackageSpec
    // matches_all_filters(&self, filter_by: &Option<Vec<OptFilter>>) -> bool  - need sample filters
    // sources(&self) -> &Vec<SourceSpec> - not implemented by SolverPackageSpec

    let repo_embedded = build_from_repo.embedded();
    assert_eq!(
        repo_embedded.len(),
        build_from_index.embedded().len(),
        "embedded() methods do not return the same number of packages in index package: '{pkg}': [{}] vs [{}]",
        repo_embedded
            .iter()
            .map(|es| es.ident().to_string())
            .join(", "),
        build_from_index
            .embedded()
            .iter()
            .map(|es| es.ident().to_string())
            .join(", "),
    );

    assert_eq!(
        build_from_repo.embedded(),
        build_from_index.embedded(),
        "embedded() methods don't match [{pkg}]"
    );

    assert_eq!(
        build_from_repo.embedded_as_packages(),
        // TODO: could recursive compare these too? all the way down, might need to
        build_from_index.embedded_as_packages(),
        "embedded_as_packages() don't match [{pkg}]"
    );

    let re = build_from_repo.embedded_as_packages();
    let ie = build_from_index.embedded_as_packages();
    tracing::error!("RE:\n{re:?}\nIE:\n{ie:?}\n");

    assert_eq!(
        build_from_repo.get_build_options(),
        build_from_index.get_build_options(),
        "get_build_options() don't match [{pkg}]",
    );

    // get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList<PinnedRequest>>> - not implemented by SolverPackageSpec

    assert_eq!(
        build_from_repo.runtime_requirements(),
        build_from_index.runtime_requirements(),
        "runtime_requirements() don't match [{pkg}]",
    );

    //  fn get_all_tests(&self) -> Vec<SpecTest> - not implemented by SolverPackageSpec

    // DownstreamRequirements - not implemented by SolverPackageSpec

    // OptionValues
    assert_eq!(
        build_from_repo.option_values(),
        build_from_index.option_values(),
        "option_values() don't match [{pkg}]",
    );
}

// Helper for comparing the packages in the original repo with the
// ones in an indexed repo constructed from the original repo.
async fn assert_repo_has_same_packages_as_other_repo(
    repo1: &RepositoryHandle,
    repo2: &RepositoryHandle,
) {
    // Spin thru' the first repo and get all the builds and look up
    // each build in the second repo and then compare by calling all
    // the relevant trait methods on both package builds and checking
    // they give the same results.
    let repos = vec![(format!("{}", repo1.name()), repo1.clone())];
    let mut repo_walker_builder = RepoWalkerBuilder::new(&repos);
    let repo_walker = repo_walker_builder
        .with_report_on_versions(true)
        .with_report_on_builds(true)
        .with_report_src_builds(true)
        .with_report_deprecated_builds(true)
        .with_report_embedded_builds(true)
        .with_end_of_markers(true)
        .with_sort_objects(true)
        .with_continue_on_error(true)
        .build();

    let mut traversal = repo_walker.walk();
    while let Some(item) = traversal.try_next().await.unwrap() {
        if let RepoWalkerItem::Build(build) = item {
            let build_from_repo1 = build.spec;
            println!(
                "Original Build: '{}' '{:#}'",
                build_from_repo1.ident(),
                build_from_repo1.ident(),
            );

            let build_from_repo2 = repo2.read_package(build_from_repo1.ident()).await.unwrap();
            println!(
                "Indexed  Build: '{}' '{:#}'",
                build_from_repo2.ident(),
                build_from_repo2.ident()
            );

            assert_packages_are_equivalent(build_from_repo1, build_from_repo2)
        }
    }
}

// Helper for comparing the packages in the original repo with the
// ones in an indexed repo constructed from the original repo.
async fn assert_repo_and_index_have_same_packages(
    repo: RepositoryHandle,
    indexed_repo: RepositoryHandle,
) {
    println!("\nCheck one");
    assert_repo_has_same_packages_as_other_repo(&repo, &indexed_repo).await;
    println!("\nCheck two");
    assert_repo_has_same_packages_as_other_repo(&indexed_repo, &repo).await;
}

#[rstest]
#[tokio::test]
async fn test_flatbuffer_index_one_package_with_no_recipe(random_build_id: BuildId) {
    let repo = RepositoryHandle::new_mem();

    let spec = v0::PackageSpec::new(build_ident!(format!("my-pkg/1.0.0/{random_build_id}")));

    // publish package without publishing spec
    let components = vec![
        (Component::Run, EMPTY_DIGEST.into()),
        (Component::Build, EMPTY_DIGEST.into()),
    ]
    .into_iter()
    .collect();
    repo.publish_package(&spec.into(), &components)
        .await
        .unwrap();
    let indexed_repo = index_for_test(repo.clone()).await;

    assert_repo_and_index_have_same_packages(repo, indexed_repo).await;
}

#[rstest]
#[tokio::test]
async fn test_flatbuffer_index_pre_release_config() {
    let repo = make_repo!(
        [
            {"pkg": "my-pkg/0.9.0"},
            {"pkg": "my-pkg/1.0.0-pre.0"},
            {"pkg": "my-pkg/1.0.0-pre.1"},
            {"pkg": "my-pkg/1.0.0-pre.2"},
        ]
    );
    let indexed_repo = index_for_test(repo.clone()).await;

    assert_repo_and_index_have_same_packages(repo, indexed_repo).await;
}

#[rstest]
#[tokio::test]
async fn test_flatbuffer_index_component_embedded_component_requirements() {
    let repo = make_repo!(
        [
            {
                "pkg": "mypkg/1.0.0",
                "install": {
                    "components": [
                        {"name": "comp1"},
                        {"name": "comp2"},
                    ],
                    "embedded": [
                        {"pkg": "dep-e1/1.0.0",
                         "install": {"components": [
                                        // comp1 requires a package that exists
                                        {"name": "comp1", "requirements": [{"pkg": "dep-e2/1.0.0"}]},
                                        // comp2 requires a package that does not exist
                                        {"name": "comp2", "requirements": [{"pkg": "dep-e3/1.0.0"}]}
                                    ]}
                        },
                    ],
                },
            },
            {"pkg": "dep-e2/1.0.0"},
        ]
    );

    let indexed_repo = index_for_test(repo.clone()).await;

    assert_repo_and_index_have_same_packages(repo, indexed_repo).await;
}

#[rstest]
#[tokio::test]
async fn test_flatbuffer_index_component_embedded_multiple_versions() {
    // test when different components embed different versions of the same
    // embedded package
    // - requesting individual components should select the correct version of
    //   the embedded package
    let repo = make_repo!(
        [
             {
                 "pkg": "mypkg/1.0.0",
                 "install": {
                     "components": [
                         {"name": "build", "embedded": ["dep-e1:all/1.0.0"]},
                         {"name": "run", "embedded": ["dep-e1:all/2.0.0"]},
                     ],
                     "embedded": [
                         {"pkg": "dep-e1/1.0.0"},
                         {"pkg": "dep-e1/2.0.0"},
                     ],
                 },
             },
            {"pkg": "dep-e1/1.0.0"},
            {"pkg": "dep-e1/2.0.0"},
            // Should solve
            {
                "pkg": "downstream1",
                "install": {
                    "requirements": [{"pkg": "dep-e1/1.0.0"}, {"pkg": "mypkg:build"}]
                },
            },
            // Should solve
            {
                "pkg": "downstream2",
                "install": {
                    "requirements": [{"pkg": "dep-e1/2.0.0"}, {"pkg": "mypkg:run"}]
                },
            },
            // Should not solve
            {
                "pkg": "downstream3",
                "install": {
                    "requirements": [{"pkg": "dep-e1/1.0.0"}, {"pkg": "mypkg:run"}]
                },
            },
        ]
    );
    let indexed_repo = index_for_test(repo.clone()).await;

    assert_repo_and_index_have_same_packages(repo, indexed_repo).await;
}

#[rstest]
#[tokio::test]
async fn test_flatbuffers_index_version_number_masking() {
    // Uses a dummy package to prime the repo
    let repo1 = make_repo!([{"pkg": "python/3.9.7"}]);
    let repo2 = make_repo!([{"pkg": "python/3.9.7"}]);

    // One repo gets 1.0.0 version and the other gets a 1.0 version
    let options = option_map! { "color" => "red" };
    let (s, cmpts) = make_package!(
        repo1,
        {
            "pkg": "my-pkg/1.0.0",
            "build": {"options": [{"var": "color"}]},
        },
        options
    );
    repo1.publish_package(&s, &cmpts).await.unwrap();
    tracing::info!(pkg=%spk_schema::Package::ident(&s), "published package to repo1");
    let options = option_map! { "color" => "blue" };
    let (s, cmpts) = make_package!(
        repo2,
        {
            "pkg": "my-pkg/1.0",
            "build": {"options": [{"var": "color"}]},
        },
        options
    );
    repo2.publish_package(&s, &cmpts).await.unwrap();
    tracing::info!(pkg=%spk_schema::Package::ident(&s), "published package to repo2");

    println!("Repo1's");

    let indexed_repo1 = index_for_test(repo1.clone()).await;

    assert_repo_and_index_have_same_packages(repo1, indexed_repo1).await;

    println!("Repo2's");

    let indexed_repo2 = index_for_test(repo2.clone()).await;

    assert_repo_and_index_have_same_packages(repo2, indexed_repo2).await;
}

#[rstest]
#[tokio::test]
async fn test_flatbuffer_deprecated_version() {
    let deprecated = make_build!({"pkg": "my-pkg/1.0.0", "deprecated": true});
    let repo = make_repo!(
        [{"pkg": "my-pkg/0.9.0"}, {"pkg": "my-pkg/1.0.0", "deprecated": true}, deprecated]
    );

    let indexed_repo = index_for_test(repo.clone()).await;

    assert_repo_and_index_have_same_packages(repo, indexed_repo).await;
}

// TODO: add rest of solves sample repos to this as tests
