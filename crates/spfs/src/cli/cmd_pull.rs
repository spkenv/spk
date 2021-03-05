use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdPull {
    #[structopt(
        long = "remote",
        short = "r",
        about = "the name or address of the remote server to pull from, \
                 defaults to searching all configured remotes"
    )]
    remote: Option<String>,
    #[structopt(
        value_name = "REF",
        required = true,
        about = "the reference(s) to pull/localize"
    )]
    refs: Vec<String>,
}

impl CmdPull {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let mut repo = config.get_repository()?.into();
        let remote = match &self.remote {
            None => config.get_remote("origin")?,
            Some(remote) => config.get_remote(remote)?,
        };

        for reference in self.refs.iter() {
            spfs::sync_ref(reference, &remote, &mut repo)?;
        }

        Ok(())
    }
}
