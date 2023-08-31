// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::{Arc, RwLock};

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::Result;

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

static CONFIG: OnceCell<RwLock<Arc<Config>>> = OnceCell::new();

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Sentry {
    /// Sentry DSN
    pub dsn: String,

    /// Sentry environment
    pub environment: Option<String>,

    /// Environment variable name to use as sentry username, if set.
    ///
    /// This is useful in CI if the CI system has a variable that contains
    /// the username of the person who triggered the build.
    pub username_override_var: Option<String>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CustomMetadata {
    /// The path to the command to run
    pub command: Option<String>,

    /// Arguments to run with the command
    pub args: Option<Vec<String>>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Solver {
    /// If true, the solver will run impossible request checks on the initial requests
    pub check_impossible_initial: bool,

    /// If true, the solver will run impossible request checks before
    /// using a package build to resolve a request
    pub check_impossible_validation: bool,

    /// If true, the solver will run impossible request checks to
    /// use in the build keys for ordering builds during the solve
    pub check_impossible_builds: bool,

    /// Increase the solver's verbosity every time this many seconds pass
    ///
    /// A solve has taken too long if it runs for more than this
    /// number of seconds and hasn't found a solution. Setting this
    /// above zero will increase the verbosity every that many seconds
    /// the solve runs. If this is zero, the solver's verbosity will
    /// not increase during a solve.
    pub too_long_seconds: u64,

    /// The maximum verbosity that automatic verbosity increases will
    /// stop at and not go above.
    pub verbosity_increase_limit: u8,

    /// Maximum number of seconds to let the solver run before halting the solve
    ///
    /// Maximum number of seconds to allow a solver to run before
    /// halting the solve. If this is zero, which is the default, the
    /// timeout is disabled and the solver will run to completion.
    pub solve_timeout: u64,

    /// Set the threshold of a longer than acceptable solves, in seconds.
    pub long_solve_threshold: u64,

    /// Set the limit for how many of the most frequent errors are
    /// displayed in solve stats reports
    pub max_frequent_errors: usize,

    /// Comma-separated list of option names to promote to the front of the
    /// build key order.
    pub build_key_name_order: String,

    /// Comma-separated list of option names to promote to the front of the
    /// resolve order.
    pub request_priority_order: String,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Statsd {
    /// Host name of the statsd server
    pub host: String,

    /// Port number of the statsd server
    pub port: u16,

    /// Prefix to add to all statsd metrics
    pub prefix: String,

    /// Format to use for statsd metrics
    pub format: String,
}

/// Configuration values for spk.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    // These sub-types should aim to only have one level of
    // values within them, otherwise they become impossible to address
    // with environment variables.
    pub sentry: Sentry,
    pub solver: Solver,
    pub statsd: Statsd,
    pub custom_metadata: CustomMetadata,
}

impl Config {
    /// Get the current loaded config, loading it if needed
    pub fn current() -> Result<Arc<Self>> {
        get_config()
    }

    /// Load the config from disk, even if it's already been loaded before
    pub fn load() -> Result<Self> {
        load_config()
    }

    /// Make this config the current global one
    pub fn make_current(self) -> Result<Arc<Self>> {
        // Note we don't know if we won the race to set the value here,
        // so we still need to try to update it.
        let config = CONFIG.get_or_try_init(|| -> Result<RwLock<Arc<Config>>> {
            Ok(RwLock::new(Arc::new(self.clone())))
        })?;

        let mut lock = config
            .write()
            .map_err(|err| crate::Error::LockPoisonedWrite(err.to_string()))?;
        *Arc::make_mut(&mut lock) = self;
        Ok(Arc::clone(&lock))
    }
}

/// Get the current spk config, fetching it from disk if needed.
pub fn get_config() -> Result<Arc<Config>> {
    let config = CONFIG.get_or_try_init(|| -> Result<RwLock<Arc<Config>>> {
        Ok(RwLock::new(Arc::new(load_config()?)))
    })?;
    let lock = config
        .read()
        .map_err(|err| crate::Error::LockPoisonedRead(err.to_string()))?;
    Ok(Arc::clone(&*lock))
}

/// Load the spk configuration from disk, even if it has already been loaded.
///
/// This includes the default, user, and system configurations (if they exist).
pub fn load_config() -> Result<Config> {
    use config::{Config as RawConfig, File};

    const USER_CONFIG_BASE: &str = "spk/spk";
    let user_config = dirs::config_local_dir()
        .map(|config| config.join(USER_CONFIG_BASE))
        .ok_or_else(|| {
            crate::Error::Config(config::ConfigError::NotFound(
                "User config area could not be found, this platform may not be supported".into(),
            ))
        })?;

    let mut config_builder = RawConfig::builder()
        // the system config can also be in any support format: toml, yaml, json, ini, etc
        .add_source(File::with_name("/etc/spk").required(false))
        // the user config can also be in any support format: toml, yaml, json, ini, etc
        .add_source(File::with_name(&format!("{}", user_config.display())).required(false));

    for (var, value) in std::env::vars() {
        let Some(tail) = var.strip_prefix("SPK_") else {
            continue;
        };
        let Some((section, name)) = tail.split_once('_') else {
            // typically, a value with no section is not a configuration
            // value, and can be skipped (eg: SPK_LOG)
            continue;
        };

        let key = format!("{}.{}", section.to_lowercase(), name.to_lowercase());
        config_builder = config_builder.set_override(key, value)?;
    }

    let config = config_builder.build()?;
    Ok(Config::deserialize(config)?)
}
