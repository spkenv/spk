use structopt::StructOpt;

use spfs;

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
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };
        for layer in repo.iter_layers() {
            let (digest, _) = layer?;
            println!(
                "{}",
                spfs::io::format_digest(&digest.to_string(), Some(&repo))?
            );
        }
        Ok(())
    }
}
