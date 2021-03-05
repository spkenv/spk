use spfs::Result;
use std::ffi::OsString;
use structopt::StructOpt;

/// This is a 'hidden' command.
///
/// This command is the entry point to new environments, and
/// is executed ahead of any desired process to setup the
/// environment variables and other configuration that can
/// only be done from within the mount namespace.
#[derive(StructOpt, Debug)]
pub struct CmdInit {
    #[structopt()]
    runtime_root_dir: String,
    #[structopt(required = true)]
    cmd: Vec<OsString>,
}

impl CmdInit {
    pub fn run(&mut self, _config: &spfs::Config) -> spfs::Result<()> {
        tracing::debug!("initializing runtime environment");
        std::env::set_var("SPFS_RUNTIME", self.runtime_root_dir.clone());
        spfs::initialize_runtime()?;

        let result = exec_runtime_command(self.cmd.clone());
        if let Err(err) = spfs::deinitialize_runtime() {
            tracing::warn!(err =?err, "failed to cleanup runtime");
        }
        std::process::exit(result?);
    }
}

fn exec_runtime_command(mut cmd: Vec<OsString>) -> Result<i32> {
    if cmd.len() == 0 || cmd[0] == OsString::from("") {
        cmd = spfs::build_interactive_shell_cmd()?;
        tracing::debug!("starting interactive shell environment");
    } else {
        cmd = spfs::build_shell_initialized_command(cmd[0].clone(), &mut cmd[1..].to_vec())?;
        tracing::debug!("executing runtime command");
    }
    tracing::debug!(?cmd);
    let mut proc = std::process::Command::new(cmd[0].clone());
    proc.args(&cmd[1..]);
    Ok(proc.status()?.code().unwrap_or(1))
}
