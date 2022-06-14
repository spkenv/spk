// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use clap::Parser;
use tokio::signal::unix::{signal, SignalKind};

#[macro_use]
mod args;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
main!(CmdEnter, sentry = false, sync = true);

/// Run a command in a configured spfs runtime
#[derive(Debug, Parser)]
#[clap(name = "spfs-enter")]
pub struct CmdEnter {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Remount the overlay filesystem, don't enter a new namespace
    #[clap(short, long)]
    remount: bool,

    /// The address of the storage being used for runtimes
    ///
    /// Defaults to the current configured local repository.
    #[clap(long)]
    runtime_storage: Option<url::Url>,

    /// The name of the runtime being entered
    #[clap(long)]
    #[cfg(feature = "runtime-compat-0.33")]
    runtime: Option<String>,
    #[cfg(not(feature = "runtime-compat-0.33"))]
    runtime: String,

    /// The command to run after initialization
    ///
    /// If not given, run an interactive shell environment
    command: Option<OsString>,

    /// Additional arguments to provide to the command
    ///
    /// In order to ensure that flags are passed as-is, place '--' before
    /// specifying any flags that should be given to the subcommand:
    ///   eg spfs enter <args> -- command --flag-for-command
    args: Vec<OsString>,
}

impl CmdEnter {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        // we need a single-threaded runtime in order to properly setup
        // and enter the namespace of the runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                spfs::Error::String(format!("Failed to establish async runtime: {:?}", err))
            })?;
        let res = rt.block_on(self.run_async(config));
        // do not block forever on drop because of any stuck blocking tasks
        rt.shutdown_timeout(std::time::Duration::from_millis(250));
        res
    }

    pub async fn run_async(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime = self.load_runtime(config).await?;
        if self.remount {
            spfs::reinitialize_runtime(&runtime).await?;
            Ok(0)
        } else {
            let mut terminate = signal(SignalKind::terminate())?;
            let mut interrupt = signal(SignalKind::interrupt())?;
            let mut quit = signal(SignalKind::quit())?;
            let owned = spfs::runtime::OwnedRuntime::upgrade_as_owner(runtime).await?;

            if let Err(err) = spfs::env::spawn_monitor_for_runtime(&owned) {
                if let Err(err) = owned.delete().await {
                    tracing::error!(
                        ?err,
                        "failed to cleanup runtime data after failure to start monitor"
                    );
                }
                return Err(err);
            }

            tracing::debug!("initializing runtime");
            spfs::initialize_runtime(&owned).await?;

            owned.ensure_startup_scripts()?;
            std::env::set_var("SPFS_RUNTIME", owned.name());

            let mut child = self.exec_runtime_command(&owned).await?;
            let res = loop {
                tokio::select! {
                    res = child.wait() => break res,
                    // we explicitly catch and ignore signals related to interruption
                    // assuming that the child process will receive them and act
                    // accordingly. This is also to ensure that we never exit before
                    // the child and forget to clean up the runtime data
                    _ = terminate.recv() => {},
                    _ = interrupt.recv() => {},
                    _ = quit.recv() => {},
                }
            };

            Ok(res?.code().unwrap_or(1))
        }
    }

    #[cfg(not(feature = "runtime-compat-0.33"))]
    async fn load_runtime(&self, config: &spfs::Config) -> spfs::Result<spfs::runtime::Runtime> {
        let repo = match &self.runtime_storage {
            Some(address) => spfs::open_repository(address).await?,
            None => config.get_local_repository_handle().await?,
        };
        let storage = spfs::runtime::Storage::new(repo);
        storage.read_runtime(&self.runtime).await
    }

    #[cfg(feature = "runtime-compat-0.33")]
    async fn load_runtime(
        &mut self,
        config: &spfs::Config,
    ) -> spfs::Result<spfs::runtime::Runtime> {
        use std::str::FromStr;

        let given_name = match &self.runtime {
            Some(name) => name.to_owned(),
            None => {
                let name = self
                    .command
                    .take()
                    .ok_or_else(|| spfs::Error::new("Target runtime name must be provided"))?
                    .to_string_lossy()
                    .to_string();
                if !self.args.is_empty() {
                    self.command = Some(self.args.remove(0))
                }
                name
            }
        };

        // Handle old-style invocation where the runtime argument is
        // a path on disk rather than a bare uuid name.
        //
        // This can be in multiple forms...

        //   - "$SPFS_STORAGE_RUNTIMES/<name>".
        let name = if std::env::var("SPFS_STORAGE_RUNTIMES")
            .map(|s| {
                given_name.starts_with(
                    // This should technically add a trailing slash (unless there
                    // is already a trailing slash in the value).
                    &s,
                )
            })
            .unwrap_or(false)
        {
            // In this case, the "runtime_storage" (repo) is not this place, only
            // the runtime configuration is here.
            std::path::PathBuf::from(&given_name)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        } else {
            match *given_name.rsplitn(3, '/').collect::<Vec<_>>().as_slice() {
                //   - "<storage path>/runtimes/<name>".
                [name, _, storage] => {
                    self.runtime_storage =
                        Some(url::Url::from_str(&format!("file://{}", storage))?);
                    name.to_owned()
                }
                _ => given_name.clone(),
            }
        };

        let repo = match &self.runtime_storage {
            Some(address) => spfs::open_repository(address).await?,
            None => config.get_local_repository_handle().await?,
        };
        let storage = spfs::runtime::Storage::new(repo);
        let mut runtime = storage.read_runtime(name).await?;

        let legacy_config_path = std::path::Path::new(&given_name);
        if legacy_config_path.is_absolute() {
            // in cases where a legacy runtime came with a full path, we
            // will load the old config file and pull any potential
            // configuration changes back into the new format
            let legacy_config = spfs::runtime::storage_033::Runtime::new(legacy_config_path)?;
            legacy_config.apply_to(&mut runtime);
            runtime.save_state_to_storage().await?;
        }
        Ok(runtime)
    }

    async fn exec_runtime_command(
        &mut self,
        rt: &spfs::runtime::OwnedRuntime,
    ) -> spfs::Result<tokio::process::Child> {
        let cmd = match self.command.take() {
            Some(exe) if !exe.is_empty() => {
                tracing::debug!("executing runtime command");
                spfs::build_shell_initialized_command(rt, exe, self.args.drain(..))?
            }
            _ => {
                tracing::debug!("starting interactive shell environment");
                spfs::build_interactive_shell_command(rt)?
            }
        };
        let mut proc = cmd.into_tokio();
        tracing::debug!("{:?}", proc);
        Ok(proc.spawn()?)
    }
}
