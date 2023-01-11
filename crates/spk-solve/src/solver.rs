// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_stream::stream;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::{Stream, TryStreamExt};
use priority_queue::priority_queue::PriorityQueue;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf};
use spk_schema::foundation::version::Compatibility;
use spk_schema::ident::{PkgRequest, Request, RequestedBy, Satisfy, VarRequest};
use spk_schema::ident_build::EmbeddedSource;
use spk_schema::{BuildIdent, Deprecate, Package, Recipe, Spec, SpecRecipe};
use spk_solve_graph::{
    Change,
    Decision,
    Graph,
    Node,
    Note,
    RequestPackage,
    RequestVar,
    SetOptions,
    SkipPackageNote,
    State,
    StepBack,
    DEAD_STATE,
};
use spk_solve_package_iterator::{
    BuildIterator,
    EmptyBuildIterator,
    PackageIterator,
    RepositoryPackageIterator,
    SortedBuildIterator,
};
use spk_solve_solution::{PackageSource, Solution};
use spk_solve_validation::{
    default_validators,
    BinaryOnlyValidator,
    ImpossibleRequestsChecker,
    ValidatorT,
    Validators,
    IMPOSSIBLE_CHECKS_TARGET,
};
use spk_storage::RepositoryHandle;

use super::error;
use crate::error::OutOfOptions;
use crate::option_map::OptionMap;
use crate::{make_build, Error, Result};

// Public to allow other tests to use its macros
#[cfg(test)]
#[path = "./solver_test.rs"]
mod solver_test;

/// Structure to hold whether the three kinds of impossible checks are
/// enabled or disabled in a solver.
#[derive(Clone, Default)]
struct ImpossibleChecksSettings {
    pub check_initial_requests: bool,
    pub check_before_resolving: bool,
    pub use_in_build_keys: bool,
}

#[derive(Clone)]
pub struct Solver {
    repos: Vec<Arc<RepositoryHandle>>,
    initial_state_builders: Vec<Change>,
    validators: Cow<'static, [Validators]>,
    // For validating candidate requests and builds by checking the
    // merged requests they will create against the builds available
    // in the repos to see if any are impossible to satisfy.
    request_validator: Arc<ImpossibleRequestsChecker>,
    // For holding the settings that say which impossible checks are enabled
    impossible_checks: ImpossibleChecksSettings,
    // For counting the number of steps (forward) taken in a solve
    number_of_steps: usize,
    // For counting number of builds skipped for some reason
    number_builds_skipped: usize,
    // For counting the number of incompatible versions
    number_incompat_versions: usize,
    // For counting the number of incompatible builds
    number_incompat_builds: usize,
    // For counting the total number of builds expanded so far in
    // the solve
    number_total_builds: usize,
    // For counting the number of StepBacks applied during the solve
    number_of_steps_back: Arc<AtomicU64>,
    // For accumulating the frequency of error messages generated
    // during the solver. Used in end-of-solve stats or if the solve
    // is interrupted by the user or timeout.
    error_frequency: HashMap<String, ErrorFreq>,
    // For counting the number of times packages are involved in
    // blocked requests for other packages during the search. Used to
    // highlight problem areas in a solve and help user home in on
    // what might be causing issues.
    problem_packages: HashMap<String, u64>,
}

impl Default for Solver {
    fn default() -> Self {
        Self {
            repos: Vec::default(),
            initial_state_builders: Vec::default(),
            validators: Cow::from(default_validators()),
            request_validator: Arc::new(ImpossibleRequestsChecker::default()),
            impossible_checks: ImpossibleChecksSettings::default(),
            number_of_steps: 0,
            number_builds_skipped: 0,
            number_incompat_versions: 0,
            number_incompat_builds: 0,
            number_total_builds: 0,
            number_of_steps_back: Arc::new(AtomicU64::new(0)),
            error_frequency: HashMap::new(),
            problem_packages: HashMap::new(),
        }
    }
}

/// The kinds of internal error encountered during a solve that are tracked for frequency
#[derive(Debug, Clone)]
pub(crate) enum ErrorDetails {
    Message(String),
    CouldNotSatisfy(String, Vec<RequestedBy>),
}

/// The details of a 'could not satisfy' error
#[derive(Debug, Clone)]
pub struct CouldNotSatisfyRecord {
    /// The unique set of requesters involved in all instances of the error
    pub requesters: HashSet<RequestedBy>,
    /// The requesters from the first occurrance of the error
    pub first_example: Vec<RequestedBy>,
}

/// Frequency counter for solver internal errors
#[derive(Debug, Clone)]
pub struct ErrorFreq {
    pub counter: u64,
    /// Extra data for 'could not satisfy' error messages
    pub record: Option<CouldNotSatisfyRecord>,
}

