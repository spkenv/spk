// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use spk_schema::foundation::format::{FormatChangeOptions, FormatRequest};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf};
use spk_schema::foundation::version::Compatibility;
use spk_schema::ident::{InclusionPolicy, PkgRequest, RangeIdent, Request};
use spk_schema::spec_ops::Versioned;
use spk_schema::{AnyIdent, BuildIdent, Package, RequirementsList, Spec};
use spk_solve_graph::{GetMergedRequestError, GetMergedRequestResult};
use spk_solve_solution::PackageSource;
use spk_storage::RepositoryHandle;
use tokio::sync::mpsc::{self, Sender};

use crate::validation::{
    BinaryOnlyValidator,
    ComponentsValidator,
    DeprecationValidator,
    GetMergedRequest,
    PkgRequestValidator,
    ValidatorT,
};
use crate::{Error, Result, Validators};

#[cfg(test)]
#[path = "./impossible_checks_test.rs"]
mod impossible_checks_test;

/// A tracing target for the impossible request checks
pub const IMPOSSIBLE_CHECKS_TARGET: &str = "spk_solve::impossible_checks";

/// Maximum number of messages in the sub-tasks communication channel
const TASK_CHANNEL_MESSAGE_CAPACITY: usize = 128;

/// Format options for requests in tracing messages
const REQUEST_FORMAT_OPTIONS: FormatChangeOptions = FormatChangeOptions {
    verbosity: 100,
    level: 100,
};

/// The default set of validators used for impossible version request
/// checks. This is a subset of the full set of validators that must
/// only include validators that detect issues with package version
/// requests.
pub const fn default_impossible_version_validators() -> &'static [Validators] {
    // The validators that detect issues with pkg version requests only.
    &[
        Validators::Deprecation(DeprecationValidator {}),
        Validators::PackageRequest(PkgRequestValidator {}),
        Validators::Components(ComponentsValidator {}),
    ]
}

/// The valid messages that can be sent from tasks launched during
/// impossible request checks.
enum Comms {
    /// Sent by the task making task when it has created and launched
    /// a new version task to check all the builds in that version.
    /// Contains the pkg/version and the future for the new task.
    NewVersionTask {
        pkg_version: AnyIdent,
        task: tokio::task::JoinHandle<Result<Compatibility>>,
    },
    /// Send by the version tasks when they have are complete.
    /// Contains the number of build specs read, and either: a valid
    /// build and a compatible compat, if a valid build was found, or:
    /// the pkg_version and incompatible compat.
    VersionTaskDone {
        build: AnyIdent,
        compat: Compatibility,
        builds_read: u64,
    },
    /// Sent by the task making task when it is complete. Contains the
    /// number of version tasks it created and launched.
    MakingTaskDone { launch_count: u64 },
}

/// Checks for impossible requests that a package would generate from
/// its install requirements and a set of unresolved requests.
/// Impossible and possible pkg requests are cached to speed up future
/// checking.
pub struct ImpossibleRequestsChecker {
    /// The validators this uses to check for impossible requests
    validators: Arc<std::sync::Mutex<Vec<Validators>>>,
    // TODO: because this just stores RangeIdents and not PkgRequests,
    // we don't have what made the requests, do we need this here,
    // should it be PkgRequests, at least for impossibles?
    /// Cache of impossible request to number of times it has been seen
    impossible_requests: DashMap<RangeIdent, u64>,
    /// Cache of possible request to number of times is has been seen
    possible_requests: DashMap<RangeIdent, u64>,
    /// Number of IfAlreadyPresent requests skipped during checks
    num_ifalreadypresent_requests: AtomicU64,
    /// Number of distinct impossible requests found
    num_impossible_requests_found: AtomicU64,
    /// Number of distinct possible requests found
    num_possible_requests_found: AtomicU64,
    /// Number of impossible requests found using the cache
    num_impossible_cache_hits: AtomicU64,
    /// Number of possible requests found using the cache
    num_possible_cache_hits: AtomicU64,
    /// Number of build specs read in during processing
    num_build_specs_read: AtomicU64,
    /// Number of version all builds reading tasks spawned during checks
    num_read_tasks_spawned: AtomicU64,
    /// Number of spawned tasks stopped before they finished
    num_read_tasks_stopped: AtomicU64,
}

