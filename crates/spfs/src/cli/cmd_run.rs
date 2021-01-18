use std::ffi::OsString;

use structopt::StructOpt;

use spfs;
use spfs::prelude::*;

#[derive(Debug, StructOpt)]
pub struct CmdRun {
    #[structopt(
        short = "p",
        help = "try to pull the latest iteration of each tag even if it exists locally"
    )]
    pull: bool,
    #[structopt(
        short = "e",
        help = "mount the /spfs filesystem in edit mode (true if REF is empty or not given)"
    )]
    edit: bool,
    #[structopt(
        long = "ref",
        help = "The tag or id of the desired runtime, use '-' or an empty string to request an empty environment"
    )]
    reference: String,
    #[structopt()]
    cmd: OsString,
    #[structopt(long = "args")]
    args: Vec<OsString>,
}

impl CmdRun {
    pub async fn run(&mut self) -> spfs::Result<()> {
        let config = spfs::get_config()?;
        let repo = config.get_repository()?;
        let runtimes = config.get_runtime_storage()?;
        let mut runtime = runtimes.create_runtime()?;
        match self.reference.as_str() {
            "-" | "" => self.edit = true,
            reference => {
                let env_spec = spfs::tracking::parse_env_spec(reference)?;
                for target in env_spec {
                    let target = target.to_string();
                    let obj = if self.pull || !repo.has_ref(target.as_str()) {
                        tracing::info!(reference = ?target, "pulling target ref");
                        spfs::pull_ref(target.as_str()).await?
                    } else {
                        repo.read_ref(target.as_str())?
                    };

                    runtime.push_digest(&obj.digest()?)?;
                }
            }
        }

        runtime.set_editable(self.edit)?;
        tracing::debug!("resolving entry process");
        let (cmd, args) =
            spfs::build_command_for_runtime(runtime, self.cmd.clone(), &mut self.args)?;
        tracing::debug!("{:?} {:?}", cmd, args);
        use std::os::unix::ffi::OsStrExt;
        let cmd = std::ffi::CString::new(cmd.as_bytes()).unwrap();
        let args: Vec<_> = args
            .into_iter()
            .map(|arg| std::ffi::CString::new(arg.as_bytes()).unwrap())
            .collect();
        nix::unistd::execv(cmd.as_ref(), args.as_slice())?;
        Ok(())
    }
}