impl ErrorFreq {
    /// Given the key under which the this error is stored in the
    /// solver, generate a combined message for original error.
    pub fn get_message(&self, error_key: String) -> String {
        match &self.record {
            Some(r) => {
                // The requesters from the first_example data help
                // reconstruct the first occurrence of the error to use
                // as a base for the combined message
                let mut message = format!(
                    "could not satisfy '{error_key}' as required by: {}",
                    r.first_example
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(", ")
                );

                // A count of any other requesters involved in later
                // occurrences needs to be added to the combined
                // message to show the volume of requesters involved
                // in this particular "could not satisfy" error.
                let num_others = r
                    .requesters
                    .iter()
                    .filter(|&reqby| !r.first_example.contains(reqby))
                    .count();
                if num_others > 0 {
                    let plural = if num_others > 1 { "others" } else { "other" };
                    message = format!("{message} and {num_others} {plural}");
                }
                message
            }
            // Errors stored with no additional data use their full
            // error message as their error_key, so that can be
            // returned as the "combined" message.
            None => error_key,
        }
    }
}

impl Solver {
    /// Add a request to this solver.
    pub fn add_request(&mut self, request: Request) {
        let request = match request {
            Request::Pkg(mut request) => {
                if request.pkg.components.is_empty() {
                    if request.pkg.is_source() {
                        request.pkg.components.insert(Component::Source);
                    } else {
                        request.pkg.components.insert(Component::default_for_run());
                    }
                }
                Change::RequestPackage(RequestPackage::new(request))
            }
            Request::Var(request) => Change::RequestVar(RequestVar::new(request)),
        };
        self.initial_state_builders.push(request);
    }

    /// Add a repository where the solver can get packages.
    pub fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>,
    {
        self.repos.push(repo.into());
    }

    /// Return a reference to the solver's list of repositories.
    pub fn repositories(&self) -> &Vec<Arc<RepositoryHandle>> {
        &self.repos
    }

    pub fn get_initial_state(&self) -> Arc<State> {
        let mut state = None;
        let base = State::default_state();
        for change in self.initial_state_builders.iter() {
            state = Some(change.apply(&base, state.as_ref().unwrap_or(&base)));
        }
        state.unwrap_or(base)
    }

    /// Increment the number of occurrences of the given error message
    pub(crate) fn increment_error_count(&mut self, error_message: ErrorDetails) {
        match error_message {
            ErrorDetails::Message(message) => {
                // Store errors that are just a message as a count
                // only, using the full message as the key because
                // there are no other distinguishing details.
                let counter = self.error_frequency.entry(message).or_insert(ErrorFreq {
                    counter: 0,
                    record: None,
                });
                counter.counter += 1;
            }
            ErrorDetails::CouldNotSatisfy(request_string, requesters) => {
                // Store counts of "could not satisfy" errors keyed by
                // their request_strings, and keep information on each
                // request_string to summarise these errors later: the
                // requesters from the first occuring example of the
                // request_string error, and a set of all the requesters
                // so they can be examined once the solve has finished.
                let counter = self
                    .error_frequency
                    .entry(request_string)
                    .or_insert(ErrorFreq {
                        counter: 0,
                        record: Some(CouldNotSatisfyRecord {
                            requesters: HashSet::new(),
                            first_example: requesters.clone(),
                        }),
                    });
                counter.counter += 1;
                for requester in requesters {
                    counter
                        .record
                        .as_mut()
                        .unwrap()
                        .requesters
                        .insert(requester);
                }
            }
        }
    }

    /// Get the error to frequency mapping
    pub fn error_frequency(&self) -> &HashMap<String, ErrorFreq> {
        &self.error_frequency
    }

    /// Increment the number of occurrences of the given error message
    pub fn increment_problem_package_count(&mut self, problem_package: String) {
        let counter = self.problem_packages.entry(problem_package).or_insert(0);
        *counter += 1;
    }

    /// Get the problem packages frequency mapping
    pub fn problem_packages(&self) -> &HashMap<String, u64> {
        &self.problem_packages
    }

    /// Get the impossible requests checker
    pub fn request_validator(&self) -> &ImpossibleRequestsChecker {
        &self.request_validator
    }

    async fn get_iterator(
        &self,
        node: &mut Arc<Node>,
        package_name: &PkgName,
    ) -> Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>> {
        if let Some(iterator) = node.get_iterator(package_name) {
            return iterator;
        }
        let iterator = self.make_iterator(package_name.to_owned());
        Arc::make_mut(node)
            .set_iterator(package_name.to_owned(), &iterator)
            .await;
        iterator
    }

    fn make_iterator(
        &self,
        package_name: PkgNameBuf,
    ) -> Arc<tokio::sync::Mutex<Box<dyn PackageIterator + Send>>> {
        debug_assert!(!self.repos.is_empty());
        Arc::new(tokio::sync::Mutex::new(Box::new(
            RepositoryPackageIterator::new(package_name, self.repos.clone()),
        )))
    }

