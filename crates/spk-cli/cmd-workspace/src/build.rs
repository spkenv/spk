use std::sync::Arc;
use std::vec;

use clap::Args;
use miette::{Context, IntoDiagnostic, Result};
use spk_cli_common::{BuildResult, CommandArgs, Run, build_required_packages, flags};
use spk_cmd_make_source::cmd_make_source;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident::{
    AsVersionIdent,
    PinnableValue,
    RangeIdent,
    ToAnyIdentWithoutBuild,
    VarRequest,
};
use spk_schema::name::RepositoryName;
use spk_schema::v1::{Override, PlatformRequirement};
use spk_schema::{ApiVersion, SpecFileData, SpecRecipe, Template, TemplateExt, VersionIdent};
use spk_solve::{Package, PkgRequest, Request, SolverExt, SolverMut};

/// Build a set of packages from this workspace
#[derive(Args, Clone)]
#[clap(visible_aliases = &["b"])]
pub struct Build {
    #[clap(flatten)]
    runtime: flags::Runtime,
    #[clap(flatten)]
    workspace: flags::Workspace,
    #[clap(flatten)]
    solver: flags::Solver,
    #[clap(flatten)]
    options: flags::Options,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The platform to build, which defines the set of packages
    platform: String,
}

#[async_trait::async_trait]
impl Run for Build {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        self.runtime.ensure_active_runtime(&["b", "build"]).await?;

        let mut workspace = self.workspace.load_or_default()?;
        let options = self.options.get_options()?;
        let platform_path = std::path::Path::new(&self.platform);
        let tpl = spk_schema::SpecTemplate::from_file(platform_path)?;
        let recipe = tpl
            .render(spk_schema::template::TemplateRenderConfig {
                options,
                ..Default::default()
            })?
            .into_recipe()?;
        let SpecRecipe::V1Platform(platform) = &*recipe else {
            miette::bail!(
                "Only {} recipe files can be used to build workspaces",
                ApiVersion::V1Platform
            )
        };
        let mut solver = self.solver.get_solver(&self.options).await?;

        let root = match workspace.root() {
            Some(root) => root.to_owned(),
            None => std::env::current_dir().into_diagnostic()?,
        };
        let repo_name = RepositoryName::new("workspace")?;

        tracing::info!("Adding requests for all platform requirements:");
        let requested_by = spk_solve::RequestedBy::BinaryBuild(
            platform.platform.to_build_ident(spk_solve::Build::Source),
        );
        for requirement in platform.requirements.iter() {
            let request = match requirement {
                PlatformRequirement::Pkg(pkg) => {
                    let Some(build) = pkg.build.as_ref() else {
                        continue;
                    };
                    let templates = workspace.find_package_templates_mut(&pkg.pkg);
                    if templates.is_empty() {
                        miette::bail!(
                            "Cannot build '{}', no spec files found in workspace",
                            pkg.pkg
                        );
                    }

                    let to_build = VersionIdent::new(pkg.pkg.clone(), build.version.clone())
                        .to_any_ident_without_build();
                    let range_ident = RangeIdent::equals(&to_build, None)
                        .with_repository(Some(repo_name.to_owned()));
                    tracing::info!(" > pkg: {range_ident}");
                    Request::Pkg(PkgRequest::new(range_ident, requested_by.clone()))
                }
                PlatformRequirement::Var(var) => {
                    let Some(Override::Replace(value)) = &var.at_build else {
                        continue;
                    };
                    let inner = VarRequest {
                        var: var.var.clone(),
                        value: PinnableValue::Pinned(Arc::from(value.as_str())),
                        description: None,
                    };
                    tracing::info!(" > var: {inner}");
                    Request::Var(inner)
                }
            };
            solver.add_request(request);
        }

        let local = Arc::<spk_storage::RepositoryHandle>::new(
            spk_storage::local_repository().await?.into(),
        );
        let workspace_repo_handle = Arc::new(spk_storage::RepositoryHandle::Workspace(
            spk_storage::WorkspaceRepository::new(&root, repo_name.to_owned(), workspace),
        ));
        // we still need a reference to the underlying workspace instance for later
        let spk_storage::RepositoryHandle::Workspace(workspace_repo) = &*workspace_repo_handle
        else {
            unreachable!()
        };
        solver.add_repository(Arc::clone(&workspace_repo_handle));
        let formatter = self
            .solver
            .decision_formatter_settings
            .get_formatter(self.verbose)?;
        solver.set_binary_only(false);
        let solution = solver.run_and_print_resolve(&formatter).await?;

        for solved in solution.items() {
            if !solved.is_source_build()
                || solved
                    .repo_name()
                    .as_deref()
                    .is_some_and(|n| n != repo_name)
            {
                continue;
            };
            // TODO: be more intelligent about when this is needed?
            //       ideally we can have SOME sense of if the recipe
            //       has changed on disk, but there's complexity
            //       around checking the source files themselves
            let ident = solved.spec.ident();
            tracing::info!(
                "Generating source package for required build: {}",
                ident.format_ident()
            );

            let mut templates = workspace_repo.find_package_template_for_version(
                ident.name(),
                spk_schema::version_range::DoubleEqualsVersion::new(ident.version().clone()),
            );
            if templates.len() != 1 {
                miette::bail!(
                    "Expected exactly one package template for {}, found {}",
                    ident.format_ident(),
                    templates.len()
                );
            }
            let template = templates[0];
            let recipe = template.render(spk_schema::template::TemplateRenderConfig {
                version: Some(ident.version().clone()),
                ..Default::default()
            })?;
            let SpecFileData::Recipe(recipe) = recipe else {
                miette::bail!(
                    "Expected recipe from file {}",
                    template.file_path().display()
                )
            };

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("collecting sources for {}", ident.format_ident());
            let (out, _components) =
                spk_build::SourcePackageBuilder::from_recipe(Arc::unwrap_or_clone(recipe))
                    .build_and_publish(
                        &template
                            .file_path()
                            .parent()
                            .unwrap_or_else(|| template.file_path()),
                        &*local,
                    )
                    .await
                    .wrap_err("Failed to collect sources")?;
            tracing::info!("created {}", out.ident().format_ident());
        }

        build_required_packages(&solution, &formatter, solver).await?;

        Ok(0)
    }
}

impl CommandArgs for Build {
    fn get_positional_args(&self) -> Vec<String> {
        vec![self.platform.clone()]
    }
}
