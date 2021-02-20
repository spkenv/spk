use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdPush {
    #[structopt(
        long = "remote",
        short = "r",
        default_value = "origin",
        about = "the name or address of the remote server to push to"
    )]
    remote: String,
    #[structopt(value_name = "REF", about = "the reference(s) to push")]
    refs: Vec<String>,
}

impl CmdPush {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let repo = config.get_repository()?.into();
        let mut remote = config.get_remote(&self.remote)?;
        for reference in self.refs.iter() {
            spfs::sync_ref(reference, &repo, &mut remote).await?;
        }

        Ok(())
    }
}