    /// Resolve the build environment, and generate a build for
    /// the given recipe and state.
    ///
    /// The returned spec describes the package that should be built
    /// in order to satisfy the set of requests in the provided state.
    /// The build environment for the package is resolved in order to
    /// validate that a build is possible and to generate the resulting
    /// spec.
    #[async_recursion::async_recursion]
    async fn resolve_new_build(&self, recipe: &SpecRecipe, state: &State) -> Result<Arc<Spec>> {
        let mut opts = state.get_option_map().clone();
        for pkg_request in state.get_pkg_requests() {
            if !opts.contains_key(pkg_request.pkg.name.as_opt_name()) {
                opts.insert(
                    pkg_request.pkg.name.as_opt_name().to_owned(),
                    pkg_request.pkg.version.to_string(),
                );
            }
        }
        for var_request in state.get_var_requests() {
            if !opts.contains_key(&var_request.var) {
                opts.insert(var_request.var.clone(), var_request.value.clone());
            }
        }

        let mut solver = Solver {
            repos: self.repos.clone(),
            ..Default::default()
        };
        solver.update_options(opts.clone());
        let solution = solver.solve_build_environment(recipe).await?;
        recipe
            .generate_binary_build(&opts, &solution)
            .map_err(|err| err.into())
            .map(Arc::new)
    }

    /// Check all the builds to see which ones would generate an
    /// impossible request when combined with the unresolved
    /// requests. Returns a map of builds that do generate impossible
    /// requests and the reasons they are impossible.
    async fn check_builds_for_impossible_requests(
        &self,
        unresolved: &HashMap<PkgNameBuf, PkgRequest>,
        builds: Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>,
    ) -> Result<HashMap<BuildIdent, Compatibility>> {
        let mut builds_with_impossible_requests: HashMap<BuildIdent, Compatibility> =
            HashMap::new();

        let builds_lock = builds.lock().await;
        let mut builds_copy = dyn_clone::clone_box(&*builds_lock);
        drop(builds_lock);

        let mut tasks = FuturesUnordered::new();
        while let Some(repos_builds) = builds_copy.next().await? {
            for (_key, data) in repos_builds.iter() {
                let task_build_spec = data.0.clone();
                let task_checker = self.request_validator.clone();
                let task_repos = self.repos.clone();
                let task_unresolved = unresolved.clone();

                let task = async move {
                    match task_checker
                        .validate_pkg_requests(&task_build_spec, &task_unresolved, &task_repos)
                        .await
                    {
                        Ok(compat) => Ok((task_build_spec.ident().clone(), compat)),
                        Err(err) => Err(err),
                    }
                };
                // Launch the new task in another thread
                tasks.push(tokio::spawn(task));
            }
        }

        // This doesn't set up any task message channels because the
        // results of all tasks are needed before this function can
        // return (does each build make an impossible request or not).
        while let Some(task_result) = tasks.next().await {
            let result = match task_result {
                Ok(r) => r,
                Err(err) => {
                    return Err(crate::Error::String(err.to_string()));
                }
            };
            let (build_id, compat) = match result {
                Ok((b, c)) => (b, c),
                Err(err) => {
                    return Err(crate::Error::String(err.to_string()));
                }
            };

            // Only builds that generate any impossible requests are
            // recorded and returned
            if !compat.is_ok() {
                builds_with_impossible_requests.insert(build_id, compat);
            }
        }

        Ok(builds_with_impossible_requests)
    }

    /// Check the given builds install requirements for requests that
    /// would be impossible when combined with the given unresolved
    /// requests.
    async fn check_requirements_for_impossible_requests(
        &self,
        spec: &Spec,
        unresolved: &HashMap<PkgNameBuf, PkgRequest>,
    ) -> Result<Compatibility> {
        self.request_validator
            .validate_pkg_requests(spec, unresolved, &self.repos)
            .await
            .map_err(Error::ValidationError)
    }

    async fn step_state(&mut self, node: &mut Arc<Node>) -> Result<Option<Decision>> {
        let mut notes = Vec::<Note>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            // May have a valid solution, but verify that all embedded packages
            // that are part of the solve also have their source packages
            // present in the solve.
            let mut non_embeds = HashSet::new();
            let mut embeds = HashMap::new();
            for (spec, _) in node.state.get_resolved_packages().values() {
                match spec.ident().build() {
                    Build::Embedded(EmbeddedSource::Package(package)) => {
                        let ident: BuildIdent = (&package.ident).try_into()?;
                        embeds.insert(ident, spec.ident().clone());
                    }
                    _ => {
                        non_embeds.insert(spec.ident().clone());
                    }
                }
            }
            let embeds_set: HashSet<BuildIdent> = embeds.keys().cloned().collect();
            let mut difference = embeds_set.difference(&non_embeds);
            if let Some(missing_embed_provider) = difference.next() {
                // This is an invalid solve!
                // Safety: All members of `difference` must also exist in `embeds`.
                let unprovided_embedded =
                    unsafe { embeds.get(missing_embed_provider).unwrap_unchecked() };

                notes.push(Note::Other(format!("Embedded package {unprovided_embedded} missing its provider {missing_embed_provider}")));
                return Err(Error::OutOfOptions(OutOfOptions {
                    request: PkgRequest::new(
                        missing_embed_provider.clone().into(),
                        RequestedBy::PackageBuild(unprovided_embedded.clone()),
                    ),
                    notes,
                }));
            }
            return Ok(None);
        };

