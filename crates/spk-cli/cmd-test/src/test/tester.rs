// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

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

        let tmpdir = tempfile::Builder::new()
            .prefix("spk-test")
            .tempdir()
            .map_err(Error::TempDirError)?;
        let script_path = tmpdir.path().join("test.sh");
        let mut script_file = std::fs::File::create(&script_path)
            .map_err(|err| Error::FileWriteError(script_path.to_owned(), err))?;
        script_file
            .write_all(self.script().as_bytes())
            .map_err(|err| Error::FileWriteError(script_path.to_owned(), err))?;
        script_file
            .sync_data()
            .map_err(|err| Error::FileWriteError(script_path.to_owned(), err))?;
        let cmd = spfs::build_shell_initialized_command(
            rt,
            Some("bash"),
            OsString::from("bash"),
            [OsString::from("-ex"), script_path.into_os_string()],
        )?;
        let mut cmd = cmd.into_std();
        let status = cmd
            .envs(env)
            .current_dir(source_dir)
            .env("SHELL", "bash")
            .status()
            .map_err(|err| {
                Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                    "bash".to_owned(),
                    err,
                    Some(source_dir.to_owned()),
                ))
            })?;
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
