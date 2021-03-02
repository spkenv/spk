use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdEdit {
    #[structopt(long = "off", about = "Disable edit mode instead")]
    off: bool,
}

impl CmdEdit {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<()> {
        if !self.off {
            spfs::make_active_runtime_editable()?;
            tracing::info!("edit mode enabled");
        } else {
            let mut rt = spfs::active_runtime()?;
            rt.set_editable(false)?;
            if let Err(err) = spfs::remount_runtime(&rt) {
                rt.set_editable(true)?;
                return Err(err);
            }
            tracing::info!("edit mode disabled");
        }
        Ok(())
    }
}