        // This is a step forward in the solve
        self.number_of_steps += 1;

        let iterator = self.get_iterator(node, &request.pkg.name).await;
        let mut iterator_lock = iterator.lock().await;
        loop {
            let (pkg, builds) = match iterator_lock.next().await {
                Ok(Some((pkg, builds))) => (pkg, builds),
                Ok(None) => break,
                Err(spk_solve_package_iterator::Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(_),
                )) => {
                    // Intercept this error in this situation to
                    // capture the request for the package that turned
                    // out to be missing.
                    return Err(spk_solve_graph::Error::PackageNotFoundDuringSolve(
                        request.clone(),
                    )
                    .into());
                }
                Err(e) => return Err(e.into()),
            };

            let mut compat = request.is_version_applicable(pkg.version());
            if !&compat {
                // Count this version and its builds as incompatible
                self.number_incompat_versions += 1;
                self.number_incompat_builds += builds.lock().await.len();

                // Skip this version and move on the to next one
                iterator_lock.set_builds(
                    pkg.version(),
                    Arc::new(tokio::sync::Mutex::new(EmptyBuildIterator::new())),
                );
                notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                    pkg.clone(),
                    compat,
                )));
                continue;
            }

            let builds = if !builds.lock().await.is_sorted_build_iterator() {
                // TODO: this could be a HashSet if build key generation
                // only looks at the idents in the hashmap.
                let builds_with_impossible_requests = if self.impossible_checks.use_in_build_keys {
                    let impossible_check_start = Instant::now();
                    let start_number = self.request_validator.num_build_specs_read();
                    let unresolved = node.state.get_unresolved_requests();

                    let problematic_builds = self
                        .check_builds_for_impossible_requests(unresolved, builds.clone())
                        .await?;

                    tracing::debug!(
                        target: IMPOSSIBLE_CHECKS_TARGET,
                        "Impossible request checks for build sorting took: {} secs ({} specs read)",
                        impossible_check_start.elapsed().as_secs_f64(),
                        self.request_validator.num_build_specs_read() - start_number,
                    );
                    problematic_builds
                } else {
                    // An empty map means none of the builds should be
                    // treated as if they generate an impossible request.
                    HashMap::new()
                };

                let builds = Arc::new(tokio::sync::Mutex::new(
                    SortedBuildIterator::new(
                        node.state.get_option_map().clone(),
                        builds.clone(),
                        builds_with_impossible_requests,
                    )
                    .await?,
                ));
                iterator_lock.set_builds(pkg.version(), builds.clone());
                builds
            } else {
                builds
            };

            while let Some(hm) = builds.lock().await.next().await? {
                // Now add this build to the total considered during
                // this overall step

                self.number_total_builds += 1;

                // Try all the hash map values to check all repos.
                for (spec, source) in hm.values() {
                    let spec = Arc::clone(spec);
                    let build_from_source =
                        spec.ident().is_source() && request.pkg.build != Some(Build::Source);

                    let mut decision = if !build_from_source {
                        compat = self.validate_package(&node.state, &spec, source)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                                Package::ident(&spec).to_any(),
                                compat.clone(),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }

                        if self.impossible_checks.check_before_resolving {
                            // The unresolved requests from the state
                            // are used to check the new requests this
                            // build would add, if it was used to
                            // resolve the current request.
                            let unresolved = node.state.get_unresolved_requests();
                            let compat = self
                                .check_requirements_for_impossible_requests(&spec, unresolved)
                                .await?;
                            if !compat.is_ok() {
                                // This build would add an impossible requst,
                                // which is a bad choice for any solve, so
                                // discard this build and try another.
                                notes.push(Note::SkipPackageNote(SkipPackageNote::new(
                                    spec.ident().to_any(),
                                    compat,
                                )));
                                self.number_builds_skipped += 1;
                                continue;
                            };
                        }

                        // This build has passed all the checks and
                        // can be used to resolve the current request
                        Decision::builder(&node.state)
                            .with_components(&request.pkg.components)
                            .resolve_package(&spec, source.clone())
                    } else {
                        if let PackageSource::Embedded = source {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                                spec.ident().to_any(),
                                &compat,
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }
                        let recipe = match source.read_recipe(spec.ident().base()).await {
                            Ok(r) if r.is_deprecated() => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        pkg.clone(),
                                        "cannot build from source, version is deprecated",
                                    ),
                                ));
                                continue;
                            }
                            Ok(r) => r,
                            Err(spk_solve_solution::Error::SpkStorageError(
                                spk_storage::Error::SpkValidatorsError(
                                    spk_schema::validators::Error::PackageNotFoundError(pkg),
                                ),
                            )) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        pkg,
                                        "cannot build from source, recipe not available",
                                    ),
                                ));
                                continue;
                            }
                            Err(err) => return Err(err.into()),
                        };
                        compat = self.validate_recipe(&node.state, &recipe)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                                spec.ident().to_any(),
                                format!("recipe is not valid: {compat}"),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }

                        let new_spec = match self.resolve_new_build(&recipe, &node.state).await {
                            Err(err) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        spec.ident().to_any(),
                                        format!("cannot resolve build env: {err}"),
                                    ),
                                ));
                                self.number_builds_skipped += 1;
                                continue;
                            }
                            res => res?,
                        };
                        let new_source = PackageSource::BuildFromSource {
                            recipe: Arc::clone(&recipe),
                        };

                        compat = self.validate_package(&node.state, &new_spec, &new_source)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(SkipPackageNote::new_from_message(
                                spec.ident().to_any(),
                                format!("built package would still be invalid: {compat}"),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }

                        match Decision::builder(&node.state)
                            .with_components(&request.pkg.components)
                            .build_package(&recipe, &new_spec)
                        {
                            Ok(decision) => decision,
                            Err(err) => {
                                notes.push(Note::SkipPackageNote(
                                    SkipPackageNote::new_from_message(
                                        spec.ident().to_any(),
                                        format!("cannot build package: {err}"),
                                    ),
                                ));
                                self.number_builds_skipped += 1;
                                continue;
                            }
                        }
                    };

                    decision.add_notes(notes.iter().cloned());
                    return Ok(Some(decision));
                }
            }
        }

        Err(error::Error::OutOfOptions(error::OutOfOptions {
            request,
            notes,
        }))
    }

    fn validate_recipe<R: Recipe>(&self, node: &State, recipe: &R) -> Result<Compatibility> {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_recipe(node, recipe)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_package<P: Package>(
        &self,
        node: &State,
        spec: &P,
        source: &PackageSource,
    ) -> Result<Compatibility>
    where
        P: Package + Satisfy<PkgRequest> + Satisfy<VarRequest>,
    {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_package(node, spec, source)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(Compatibility::Compatible)
    }

    /// Checks the initial requests for impossible requests, warning
    /// the user about each one that is impossible, and error if any
    /// of them are impossible.
    async fn check_initial_requests_for_impossible_requests(
        &mut self,
        initial_state: &State,
    ) -> Result<()> {
        let mut impossible_request_count = 0;

        let tasks = FuturesUnordered::new();
        let mut requests = Vec::new();

        let initial_requests = initial_state.get_unresolved_requests();
        for (count, req) in initial_requests.values().enumerate() {
            // For the warning messages later, in the following loop
            requests.push(req.pkg.clone());

            // Have to make a dummy spec for an "initialrequest"
            // package to interact with the request_validator's
            // interface. It expects a package with install
            // requirements (i.e. the 'req' being checked here).
            //
            // The count is used as a fake version number to
            // distinquish which initial request is being checked in
            // each dummy package.
            let dummy_spec = make_build!({"pkg": format!("initialrequest/{}", count + 1),
                                          "install": {
                                              "requirements": [
                                                  req
                                              ]
                                          }
            });

            // All the initial_requests are passed in as well to
            // handle situations when same package is requested
            // multiple times in the initial requests.
            let task_checker = self.request_validator.clone();
            let task_repos = self.repos.clone();
            let task = async move {
                task_checker
                    .validate_pkg_requests(&dummy_spec, initial_requests, &task_repos)
                    .await
            };

            // The tasks are run concurrenly via async, in the same thread.
            tasks.push(task);
        }

        // Only once all the tasks are finished, the user is warned
        // about the impossible initial request, if there are any.
        let results: Vec<_> = tasks.collect().await;
        for (index, result) in results.into_iter().enumerate() {
            let compat = match result {
                Ok(c) => c,
                Err(err) => {
                    return Err(crate::Error::String(err.to_string()));
                }
            };
            if !compat.is_ok() {
                tracing::warn!(
                    "Impossible initial request, no builds in the repos [{}] satisfy: {}",
                    self.repos
                        .iter()
                        .map(|r| format!("{}", r.name()))
                        .collect::<Vec<String>>()
                        .join(", "),
                    requests[index]
                );
                impossible_request_count += 1;
            }
        }

        // Error if any initial request is impossible
        if impossible_request_count > 0 {
            return Err(Error::InitialRequestsContainImpossibleError(
                impossible_request_count,
            ));
        }

        Ok(())
    }

    /// Put this solver back into its default state
    pub fn reset(&mut self) {
        self.repos.truncate(0);
        self.initial_state_builders.truncate(0);
        self.validators = Cow::from(default_validators());
        (*self.request_validator).reset();

        self.number_of_steps = 0;
        self.number_builds_skipped = 0;
        self.number_incompat_versions = 0;
        self.number_incompat_builds = 0;
        self.number_total_builds = 0;
        self.number_of_steps_back.store(0, Ordering::SeqCst);
        self.error_frequency.clear();
        self.problem_packages.clear();
    }

    /// Run this solver
    pub fn run(&self) -> SolverRuntime {
        SolverRuntime::new(self.clone())
    }

    /// If true, only solve pre-built binary packages.
    ///
    /// When false, the solver may return packages where the build is not set.
    /// These packages are known to have a source package available, and the requested
    /// options are valid for a new build of that source package.
    /// These packages are not actually built as part of the solver process but their
    /// build environments are fully resolved and dependencies included
    pub fn set_binary_only(&mut self, binary_only: bool) {
        self.request_validator.set_binary_only(binary_only);

        let has_binary_only = self
            .validators
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
            // Add BinaryOnly validator because it was missing.
            self.validators
                .to_mut()
                .insert(0, Validators::BinaryOnly(BinaryOnlyValidator {}))
        } else {
            // Remove all BinaryOnly validators because one was found.
            self.validators = take(self.validators.to_mut())
                .into_iter()
                .filter(|v| !matches!(v, Validators::BinaryOnly(_)))
                .collect();
        }
    }

    /// Enable or disable running impossible checks on the initial requests
    /// before the solve starts
    pub fn set_initial_request_impossible_checks(&mut self, enabled: bool) {
        self.impossible_checks.check_initial_requests = enabled;
    }

    /// Enable or disable running impossible checks before using a build to
    /// resolve a request
    pub fn set_resolve_validation_impossible_checks(&mut self, enabled: bool) {
        self.impossible_checks.check_before_resolving = enabled;
    }

    /// Enable or disable running impossible checks for build key generation
    /// when ordering builds for selection
    pub fn set_build_key_impossible_checks(&mut self, enabled: bool) {
        self.impossible_checks.use_in_build_keys = enabled;
    }

    /// Return true is any of the impossible request checks are
    /// enabled for this solver, otherwise false
    pub fn any_impossible_checks_enabled(&self) -> bool {
        self.impossible_checks.check_initial_requests
            || self.impossible_checks.check_before_resolving
            || self.impossible_checks.use_in_build_keys
    }

    pub async fn solve(&mut self) -> Result<Solution> {
        let mut runtime = self.run();
        {
            let iter = runtime.iter();
            tokio::pin!(iter);
            while let Some(_step) = iter.try_next().await? {}
        }
        runtime.current_solution().await
    }

    /// Adds requests for all build requirements
    pub fn configure_for_build_environment<T: Recipe>(&mut self, recipe: &T) -> Result<()> {
        let state = self.get_initial_state();

        let build_options = recipe.resolve_options(state.get_option_map())?;
        for req in recipe.get_build_requirements(&build_options)? {
            self.add_request(req)
        }

        Ok(())
    }

    /// Adds requests for all build requirements and solves
    pub async fn solve_build_environment(&mut self, recipe: &SpecRecipe) -> Result<Solution> {
        self.configure_for_build_environment(recipe)?;
        self.solve().await
    }

    pub fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Change::SetOptions(SetOptions::new(options)))
    }

    /// Get the number of steps (forward) taken in the solve
    pub fn get_number_of_steps(&self) -> usize {
        self.number_of_steps
    }

    /// Get the number of builds skipped during the solve
    pub fn get_number_of_builds_skipped(&self) -> usize {
        self.number_builds_skipped
    }

    /// Get the number of incompatible versions avoided during the solve
    pub fn get_number_of_incompatible_versions(&self) -> usize {
        self.number_incompat_versions
    }

    /// Get the number of incompatible builds avoided during the solve
    pub fn get_number_of_incompatible_builds(&self) -> usize {
        self.number_incompat_builds
    }

    /// Get the total number of builds examined during the solve
    pub fn get_total_builds(&self) -> usize {
        self.number_total_builds
    }

    /// Get the number of steps back taken during the solve
    pub fn get_number_of_steps_back(&self) -> u64 {
        self.number_of_steps_back.load(Ordering::SeqCst)
    }
}

