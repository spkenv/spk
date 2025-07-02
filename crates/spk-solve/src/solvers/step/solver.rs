// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::mem::take;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
use spk_schema::version::{ComponentsMissingProblem, IncompatibleReason, IsSameReasonAs};
use spk_schema::{BuildIdent, Deprecate, Package, Recipe, Spec, SpecRecipe, try_recipe};
use spk_solve_graph::{
    Change,
    DEAD_STATE,
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
};
use spk_solve_package_iterator::{
    BuildIterator,
    EmptyBuildIterator,
    PackageIterator,
    RepositoryPackageIterator,
    SortedBuildIterator,
};
use spk_solve_solution::{PackageSource, Solution};
use spk_solve_validation::validators::BinaryOnlyValidator;
use spk_solve_validation::{
    IMPOSSIBLE_CHECKS_TARGET,
    ImpossibleRequestsChecker,
    ValidatorT,
    Validators,
    default_validators,
};
use spk_storage::RepositoryHandle;

use crate::error::{self, OutOfOptions};
use crate::option_map::OptionMap;
use crate::solver::Solver as SolverTrait;
use crate::{DecisionFormatter, Error, Result, SolverExt, SolverMut};

/// Structure to hold whether the three kinds of impossible checks are
/// enabled or disabled in a solver.
#[derive(Clone)]
struct ImpossibleChecksSettings {
    pub check_initial_requests: bool,
    pub check_before_resolving: bool,
    pub use_in_build_keys: bool,
}

impl Default for ImpossibleChecksSettings {
    fn default() -> Self {
        if let Ok(config) = spk_config::get_config() {
            Self {
                check_before_resolving: config.solver.check_impossible_validation,
                check_initial_requests: config.solver.check_impossible_initial,
                use_in_build_keys: config.solver.check_impossible_builds,
            }
        } else {
            Self {
                check_before_resolving: false,
                check_initial_requests: false,
                use_in_build_keys: false,
            }
        }
    }
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
    /// The requesters from the first occurrence of the error
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
                // request_string to summarize these errors later: the
                // requesters from the first occurring example of the
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
                opts.insert(
                    var_request.var.clone(),
                    var_request
                        .value
                        .as_pinned()
                        .unwrap_or_default()
                        .to_string(),
                );
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

    /// Default behavior for skipping an incompatible build.
    fn skip_build(&mut self, notes: &mut Vec<Note>, spec: &Spec, compat: &Compatibility) {
        notes.push(Note::SkipPackageNote(Box::new(SkipPackageNote::new(
            spec.ident().to_any_ident(),
            compat.clone(),
        ))));
        self.number_builds_skipped += 1;
    }

