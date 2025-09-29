use clap::Args;
use colored::Colorize;
use itertools::Itertools;
use miette::{IntoDiagnostic, Result};
use spk_cli_common::{CommandArgs, Run};
use spk_schema::Template;

/// Print information about the current workspace
#[derive(Args, Clone)]
#[clap(visible_aliases = &["i"])]
pub struct Info {}

#[async_trait::async_trait]
impl Run for Info {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let mut root = std::env::current_dir().into_diagnostic()?;
        let workspace = match spk_workspace::WorkspaceFile::discover(".") {
            Ok((file, path)) => {
                root = path;
                spk_workspace::Workspace::builder()
                    .load_from_file(file)?
                    .build()?
            }
            Err(spk_workspace::error::LoadWorkspaceFileError::WorkspaceNotFound(_)) => {
                tracing::warn!(
                    "Workspace file not found using the current path, loading ephemerally"
                );
                spk_workspace::Workspace::builder()
                    .load_from_current_dir()?
                    .build()?
            }
            Err(err) => return Err(err.into()),
        };
        println!("root: {}", root.display());
        println!("packages:");
        // we'd like the items in the workspace to be sorted alphabetically
        let mut packages = workspace.iter().collect_vec();
        packages.sort_by(|a, b| a.0.cmp(b.0));
        for (pkg, tpl) in packages {
            let mut versions = tpl
                .config
                .versions
                .iter()
                .map(|v| v.to_string())
                .collect_vec();
            if versions.len() > 5 {
                let tail = format!("and {} more...", versions.len() - 5);
                versions.truncate(5);
                versions.push(tail);
            }
            let path = tpl.template.file_path();
            // try to get a relative workspace path, if possible
            let path = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            println!(". {} {}", pkg.bold(), path.dimmed());
            if !versions.is_empty() {
                println!("    {}", versions.join(", "));
            };
        }
        Ok(0)
    }
}

impl CommandArgs for Info {
    fn get_positional_args(&self) -> Vec<String> {
        Vec::default()
    }
}
