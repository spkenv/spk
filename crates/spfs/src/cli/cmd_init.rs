use spfs::Result;
use std::ffi::OsString;
use structopt::StructOpt;

#[macro_use]
mod args;

main!(CmdInit);

#[derive(StructOpt, Debug)]
pub struct CmdInit {
    #[structopt(short = "v", long = "verbose", global = true, parse(from_occurrences), env = args::SPFS_VERBOSITY)]
    pub verbose: usize,

    #[structopt(long = "runtime-dir")]
    runtime_root_dir: Option<String>,
    #[structopt(required = true)]
    cmd: Vec<OsString>,
}

impl CmdInit {
    pub fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        tracing::debug!("initializing runtime environment");
        let _handle = match &self.runtime_root_dir {
            Some(root) => {
                let runtime = spfs::runtime::Runtime::new(root)?;
                std::env::set_var("SPFS_RUNTIME", runtime.name());
                Some(spfs::runtime::OwnedRuntime::upgrade(runtime)?)
            }
            None => {
                std::env::remove_var("SPFS_RUNTIME");
                None
            }
        };

        exec_runtime_command(self.cmd.clone())
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
