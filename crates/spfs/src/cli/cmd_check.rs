use structopt::StructOpt;

use spfs;
use spfs::prelude::*;

#[derive(Debug, StructOpt)]
pub struct CmdCheck {
    #[structopt(
        short = "r",
        long = "remote",
        about = "Trigger the check operation on a remote repository"
    )]
    remote: Option<String>,
}

impl CmdCheck {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        tracing::info!("walking repository...");
        let errors = match repo {
            RepositoryHandle::FS(repo) => spfs::graph::check_database_integrity(repo),
            RepositoryHandle::Tar(repo) => spfs::graph::check_database_integrity(repo),
        };
        for error in errors.iter() {
            tracing::error!("{:?}", error);
        }
        if errors.len() > 0 {
            std::process::exit(1);
        }
        tracing::info!("repository OK");
        Ok(())
    }
}
