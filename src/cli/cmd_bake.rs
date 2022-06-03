// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use serde::Serialize;

use super::{flags, Run};
use spk::{api, solve::PackageSource};

/// Bake an executable environment from a set of requests or the current environment.
#[derive(Args)]
pub struct Bake {
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub requests: flags::Requests,

    // TODO: these three would ideally be mutually exclusive
    #[clap(short, long)]
    pub show_builds: bool,

    #[clap(short, long)]
    pub yaml: bool,

    #[clap(short, long)]
    pub json: bool,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The requests to resolve and bake
    #[clap(name = "REQUESTS")]
    pub requested: Vec<String>,
}

/// Data that can be output for a layer in a bake
#[derive(Serialize)]
struct BakeLayer {
    #[serde(default)]
    spfs_layer: String,
    #[serde(default)]
    spk_package: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    spk_requester: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    spfs_tag: String,
}

const EMPTY_TAG: &str = "";
const UNKNOWN_PACKAGE: &str = "";

impl Run for Bake {
    fn run(&mut self) -> Result<i32> {
        // Get the layer data from either the active runtime, or the
        // requests made on the command line
        let layers = if self.requested.is_empty() {
            self.get_active_runtime_info()?
        } else {
            self.runtime.ensure_active_runtime()?;
            self.get_new_solve_info()?
        };

        // Based on the output options, output the bake info.
        //
        // Note: the show_builds and yaml flags are mutually
        // exclusive.
        if self.show_builds {
            // Show the package builds instead of the layers
            for layer in layers.iter() {
                println!("{}", layer.spk_package);
            }
            return Ok(0);
        }

        if self.yaml {
            // Output the layers data in yaml format for other
            // programs to use
            let data = serde_yaml::to_string(&layers)?;
            println!("{}", data);
            return Ok(0);
        }

        if self.json {
            // Output the layers data in json format for other
            // programs to use
            let data = serde_json::to_string(&layers)?;
            println!("{}", data);
            return Ok(0);
        }

        // Otherwise, just show the spfs layers - this is the default
        for layer in layers.iter() {
            println!("{}", layer.spfs_layer)
        }
        Ok(0)
    }
}

impl Bake {
    /// Get the layers from the active stack. These are digests for
    /// the layers from any packages resolved into the current
    /// environment, and may include other layers added by other
    /// means (the user and spfs.)
    fn get_active_runtime_info(&self) -> Result<Vec<BakeLayer>> {
        let runtime = spk::HANDLE.block_on(spfs::active_runtime())?;
        let solution = spk::current_env()?;

        // These come out of the runtime in the spfs order, no
        // reversing needed.
        let items = solution.items();

        // Need to get the repos from the command line because the
        // active runtime repo has no information about components for
        // the packages' layers it has.
        let repos = self.solver.repos.get_repos(&["origin".to_string()])?;

        // Get the layer(s) for the packages from their source repos
        let mut layers_to_packages = HashMap::new();
        for resolved in items {
            let spfs_layer = match resolved.source {
                PackageSource::Spec(s) => {
                    // The source of the resolved package is another
                    // package, not a repo.
                    if resolved.spec.pkg.build.as_ref().unwrap().is_embedded() {
                        // Embedded builds are provided by another package
                        // in the solve, they don't have a layer of their
                        // own so they can be skipped over.
                        continue;
                    } else {
                        // This is a /src build of a package, and bake
                        // doesn't build packages from source
                        return Err(spk::Error::String(format!("Cannot bake, solution requires packages that need building - Request for: {}, Resolved to: {}, Provided by: {}", resolved.request.pkg, resolved.spec.pkg, s.pkg)).into());
                    }
                }
                PackageSource::Repository {
                    repo: _,
                    components,
                } if components.is_empty() => {
                    // The active runtime repo has no information
                    // about components for the packages related to
                    // the layers it has in the runtime.
                    let mut possible_components: HashMap<spk::api::Component, spfs::Digest> =
                        HashMap::new();
                    for (_name, repo) in repos.iter() {
                        // TODO: calling get_package() over and over
                        // for many packages will be slower than a
                        // bulk call per repo. Either need to port
                        // get_packages(), or change spk/spfs to store
                        // the component info in the runtime
                        // environment so that current_environment()
                        // doesn't create empty component mappings
                        match repo.get_package(&resolved.spec.pkg) {
                            Ok(found) => possible_components = found,
                            Err(_) => continue,
                        }
                        if !possible_components.is_empty() {
                            // Note: assumes the first package it
                            // finds with components is a match for
                            // the one that was used to populate the
                            // runtime originally.
                            break;
                        }
                    }
                    possible_components
                        .values()
                        .unique()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(", ")
                }
                PackageSource::Repository {
                    repo: _,
                    components,
                } => {
                    // Currently this never happens. But if the
                    // active runtime repo kept a mapping of
                    // components to digests for the packages it
                    // had in it, then this would work and we
                    // would not need the other part of the if
                    // statement
                    components
                        .values()
                        .unique()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(", ")
                }
            };

            // TODO: the join(", ") above can turn multiple layers
            // into a single string blob that probably won't work if
            // feed back into a component supporting spk version
            layers_to_packages.insert(spfs_layer, resolved.spec.pkg.to_string());
        }

        // Keep runtime stack order with the first layer at the
        // bottom. Usually the runtime layers match will the current
        // environment's packages. However, additional layers may have
        // been added to the runtime (see get_stack() call above).
        // Those layers are included, but we don't know what package
        // they came from so they are marked "unknown".
        // Note: this may not interact well with spfs run's layer merging
        // for overlay fs mount commands.
        let mut layers: Vec<BakeLayer> = Vec::with_capacity(runtime.status.stack.len());
        for layer in runtime.status.stack.iter() {
            // There's no requester or spfs tag information in an
            // active runtime, yet.
            // TODO: store this info in an active runtime, from the
            // solve that made it, so it can be properly accessed here.
            let requested_by = api::RequestedBy::CurrentEnvironment.to_string();
            // TODO: need to expose spfs's repository's find_aliases()
            // or find_tags() in spk to get the tag from a digest
            let spfs_tag = EMPTY_TAG.to_string();

            let spk_package = match layers_to_packages.get(&layer.to_string()) {
                Some(p) => p.to_string(),
                None => UNKNOWN_PACKAGE.to_string(),
            };

            layers.push(BakeLayer {
                spfs_layer: layer.to_string(),
                spk_package,
                spk_requester: requested_by,
                spfs_tag,
            });
        }

        Ok(layers)
    }

