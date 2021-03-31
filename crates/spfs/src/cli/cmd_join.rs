use spfs::Result;
use std::ffi::OsString;
use std::os::unix::io::AsRawFd;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct CmdJoin {
    #[structopt(about = "The name or id of the runtime to join")]
    runtime: String,
    #[structopt(about = "Optional command to run in the environment, spawns a shell if not given")]
    cmd: Vec<OsString>,
}

impl CmdJoin {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let storage = config.get_runtime_storage()?;
        let rt = storage.read_runtime(&self.runtime)?;
        let pid = match rt.get_pid() {
            None => return Err("Runtime has not been initialized".into()),
            Some(pid) => pid,
        };
        let ns_path = std::path::Path::new("/proc")
            .join(pid.to_string())
            .join("ns/mnt");
        tracing::debug!(?ns_path, "Getting process namespace");
        let (_fd_handle, ns_fd) = match std::fs::File::open(&ns_path) {
            Ok(file) => {
                let fd = file.as_raw_fd();
                (file, fd)
            }
            Err(err) => {
                return match err.kind() {
                    std::io::ErrorKind::NotFound => Err("Runtime does not exist".into()),
                    _ => Err(err.into()),
                }
            }
        };

        if let Err(err) = nix::sched::setns(ns_fd, nix::sched::CloneFlags::CLONE_NEWNS) {
            return Err(match err.as_errno() {
                Some(nix::errno::Errno::EPERM) => spfs::Error::new_errno(
                    libc::EPERM,
                    "spfs binary was not installed with required capabilities",
                ),
                _ => err.into(),
            });
        }

        std::env::set_var("SPFS_RUNTIME", rt.name());
        let result = exec_runtime_command(self.cmd.clone());
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
