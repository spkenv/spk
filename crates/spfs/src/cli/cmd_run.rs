use std::ffi::OsString;

use structopt::StructOpt;

use spfs;
use spfs::prelude::*;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spfs-run",
    about = "Run a program in a configured spfs environment"
)]
pub struct CmdRun {
    #[structopt(short = "v", long = "verbose", global = true, parse(from_occurrences), env = super::args::SPFS_VERBOSITY)]
    pub verbose: usize,
    #[structopt(
        short = "p",
        long = "pull",
        help = "try to pull the latest iteration of each tag even if it exists locally"
    )]
    pub pull: bool,
    #[structopt(
        short = "e",
        long = "edit",
        help = "mount the /spfs filesystem in edit mode (true if REF is empty or not given)"
    )]
    pub edit: bool,
    #[structopt(
        short = "n",
        long = "name",
        about = "provide a name for this runtime to make it easier to identify"
    )]
    pub name: Option<String>,
    #[structopt(
        help = "The tag or id of the desired runtime, use '-' or an empty string to request an empty environment"
    )]
    pub reference: String,
    #[structopt()]
    pub cmd: OsString,
    #[structopt()]
    pub args: Vec<OsString>,
}

impl CmdRun {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_repository()?;
        let runtimes = config.get_runtime_storage()?;
        let mut runtime = match &self.name {
            Some(name) => runtimes.create_named_runtime(name)?,
            None => runtimes.create_runtime()?,
        };
        match self.reference.as_str() {
            "-" | "" => self.edit = true,
            reference => {
                let env_spec = spfs::tracking::parse_env_spec(reference)?;
                for target in env_spec {
                    let target = target.to_string();
                    if self.pull || !repo.has_ref(target.as_str()) {
                        tracing::info!(reference = ?target, "pulling target ref");
                        spfs::pull_ref(target.as_str())?
                    }

                    let obj = repo.read_ref(target.as_str())?;
                    runtime.push_digest(&obj.digest()?)?;
                }
            }
        }

        runtime.set_editable(self.edit)?;
        tracing::debug!("resolving entry process");
        let (cmd, args) =
            spfs::build_command_for_runtime(&runtime, self.cmd.clone(), &mut self.args)?;
        tracing::trace!("{:?} {:?}", cmd, args);
        use std::os::unix::ffi::OsStrExt;
        let cmd = std::ffi::CString::new(cmd.as_bytes()).unwrap();
        let mut args: Vec<_> = args
            .into_iter()
            .map(|arg| std::ffi::CString::new(arg.as_bytes()).unwrap())
            .collect();
        args.insert(0, cmd.clone());
        runtime.set_running(true)?;
        nix::unistd::execv(cmd.as_ref(), args.as_slice())?;
        Ok(0)
    }
}
