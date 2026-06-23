// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use config::Environment;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use spfs::Sentry;

use crate::Result;

pub const FLATBUFFER_INDEX_TOKEN: &str = "flatb";

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

static CONFIG: OnceCell<RwLock<Arc<Config>>> = OnceCell::new();

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Metadata {
    /// List of commands that output global package metatdata in json to be
    /// collected and added to every built package
    pub global: Vec<MetadataCommand>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct MetadataCommand {
    /// List containing the executable and its arguments
    pub command: Vec<String>,
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

    /// Name of the solver, or all, to run when performing a solve
    pub solver_to_run: String,

    /// Name of the solver whose output to show  when multiple solvers are being run.
    pub solver_to_show: String,

    /// Whether to get the solver to use repository indexes, if
    /// available, instead of the repository directly.
    pub use_indexes: bool,

    /// Default setting for indexes, if using indexes is enabled for
    /// the solver.
    pub indexes: Index,
}

/// The settings for a spk repository index
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Index {
    /// Whether to validate the index data before using it.
    /// Validating is safer but can add overhead at the start of a
    /// solve that uses indexes.
    pub verify_before_use: bool,

    /// What kind of index to use. Only applies if there is more than
    /// one kind of index available for the repository. The default is
    /// 'flatb', a flatbuffers file based index.
    pub kind: String,

    /// Time to sleep between getting a write lock on the index data,
    /// for index generation and updates.
    pub lock_sleep_seconds: u64,

    /// Maximum number of times to try to get a write lock on the
    /// index data, for index generation and updates.
    pub lock_max_tries: u64,

    /// Name of the configured messaging channel (see MessageChannel)
    /// to send index status update messages too.
    pub update_message_channel: String,

    /// How often index generation and update processing should send
    /// index update event messages.
    pub update_event_send_freq_ms: u64,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            // Default to verifying indexes before using them. This is
            // safer but can add some overhead.
            verify_before_use: true,
            kind: String::from(FLATBUFFER_INDEX_TOKEN),
            // Sleeping for 6 seconds between write lock attempts and
            // allowing 5 tries before bailing out, gives a total of
            // about 30 seconds before timing out of getting a lock
            // for index writing.
            lock_sleep_seconds: 6,
            lock_max_tries: 5,
            update_message_channel: String::from("kafka"),
            // 5 seconds in milliseconds
            update_event_send_freq_ms: 5000,
        }
    }
}

