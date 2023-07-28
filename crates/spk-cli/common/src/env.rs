// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::{OsStr, OsString};
#[cfg(feature = "sentry")]
use std::panic::catch_unwind;
use std::sync::Arc;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy};
use spk_schema::{Package, VersionIdent};
use spk_solve::package_iterator::BUILD_SORT_TARGET;
use spk_solve::solution::{PackageSource, Solution};
use spk_solve::validation::IMPOSSIBLE_CHECKS_TARGET;
use spk_storage as storage;
use tracing_subscriber::prelude::*;

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
    let mut packages_in_runtime_f = Vec::new();
    for name in repo.list_packages().await? {
        let repo = Arc::clone(&repo);
        packages_in_runtime_f.push(tokio::spawn(async move {
            let mut specs = Vec::new();
            for version in repo.list_package_versions(&name).await?.iter() {
                let pkg = VersionIdent::new(name.clone(), (**version).clone());
                for pkg in repo.list_package_builds(&pkg).await? {
                    let spec = repo.read_package(&pkg).await?;
                    specs.push(spec);
                }
            }
            Ok::<_, Error>(specs)
        }));
    }

    let mut solution = Solution::default();

    // Transform `FuturesUnordered<JoinHandle<impl Future<Output = Result<Vec<Arc<Spec>>>>>>`
    // into flattened `Vec<Arc<Spec>>`.
    let mut packages_in_runtime = Vec::new();
    for package_f in packages_in_runtime_f {
        let specs = package_f
            .await
            .map_err(|err| Error::String(format!("Tokio join error: {err}")))??;
        packages_in_runtime.extend(specs.into_iter());
    }

    let package_idents_in_runtime = packages_in_runtime
        .iter()
        .map(|spec| spec.ident())
        .collect::<Vec<_>>();

    let components_in_runtime = match &*repo {
        spk_solve::RepositoryHandle::Runtime(runtime) => {
            runtime
                .read_components_bulk(&package_idents_in_runtime)
                .await?
        }
        _ => unreachable!(),
    };

    // The results that come out of `read_components_bulk` are in the order of
    // the argument elements, so here we iterate over them and walk the
    // packages again in that same order to process the results.

    debug_assert_eq!(
        components_in_runtime.len(),
        packages_in_runtime.len(),
        "return value from read_components_bulk expected to match input length"
    );

    for (spec, components) in packages_in_runtime
        .into_iter()
        .zip(components_in_runtime.into_iter())
    {
        let range_ident = RangeIdent::equals(&spec.ident().to_any(), components.keys().cloned());
        let mut request = PkgRequest::new(range_ident, RequestedBy::CurrentEnvironment);
        request.prerelease_policy = PreReleasePolicy::IncludeAll;
        let repo = repo.clone();
        solution.add(
            request,
            spec,
            PackageSource::Repository { repo, components },
        );
    }

    Ok(solution)
}

#[cfg(feature = "sentry")]
pub fn configure_sentry() -> Option<sentry::ClientInitGuard> {
    // Call this before `sentry::init` to avoid potential `SIGSEGV`.
    let username = get_username_for_sentry();

    // When using the sentry feature it is expected that the DSN
    // and other configuration is provided at *compile* time.
    let guard = match catch_unwind(|| {
        sentry::init((
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
        ))
    }) {
        Ok(g) => g,
        Err(cause) => {
            // Added to try to get more info on this kind of panic:
            //
            // thread 'main' panicked at 'called `Result::unwrap()` on
            // an `Err` value: Os { code: 11, kind: WouldBlock,
            // message: "Resource temporarily unavailable" }',
            // /.../sentry-core-0.27.0/src/session.rs:228:14
            //
            // See also, maybe?: https://github.com/rust-lang/rust/issues/46345
            eprintln!("WARNING: configuring Sentry for spk failed: {:?}", cause);
            return None;
        }
    };

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

    Some(guard)
}

#[cfg(feature = "sentry")]
fn get_username_for_sentry() -> String {
    // If this is being run from an automated process run by a
    // non-human user, e.g. gitlab CI job, then use the configured env
    // var name to get the username of the person that triggered the
    // job. Otherwise get the username of the person who ran this spk
    // instance.
    let username_override_var = option_env!("SENTRY_USERNAME_OVERRIDE_VAR");
    username_override_var
        .map(std::env::var)
        .and_then(Result::ok)
        .unwrap_or_else(|| {
            // Call this before `sentry::init` to avoid potential `SIGSEGV`.
            whoami::username()
        })
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

pub fn configure_logging(verbosity: u8) -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    // NOTE: If you change these, please update docs/ref/logging.md
    let mut directives = match verbosity {
        // Sets "error" level as the global default level
        0 => "error,spk=info,spfs=warn".to_string(),
        1 => "error,spk=debug,spfs=info".to_string(),
        2 => "error,spk=trace,spfs=debug".to_string(),
        _ => "error,spk=trace,spfs=trace".to_string(),
    };

    // Ensure all more detailed tracing targets are turned off. They
    // have to be set explicitly because otherwise they will match the
    // 'spk' target in the directives above. They can be re-enabled by
    // setting them in "SPK_LOG" as needed, e.g.
    // env SPK_LOG="spk_solve::impossible_checks=debug" spk explain ...
    let tracing_targets = vec![BUILD_SORT_TARGET, IMPOSSIBLE_CHECKS_TARGET];
    let defaults = tracing_targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>()
        .join("=error,");
    directives = format!("{directives},{defaults}=error");

    if let Ok(overrides) = std::env::var("SPK_LOG") {
        // this is a common scenario because spk often calls itself
        if directives != overrides {
            directives = format!("{directives},{overrides}");
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
    let stderr_log = tracing_subscriber::fmt::layer()
        .with_target(verbosity > 2)
        .with_writer(std::io::stderr);
    let stderr_log = if std::env::var("SPK_LOG_ENABLE_TIMESTAMP").is_ok() {
        stderr_log.boxed()
    } else {
        stderr_log.without_time().boxed()
    };

    #[cfg(not(feature = "sentry"))]
    let sub = tracing_subscriber::registry().with(stderr_log.with_filter(env_filter).with_filter(
        tracing_subscriber::filter::filter_fn(|metadata| {
            // Don't log breadcrumbs to console, etc.
            !metadata.target().starts_with("sentry")
        }),
    ));

    #[cfg(feature = "sentry")]
    let sub = {
        let sentry_layer =
            sentry_tracing::layer().with_filter(tracing_subscriber::filter::LevelFilter::INFO);

        tracing_subscriber::registry()
            .with(
                stderr_log
                    .and_then(sentry_tracing::layer())
                    .with_filter(env_filter)
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        // Don't log breadcrumbs to console, etc.
                        !metadata.target().starts_with("sentry")
                    })),
            )
            .with(
                sentry_layer.with_filter(tracing_subscriber::filter::filter_fn(
                    // Only log breadcrumbs here.
                    |metadata| metadata.target().starts_with("sentry"),
                )),
            )
    };

    tracing::subscriber::set_global_default(sub).context("Failed to set default logger")
}

static SPK_EXE: Lazy<&OsStr> = Lazy::new(|| match std::env::var_os("SPK_BIN_PATH") {
    Some(p) => Box::leak(Box::new(p)),
    None => Box::leak(Box::new(OsString::from("spk"))),
});

/// Return the spk executable to use when launching spk as a subprocess.
pub fn spk_exe() -> &'static OsStr {
    &SPK_EXE
}
