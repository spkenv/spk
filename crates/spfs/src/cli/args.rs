// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;

use sentry::IntoDsn;

pub static SPFS_VERBOSITY: &str = "SPFS_VERBOSITY";

pub fn configure_sentry() {
    let mut opts = sentry::ClientOptions {
        dsn: "http://3dd72e3b4b9a4032947304fabf29966e@sentry.k8s.spimageworks.com/4"
            .into_dsn()
            .unwrap_or(None),
        environment: Some(
            std::env::var("SENTRY_ENVIRONMENT")
                .unwrap_or_else(|_| "production".to_string())
                .into(),
        ),
        // spdev follows sentry recommendation of using the release
        // tag as the name of the release in sentry
        release: Some(format!("v{}", spfs::VERSION).into()),
        ..Default::default()
    };
    opts = sentry::apply_defaults(opts);

    // Proxy values may have been read from env.
    // If they do not contain a scheme prefix, sentry-transport
    // produces a panic log output
    if let Some(url) = opts.http_proxy.as_ref().map(ToString::to_string) {
        if !url.contains("://") {
            opts.http_proxy = Some(format!("http://{}", url)).map(Cow::Owned);
        }
    }
    if let Some(url) = opts.https_proxy.as_ref().map(ToString::to_string) {
        if !url.contains("://") {
            opts.https_proxy = Some(format!("https://{}", url)).map(Cow::Owned);
        }
    }

    let _guard = sentry::init(opts);

    sentry::configure_scope(|scope| {
        let username = whoami::username();
        scope.set_user(Some(sentry::protocol::User {
            email: Some(format!("{}@imageworks.com", &username)),
            username: Some(username),
            ..Default::default()
        }))
    })
}

pub fn configure_spops(_verbosity: usize) {
    // TODO: have something for this
    // try:
    //     spops.configure(
    //         {
    //             "statsd": {"host": "statsd.k8s.spimageworks.com", "port": 30111},
    //             "labels": {
    //                 "environment": os.getenv("SENTRY_ENVIRONMENT", "production"),
    //                 "user": getpass.getuser(),
    //                 "host": socket.gethostname(),
    //             },
    //         },
    //     )
    // except Exception as e:
    //     print(f"failed to initialize spops: {e}", file=sys.stderr)
}

pub fn configure_logging(mut verbosity: usize) {
    if verbosity == 0 {
        let parse_result = std::env::var(SPFS_VERBOSITY)
            .unwrap_or_else(|_| "0".to_string())
            .parse::<usize>();
        if let Ok(parsed) = parse_result {
            verbosity = usize::max(parsed, verbosity);
        }
    }
    std::env::set_var(SPFS_VERBOSITY, verbosity.to_string());
    use tracing_subscriber::layer::SubscriberExt;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "spfs=trace");
    }
    let env_filter = tracing_subscriber::filter::EnvFilter::from_default_env();
    let level_filter = match verbosity {
        0 => tracing_subscriber::filter::LevelFilter::INFO,
        1 => tracing_subscriber::filter::LevelFilter::DEBUG,
        _ => tracing_subscriber::filter::LevelFilter::TRACE,
    };
    let registry = tracing_subscriber::Registry::default()
        .with(env_filter)
        .with(level_filter);
    let mut fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .without_time();
    if verbosity < 3 {
        fmt_layer = fmt_layer.with_target(false);
    }
    let sub = registry.with(fmt_layer);
    tracing::subscriber::set_global_default(sub).unwrap();
}

#[macro_export]
macro_rules! main {
    ($cmd:ident) => {
        main!($cmd, sentry = true, sync = false);
    };
    ($cmd:ident, sentry = $sentry:literal, sync = true) => {
        fn main() {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2())
        }
        fn main2() -> i32 {
            let mut opt = $cmd::from_args();
            let config = configure!(opt, $sentry);

            let result = opt.run(&config);

            handle_result!(result)
        }
    };
    ($cmd:ident, sentry = $sentry:literal, sync = false) => {
        fn main() {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2())
        }
        fn main2() -> i32 {
            let mut opt = $cmd::from_args();
            let config = configure!(opt, $sentry);

            let rt = match tokio::runtime::Runtime::new() {
                Err(err) => {
                    tracing::error!("Failed to establish runtime: {:?}", err);
                    return 1;
                }
                Ok(rt) => rt,
            };
            let result = rt.block_on(opt.run(&config));

            handle_result!(result)
        }
    };
}

macro_rules! configure {
    ($opt:ident, $sentry:literal) => {{
        // sentry makes this process multithreaded, and must be disabled
        // for commands that use system calls which are bothered by this
        if $sentry { args::configure_sentry() }
        args::configure_logging($opt.verbose);
        args::configure_spops($opt.verbose);

        match spfs::load_config() {
            Err(err) => {
                tracing::error!(err = ?err, "failed to load config");
                return 1;
            }
            Ok(config) => config,
        }
    }};
}

macro_rules! handle_result {
    ($result:ident) => {
        match $result {
            Err(err) => {
                args::capture_if_relevant(&err);
                tracing::error!("{}", err);
                1
            }
            Ok(code) => code,
        }
    };
}

pub fn capture_if_relevant(err: &spfs::Error) {
    match err {
        spfs::Error::NoActiveRuntime => (),
        spfs::Error::UnknownObject(_) => (),
        spfs::Error::UnknownReference(_) => (),
        spfs::Error::AmbiguousReference(_) => (),
        spfs::Error::NothingToCommit => (),
        _ => {
            sentry::capture_error(err);
        }
    }
}