// This is needed so `PriorityQueue` doesn't need to hash the node itself.
struct NodeWrapper {
    pub(crate) node: Arc<tokio::sync::RwLock<Arc<Node>>>,
    pub(crate) hash: u64,
}

impl std::cmp::Eq for NodeWrapper {}

impl std::cmp::PartialEq for NodeWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl std::hash::Hash for NodeWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

type SolverHistory = PriorityQueue<NodeWrapper, std::cmp::Reverse<u64>>;

#[must_use = "The solver runtime does nothing unless iterated to completion"]
pub struct SolverRuntime {
    pub solver: Solver,
    graph: Arc<tokio::sync::RwLock<Graph>>,
    history: SolverHistory,
    current_node: Option<Arc<tokio::sync::RwLock<Arc<Node>>>>,
    decision: Option<Arc<Decision>>,
}

impl SolverRuntime {
    pub fn new(solver: Solver) -> Self {
        let initial_decision = Decision::new(solver.initial_state_builders.clone());
        Self {
            solver,
            graph: Arc::new(tokio::sync::RwLock::new(Graph::new())),
            history: SolverHistory::default(),
            current_node: None,
            decision: Some(Arc::new(initial_decision)),
        }
    }

    /// A reference to the solve graph being built by this runtime
    pub fn graph(&self) -> Arc<tokio::sync::RwLock<Graph>> {
        self.graph.clone()
    }

