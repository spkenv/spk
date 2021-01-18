use structopt::StructOpt;

use spfs;
use spfs::prelude::*;
use spfs::Result;

#[derive(Debug, StructOpt)]
pub struct CmdShell {
    #[structopt(
        short = "-p",
        about = "try to pull the latest iteration of each tag even if it exists locally"
    )]
    pull: bool,
    #[structopt(
        short = "e",
        about = "mount the /spfs filesystem in edit mode (true if REF is empty or not given)"
    )]
    edit: bool,
    #[structopt(
        short = "ref",
        about = "The tag or id of the desired runtime, use '-' or nothing to request an empty environment"
    )]
    reference: Option<String>,
}

impl CmdShell {
    pub async fn run(&mut self) -> Result<()> {
        let config = spfs::get_config()?;
        let repo = config.get_repository()?;
        let runtimes = config.get_runtime_storage()?;
        let mut runtime = runtimes.create_runtime()?;
        match &self.reference {
            Some(reference) if reference != "" && reference != "-" => {
                let env_spec = spfs::tracking::EnvSpec::new(reference.as_str())?;
                for target in env_spec.items {
                    let target = target.to_string();
                    let obj = if self.pull || !repo.has_ref(target.as_str()) {
                        tracing::info!(reference = ?target, "pulling target ref");
                        spfs::pull_ref(target.as_str()).await?
                    } else {
                        repo.read_ref(target.as_str())?
                    };
                    runtime.push_digest(&obj.digest()?)?
                }
            }
            _ => {
                self.edit = true;
            }
        }

        runtime.set_editable(self.edit)?;

        tracing::debug!("resolving entry process");
        let (cmd, args) = spfs::build_command_for_runtime(runtime, "".into(), &mut Vec::new())?;
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
