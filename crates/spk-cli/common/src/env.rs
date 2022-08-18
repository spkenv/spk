// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    ffi::{OsStr, OsString},
    sync::Arc,
};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use spk_schema::foundation::spec_ops::PackageOps;
use spk_schema::ident::{parse_ident, PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy};
use spk_solve::solution::{PackageSource, Solution};
use spk_storage::{self as storage};

use crate::Error;

/// Load the current environment from the spfs file system.
pub async fn current_env() -> crate::Result<Solution> {
    match spfs::active_runtime().await {
        Err(spfs::Error::NoActiveRuntime) => {
            return Err(Error::NoEnvironment);
        }
        Err(err) => return Err(err.into()),
        Ok(_) => {}
    }

    let repo = Arc::new(storage::RepositoryHandle::Runtime(Default::default()));
    let mut solution = Solution::new(None);
    for name in repo.list_packages().await? {
        for version in repo.list_package_versions(&name).await?.iter() {
            let pkg = parse_ident(format!("{name}/{version}"))?;
            for pkg in repo.list_package_builds(&pkg).await? {
                let spec = repo.read_package(&pkg).await?;
                let components = match repo.read_components(spec.ident()).await {
                    Ok(c) => c,
                    Err(spk_storage::Error::SpkValidatorsError(
                        spk_schema::validators::Error::PackageNotFoundError(_),
                    )) => {
                        tracing::info!("Skipping missing build {pkg}; currently being built?");
                        continue;
                    }
                    Err(err) => return Err(err.into()),
                };
                let range_ident = RangeIdent::equals(spec.ident(), components.keys().cloned());
                let mut request = PkgRequest::new(range_ident, RequestedBy::CurrentEnvironment);
                request.prerelease_policy = PreReleasePolicy::IncludeAll;
                let repo = repo.clone();
                solution.add(
                    &request,
                    spec,
                    PackageSource::Repository { repo, components },
                );
            }
        }
    }

    Ok(solution)
}

#[cfg(feature = "sentry")]
pub fn configure_sentry() -> sentry::ClientInitGuard {
    // Call this before `sentry::init` to avoid potential `SIGSEGV`.
    let username = get_username_for_sentry();

    // When using the sentry feature it is expected that the DSN
    // and other configuration is provided at *compile* time.
    let guard = sentry::init((
        option_env!("SENTRY_DSN"),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: option_env!("SENTRY_ENVIRONMENT")
                .map(ToString::to_string)
                .map(std::borrow::Cow::Owned),
            before_send: Some(std::sync::Arc::new(|mut event| {
                // Remove ansi color codes from the event message
                if let Some(message) = event.message {
                    event.message = Some(remove_ansi_escapes(message));
                }
                Some(event)
            })),
            before_breadcrumb: Some(std::sync::Arc::new(|mut breadcrumb| {
                // Remove ansi color codes from the breadcrumb message
                if let Some(message) = breadcrumb.message {
                    breadcrumb.message = Some(remove_ansi_escapes(message));
                }
                Some(breadcrumb)
            })),
            ..Default::default()
        },
    ));

    let (command, data) = get_spk_context();

    sentry::configure_scope(|scope| {
        scope.set_user(Some(sentry::User {
            username: Some(username),
            ..Default::default()
        }));

        // Tags are searchable
        scope.set_tag("command", command);
        // Contexts are not searchable
        scope.set_context("SPK", sentry::protocol::Context::Other(data));

        // Okay for captured errors/anyhow, not good for direct
        // messages because they have no error value
        scope.set_fingerprint(Some(["{{ error.value }}"].as_ref()));
    });

    guard
}

#[cfg(feature = "sentry")]
fn get_username_for_sentry() -> String {
    // If this is being run from a gitlab CI job, then return the
    // username of the person that triggered the job. Otherwise get
    // the username of the person who ran this spk instance.
    if let Ok(value) = std::env::var("GITLAB_USER_LOGIN") {
        value
    } else {
        // Call this before `sentry::init` to avoid potential `SIGSEGV`.
        whoami::username()
    }
}

#[cfg(feature = "sentry")]
fn get_spk_context() -> (
    String,
    std::collections::BTreeMap<String, serde_json::Value>,
) {
    use serde_json::json;

    let args: Vec<_> = std::env::args().collect();
    let program = args[0].clone();
    let command = args[1].clone();
    let cwd = std::env::current_dir().ok();

    let mut data = std::collections::BTreeMap::new();
    data.insert(String::from("program"), json!(program));
    data.insert(String::from("command"), json!(command));
    data.insert(String::from("args"), json!(args.join(" ")));
    data.insert(String::from("cwd"), json!(cwd));

    let env_vars_to_add = vec!["SPK_BIN_TAG", "CI_JOB_URL"];
    for env_var in env_vars_to_add {
        if let Ok(value) = std::env::var(env_var) {
            data.insert(String::from(env_var), json!(value));
        }
    }

    (command, data)
}

#[cfg(feature = "sentry")]
fn remove_ansi_escapes(message: String) -> String {
    if let Ok(b) = strip_ansi_escapes::strip(message.clone()) {
        if let Ok(s) = std::str::from_utf8(&b) {
            return s.to_string();
        }
    }
    message
}

pub fn configure_logging(verbosity: u32) -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    let mut directives = match verbosity {
        0 => "spk=info,spfs=warn",
        1 => "spk=debug,spfs=info",
        2 => "spk=trace,spfs=debug",
        3..=6 => "spk=trace,spfs=trace,build_sort=info",
        _ => "spk=trace,spfs=trace,build_sort=debug",
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

    #[cfg(not(feature = "sentry"))]
    let sub = registry.with(fmt_layer);

    #[cfg(feature = "sentry")]
    let sub = registry.with(fmt_layer).with(sentry_tracing::layer());

    tracing::subscriber::set_global_default(sub).context("Failed to set default logger")
}

static SPK_EXE: Lazy<&OsStr> = Lazy::new(|| match std::env::var_os("SPK_BIN_PATH") {
    Some(p) => Box::leak(Box::new(p)),
    None => Box::leak(Box::new(OsString::from("spk"))),
});

/// Return the spk executable to use when launching spk as a subprocess.
pub fn spk_exe() -> &'static OsStr {
    *SPK_EXE
}
