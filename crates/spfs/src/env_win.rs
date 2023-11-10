use crate::tracking::EnvSpec;
use crate::{runtime, Error, Result};

pub const SPFS_DIR: &str = "C:\\spfs";
pub const SPFS_DIR_PREFIX: &str = "C:\\spfs";

/// Manages the configuration of an spfs runtime environment.
///
/// Specifically thing like, privilege escalation, mount namespace,
/// filesystem mounts, etc.
#[derive(Default)]
pub struct RuntimeConfigurator;

impl RuntimeConfigurator {
    /// Make this configurator for an existing runtime.
    pub fn current_runtime(self, _rt: &runtime::Runtime) -> Result<Self> {
        todo!()
    }

    /// Move this process into the namespace of an existing runtime
    pub fn join_runtime(self, _rt: &runtime::Runtime) -> Result<Self> {
        todo!()
    }

    /// Mount the provided runtime via the winfsp backend
    pub async fn mount_env_winfsp(&self, rt: &runtime::Runtime) -> Result<()> {
        let Some(root_pid) = rt.status.owner else {
            return Err(Error::RuntimeNotInitialized(
                "Missing owner in runtime, cannot initialize".to_string(),
            ));
        };

        let env_spec = rt
            .status
            .stack
            .iter()
            .copied()
            .collect::<EnvSpec>()
            .to_string();

        let exe =
            crate::which_spfs("winfsp").ok_or_else(|| Error::MissingBinary("spfs-winfsp.exe"))?;
        let mut cmd = tokio::process::Command::new(exe);
        cmd.arg("mount")
            .arg("--root-process")
            .arg(root_pid.to_string())
            .arg(env_spec);
        tracing::debug!("{cmd:?}");
        let status = cmd.status().await;
        match status {
            Err(err) => Err(Error::process_spawn_error("spfs-winfsp", err, None)),
            Ok(st) if st.success() => Ok(()),
            Ok(st) => Err(Error::String(format!(
                "Failed to mount winfsp filesystem, mount command exited with non-zero status {:?}",
                st.code()
            ))),
        }
    }
}
