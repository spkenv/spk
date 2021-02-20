use structopt::StructOpt;

use spfs;
#[derive(Debug, StructOpt)]
pub struct CmdPlatforms {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
}

impl CmdPlatforms {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote)?;
                for platform in repo.iter_platforms() {
                    let (digest, _) = platform?;
                    println!(
                        "{}",
                        spfs::io::format_digest(&digest.to_string(), Some(&repo))?
                    );
                }
            }
            None => {
                let repo: spfs::storage::RepositoryHandle = config.get_repository()?.into();
                for platform in repo.iter_platforms() {
                    let (digest, _) = platform?;
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
