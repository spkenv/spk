use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdDiff {
    #[structopt(
        value_name = "FROM",
        about = "The tag or id to use as the base of the computed diff, defaults to the current runtime"
    )]
    base: Option<String>,
    #[structopt(
        value_name = "TO",
        about = "The tag or id to diff the base against, defaults to the contents of /spfs"
    )]
    top: Option<String>,
}

impl CmdDiff {
    pub fn run(&mut self, _config: &spfs::Config) -> spfs::Result<()> {
        let diffs = spfs::diff(self.base.as_ref(), self.top.as_ref())?;
        let out = spfs::io::format_changes(diffs.iter());
        if out.trim().len() == 0 {
            tracing::info!("no changes");
        } else {
            println!("{}", out);
        }
        Ok(())
    }
}
