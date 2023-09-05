// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use statsd::client::Pipeline;
use statsd::Client;

use crate::{Error, Result};

const VERSION: &str = env!("CARGO_PKG_VERSION");

// Value that denotes the naming format supported by statsd
const STATSD_FORMAT: &str = "statsd";
// Value that denotes the librato naming/tagging format supported by
// statsd-exporter. For more details, see:
// https://github.com/prometheus/statsd_exporter#tagging-extensions
const LIBRATO_FORMAT: &str = "statsd-exporter-librato";

static METRICS_CLIENT: Lazy<Option<MetricsClient>> = Lazy::new(|| {
    let Ok(config) = spk_config::get_config() else {
        return None;
    };
    let statsd_config = &config.statsd;

    let Ok(statsd_format) = StatsdFormat::from_str(&statsd_config.format) else {
        return None;
    };

    let host_port = format!(
        "{host}:{port}",
        host = statsd_config.host,
        port = statsd_config.port
    );
    let statsd_client = match Client::new(host_port.clone(), &statsd_config.prefix) {
        Ok(c) => Some(c),
        Err(err) => {
            // If anything goes wrong, sending metrics to statsd is disabled.
            println!("Warning: statsd metrics disabled because {host_port}: {err}");
            None
        }
    };

    let args: Vec<_> = std::env::args().collect();
    // The second thing on the command line is the spk command name,
    // if the program being run is spk. But this might not always be
    // called from a spk command line.
    let program = args[0].clone();
    let command = if program.ends_with("spk") {
        args[1].clone()
    } else {
        "".to_string()
    };

    Some(MetricsClient::new(command, statsd_format, statsd_client))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_RUN_COUNT_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_RUN_COUNT_METRIC").unwrap_or_else(|_| String::from("spk.run_count"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_RUN_TIME_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_RUN_TIME_METRIC").unwrap_or_else(|_| String::from("spk.run_time"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_ERROR_COUNT_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_ERROR_COUNT_METRIC").unwrap_or_else(|_| String::from("spk.error_count"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_SOLUTION_PACKAGE_COUNT_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_SOLUTION_PACKAGE_COUNT_METRIC")
        .unwrap_or_else(|_| String::from("spk.solution_package_count"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_SOLVER_RUN_COUNT_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_SOLVER_RUN_COUNT_METRIC")
        .unwrap_or_else(|_| String::from("spk.solver_run_count"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_SOLVER_RUN_TIME_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_SOLVER_RUN_TIME_METRIC")
        .unwrap_or_else(|_| String::from("spk.solver_run_time"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC")
        .unwrap_or_else(|_| String::from("spk.solver_initial_requests_count"))
});

// TODO: add the default value to a config file, once spk has one
pub static SPK_SOLVER_SOLUTION_SIZE_METRIC: Lazy<String> = Lazy::new(|| {
    std::env::var("SPK_SOLVER_SOLUTION_SIZE_METRIC")
        .unwrap_or_else(|_| String::from("spk.solver_solution_size_count"))
});

/// Supported metrics naming formats
///
enum StatsdFormat {
    Statsd,
    Librato,
}

impl FromStr for StatsdFormat {
    type Err = Error;

    fn from_str(input: &str) -> Result<StatsdFormat> {
        match input {
            STATSD_FORMAT => Ok(StatsdFormat::Statsd),
            LIBRATO_FORMAT => Ok(StatsdFormat::Librato),
            _ => {
                let valid_values = [StatsdFormat::Statsd, StatsdFormat::Librato]
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(", ");
                Err(Error::String(format!("Unsupported statsd metric format: {input}. Please specify SPK_STATSD_FORMAT as one of: {valid_values}")))
            }
        }
    }
}

impl ToString for StatsdFormat {
    fn to_string(&self) -> String {
        match self {
            StatsdFormat::Statsd => STATSD_FORMAT.to_string(),
            StatsdFormat::Librato => LIBRATO_FORMAT.to_string(),
        }
    }
}

/// Helper struct for managing statsd connections and sending spk
/// metrics.
pub struct MetricsClient {
    start_time: Instant,
    command: String,
    statsd_format: StatsdFormat,
    statsd_client: Option<Client>,
}

impl MetricsClient {
    fn new(command: String, statsd_format: StatsdFormat, statsd_client: Option<Client>) -> Self {
        Self {
            start_time: Instant::now(),
            command,
            statsd_format,
            statsd_client,
        }
    }

    /// Get the time this metrics client was created, for us in timers
    pub fn start_time(&self) -> &Instant {
        &self.start_time
    }

    /// Helper to send a timer metric using the duration between the
    /// start_time() and now.
    pub fn record_duration_from_start(&self, metric: &str) {
        self.timer(metric, self.start_time.elapsed());
    }

    /// Increment a count metric immediately
    pub fn incr(&self, metric: &str) {
        self.count(metric, 1.0);
    }

    /// Increment a count metric immediately, and include some extra
    /// labels for prometheus
    pub fn incr_with_extra_labels(&self, metric: &str, labels: &Vec<String>) {
        self.count_with_extra_labels(metric, 1.0, labels);
    }

    /// Add a number to a count metric immediately
    pub fn count(&self, metric: &str, value: f64) {
        if let Some(client) = &self.statsd_client {
            client.count(&self.format_metric_name(metric, None), value);
        }
    }
    /// Add a number to a count metric immediately, and include some
    /// extra labels for prometheus
    pub fn count_with_extra_labels(&self, metric: &str, value: f64, labels: &Vec<String>) {
        if let Some(client) = &self.statsd_client {
            client.count(&self.format_metric_name(metric, Some(labels)), value);
        }
    }

    /// Record a timer metric immediately
    pub fn timer(&self, metric: &str, value: Duration) {
        if let Some(client) = &self.statsd_client {
            client.timer(
                &self.format_metric_name(metric, None),
                value.as_secs_f64() * 1000.0,
            );
        }
    }

    /// Return the appropriate statsd metric name for the given
    /// name. This will automatically add the default labels and
    /// values (command, version) and any extra labels it is given.
    pub fn format_metric_name(&self, base_name: &str, labels: Option<&Vec<String>>) -> String {
        match self.statsd_format {
            StatsdFormat::Statsd => {
                // Plain statsd metric naming, only uses the
                // base_name, does not use any labels.
                base_name.to_string()
            }
            StatsdFormat::Librato => {
                // Librato format supported by statsd-exporter for
                // prometheus. Uses the base_name and labels, with
                // command and version values included automatically.

                // TODO: make the labels configurable, a list of known
                // things to add? user, show? If so, add to config file
                // once spk has one
                let mut metric_labels: Vec<String> = Vec::from([
                    format!("command={}", self.command),
                    format!("version={}", VERSION),
                ]);

                if let Some(extra_labels) = labels {
                    for label in extra_labels {
                        metric_labels.push(label.clone());
                    }
                };

                format!("{base_name}#{}", metric_labels.join(","))
            }
        }
    }

    /// Returns a new statsd pipeline, if there is a valid statsd
    /// client. It is up to the caller to check if the pipeline is
    /// valid and call metrics functions on it, before calling
    /// pipeline.send().
    pub fn start_a_pipeline(&self) -> Option<Pipeline> {
        self.statsd_client.as_ref().map(|client| client.pipeline())
    }

    /// Add and increment a count metric to a pipeline. This does not
    /// send the metric until pipeline_send() is called with the pipeline.
    pub fn pipeline_incr(&self, pipeline: &mut Pipeline, metric: &str) {
        pipeline.incr(&self.format_metric_name(metric, None));
    }

    /// Add and add a value to a count metric to a pipeline. This does
    /// not send the metric until pipeline_send() is called with the pipeline.
    pub fn pipeline_count(&self, pipeline: &mut Pipeline, metric: &str, value: f64) {
        pipeline.count(&self.format_metric_name(metric, None), value);
    }

    /// Add and set a timer metric to a pipeline. This does not send
    /// the metric until pipeline_send() is called with the pipeline.
    pub fn pipeline_timer(&self, pipeline: &mut Pipeline, metric: &str, value: Duration) {
        pipeline.timer(
            &self.format_metric_name(metric, None),
            value.as_secs_f64() * 1000.0,
        );
    }

    /// Add and increment of a count metric to a pipeline with extra
    /// labels. This does not send the metric until pipeline_send() is
    /// called with the pipeline.
    pub fn pipeline_incr_with_extra_labels(
        &self,
        pipeline: &mut Pipeline,
        metric: &str,
        labels: &Vec<String>,
    ) {
        pipeline.incr(&self.format_metric_name(metric, Some(labels)));
    }

    /// Send all the metrics in the pipeline to statsd.
    pub fn pipeline_send(&self, mut pipeline: Pipeline) {
        if let Some(client) = &self.statsd_client {
            pipeline.send(client);
        }
    }
}

/// Return a configured metrics statsd client
pub fn get_metrics_client() -> Option<&'static MetricsClient> {
    METRICS_CLIENT.as_ref()
}
