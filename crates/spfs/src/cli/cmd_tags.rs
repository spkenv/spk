use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdTags {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
}

impl CmdTags {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };
        for tag in repo.iter_tags() {
            let (_, tag) = tag?;
            println!(
                "{}",
                spfs::io::format_digest(&tag.target.to_string(), Some(&repo))?
            );
        }
        Ok(())
    }
}