    /// Returns the completed solution for this runtime.
    ///
    /// If needed, this function will iterate any remaining
    /// steps for the current state.
    pub async fn solution(&mut self) -> Result<Solution> {
        {
            let iter = self.iter();
            tokio::pin!(iter);
            while let Some(_item) = iter.try_next().await? {}
        }
        self.current_solution().await
    }

    /// Return the current solution for this runtime.
    ///
    /// If the runtime has not yet completed, this solution
    /// may be incomplete or empty.
    pub async fn current_solution(&self) -> Result<Solution> {
        let current_node = self
            .current_node
            .as_ref()
            .ok_or_else(|| Error::String("Solver runtime has not been consumed".into()))?;
        let current_node_lock = current_node.read().await;

        let is_dead = current_node_lock.state.id()
            == self.graph.read().await.root.read().await.state.id()
            || Arc::ptr_eq(&current_node_lock.state, &DEAD_STATE);
        let is_empty = self
            .solver
            .get_initial_state()
            .get_pkg_requests()
            .is_empty();
        if is_dead && !is_empty {
            Err(spk_solve_graph::Error::FailedToResolve((*self.graph).read().await.clone()).into())
        } else {
            current_node_lock
                .state
                .as_solution()
                // TODO: make a proper conversion for this
                .map_err(|e| crate::Error::String(e.to_string()))
        }
    }

