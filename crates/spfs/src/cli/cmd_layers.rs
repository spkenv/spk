use structopt::StructOpt;

use spfs::{self, prelude::*};

#[derive(Debug, StructOpt)]
pub struct CmdLayers {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
}

impl CmdLayers {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote)?;
                for layer in repo.iter_layers() {
                    let (digest, _) = layer?;
                    println!(
                        "{}",
                        spfs::io::format_digest(&digest.to_string(), Some(&repo))?
                    );
                }
            }
            None => {
                let repo: RepositoryHandle = config.get_repository()?.into();
                for layer in repo.iter_layers() {
                    let (digest, _) = layer?;
                    println!(
                        "{}",
                        spfs::io::format_digest(&digest.to_string(), Some(&repo))?
                    );
                }
            }
        }
        Ok(())
    }
}
