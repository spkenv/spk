// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};

pub fn configure_sentry() {
    // from sentry_sdk.integrations.stdlib import StdlibIntegration
    // from sentry_sdk.integrations.excepthook import ExcepthookIntegration
    // from sentry_sdk.integrations.dedupe import DedupeIntegration
    // from sentry_sdk.integrations.atexit import AtexitIntegration
    // from sentry_sdk.integrations.logging import LoggingIntegration
    // from sentry_sdk.integrations.argv import ArgvIntegration
    // from sentry_sdk.integrations.modules import ModulesIntegration
    // from sentry_sdk.integrations.threading import ThreadingIntegration

    // sentry_sdk.init(
    //     "http://4506b47108ac4b648fdf18a8d803f403@sentry.k8s.spimageworks.com/25",
    //     environment=os.getenv("SENTRY_ENVIRONMENT", "production"),
    //     release=spk.__version__,
    //     default_integrations=False,
    //     integrations=[
    //         StdlibIntegration(),
    //         ExcepthookIntegration(),
    //         DedupeIntegration(),
    //         AtexitIntegration(),
    //         LoggingIntegration(),
    //         ArgvIntegration(),
    //         ModulesIntegration(),
    //         ThreadingIntegration(),
    //     ],
    // )
    // with sentry_sdk.configure_scope() as scope:
    //     username = getpass.getuser()
    //     scope.user = {"email": f"{username}@imageworks.com", "username": username}
    todo!();
}

pub fn configure_logging(verbosity: u32) -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    let mut directives = match verbosity {
        0 => "spk=info,spfs=warn",
        1 => "spk=debug,spfs=info",
        2 => "spk=trace,spfs=debug",
        _ => "spk=trace,spfs=trace",
    }
    .to_string();
    if let Ok(overrides) = std::env::var("SPK_LOG") {
        // this is a common scenario because spk often calls itself
        if directives != overrides {
            directives = format!("{},{}", directives, overrides);
        }
    }
    std::env::set_var("SPK_LOG", &directives);
    if let Ok(overrides) = std::env::var("RUST_LOG") {
        // we also allow a full override via the RUST_LOG variable for debugging
        directives = overrides;
    }
    // this is not ideal, because it can propagate annoyingly into
    // created environments, but without it the spfs logging configuration
    // takes over in it's current setup/state.
    std::env::set_var("RUST_LOG", &directives);
    let env_filter = tracing_subscriber::filter::EnvFilter::new(directives);
    let registry = tracing_subscriber::Registry::default().with(env_filter);
    let mut fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .without_time();
    if verbosity < 3 {
        fmt_layer = fmt_layer.with_target(false);
    }
    let sub = registry.with(fmt_layer);
    tracing::subscriber::set_global_default(sub).context("Failed to set default logger")
}
