use crate::{runtime, Result};

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
}
