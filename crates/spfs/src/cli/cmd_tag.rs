use structopt::StructOpt;

use spfs::{self, prelude::*};

#[derive(Debug, StructOpt)]
pub struct CmdTag {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Create tags in a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(value_name = "TARGET_REF")]
    reference: String,
    #[structopt(value_name = "TAG", required = true)]
    tags: Vec<String>,
}

impl CmdTag {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let mut repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        let target = repo.read_ref(self.reference.as_str())?.digest()?;
        for tag in self.tags.iter() {
            let tag = tag.parse()?;
            repo.push_tag(&tag, &target)?;
            tracing::info!(?tag, "created");
        }
        Ok(())
    }
}