impl Default for ImpossibleRequestsChecker {
    fn default() -> Self {
        Self {
            validators: Arc::new(std::sync::Mutex::new(Vec::from(
                default_impossible_version_validators(),
            ))),
            impossible_requests: DashMap::new(),
            possible_requests: DashMap::new(),
            num_ifalreadypresent_requests: AtomicU64::new(0),
            num_impossible_requests_found: AtomicU64::new(0),
            num_possible_requests_found: AtomicU64::new(0),
            num_impossible_cache_hits: AtomicU64::new(0),
            num_possible_cache_hits: AtomicU64::new(0),
            num_build_specs_read: AtomicU64::new(0),
            num_read_tasks_spawned: AtomicU64::new(0),
            num_read_tasks_stopped: AtomicU64::new(0),
        }
    }
}

impl ImpossibleRequestsChecker {
    /// Set whether to only allow pre-built binary packages. If true,
    /// src packages will be treated as invalid for requests,
    /// otherwise src packages will be allowed to satisfy request checking
    pub fn set_binary_only(&self, binary_only: bool) {
        let mut validators_lock = self.validators.lock().unwrap();

        let has_binary_only = validators_lock
            .iter()
            .find_map(|v| match v {
                Validators::BinaryOnly(_) => Some(true),
                _ => None,
            })
            .unwrap_or(false);
        if !(has_binary_only ^ binary_only) {
            return;
        }
        if binary_only {
            // Add BinaryOnly validator because it was missing
            validators_lock.insert(0, Validators::BinaryOnly(BinaryOnlyValidator {}))
        } else {
            // Remove all BinaryOnly validators because one was found
            (*validators_lock).retain(|v| !matches!(v, Validators::BinaryOnly(_)));
        }
    }

    /// Reset the ImpossibleChecker's counters and request caches
    pub fn reset(&self) {
        self.impossible_requests.clear();
        self.possible_requests.clear();

        self.num_ifalreadypresent_requests
            .store(0, Ordering::Relaxed);
        self.num_impossible_requests_found
            .store(0, Ordering::Relaxed);
        self.num_possible_requests_found.store(0, Ordering::Relaxed);
        self.num_impossible_cache_hits.store(0, Ordering::Relaxed);
        self.num_possible_cache_hits.store(0, Ordering::Relaxed);
        self.num_build_specs_read.store(0, Ordering::Relaxed);
        self.num_read_tasks_spawned.store(0, Ordering::Relaxed);
        self.num_read_tasks_stopped.store(0, Ordering::Relaxed);
    }

    /// Get the impossible requests to frequency mapping
    pub fn impossible_requests(&self) -> &DashMap<RangeIdent, u64> {
        &self.impossible_requests
    }

    /// Get the possible requests to frequency mapping
    pub fn possible_requests(&self) -> &DashMap<RangeIdent, u64> {
        &self.possible_requests
    }

    /// Get the number of IfAlreadyPresent requests skipped during checks
    pub fn num_ifalreadypresent_requests(&self) -> u64 {
        self.num_ifalreadypresent_requests.load(Ordering::Relaxed)
    }

    /// Get the number of distinct impossible requests found
    pub fn num_impossible_requests_found(&self) -> u64 {
        self.num_impossible_requests_found.load(Ordering::Relaxed)
    }

    /// Get the number of distinct possible requests found
    pub fn num_possible_requests_found(&self) -> u64 {
        self.num_possible_requests_found.load(Ordering::Relaxed)
    }

    /// Get the number of impossible requests found using the cache
    pub fn num_impossible_hits(&self) -> u64 {
        self.num_impossible_cache_hits.load(Ordering::Relaxed)
    }

    /// Get the number of possible requests found using the cache
    pub fn num_possible_hits(&self) -> u64 {
        self.num_possible_cache_hits.load(Ordering::Relaxed)
    }

    /// Get the number of builds read in during processing so far
    pub fn num_build_specs_read(&self) -> u64 {
        self.num_build_specs_read.load(Ordering::Relaxed)
    }

    /// Get the number of read tasks spawned
    pub fn num_read_tasks_spawned(&self) -> u64 {
        self.num_read_tasks_spawned.load(Ordering::Relaxed)
    }

    /// Get the number of read tasks stopped
    pub fn num_read_tasks_stopped(&self) -> u64 {
        self.num_read_tasks_stopped.load(Ordering::Relaxed)
    }