    /// Get the layers from the stack would result from new solve of
    /// the requests given on the command line. This won't consider
    /// anything in the current environment.
    ///
    fn get_new_solve_info(&self) -> Result<Vec<BakeLayer>> {
        // Setup a solver for the requests and generate a solution
        // with it.
        let mut solver = self.solver.get_solver(&self.options)?;
        let requests = self
            .requests
            .parse_requests(&self.requested, &self.options)?;
        for request in requests {
            solver.add_request(request)
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose);
        let solution = formatter.run_and_print_resolve(&solver)?;

        // The solution order is the order things were found during
        // the solve. We want to reverse it to match up with the spfs
        // layering order, which is the order they would come out of
        // an active runtime.
        let mut items = solution.items();
        items.reverse();

        let mut stack: Vec<BakeLayer> = Vec::with_capacity(items.len());
        for resolved in items.iter() {
            let spfs_layer = match &resolved.source {
                PackageSource::Spec(s) => {
                    // The source of the resolved package is another
                    // package, not a repo.
                    if resolved.spec.pkg.build.as_ref().unwrap().is_embedded() {
                        // Embedded builds are provided by another package
                        // in the solve, they don't have a layer of their
                        // own so they can be skipped over.
                        continue;
                    } else {
                        // This is a /src build of a package, and bake
                        // doesn't build packages from source
                        return Err(spk::Error::String(format!("Cannot bake, solution requires packages that need building - Request for: {}, Resolved to: {}, Provided by: {}", resolved.request.pkg, resolved.spec.pkg, s.pkg)).into());
                    }
                }
                PackageSource::Repository {
                    repo: _,
                    components,
                } => {
                    // Packages published before components will have
                    // run: and build: components that point to the
                    // same layer, so the unique() call is used to
                    // reduce those to a single entry.
                    components
                        .values()
                        .map(ToString::to_string)
                        .unique()
                        .collect::<Vec<String>>()
                        .join(", ")
                }
            };

            // TODO: the join(", ") can turn multiple layers into a
            // single string blob that probably won't work for
            // component supporting spk

            // Work out where the requests for this item came from
            let requested_by = resolved
                .request
                .get_requesters()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>();

            // There's no spfs tag information for this yet.
            // TODO: need to expose spfs's repository's
            // find_aliases()/find_tags() in spk to get this from a digest
            let spfs_tag = EMPTY_TAG.to_string();

            stack.push(BakeLayer {
                spfs_layer,
                spk_package: resolved.spec.pkg.to_string(),
                spk_requester: requested_by.join(", "),
                spfs_tag,
            });
        }
        Ok(stack)
    }
}
