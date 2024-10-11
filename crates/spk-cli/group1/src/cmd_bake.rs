// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use futures::TryFutureExt;
use miette::IntoDiagnostic;
use serde::Serialize;
use spk_cli_common::{current_env, flags, CommandArgs, Run};
use spk_schema::ident::RequestedBy;
use spk_schema::Package;
use spk_solve::solution::{get_spfs_layers_to_packages, LayerPackageAndComponents, PackageSource};

#[cfg(test)]
#[path = "./cmd_bake_test.rs"]
mod cmd_bake_test;

// Verbosity level above which repo and component names will be
// included in the package display values.
const NO_VERBOSITY: u8 = 0;

// Constants for the valid output formats
const LAYER_FORMAT: &str = "layers";
const BUILD_FORMAT: &str = "builds";
const YAML_FORMAT: &str = "yaml";
const JSON_FORMAT: &str = "json";
const OUTPUT_FORMATS: [&str; 4] = [LAYER_FORMAT, BUILD_FORMAT, YAML_FORMAT, JSON_FORMAT];

// TODO: a duplicate of this exists in spk-cli/common/src hidden
// behind the "sentry" feature. Might want consider refactoring these
// two functions to a single place not hidden behind any feature.
/// Utility for removing ansi-colour/terminal escape codes from a String
fn remove_ansi_escapes(message: String) -> String {
    if let Ok(b) = strip_ansi_escapes::strip(message.clone()) {
        if let Ok(s) = std::str::from_utf8(&b) {
            return s.to_string();
        }
    }
    message
}

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
    #[clap(short, long, value_parser=OUTPUT_FORMATS, default_value=LAYER_FORMAT)]
    pub format: String,

    /// Verbosity level, can be specified multiple times for more verbose output
    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

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
    #[serde(default)]
    spk_components: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    spk_requester: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    spfs_tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    spfs_repo_name: String,
}

const EMPTY_TAG: &str = "";
const UNKNOWN_PACKAGE: &str = "";
const UNKNOWN_COMPONENT: &str = "";

#[async_trait::async_trait]
impl Run for Bake {
    type Output = i32;

    async fn run(&mut self) -> miette::Result<Self::Output> {
        // Get the layer data from either the active runtime, or the
        // requests made on the command line
        let layers = if self.requested.is_empty() {
            self.get_active_runtime_info().await?
        } else {
            let (_, layers) = tokio::try_join!(
                self.runtime.ensure_active_runtime(&["bake"]),
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
                serde_yaml::to_string(&layers).into_diagnostic()?
            }
            JSON_FORMAT => {
                // Turn layer data into json for other programs to use
                serde_json::to_string(&layers).into_diagnostic()?
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
        println!("{data}");
        Ok(0)
    }
}

impl CommandArgs for Bake {
    fn get_positional_args(&self) -> Vec<String> {
        self.requested.clone()
    }
}

impl Bake {
    /// Get the layers from the active stack. These are digests for
    /// the layers from any packages resolved into the current
    /// environment, and may include other layers added by other
    /// means (the user and spfs.)
    async fn get_active_runtime_info(&self) -> miette::Result<Vec<BakeLayer>> {
        let (runtime, solution) = tokio::try_join!(
            spfs::active_runtime().map_err(|err| err.into()),
            current_env()
        )?;

        // These come out of the runtime in the spfs order, no
        // reversing needed.
        let items = solution.items();

        // Get the layer(s) for the packages mapping from their source repos
        let layers_to_packages = get_spfs_layers_to_packages(&items)?;

        // Reverse the runtime stack order with the first layer at the
        // top to remain consistent with other console-based output.
        // Usually, the runtime layers match will the current
        // environment's packages. However, additional layers may have
        // been added to the runtime (see get_stack() call above).
        // Those layers are included, but we don't know what package
        // they came from so they are marked "unknown".
        //
        // Note: this may not interact well with spfs run's layer
        // merging for overlay fs mount commands.
        let mut layers: Vec<BakeLayer> = Vec::new();
        for layer in runtime.status.stack.to_top_down() {
            let (spk_package, mut components) =
                if let Some(LayerPackageAndComponents(sr, c)) = layers_to_packages.get(&layer) {
                    (
                        if self.verbose > NO_VERBOSITY {
                            remove_ansi_escapes(sr.format_as_installed_package())
                        } else {
                            sr.spec.ident().to_string()
                        },
                        c.iter().map(ToString::to_string).collect::<Vec<String>>(),
                    )
                } else {
                    (
                        UNKNOWN_PACKAGE.to_string(),
                        vec![UNKNOWN_COMPONENT.to_string()],
                    )
                };
            components.sort();

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
                spk_components: components,
                spk_requester: requested_by,
                spfs_tag,
                spfs_repo_name: runtime.name().to_string(),
            });
        }

        Ok(layers)
    }

    /// Get the layers from the stack would result from new solve of
    /// the requests given on the command line. This won't consider
    /// anything in the current environment.
    ///
    async fn get_new_solve_info(&self) -> miette::Result<Vec<BakeLayer>> {
        // Setup a solver for the requests and generate a solution
        // with it.
        let mut solver = self.solver.get_solver(&self.options).await?;

        let requests = self
            .requests
            .parse_requests(&self.requested, &self.options, solver.repositories())
            .await?;
        for request in requests {
            solver.add_request(request)
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose)?;
        let (solution, _) = formatter.run_and_print_resolve(&solver).await?;

        // The solution order is the order things were found during
        // the solve. Need to reverse it to match up with the spfs
        // layering order, which is the order they would come out of
        // an active runtime.
        let mut items = solution.items().collect::<Vec<_>>();
        items.reverse();

        let mut stack: Vec<BakeLayer> = Vec::with_capacity(items.len());
        for resolved in items {
            let spfs_layers = match resolved.component_layers() {
                Ok(layers) => layers,
                Err(spk_solve::solution::Error::EmbeddedHasNoComponentLayers) => continue,
                Err(spk_solve::solution::Error::SpkInternalTestHasNoComponentLayers) => continue,
                Err(e) => return Err(e.into()),
            };

            // Work out where the requests for this item came from
            let requested_by = resolved
                .request
                .get_requesters()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>();

            // The repo name for this package is valid if the package comes from a
            // repository, otherwise a broad placeholder is used.
            let repo_name = match &resolved.source {
                PackageSource::Repository { repo, .. } => repo.name().to_string(),
                PackageSource::BuildFromSource { .. } => "source".to_string(),
                PackageSource::Embedded { .. } => "embedded".to_string(),
                PackageSource::SpkInternalTest => "internal test".to_string(),
            };

            // There's no spfs tag information for this yet.
            // TODO: need to expose spfs's repository's
            // find_aliases()/find_tags() in spk to get this from a digest
            let spfs_tag = EMPTY_TAG.to_string();

            for (component, layer) in spfs_layers.iter() {
                stack.push(BakeLayer {
                    spfs_layer: layer.to_string(),
                    spk_package: if self.verbose > NO_VERBOSITY {
                        remove_ansi_escapes(resolved.format_as_installed_package())
                    } else {
                        resolved.spec.ident().to_string()
                    },
                    spk_components: vec![component.to_string()],
                    spk_requester: requested_by.join(", "),
                    spfs_tag: spfs_tag.clone(),
                    spfs_repo_name: repo_name.clone(),
                });
            }
        }
        Ok(stack)
    }
}