    /// Ensures the request is cached as an impossible request and
    /// updates the appropriate counter based on whether this is the
    /// first time it has been cached or not.
    fn cache_and_count_impossible_request(&self, request: RangeIdent) {
        let mut counter = self.impossible_requests.entry(request).or_insert(0);
        *counter += 1;

        if *counter == 1 {
            self.num_impossible_requests_found
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.num_impossible_cache_hits
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Ensures the request is cached as a possible request and
    /// updates the appropriate counter based on whether this is the
    /// first time it has been cached or not.
    fn cache_and_count_possible_request(&self, request: RangeIdent) {
        let mut counter = self.possible_requests.entry(request).or_insert(0);
        *counter += 1;

        if *counter == 1 {
            self.num_possible_requests_found
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.num_possible_cache_hits.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Check that the given package's install pkg requests are
    /// possible when combined with the unresolved requests.
    pub async fn validate_pkg_requests(
        &self,
        package: &Spec,
        unresolved_requests: &HashMap<PkgNameBuf, PkgRequest>,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<Compatibility> {
        let requirements: &RequirementsList = package.runtime_requirements();
        if requirements.is_empty() {
            return Ok(Compatibility::Compatible);
        }

        tracing::debug!(
            target: IMPOSSIBLE_CHECKS_TARGET,
            "{}: package has requirements: {}",
            package.ident(),
            requirements
                .iter()
                .filter_map(|r| match r {
                    Request::Pkg(pr) => Some(format!("{}", pr.pkg)),
                    _ => None,
                })
                .collect::<Vec<String>>()
                .join(", ")
        );

        for req in requirements.iter() {
            let request = match req {
                Request::Var(_) => {
                    // Any var requests are not part of these checks
                    continue;
                }
                Request::Pkg(r) => r,
            };
            tracing::debug!(
                target: IMPOSSIBLE_CHECKS_TARGET,
                "Build {} checking req: {}",
                package.ident(),
                request.pkg
            );

            // Generate the combined request that would be created if
            // this request was added to the unresolved requests.
            let combined_request = match unresolved_requests.get(&request.pkg.name) {
                None => request.clone(),
                Some(unresolved_request) => {
                    tracing::debug!(
                        target: IMPOSSIBLE_CHECKS_TARGET,
                        "Unresolved request: {}",
                        unresolved_request.pkg
                    );
                    let mut combined_request = request.clone();
                    if let Err(err) = combined_request.restrict(unresolved_request) {
                        // The requests cannot be combined, usually because
                        // their ranges do not intersect. This makes the request
                        // an impossible one, but the combined request cannot be
                        // represented by a single request object - the combining
                        // failed - so the request is treated as impossible, but
                        // not cached for next time.
                        self.num_impossible_requests_found
                            .fetch_add(1, Ordering::Relaxed);
                        return Ok(Compatibility::incompatible(format!(
                            "depends on {} which generates an impossible request {},{unresolved_request} - {err}",
                            request.pkg,request.pkg,
                        )));
                    };
                    combined_request
                }
            };
            tracing::debug!(
                target: IMPOSSIBLE_CHECKS_TARGET,
                "Combined request: {combined_request} [{}]",
                combined_request.format_request(
                    &None,
                    &combined_request.pkg.name,
                    &REQUEST_FORMAT_OPTIONS
                )
            );

            if combined_request.inclusion_policy == InclusionPolicy::IfAlreadyPresent {
                // IfAlreadyPresent requests are optional until a
                // resolved package makes a real/'Always' dependency
                // request for the package. Until that happens they
                // are considered always possible.
                self.num_ifalreadypresent_requests
                    .fetch_add(1, Ordering::Relaxed);
                tracing::debug!( target: IMPOSSIBLE_CHECKS_TARGET,
                                "Combined request: {combined_request} has `IfAlreadyPresent` set, so it's possible"
                );
                continue;
            }

            if self.impossible_requests.contains_key(&combined_request.pkg) {
                tracing::debug!(
                    target: IMPOSSIBLE_CHECKS_TARGET,
                    "Matches cached Impossible request: denying {}",
                    combined_request.pkg
                );
                self.cache_and_count_impossible_request(combined_request.pkg.clone());
                return Ok(Compatibility::incompatible(format!(
                    "depends on {} which generates an impossible request {}",
                    request.pkg, combined_request.pkg
                )));
            }

            if self.possible_requests.contains_key(&combined_request.pkg) {
                tracing::debug!(
                    target: IMPOSSIBLE_CHECKS_TARGET,
                    "Matches cached Possible request: allowing {}",
                    combined_request.pkg
                );
                self.cache_and_count_possible_request(combined_request.pkg.clone());
                continue;
            }

            // Is there any valid build for the combined pkg request
            // among all the versions and builds in the repositories?
            // If so, then the request is possible, otherwise it is
            // impossible.
            let found_a_build = match self
                .any_build_valid_for_request(&combined_request, repos)
                .await
            {
                Ok(value) => value,
                Err(err) => return Err(err),
            };

            if found_a_build {
                tracing::debug!(
                    target: IMPOSSIBLE_CHECKS_TARGET,
                    "Found Possible request, allowing and caching for next time: {}\n",
                    combined_request.pkg
                );
                self.cache_and_count_possible_request(combined_request.pkg.clone());
            } else {
                tracing::debug!(
                    target: IMPOSSIBLE_CHECKS_TARGET,
                    "Found Impossible request, denying and caching for next time: {}\n",
                    combined_request.pkg
                );
                self.cache_and_count_impossible_request(combined_request.pkg.clone());
                return Ok(Compatibility::incompatible(format!(
                    "depends on {} which generates an impossible request {}",
                    request.pkg, combined_request.pkg
                )));
            }
        }

        Ok(Compatibility::Compatible)
    }

    /// Return true if there is any build in the repos that is valid
    /// for the given request, otherwise return false
    async fn any_build_valid_for_request(
        &self,
        combined_request: &PkgRequest,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<bool> {
        let package = AnyIdent::from(combined_request.pkg.name.clone());

        // Set up a channel for communication between this and the all
        // the spawned tasks. This will allow the processing to
        // short-circuit as soon as a valid build is found.
        let (tx, mut rx) = mpsc::channel(TASK_CHANNEL_MESSAGE_CAPACITY);

        let validators = Arc::clone(&self.validators);
        let combined_request_copy = combined_request.clone();
        let package_copy = package.clone();
        let repos_copy: Vec<Arc<RepositoryHandle>> = repos.iter().map(Arc::clone).collect();

        // This function only spawns one task, but it will spawn many
        // more tasks that will send messages back via the channel.
        let maker_task = async move {
            make_task_per_version(
                repos_copy,
                package_copy,
                combined_request_copy,
                validators,
                tx,
            )
            .await
        };

        // Launch the maker task in a separate thread. It will begin
        // running and spawning tasks (one per_version), which in term
        // will start sending messages back here.
        let list_and_launch_task = tokio::spawn(maker_task);

        let tasks = FuturesUnordered::new();
        let mut found_a_valid_build: bool = false;
        let mut task_count = 0;
        let mut task_done_count = 0;
        let mut launched_count = 0;

        while let Some(message) = rx.recv().await {
            match message {
                Comms::NewVersionTask { pkg_version, task } => {
                    // Remember the new task so this knows how many
                    // are still running when other messages come in
                    tasks.push(task);
                    task_count += 1;
                    self.num_read_tasks_spawned.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(
                        target: IMPOSSIBLE_CHECKS_TARGET,
                        "Task to read and validate {pkg_version} is running"
                    );
                }
                Comms::VersionTaskDone {
                    build,
                    compat,
                    builds_read,
                } => {
                    task_done_count += 1;
                    self.num_build_specs_read
                        .fetch_add(builds_read, Ordering::Relaxed);

                    if !compat.is_ok() {
                        tracing::debug!(
                            target: IMPOSSIBLE_CHECKS_TARGET,
                            "Invalid build {build} for the combined request: {compat}"
                        );
                    } else {
                        // Compatible with the request, which makes the request
                        // a possible one so don't need to look any further
                        rx.close();
                        list_and_launch_task.abort();
                        self.stop_all_tasks(&tasks);
                        let _ = list_and_launch_task.await;
                        let version_tasks_stopped = tasks.len();
                        let _: Vec<_> = tasks.collect().await;

                        found_a_valid_build = true;

                        tracing::debug!(
                            target: IMPOSSIBLE_CHECKS_TARGET,
                            "Found a valid build {build} for the combined request: {}",
                            combined_request.pkg
                        );
                        tracing::debug!(
                            target: IMPOSSIBLE_CHECKS_TARGET,
                            "Stopped all tasks: {} stopped, {} launched, {} done",
                            version_tasks_stopped,
                            task_count,
                            task_done_count
                        );
                        break;
                    }
                }
                Comms::MakingTaskDone { launch_count } => {
                    tracing::debug!(
                        target: IMPOSSIBLE_CHECKS_TARGET,
                        "Making task complete for {package}. It launched {launch_count} version tasks"
                    );
                    launched_count = launch_count;
                    if launch_count == 0 {
                        // No tasks were made by the maker task, so no
                        // need to keep listening to the channel.
                        break;
                    }
                }
            }

            tracing::debug!(
                target: IMPOSSIBLE_CHECKS_TARGET,
                "  Task counts: {task_done_count}/{task_count}/{launched_count}"
            );
            if launched_count > 0 && task_done_count == launched_count {
                // All the tasks that the maker task launched are
                // finished and done, no more will be sending messages.
                break;
            }
        }

        Ok(found_a_valid_build)
    }

    /// Stops all the given tasks
    fn stop_all_tasks(
        &self,
        build_spec_reads: &FuturesUnordered<tokio::task::JoinHandle<Result<Compatibility>>>,
    ) {
        for build_read in build_spec_reads.iter() {
            build_read.abort();
        }
        self.num_read_tasks_stopped
            .fetch_add(build_spec_reads.len() as u64, Ordering::Relaxed);
    }
}

/// Helper to get a map of components (names) to empty default layer
/// digests instead of the correct spfs layer digests. This is
/// suitable for use with the ComponentsValidator's validation checks
/// because that validator does not look at the digests.
async fn get_mock_build_components(
    repo: &Arc<RepositoryHandle>,
    build: &BuildIdent,
) -> Result<HashMap<Component, spfs::encoding::Digest>> {
    // An empty default digest is used to avoid calling
    // read_components() and the additional lookups it does (which do
    // give the correct real digest values).
    let mut components: HashMap<Component, spfs::encoding::Digest> = HashMap::new();
    match repo.list_build_components(build).await {
        Ok(v) => {
            for c in v.iter() {
                components.insert(c.clone(), spfs::encoding::Digest::default());
            }
        }
        Err(spk_storage::Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(..),
        )) => {}
        Err(err) => return Err(Error::SpkStorageError(err)),
    };

    Ok(components)
}

/// A wrapper for the combined package request, for using the package
/// request supporting validators.
struct PotentialPackageRequest<'a> {
    package_request: &'a PkgRequest,
}

impl<'a> PotentialPackageRequest<'a> {
    pub fn new(package_request: &'a PkgRequest) -> Self {
        PotentialPackageRequest { package_request }
    }
}

impl GetMergedRequest for PotentialPackageRequest<'_> {
    fn get_merged_request(&self, name: &PkgName) -> GetMergedRequestResult<PkgRequest> {
        // This should only be used to validate the package named in
        // the combined package request that it was created with. No
        // other package requests are present.
        if self.package_request.pkg.name() == name {
            Ok(self.package_request.clone())
        } else {
            Err(GetMergedRequestError::NoRequestFor(format!(
                "No requests for '{name}' [INTERNAL ERROR - impossible check validation only looking at a request for: {}]", self.package_request.pkg.name()
            )))
        }
    }
}

/// Return Compatible if the given spec is valid for the pkg request,
/// otherwise return the Incompatible reason from the first validation
/// check that failed.
fn validate_against_pkg_request(
    validators: &Arc<std::sync::Mutex<Vec<Validators>>>,
    combined_request: &PkgRequest,
    spec: &Spec,
    source: &PackageSource,
) -> Result<Compatibility> {
    let packages_state = PotentialPackageRequest::new(combined_request);
    for validator in validators.lock().unwrap().iter() {
        let compat = validator.validate_package_against_request(&packages_state, spec, source)?;
        if !compat.is_ok() {
            return Ok(compat);
        }
    }
    Ok(Compatibility::Compatible)
}

/// Make a task for each version a package has. Each version task will
/// check all that version's builds for one that is valid for the
/// request
async fn make_task_per_version(
    repos: Vec<Arc<RepositoryHandle>>,
    package: AnyIdent,
    request: PkgRequest,
    validators: Arc<std::sync::Mutex<Vec<Validators>>>,
    channel: Sender<Comms>,
) -> Result<()> {
    let mut number = 0;
    for repo in repos.iter() {
        for version in repo.list_package_versions(package.name()).await?.iter() {
            let compat = request.is_version_applicable(version);
            if !compat.is_ok() {
                tracing::debug!(
                    target: IMPOSSIBLE_CHECKS_TARGET,
                    "version {version} isn't applicable to the request, skipping its builds: {compat}"
                );
                continue;
            }

            let pkg_version = package.with_version((**version).clone());

            // Clone things the build checking task needs
            let pkg_version_for_task = pkg_version.clone();
            let validators_for_task = Arc::clone(&validators);
            let repo_copy = repo.clone();
            let request_for_task = request.clone();
            let task_channel = channel.clone();

            let task = async move {
                any_valid_build_in_version(
                    repo_copy,
                    pkg_version_for_task,
                    validators_for_task,
                    request_for_task,
                    task_channel,
                )
                .await
            };

            // Launch the task in a separate thread so it can begin
            // processing at once.
            let new_task = tokio::spawn(task);

            // Send a message about the new task to notify the
            // receiving function that another task has started.
            let message = Comms::NewVersionTask {
                pkg_version,
                task: new_task,
            };
            if (channel.send(message).await).is_err() {
                // The channel might have been shutdown because one of
                // the earlier tasks has already found a valid build.
                // There's no reason to continue launching tasks in
                // this situation.
                tracing::debug!(
                    target: IMPOSSIBLE_CHECKS_TARGET,
                    "Channel closed. Making task cut short for {package}"
                );
                return Ok::<(), Error>(());
            };

            number += 1;
        }
    }

    // Send a done message with number of launched tasks so the
    // receiving function can detect when all the versions tasks are done.
    let message = Comms::MakingTaskDone {
        launch_count: number,
    };
    // Ignore errors when sending, the channel might have been
    // shutdown before this can send if a task has already found a
    // valid build.
    let _ = channel.send(message).await;

    Ok::<(), Error>(())
}

/// Looks for a valid build for the request among the builds available
/// for the version. Sends a message on the channel as soon as a valid
/// build is found, or if all builds are invalid. Returns Compatible
/// if there is a valid build, and Incompatible if there isn't.
async fn any_valid_build_in_version(
    repo: Arc<RepositoryHandle>,
    pkg_version: AnyIdent,
    validators: Arc<std::sync::Mutex<Vec<Validators>>>,
    request: PkgRequest,
    channel: Sender<Comms>,
) -> Result<Compatibility> {
    // Note: because the builds aren't sorted, the order
    // they are returned in can vary from version to
    // version. That's okay this just needs to find one
    // that satisfies the request.
    let mut builds_read: u64 = 0;
    let builds = repo.list_package_builds(pkg_version.as_version()).await?;
    tracing::debug!(
        target: IMPOSSIBLE_CHECKS_TARGET,
        "Version task {pkg_version} got {} builds",
        builds.len()
    );
    for build in builds {
        let spec = match repo.read_package(&build).await {
            Ok(s) => s,
            Err(err @ spk_storage::Error::InvalidPackageSpec(..)) => {
                tracing::debug!(target: IMPOSSIBLE_CHECKS_TARGET, "Skipping: {err}");
                return Err(Error::SpkStorageError(err));
            }
            Err(err) => return Err(Error::SpkStorageError(err)),
        };
        builds_read += 1;

        tracing::debug!(
            target: IMPOSSIBLE_CHECKS_TARGET,
            "Read package spec for: {build} [compat={}]",
            spec.compat()
        );

        // These are only needed for the Components validator
        let components = get_mock_build_components(&repo, &build).await.unwrap();
        tracing::debug!(
            target: IMPOSSIBLE_CHECKS_TARGET,
            "Read components for: {build} and validation",
        );

        let compat = validate_against_pkg_request(
            &validators,
            &request,
            &spec,
            &PackageSource::Repository {
                repo: Arc::clone(&repo),
                components,
            },
        )?;
        if !compat.is_ok() {
            // Not compatible, move on to check the next build
            tracing::debug!(
                target: IMPOSSIBLE_CHECKS_TARGET,
                "Invalid build {build} for the combined request: {compat}"
            );
            continue;
        } else {
            // Compatible, so send a message and return immediately
            send_version_task_done_message(channel, build.to_any(), compat.clone(), builds_read)
                .await;

            return Ok(compat);
        }
    }

    // This is only reached if none of the builds were compatible
    let nothing_valid = Compatibility::incompatible(format!(
        "None of {pkg_version}'s builds were compatible with {request}"
    ));
    send_version_task_done_message(
        channel,
        pkg_version.clone(),
        nothing_valid.clone(),
        builds_read,
    )
    .await;

    Ok(nothing_valid)
}

/// Helper for sending VersionTaskDone messages to the channel
async fn send_version_task_done_message(
    channel: Sender<Comms>,
    build: AnyIdent,
    compat: Compatibility,
    builds_read: u64,
) {
    let message = Comms::VersionTaskDone {
        build,
        compat,
        builds_read,
    };
    // Ignore errors when sending, the channel might have been
    // shutdown before this can send if an earlier task found a valid
    // build.
    let _ = channel.send(message).await;
}