/// The settings for a single repository
#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Repository {
    /// Whether to use an index with this repository, if one is
    /// available.
    pub use_index: bool,

    /// Setting for the repositories index, if an index is enabled.
    pub index: Index,
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

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Ls {
    /// Use all current host's host options by default for filtering in ls
    pub host_filtering: bool,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Cli {
    /// Entries for command line command that have configuration
    pub ls: Ls,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct DistroRule {
    /// The compat rule to set for the distro, e.g., "x.ab"
    pub compat_rule: Option<String>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct HostOptions {
    /// A mapping of distro names to recognize and customizations for each.
    pub distro_rules: HashMap<String, DistroRule>,
}

/// Helper for the default kafka message channel name, when not
/// specified in config.
fn default_kafka_channel_name() -> String {
    String::from("kafka")
}

/// Helper for a default kafka message timeout in ms, when not specified in
/// config file.
fn default_kafka_message_timeout_ms() -> u64 {
    // 5 seconds
    5 * 1000
}

/// Helper for a default kafka producer queue timeout in ms, when not specified in
/// config file.
fn default_kafka_producer_queue_timeout_ms() -> u64 {
    // 4 seconds
    4 * 1000
}

/// Helper for a default kafka index update listener's timeout in ms,
/// when not specified in config file.
fn default_kafka_index_update_listener_timeout_ms() -> u64 {
    // 10 seconds
    10 * 1000
}

/// Helper for a default kafka index update listener's session timeout
/// in ms, when not specified in config file.
fn default_kafka_index_update_listener_session_timeout_ms() -> u64 {
    // 10 seconds
    10 * 1000
}

/// Helper for a default kafka index update listener's maximum polling
/// timeout in ms, when not specified in config file.
fn default_kafka_index_update_listener_max_polling_interval_ms() -> u64 {
    // 10 seconds
    10 * 1000
}

/// Helper for a default kafka index update listener's recent past
/// duration amount in seconds, when not specified in config file.
fn default_kafka_index_update_listener_recent_past_duration_ms() -> i64 {
    // 2 minutes
    2 * 60 * 1000
}

/// Helper for a default kafka index update listener's broker fetch
/// timeout in seconds, when not specified in config file.
fn default_kafka_index_update_listener_broker_fetch_timeout_ms() -> u64 {
    // 20 seconds
    20 * 1000
}

/// Configuration for using Kafka as a message channel.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KafkaChannel {
    /// Name of this configured messaging system. Used to distinguish
    /// it from other configured messaging systems.
    #[serde(default = "default_kafka_channel_name")]
    pub name: String,
    /// List of brokers to connect to, specified as "hostname:port"
    pub brokers: Vec<String>,

    /// Topic name to push package updates to
    pub package_updates_topic_name: Option<String>,

    /// Topic name to push index status updates to
    pub index_updates_topic_name: Option<String>,

    /// Names of the SPK repositories that can send package update
    /// messages to the 'package_updates_topic_name'.
    pub repo_names: Vec<String>,

    /// Message sending timeout in milliseconds, defaults to
    /// 5000 ms (5 seconds)
    #[serde(default = "default_kafka_message_timeout_ms")]
    pub message_timeout_ms: u64,

    /// Producer sending queue timeout in milliseconds, defaults to
    /// 4000 ms (4 seconds). This determines how to long retry for if
    /// the producer queue is full.
    #[serde(default = "default_kafka_producer_queue_timeout_ms")]
    pub producer_queue_timeout_ms: u64,

    /// How long an index update message listener should wait after
    /// the last index update message before timing out and assuming
    /// no more updates are coming. Defaults to 10000 ms (10 seconds)
    #[serde(default = "default_kafka_index_update_listener_timeout_ms")]
    pub index_update_listener_timeout_ms: u64,

    /// The kafka session timeout for an index update listener in milliseconds
    #[serde(default = "default_kafka_index_update_listener_session_timeout_ms")]
    pub index_update_listener_session_timeout_ms: u64,

    /// The maximum polling interval for an index update listener in milliseconds
    #[serde(default = "default_kafka_index_update_listener_max_polling_interval_ms")]
    pub index_update_listener_max_polling_interval_ms: u64,

    /// How far back in time counts as recent for an index update
    /// listener when it is trying to workout if an indexer is running
    /// for the repository index it wants to know about, in milliseconds.
    #[serde(default = "default_kafka_index_update_listener_recent_past_duration_ms")]
    pub index_update_listener_recent_past_duration_ms: i64,

    /// The data fetching timeout used by an index update listener
    /// when querying a kafka broker, in milliseconds.
    #[serde(default = "default_kafka_index_update_listener_broker_fetch_timeout_ms")]
    pub index_update_listener_broker_fetch_timeout_ms: u64,
}

/// Types of message channels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum MessageChannel {
    Kafka(KafkaChannel),
}

/// Helper for a default kafka indexer heartbeat frequency in ms when
/// not specified in the config file.
fn default_indexer_heartbeat_freq_ms() -> u64 {
    // 1 minute
    60 * 1000
}

/// Helper for a default kafka indexer session timeout in ms when
/// not specified in the config file.
fn default_indexer_session_timeout_ms() -> u64 {
    // 2 minutes
    120 * 1000
}

/// Helper for a default kafka indexer's maximum polling interval in
/// ms, when not specified in the config file.
fn default_indexer_max_polling_interval_ms() -> u64 {
    // 24 hours
    86400000
}

/// Configuration for an index update server (an indexer)
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Indexer {
    /// Name of the configured message channel to use. The
    /// MessageChannel must have a publish modification topic, an
    /// index status update topic, and at least one repo name
    /// configured.
    pub message_channel_name: String,

    /// SPK repository name to listen for package modifications about.
    /// The name must be present in the configure MessageChannel's
    /// list of repo names. The indexer will ignore package updates
    /// for other repositories, and will only update the named
    /// repository's index,
    pub repo_name: String,

    /// How frequently this indexer should send heartbeat messages to
    /// the index status channel, when not updating an index. Defaults
    /// to 60000 ms (1 minute). When updating an index it will send
    /// index status messages more frequently.
    #[serde(default = "default_indexer_heartbeat_freq_ms")]
    pub heartbeat_freq_ms: u64,

    /// Indexer's kafka session timeout in milliseconds
    #[serde(default = "default_indexer_session_timeout_ms")]
    pub session_timeout_ms: u64,

    /// Indexer's kafka broker maximum polling interval in milliseconds
    #[serde(default = "default_indexer_max_polling_interval_ms")]
    pub max_polling_interval_ms: u64,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Graph {
    /// List of package names to ignore when making package connection graphs.
    pub ignore: Vec<String>,
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
    pub repositories: HashMap<String, Repository>,
    pub statsd: Statsd,
    pub metadata: Metadata,
    pub cli: Cli,
    pub host_options: HostOptions,
    pub messaging: Vec<MessageChannel>,
    pub indexers: HashMap<String, Indexer>,
    pub graph: Graph,
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

/// Get the current index config for the given repo name
pub fn get_index_config(repo_name: &str) -> Index {
    match get_config() {
        Ok(config) => {
            if let Some(repo_config) = config.repositories.get(repo_name) {
                repo_config.index.clone()
            } else {
                Index::default()
            }
        }
        Err(err) => {
            tracing::warn!("Unable to read spk config file, using Index defaults, due to: {err}");
            Index::default()
        }
    }
}

/// Get the index update server (indexer) config with the given name,
/// if one exits.
pub fn get_indexer_config(indexer_name: &str) -> Option<Indexer> {
    match get_config() {
        Ok(config) => config.indexers.get(indexer_name).cloned(),
        Err(err) => {
            tracing::warn!("Unable to read spk config file to get indexers config, due to: {err}");
            None
        }
    }
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

    let config = RawConfig::builder()
        // the system config can also be in any support format: toml, yaml, json, ini, etc
        .add_source(File::with_name("/etc/spk").required(false))
        // the user config can also be in any support format: toml, yaml, json, ini, etc
        .add_source(File::with_name(&format!("{}", user_config.display())).required(false))
        // Note: if a var using single underscores is set, it will have precedence
        .add_source(
            Environment::with_prefix("SPK")
                .prefix_separator("_")
                .separator("__"),
        )
        // for backwards compatibility with vars not using double underscores
        .add_source(Environment::with_prefix("SPK").separator("_"))
        .build()?;

    Ok(Config::deserialize(config)?)
}
