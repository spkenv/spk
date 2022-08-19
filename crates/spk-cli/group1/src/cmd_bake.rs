// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use clap::Args;
use futures::TryFutureExt;
use itertools::Itertools;
use serde::Serialize;
use spk_cli_common::{current_env, flags, CommandArgs, Error, Result, Run};
use spk_foundation::spec_ops::PackageOps;
use spk_ident::RequestedBy;
use spk_solve::solution::{PackageSource, SolvedRequest};

// Constants for the valid output formats
const LAYER_FORMAT: &str = "layers";
const BUILD_FORMAT: &str = "builds";
const YAML_FORMAT: &str = "yaml";
const JSON_FORMAT: &str = "json";
const OUTPUT_FORMATS: &[&str] = &[LAYER_FORMAT, BUILD_FORMAT, YAML_FORMAT, JSON_FORMAT];

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

    /// Format to output the layer data in:
    ///
    /// 'layers' outputs only the spfs layer digests one per line, 'builds' outputs
    /// only the spk package builds one per line, and 'yaml' and 'json' output all
    /// the available layer data in the matching format.
    #[clap(short, long, possible_values=OUTPUT_FORMATS, default_value=LAYER_FORMAT)]
    pub format: String,

    /// Verbosity level, can be specified multiple times for more verbose output
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

#[async_trait::async_trait]
impl Run for Bake {
    async fn run(&mut self) -> anyhow::Result<i32> {
        // Get the layer data from either the active runtime, or the
        // requests made on the command line
        let layers = if self.requested.is_empty() {
            self.get_active_runtime_info().await?
        } else {
            let (_, layers) = tokio::try_join!(
                self.runtime.ensure_active_runtime(),
                self.get_new_solve_info()
            )?;
            layers
        };

        // Based on the format option, output the bake info.
        let data = match &*self.format {
            BUILD_FORMAT => {
                // Output the package builds instead of the spfs layers
                layers
                    .into_iter()
                    .map(|l| l.spk_package)
                    .collect::<Vec<String>>()
                    .join("\n")
            }
            YAML_FORMAT => {
                // True layer data into yaml for other programs to use
                serde_yaml::to_string(&layers)?
            }
            JSON_FORMAT => {
                // Turn layer data into json for other programs to use
                serde_json::to_string(&layers)?
            }
            // LAYER_FORMAT
            _ => {
                // Otherwise, just output the spfs layers (digests)
                layers
                    .into_iter()
                    .map(|l| l.spfs_layer)
                    .collect::<Vec<String>>()
                    .join("\n")
            }
        };
        println!("{}", data);
        Ok(0)
    }
}

impl CommandArgs for Bake {
    fn get_positional_args(&self) -> Vec<String> {
        self.requested.clone()
    }
}

impl Bake {
    /// Get the spfs layer for a resolved request from it's source
    /// repo, if possible. This returns a SkipEmbedded error if the
    /// resolved request is an embedded package. These can be skipped
    /// for the purposes of the Bake command. It returns a String
    /// message error if the request is for a src package, which the
    /// Bake command can do nothing with.
    fn get_spfs_layer(&self, resolved: &SolvedRequest) -> Result<String> {
        let spfs_layer = match &resolved.source {
            PackageSource::Embedded => {
                // Embedded builds are provided by another package
                // in the solve. They don't have a layer of their
                // own so they can be skipped over.
                return Err(Error::SkipEmbedded);
            }
            PackageSource::BuildFromSource { .. } => {
                // bake doesn't build packages from source
                return Err(Error::String(format!("Cannot bake, solution requires packages that need building - Request for: {}, Resolved to: {}", resolved.request.pkg, resolved.spec.ident())));
            }
            PackageSource::Repository {
                repo: _,
                components,
            } => {
                // Packages published before component support was
                // added will have 'run:' and 'build:' components that
                // point to the same layer, so the unique() call is
                // used to reduce those to a single entry.
                components
                    .values()
                    .map(ToString::to_string)
                    .unique()
                    .collect::<Vec<String>>()
                    .join(", ")
            }
        };

        // TODO: the join(", ") above can turn multiple layers into a
        // single string blob that probably won't work well for
        // component supporting spk commands
        Ok(spfs_layer)
    }

    /// Get the layers from the active stack. These are digests for
    /// the layers from any packages resolved into the current
    /// environment, and may include other layers added by other
    /// means (the user and spfs.)
    async fn get_active_runtime_info(&self) -> anyhow::Result<Vec<BakeLayer>> {
        let (runtime, solution) = tokio::try_join!(
            spfs::active_runtime().map_err(|err| err.into()),
            current_env()
        )?;

        // These come out of the runtime in the spfs order, no
        // reversing needed.
        let items = solution.items();

        // Get the layer(s) for the packages from their source repos
        let mut layers_to_packages = HashMap::new();
        for resolved in items {
            let spfs_layer = match self.get_spfs_layer(&resolved) {
                Ok(layer) => layer,
                Err(Error::SkipEmbedded) => continue,
                Err(e) => return Err(e.into()),
            };

            // Store in a map so they can be matched up with the
            // layers in the runtime environment in the next loop.
            layers_to_packages.insert(spfs_layer, resolved.spec.ident().to_string());
        }

        // Keep the runtime stack order with the first layer at the
        // bottom. Usually the runtime layers match will the current
        // environment's packages. However, additional layers may have
        // been added to the runtime (see get_stack() call above).
        // Those layers are included, but we don't know what package
        // they came from so they are marked "unknown".
        //
        // Note: this may not interact well with spfs run's layer
        // merging for overlay fs mount commands.
        let mut layers: Vec<BakeLayer> = Vec::with_capacity(runtime.status.stack.len());
        for layer in runtime.status.stack.iter() {
            let spk_package = match layers_to_packages.get(&layer.to_string()) {
                Some(p) => p.to_string(),
                None => UNKNOWN_PACKAGE.to_string(),
            };

            // There's no "requested by" or "spfs tag" information in
            // an active runtime, yet.
            // TODO: store this info in an active runtime, from the
            // solve that made it, so it can be properly accessed here.
            let requested_by = RequestedBy::CurrentEnvironment.to_string();
            // TODO: need to expose spfs's repository's find_aliases()
            // or find_tags() in spk to get the tag from a digest
            let spfs_tag = EMPTY_TAG.to_string();

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
    async fn get_new_solve_info(&self) -> anyhow::Result<Vec<BakeLayer>> {
        // Setup a solver for the requests and generate a solution
        // with it.
        let (mut solver, requests) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            self.requests.parse_requests(&self.requested, &self.options)
        )?;
        for request in requests {
            solver.add_request(request)
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose);
        let solution = formatter.run_and_print_resolve(&solver).await?;

        // The solution order is the order things were found during
        // the solve. Need to reverse it to match up with the spfs
        // layering order, which is the order they would come out of
        // an active runtime.
        let mut items = solution.items();
        items.reverse();

        let mut stack: Vec<BakeLayer> = Vec::with_capacity(items.len());
        for resolved in items {
            let spfs_layer = match self.get_spfs_layer(&resolved) {
                Ok(layer) => layer,
                Err(Error::SkipEmbedded) => continue,
                Err(e) => return Err(e.into()),
            };

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
                spk_package: resolved.spec.ident().to_string(),
                spk_requester: requested_by.join(", "),
                spfs_tag,
            });
        }
        Ok(stack)
    }
}
