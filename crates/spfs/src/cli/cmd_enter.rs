use std::ffi::OsString;
use std::path::PathBuf;

use structopt::StructOpt;

use spfs;

#[macro_use]
mod args;

main!(CmdEnter, sentry = false);

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spfs-enter",
    about = "Run a command in a configured spfs runtime"
)]
pub struct CmdEnter {
    #[structopt(short = "v", long = "verbose", global = true, parse(from_occurrences))]
    pub verbose: usize,

    #[structopt(
        long = "edit",
        short = "e",
        about = "make the mounted filesytem editablespfs"
    )]
    editable: bool,
    #[structopt(
        short = "r",
        long = "remount",
        about = "remount the overlay filesystem, don't enter a new namepace"
    )]
    remount: bool,
    #[structopt(
        short = "t",
        long = "tmpfs-opts",
        default_value = "size=50%",
        about = "options for the tmpfs mount in which all edits live"
    )]
    tmpfs_opts: String,
    #[structopt(
        short = "d",
        long = "dir-layer",
        about = "Include the given directory in the overlay mount (can be specified more than once)"
    )]
    dirs: Vec<PathBuf>,
    #[structopt(
        short = "m",
        long = "mask",
        about = "mask a filepath so it does not appear in the mounted filesystem (can be specified more than once)"
    )]
    masks: Vec<PathBuf>,

    #[structopt()]
    pub cmd: Option<OsString>,
    #[structopt()]
    pub args: Vec<OsString>,
}

impl CmdEnter {
    pub fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        if self.remount {
            self.remount_current_environment()?;
            Ok(0)
        } else {
            self.enter_new_environment()
        }
    }

    fn remount_current_environment(&self) -> spfs::Result<()> {
        let original = spfs::env::become_root()?;
        spfs::env::ensure_mounts_already_exist()?;
        spfs::env::unmount_env()?;
        spfs::env::setup_runtime(self.editable)?;
        spfs::env::unlock_runtime(None)?;
        spfs::env::mount_env(&self.dirs)?;
        spfs::env::mask_files(&self.masks)?;
        spfs::env::set_runtime_lock(self.editable, None)?;
        spfs::env::become_original_user(original)?;
        spfs::env::drop_all_capabilities()?;
        Ok(())
    }

    fn enter_new_environment(&mut self) -> spfs::Result<i32> {
        let cmd = match self.cmd.take() {
            Some(cmd) => cmd,
            None => return Err("command is required and was not given".into()),
        };

        spfs::env::enter_mount_namespace()?;
        let original = spfs::env::become_root()?;
        spfs::env::privatize_existing_mounts()?;
        spfs::env::ensure_mount_targets_exist()?;
        spfs::env::mount_runtime(None)?;
        spfs::env::setup_runtime(self.editable)?;
        spfs::env::mount_env(&self.dirs)?;
        spfs::env::mask_files(&self.masks)?;
        spfs::env::set_runtime_lock(self.editable, None)?;
        spfs::env::become_original_user(original)?;
        spfs::env::drop_all_capabilities()?;

        tracing::trace!("{:?} {:?}", cmd, self.args);
        use std::os::unix::ffi::OsStrExt;
        let cmd = std::ffi::CString::new(cmd.as_bytes()).unwrap();
        let mut args: Vec<_> = self
            .args
            .iter()
            .map(|arg| std::ffi::CString::new(arg.as_bytes()).unwrap())
            .collect();
        args.insert(0, cmd.clone());
        nix::unistd::execv(cmd.as_ref(), args.as_slice())?;
        Ok(0)
    }
}