    async fn step_state(
        &mut self,
        graph: &Arc<tokio::sync::RwLock<Graph>>,
        node: &mut Arc<Node>,
    ) -> Result<Option<Decision>> {
        let mut notes = Vec::<Note>::new();
        let request = if let Some(request) = node.state.get_next_request()? {
            request
        } else {
            // May have a valid solution, but verify that all embedded packages
            // that are part of the solve also have their source packages
            // present in the solve.
            let mut non_embeds = HashSet::new();
            let mut embeds = HashMap::new();
            for (spec, _, _) in node.state.get_resolved_packages().values() {
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
                return Err(Error::OutOfOptions(Box::new(OutOfOptions {
                    request: PkgRequest::new(
                        missing_embed_provider.clone().into(),
                        RequestedBy::PackageBuild(unprovided_embedded.clone()),
                    ),
                    notes,
                })));
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
                Err(spk_solve_package_iterator::Error::SpkStorageError(
                    spk_storage::Error::PackageNotFound(_),
                )) => {
                    // Intercept this error in this situation to
                    // capture the request for the package that turned
                    // out to be missing.
                    return Err(spk_solve_graph::Error::PackageNotFoundDuringSolve(Box::new(
                        request.clone(),
                    ))
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
                notes.push(Note::SkipPackageNote(Box::new(SkipPackageNote::new(
                    pkg.clone(),
                    compat,
                ))));
                continue;
            }

            let builds: Arc<tokio::sync::Mutex<dyn BuildIterator + Send>> = if !builds
                .lock()
                .await
                .is_sorted_build_iterator()
            {
                // TODO: this could be a HashSet if build key generation
                // only looks at the idents in the hashmap.
                let builds_with_impossible_requests = if self.impossible_checks.use_in_build_keys {
                    let impossible_check_start = Instant::now();
                    let start_number = self.request_validator.num_build_specs_read();
                    let unresolved = node.state.get_unresolved_requests()?;

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
                        match self.validate_package(&node.state, &spec, source)? {
                            Compatibility::Compatible => {
                                if self.impossible_checks.check_before_resolving {
                                    // The unresolved requests from the state
                                    // are used to check the new requests this
                                    // build would add, if it was used to
                                    // resolve the current request.
                                    let unresolved = node.state.get_unresolved_requests()?;
                                    let compat = self
                                        .check_requirements_for_impossible_requests(
                                            &spec, unresolved,
                                        )
                                        .await?;
                                    if !compat.is_ok() {
                                        // This build would add an impossible request,
                                        // which is a bad choice for any solve, so
                                        // discard this build and try another.
                                        notes.push(Note::SkipPackageNote(Box::new(
                                            SkipPackageNote::new(
                                                spec.ident().to_any_ident(),
                                                compat,
                                            ),
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
                            }
                            Compatibility::Incompatible(
                                IncompatibleReason::ConflictingEmbeddedPackage(
                                    conflicting_package_name,
                                ),
                            ) => {
                                // This build couldn't be used because it
                                // conflicts with an existing package in the
                                // solve. Jump back to before the conflicting
                                // package was added and try adding this
                                // build again.
                                let (conflicting_pkg, conflicting_pkg_source, state_id) = node
                                    .state
                                    .get_current_resolve(&conflicting_package_name)
                                    .map_err(|_| {
                                        Error::String("package not found in resolve".into())
                                    })?;

                                // Is the conflicting package already embedded
                                // by some other package?
                                if conflicting_pkg.ident().is_embedded() {
                                    notes.push(Note::SkipPackageNote(Box::new(SkipPackageNote::new(
                                        spec.ident().to_any_ident(),
                                        Compatibility::Incompatible({
                                            match conflicting_pkg_source {
                                                PackageSource::Embedded { parent, .. } => {
                                                    IncompatibleReason::AlreadyEmbeddedPackage {
                                                        embedded: conflicting_package_name,
                                                        embedded_by: parent.name().to_owned(),
                                                    }
                                                }
                                                _ => {
                                                    // As the solver exhausts
                                                    // all possibilities it
                                                    // eventually tries to
                                                    // resolve an embedded stub
                                                    // for a package that is
                                                    // already in the solve.
                                                    IncompatibleReason::ConflictingEmbeddedPackage(
                                                        conflicting_package_name,
                                                    )
                                                }
                                            }
                                        }),
                                    ))));
                                    self.number_builds_skipped += 1;
                                    continue;
                                }

                                let graph_lock = graph.read().await;

                                let target_state_node = graph_lock
                                    .nodes
                                    .get(&state_id.id())
                                    .ok_or_else(|| Error::String("state not found".into()))?
                                    .read()
                                    .await;

                                Decision::builder(&target_state_node.state).reconsider_package(
                                    request.clone(),
                                    conflicting_package_name.as_ref(),
                                    Arc::clone(&self.number_of_steps_back),
                                )
                            }
                            Compatibility::Incompatible(IncompatibleReason::ComponentsMissing(
                                ComponentsMissingProblem::EmbeddedComponentsNotProvided {
                                    embedder,
                                    embedded,
                                    needed,
                                    ..
                                },
                            )) => {
                                if let Ok((parent_spec, _, state_id)) =
                                    node.state.get_current_resolve(&embedder)
                                {
                                    // This build couldn't be used because it needs
                                    // components from an embedded package that
                                    // have not been provided from the package that
                                    // embeds it. It might be possible to add a
                                    // request for a component from the parent
                                    // package to bring in the needed component.

                                    // Find which component(s) of parent_spec
                                    // embed the missing components.
                                    let mut remaining_missing_components: BTreeSet<Component> =
                                        needed
                                            .0
                                            .iter()
                                            .map(|c| {
                                                // The error will only contain
                                                // stringified components, so
                                                // converting back should not
                                                // fail.
                                                Component::parse(c).expect("valid component")
                                            })
                                            .collect();
                                    let mut components_to_request = BTreeSet::new();
                                    for component in parent_spec.components().iter() {
                                        for embedded_package in component.embedded.iter() {
                                            if embedded_package.pkg.name() != embedded {
                                                continue;
                                            }

                                            let overlapping: Vec<_> = remaining_missing_components
                                                .intersection(embedded_package.components())
                                                .collect();
                                            if overlapping.is_empty() {
                                                continue;
                                            }

                                            components_to_request.insert(component.name.clone());

                                            remaining_missing_components =
                                                remaining_missing_components
                                                    .difference(
                                                        &overlapping.into_iter().cloned().collect(),
                                                    )
                                                    .cloned()
                                                    .collect();

                                            if remaining_missing_components.is_empty() {
                                                break;
                                            }
                                        }

                                        if remaining_missing_components.is_empty() {
                                            break;
                                        }
                                    }

                                    if !remaining_missing_components.is_empty() {
                                        // Couldn't find a way to satisfy all
                                        // the missing components.
                                        self.skip_build(&mut notes, &spec, &compat);
                                        continue;
                                    }

                                    // Add a request for the components of the
                                    // parent package that will provide the
                                    // missing components in the embedded
                                    // package, and retry this build.

                                    let graph_lock = graph.read().await;

                                    let target_state_node = graph_lock
                                        .nodes
                                        .get(&state_id.id())
                                        .ok_or_else(|| Error::String("state not found".into()))?
                                        .read()
                                        .await;

                                    Decision::builder(&target_state_node.state)
                                        .reconsider_package_with_additional_components(
                                            {
                                                let mut pkg_request = PkgRequest::new(
                                                    parent_spec.ident().to_any_ident().into(),
                                                    RequestedBy::PackageBuild(spec.ident().clone()),
                                                );

                                                let existing_requested_components = node
                                                    .state
                                                    .get_merged_request(parent_spec.ident().name())
                                                    .map(|r| r.pkg.components)
                                                    .unwrap_or_default();

                                                pkg_request.pkg.components =
                                                    existing_requested_components
                                                        .union(&components_to_request)
                                                        .cloned()
                                                        .collect();
                                                pkg_request
                                            },
                                            spec.ident().name(),
                                            Arc::clone(&self.number_of_steps_back),
                                        )
                                } else {
                                    self.skip_build(&mut notes, &spec, &compat);
                                    continue;
                                }
                            }
                            compat @ Compatibility::Incompatible(_) => {
                                self.skip_build(&mut notes, &spec, &compat);
                                continue;
                            }
                        }
                    } else {
                        if let PackageSource::Embedded { .. } = source {
                            notes.push(Note::SkipPackageNote(Box::new(
                                SkipPackageNote::new_from_message(
                                    spec.ident().to_any_ident(),
                                    &compat,
                                ),
                            )));
                            self.number_builds_skipped += 1;
                            continue;
                        }
                        let recipe = match source.read_recipe(spec.ident().base()).await {
                            Ok(r) if r.is_deprecated() => {
                                notes.push(Note::SkipPackageNote(Box::new(
                                    SkipPackageNote::new_from_message(
                                        pkg.clone(),
                                        "cannot build from source, version is deprecated",
                                    ),
                                )));
                                continue;
                            }
                            Ok(r) => r,
                            Err(spk_solve_solution::Error::SpkStorageError(
                                spk_storage::Error::PackageNotFound(pkg),
                            )) => {
                                notes.push(Note::SkipPackageNote(Box::new(
                                    SkipPackageNote::new_from_message(
                                        *pkg,
                                        "cannot build from source, recipe not available",
                                    ),
                                )));
                                continue;
                            }
                            Err(err) => return Err(err.into()),
                        };
                        compat = self.validate_recipe(&node.state, &recipe)?;
                        if !&compat {
                            notes.push(Note::SkipPackageNote(Box::new(SkipPackageNote::new_from_message(
                                spec.ident().to_any_ident(),
                                format!("building from source is not possible with this recipe: {compat}"),
                            ))));
                            self.number_builds_skipped += 1;
                            continue;
                        }

                        let new_spec = match self.resolve_new_build(&recipe, &node.state).await {
                            Err(err) => {
                                notes.push(Note::SkipPackageNote(Box::new(
                                    SkipPackageNote::new_from_message(
                                        spec.ident().to_any_ident(),
                                        format!("cannot resolve build env for source build: {err}"),
                                    ),
                                )));
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
                            notes.push(Note::SkipPackageNote(Box::new(
                                SkipPackageNote::new_from_message(
                                    spec.ident().to_any_ident(),
                                    format!("building from source not possible: {compat}"),
                                ),
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
                                notes.push(Note::SkipPackageNote(Box::new(
                                    SkipPackageNote::new_from_message(
                                        spec.ident().to_any_ident(),
                                        format!("cannot build package from source: {err}"),
                                    ),
                                )));
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

        Err(error::Error::OutOfOptions(Box::new(error::OutOfOptions {
            request,
            notes,
        })))
    }

    fn validate_recipe<R: Recipe>(&self, state: &State, recipe: &R) -> Result<Compatibility> {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_recipe(state, recipe)?;
            if !&compat {
                return Ok(compat);
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> Result<Compatibility>
    where
        P: Package + Satisfy<PkgRequest> + Satisfy<VarRequest>,
    {
        for validator in self.validators.as_ref() {
            let compat = validator.validate_package(state, spec, source)?;
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

        let initial_requests = initial_state.get_unresolved_requests()?;
        for (count, req) in initial_requests.values().enumerate() {
            // Have to make a dummy spec for an "initialrequest"
            // package to interact with the request_validator's
            // interface. It expects a package with install
            // requirements (i.e. the 'req' being checked here).
            //
            // The count is used as a fake version number to
            // distinguish which initial request is being checked in
            // each dummy package.
            let recipe = try_recipe!({"pkg": format!("initialrequest/{}", count + 1),
                                      "install": {
                                          "requirements": [
                                              req,
                                          ]
                                      }
            })
            .map_err(|err| {
                Error::String(format!(
                    "Unable to generate dummy spec for initial checks: {err}"
                ))
            })?;

            let solution = Solution::new(OptionMap::default());
            let mut build_opts = OptionMap::default();
            let mut resolved_opts = recipe.resolve_options(&build_opts).unwrap().into_iter();
            build_opts.extend(&mut resolved_opts);
            let dummy_spec = recipe.generate_binary_build(&build_opts, &solution)?;

            // All the initial_requests are passed in as well to
            // handle situations when same package is requested
            // multiple times in the initial requests.
            let task_checker = self.request_validator.clone();
            let task_repos = self.repos.clone();
            let task_req = req.pkg.clone();
            let task = async move {
                let result = task_checker
                    .validate_pkg_requests(&dummy_spec, initial_requests, &task_repos)
                    .await;

                (task_req, result)
            };

            // The tasks are run concurrently via async, in the same thread.
            tasks.push(task);
        }

        // Only once all the tasks are finished, the user is warned
        // about the impossible initial request, if there are any.
        for (checked_req, result) in tasks.collect::<Vec<_>>().await {
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
                    checked_req
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

    /// Run this solver
    pub fn run(&self) -> SolverRuntime {
        SolverRuntime::new(self.clone())
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

    /// Adds requests for all build requirements and solves
    pub async fn solve_build_environment(&mut self, recipe: &SpecRecipe) -> Result<Solution> {
        self.configure_for_build_environment(recipe)?;
        self.solve().await
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

impl SolverTrait for Solver {
    fn get_options(&self) -> Cow<'_, OptionMap> {
        Cow::Owned(self.get_initial_state().get_option_map().clone())
    }

    fn get_pkg_requests(&self) -> Vec<PkgRequest> {
        self.get_initial_state()
            .get_pkg_requests()
            .iter()
            .map(|pkg_request| (***pkg_request).clone())
            .collect()
    }

    fn get_var_requests(&self) -> Vec<VarRequest> {
        self.get_initial_state()
            .get_var_requests()
            .iter()
            .cloned()
            .collect()
    }

    fn repositories(&self) -> &[Arc<RepositoryHandle>] {
        &self.repos
    }
}

#[async_trait::async_trait]
impl SolverMut for Solver {
    fn add_request(&mut self, request: Request) {
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

    fn reset(&mut self) {
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

    async fn run_and_log_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution> {
        let (solution, _graph) = formatter.run_and_log_resolve(self).await?;
        Ok(solution)
    }

    async fn run_and_print_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution> {
        let (solution, _graph) = formatter.run_and_print_resolve(self).await?;
        Ok(solution)
    }

    fn set_binary_only(&mut self, binary_only: bool) {
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

    async fn solve(&mut self) -> Result<Solution> {
        let mut runtime = self.run();
        {
            let iter = runtime.iter();
            tokio::pin!(iter);
            while let Some(_step) = iter.try_next().await? {}
        }
        runtime.current_solution().await
    }

    fn update_options(&mut self, options: OptionMap) {
        self.initial_state_builders
            .push(Change::SetOptions(SetOptions::new(options)))
    }
}

#[async_trait::async_trait]
impl SolverExt for Solver {
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>,
    {
        self.repos.push(repo.into());
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
            current_node_lock.state.as_solution().map_err(Into::into)
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
        // a valid solution much faster than going back to the newest fork,
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

                self.decision = match self.solver.step_state(&self.graph, &mut current_node_lock).await
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
                            'added_notes: {
                                // Condense notes if possible. If all options
                                // were skipped for the same reason, then
                                // replace the individual skip notes with a
                                // single summary note.
                                if let Some(first) = err.notes.first() {
                                    if err.notes.iter().all(|n| {
                                        match (n, first) {
                                            (Note::SkipPackageNote(n), Note::SkipPackageNote(first)) => n.is_same_reason_as(first),
                                            (Note::Other(n), Note::Other(first)) => n == first,
                                            _ => false,
                                        }
                                    }) {
                                        match first {
                                            Note::SkipPackageNote(first) => {
                                                Arc::make_mut(d).add_notes(vec![Note::Other(format!("All options for '{}' were skipped: {}", first.pkg.name(), first.reason))]);
                                            }
                                            Note::Other(first) => {
                                                Arc::make_mut(d).add_notes(vec![Note::Other(format!("All options were skipped: {first}"))]);
                                            }
                                        }
                                        break 'added_notes;
                                    }
                                }

                                Arc::make_mut(d).add_notes(err.notes.iter().cloned());
                            }
                        }
                        yield Ok(to_yield);
                        continue 'outer;
                    }
                    Err(Error::GraphError(graph_error)) if matches!(&*graph_error, spk_solve_graph::Error::PackageNotFoundDuringSolve(_)) => {
                        let spk_solve_graph::Error::PackageNotFoundDuringSolve(err_req) = &*graph_error else {
                            unreachable!()
                        };
                        let requested_by = err_req.get_requesters();
                        for req in &requested_by {
                            // Can't recover from a command line request for a
                            // missing package.
                            if let RequestedBy::CommandLine = req {
                                yield Err(Error::GraphError(graph_error));
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
