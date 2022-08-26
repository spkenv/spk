// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashMap, ffi::OsString, io::Write, path::Path};

use spfs::runtime::Runtime;
use spk_cli_common::{Error, Result, TestError};

/// Common code and logic for all test flavors.
#[async_trait::async_trait]
pub trait Tester: Send {
    /// Create the runtime environment for the defined test and then execute
    /// the test.
    async fn test(&mut self) -> Result<()>;

    /// Generate and invoke the test script defined in the recipe.
    fn execute_test_script(
        &self,
        source_dir: &Path,
        mut env: HashMap<String, String>,
        rt: &Runtime,
    ) -> Result<()> {
        env.insert(
            "PREFIX".to_string(),
            self.prefix()
                .to_str()
                .ok_or_else(|| {
                    Error::String("Test prefix must be a valid unicode string".to_string())
                })?
                .to_string(),
        );

        let tmpdir = tempfile::Builder::new().prefix("spk-test").tempdir()?;
        let script_path = tmpdir.path().join("test.sh");
        let mut script_file = std::fs::File::create(&script_path)?;
        script_file.write_all(self.script().as_bytes())?;
        script_file.sync_data()?;
        // TODO: this should be more easily configurable on the spfs side
        std::env::set_var("SHELL", "bash");
        let cmd = spfs::build_shell_initialized_command(
            rt,
            OsString::from("bash"),
            &[OsString::from("-ex"), script_path.into_os_string()],
        )?;
        let mut cmd = cmd.into_std();
        let status = cmd.envs(env).current_dir(source_dir).status()?;
        if !status.success() {
            Err(TestError::new_error(format!(
                "Test script returned non-zero exit status: {}",
                status.code().unwrap_or(1)
            )))
        } else {
            Ok(())
        }
    }

    /// Return the root path of the overlayfs
    fn prefix(&self) -> &Path;

    /// Return the text of the test script.
    fn script(&self) -> &String;
}