    // TODO: turn this into an instance method, and rework the
    // borrowing in next() to allow fewer parameters to be passed into
    // this method.
    /// Generate step-back decision from a node history
    async fn take_a_step_back(
        history: &mut SolverHistory,
        decision: &mut Option<Arc<Decision>>,
        solver: &Solver,
        message: &String,
    ) {
        // After encountering a solver error, start trying a new path from the
        // oldest fork. Experimentation shows that this is able to discover
        // a valid solution must faster than going back to the newest fork,
        // for problem cases that get stuck in a bad path.
        match history.pop() {
            Some((n, _)) => {
                let n_lock = n.node.read().await;
                *decision = Some(Arc::new(
                    Change::StepBack(StepBack::new(
                        message,
                        &n_lock.state,
                        Arc::clone(&solver.number_of_steps_back),
                    ))
                    .as_decision(),
                ))
            }
            None => {
                *decision = Some(Arc::new(
                    Change::StepBack(StepBack::new(
                        message,
                        &DEAD_STATE,
                        Arc::clone(&solver.number_of_steps_back),
                    ))
                    .as_decision(),
                ))
            }
        }
    }

    /// Iterate through each step of this runtime, trying to converge on a solution
    pub fn iter(&mut self) -> impl Stream<Item = Result<(Arc<Node>, Arc<Decision>)>> + Send + '_ {
        stream! {
            let mut first_iter = true;
            'outer: loop {
                if self.decision.is_none()
                    || (self.current_node.is_some()
                        && {
                            if let Some(n) = self.current_node.as_ref() {
                                Arc::ptr_eq(&n.read().await.state, &DEAD_STATE)
                            }
                            else {
                                false
                            }
                        })
                {
                    break 'outer;
                }

                let to_yield = (
                    // A clone of Some(current_node) or the root node
                    {
                        if let Some(n) = self.current_node.as_ref() {
                            n.read().await.clone()
                        }
                        else {
                            self.graph.read().await.root.read().await.clone()
                        }
                    },
                    self.decision.as_ref().expect("decision is some").clone(),
                );

                self.current_node = Some({
                    let mut sg = self.graph.write().await;
                    let root_id = sg.root.read().await.id();
                    match sg.add_branch({
                        if let Some(n) = self.current_node.as_ref() {
                            n.read().await.id()
                        }
                        else {
                            root_id
                        }},
                        self.decision.take().unwrap(),
                    ).await {
                        Ok(cn) => cn,
                        Err(err) => {
                            SolverRuntime::take_a_step_back(
                                &mut self.history,
                                &mut self.decision,
                                &self.solver,
                                &err.to_string(),
                            ).await;
                            yield Ok(to_yield);
                            continue 'outer;
                        }
                    }
                });
                let current_node = self
                    .current_node
                    .as_ref()
                    .expect("current_node always `is_some` here");
                let mut current_node_lock = current_node.write().await;
                let current_level = current_node_lock.state.state_depth;

                if first_iter {
                    // Check for impossible requests only the first
                    // time this is reached. The current node will
                    // have the initial state and the initial requests.
                    first_iter = false;
                    if self.solver.impossible_checks.check_initial_requests {
                        if let Err(err) = self.solver.check_initial_requests_for_impossible_requests(&current_node_lock.state).await {
                            let cause = format!("{err}");
                            self.solver.increment_error_count(ErrorDetails::Message(cause));
                            yield Err(err);
                            continue 'outer;
                        }
                    }
                }

                self.decision = match self.solver.step_state(&mut current_node_lock).await
                {
                    Ok(decision) => decision.map(Arc::new),
                    Err(crate::Error::OutOfOptions(ref err)) => {
                        // Add to problem package counts based on what made
                        // the request for the blocked package.
                        let requested_by = err.request.get_requesters();
                        for req in &requested_by {
                            if let RequestedBy::PackageBuild(problem_package) = req {
                                self.solver
                                    .increment_problem_package_count(problem_package.name().to_string())
                            }
                        }

                        // Add the requirers to the output so where the
                        // requests came from is more visible to the user.
                        let requirers: Vec<String> = requested_by.iter().map(ToString::to_string).collect();
                        let cause = format!(
                            "could not satisfy '{}' as required by: {}",
                            err.request.pkg,
                            requirers.join(", ")
                        );

                        SolverRuntime::take_a_step_back(
                            &mut self.history,
                            &mut self.decision,
                            &self.solver,
                            &cause,
                        ).await;

                        self.solver.increment_error_count(ErrorDetails::CouldNotSatisfy(err.request.pkg.to_string(), requested_by));

                        if let Some(d) = self.decision.as_mut() {
                            Arc::make_mut(d).add_notes(err.notes.iter().cloned())
                        }
                        yield Ok(to_yield);
                        continue 'outer;
                    }
                    Err(Error::GraphError(spk_solve_graph::Error::PackageNotFoundDuringSolve(err_req))) => {
                        let requested_by = err_req.get_requesters();
                        for req in &requested_by {
                            // Can't recover from a command line request for a
                            // missing package.
                            if let RequestedBy::CommandLine = req {
                                yield Err(Error::GraphError(spk_solve_graph::Error::PackageNotFoundDuringSolve(err_req)));
                                continue 'outer;
                            }

                            // Add to problem package counts based on what
                            // made the request for the blocked package.
                            if let RequestedBy::PackageBuild(problem_package) = req {
                                self.solver
                                    .increment_problem_package_count(problem_package.name().to_string())
                            };
                        }

                        let requirers: Vec<String> = requested_by.iter().map(ToString::to_string).collect();
                        let cause = format!("Package '{}' not found during the solve as required by: {}. Please check the spelling of the package's name", err_req.pkg, requirers.join(", "));

                        SolverRuntime::take_a_step_back(
                            &mut self.history,
                            &mut self.decision,
                            &self.solver,
                            &cause,
                        ).await;

                        self.solver.increment_error_count(ErrorDetails::Message(cause));

                        // This doesn't halt the solve because the missing
                        // package might only have been requested by one
                        // build. It may not be required in the next build the
                        // solver looks at. However, this kind of error
                        // usually occurs either because of command line
                        // request that was mistyped, or because a requirement
                        // was misspelt in a yaml file that was given on a
                        // command line. So the solver is likely to hit a dead
                        // end and halt fairly soon.
                        yield Ok(to_yield);
                        continue 'outer;
                    }
                    Err(err) => {
                        let cause = format!("{}", err);
                        self.solver.increment_error_count(ErrorDetails::Message(cause));
                        yield Err(err);
                        continue 'outer;
                    }
                };
                self.history.push(
                    NodeWrapper {
                        node: current_node.clone(),
                        // I tried reversing the order of the hash here and it
                        // produced the same result.
                        hash: self.solver.get_number_of_steps() as u64,
                    },
                    std::cmp::Reverse(current_level),
                );
                yield Ok(to_yield)
            }
        }
    }
}
