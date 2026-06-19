// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod config;
mod error;
#[cfg(feature = "statsd")]
mod metrics;

pub use error::{Error, Result};
#[cfg(feature = "statsd")]
pub use metrics::{
    MetricsClient,
    SPK_ERROR_COUNT_METRIC,
    SPK_INDEXER_HEARTBEAT_METRIC,
    SPK_INDEXER_INDEX_UPDATE_METRIC,
    SPK_RUN_COUNT_METRIC,
    SPK_RUN_TIME_METRIC,
    SPK_SOLUTION_PACKAGE_COUNT_METRIC,
    SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC,
    SPK_SOLVER_RUN_COUNT_METRIC,
    SPK_SOLVER_RUN_TIME_METRIC,
    SPK_SOLVER_SOLUTION_SIZE_METRIC,
    get_metrics_client,
};

pub use self::config::*;
