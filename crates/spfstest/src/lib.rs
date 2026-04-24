// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::atomic::{AtomicBool, Ordering};

pub use spfstest_macro::spfstest;

static REEXEC_TRIGGERED: AtomicBool = AtomicBool::new(false);

/// Build the full test path from `module_path!()` and the function name.
///
/// `module_path!()` includes the crate name as the first segment, but the
/// test harness uses paths without it.
pub fn current_test_name(module_path: &str, fn_name: &str) -> String {
    let without_crate = match module_path.find("::") {
        Some(pos) => &module_path[pos + 2..],
        None => "",
    };
    if without_crate.is_empty() {
        fn_name.to_string()
    } else {
        format!("{without_crate}::{fn_name}")
    }
}

/// Re-exec the test binary inside `spfs run - --` if not already in a runtime.
///
/// Intended for synchronous test functions. Panics if the subprocess fails.
pub fn maybe_reexec_in_spfs(test_path: &str) {
    if std::env::var("SPFS_RUNTIME").is_ok() {
        return;
    }
    if REEXEC_TRIGGERED.swap(true, Ordering::SeqCst) {
        return;
    }
    if std::env::var("__SPFSTEST_DEPTH").is_ok() {
        panic!(
            "spfstest: re-exec'd into spfs but $SPFS_RUNTIME is not set. \
             Is spfs installed and working?"
        );
    }

    let exe = std::env::current_exe().expect("spfstest: failed to get current executable path");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let status = std::process::Command::new("spfs")
        .args(["run", "-", "--"])
        .arg(&exe)
        .args(&args)
        .env("__SPFSTEST_DEPTH", "1")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .expect("spfstest: failed to spawn 'spfs run' -- is spfs installed and on PATH?");

    assert!(
        status.success(),
        "spfstest: test '{}' failed inside spfs runtime (exit code: {:?})",
        test_path,
        status.code()
    );
    std::process::exit(0);
}

/// Async version of [`maybe_reexec_in_spfs`] for `#[tokio::test]` functions.
pub async fn maybe_reexec_in_spfs_async(test_path: &str) {
    if std::env::var("SPFS_RUNTIME").is_ok() {
        return;
    }
    if REEXEC_TRIGGERED.swap(true, Ordering::SeqCst) {
        return;
    }
    if std::env::var("__SPFSTEST_DEPTH").is_ok() {
        panic!(
            "spfstest: re-exec'd into spfs but $SPFS_RUNTIME is not set. \
             Is spfs installed and working?"
        );
    }

    let exe = std::env::current_exe().expect("spfstest: failed to get current executable path");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let status = tokio::process::Command::new("spfs")
        .args(["run", "-", "--"])
        .arg(&exe)
        .args(&args)
        .env("__SPFSTEST_DEPTH", "1")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .expect("spfstest: failed to spawn 'spfs run' -- is spfs installed and on PATH?");

    assert!(
        status.success(),
        "spfstest: test '{}' failed inside spfs runtime (exit code: {:?})",
        test_path,
        status.code()
    );
    std::process::exit(0);
}
