// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::process::ExitStatus;
use std::sync::{Condvar, Mutex, OnceLock};

#[cfg(test)]
extern crate self as spfstest;

pub use spfstest_macro::spfstest;

static REEXEC_COORDINATOR: OnceLock<ReexecCoordinator> = OnceLock::new();

#[derive(Default)]
struct ReexecCoordinator {
    state: Mutex<ReexecState>,
    ready: Condvar,
}

#[derive(Default)]
enum ReexecState {
    #[default]
    Idle,
    Running,
    LaunchFailed(String),
}

enum ReexecResult {
    Exit(i32),
    LaunchFailed(String),
}

impl ReexecCoordinator {
    fn begin(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        loop {
            match &*state {
                ReexecState::Idle => {
                    *state = ReexecState::Running;
                    return true;
                }
                ReexecState::Running => {
                    state = self.ready.wait(state).unwrap();
                }
                ReexecState::LaunchFailed(message) => {
                    let message = message.clone();
                    drop(state);
                    panic!("{message}");
                }
            }
        }
    }

    fn fail_launch(&self, message: String) {
        let mut state = self.state.lock().unwrap();
        *state = ReexecState::LaunchFailed(message);
        self.ready.notify_all();
    }
}

fn reexec_coordinator() -> &'static ReexecCoordinator {
    REEXEC_COORDINATOR.get_or_init(ReexecCoordinator::default)
}

fn should_reexec_in_spfs() -> bool {
    if std::env::var("SPFS_RUNTIME").is_ok() {
        return false;
    }
    if std::env::var("__SPFSTEST_DEPTH").is_ok() {
        panic!(
            "spfstest: re-exec'd into spfs but $SPFS_RUNTIME is not set. \
             Is spfs installed and working?"
        );
    }

    reexec_coordinator().begin()
}

fn fail_reexec_launch(message: String) {
    reexec_coordinator().fail_launch(message);
}

fn reexec_result(test_path: &str, result: std::io::Result<ExitStatus>) -> ReexecResult {
    match result {
        Ok(status) => ReexecResult::Exit(status.code().unwrap_or(1)),
        Err(err) => ReexecResult::LaunchFailed(format!(
            "spfstest: failed to spawn 'spfs run' for test '{test_path}': {err}"
        )),
    }
}

fn finish_reexec(test_path: &str, result: std::io::Result<ExitStatus>) -> ! {
    match reexec_result(test_path, result) {
        ReexecResult::Exit(code) => std::process::exit(code),
        ReexecResult::LaunchFailed(message) => {
            fail_reexec_launch(message.clone());
            panic!("{message}");
        }
    }
}

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
    if !should_reexec_in_spfs() {
        return;
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
        .status();
    finish_reexec(test_path, status);
}

/// Async version of [`maybe_reexec_in_spfs`] for `#[tokio::test]` functions.
pub async fn maybe_reexec_in_spfs_async(test_path: &str) {
    if !should_reexec_in_spfs() {
        return;
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
        .await;
    finish_reexec(test_path, status);
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};
    use std::process::ExitStatus;
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    use super::{ReexecCoordinator, ReexecResult};
    use crate::spfstest;

    #[cfg(unix)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;

        ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;

        ExitStatus::from_raw(code as u32)
    }

    #[test]
    fn begin_returns_start_for_first_caller() {
        let coordinator = ReexecCoordinator::default();
        assert!(coordinator.begin());
    }

    #[test]
    fn waiting_callers_receive_reexec_launch_failure() {
        let coordinator = Arc::new(ReexecCoordinator::default());
        assert!(coordinator.begin());

        let (started_tx, started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let waiting = Arc::clone(&coordinator);

        std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                waiting.begin();
            }));
            result_tx.send(result).unwrap();
        });

        started_rx.recv().unwrap();
        assert!(result_rx.recv_timeout(Duration::from_millis(50)).is_err());

        let message = "spfstest: failed to spawn 'spfs run'".to_string();
        coordinator.fail_launch(message.clone());

        let err = result_rx.recv().unwrap().unwrap_err();
        let panic_message = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(ToString::to_string))
            .unwrap();
        assert_eq!(panic_message, message);

        let err = panic::catch_unwind(AssertUnwindSafe(|| coordinator.begin())).unwrap_err();
        let panic_message = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(ToString::to_string))
            .unwrap();
        assert_eq!(panic_message, message);
    }

    #[test]
    fn reexec_result_exits_with_inner_test_status() {
        let result = super::reexec_result("some::test", Ok(exit_status(23)));
        assert!(matches!(result, ReexecResult::Exit(23)));
    }

    #[test]
    fn reexec_result_reports_launch_failure() {
        let result = super::reexec_result(
            "some::test",
            Err(io::Error::new(io::ErrorKind::NotFound, "missing spfs")),
        );

        let ReexecResult::LaunchFailed(message) = result else {
            panic!("expected launch failure");
        };
        assert!(message.contains("some::test"));
        assert!(message.contains("missing spfs"));
    }

    fn regression_dir() -> std::path::PathBuf {
        std::env::var_os("SPFSTEST_REGRESSION_DIR")
            .map(std::path::PathBuf::from)
            .expect("SPFSTEST_REGRESSION_DIR must be set for regression helper cases")
    }

    fn write_regression_marker(case_name: &str) {
        let dir = regression_dir();
        fs::write(dir.join(case_name), case_name).unwrap();
    }

    fn run_regression_case(case_name: &str, should_fail: bool) {
        write_regression_marker(case_name);
        println!("spfstest regression marker: {case_name}");
        assert!(
            !should_fail,
            "intentional regression failure for {case_name}"
        );
    }

    #[test]
    fn multiple_spfstest_cases_exit_nonzero_when_inner_tests_fail() {
        let dir = std::env::temp_dir().join(format!(
            "spfstest-regression-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "spfstest_regression_case_",
                "--nocapture",
                "--test-threads=6",
            ])
            .env("SPFSTEST_REGRESSION_DIR", &dir)
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        assert!(
            !output.status.success(),
            "subprocess reported failed tests with a successful status {:?}\n{combined}",
            output.status,
        );

        for case_name in [
            "spfstest_regression_case_pass_alpha",
            "spfstest_regression_case_pass_bravo",
            "spfstest_regression_case_pass_charlie",
            "spfstest_regression_case_pass_delta",
            "spfstest_regression_case_fail_echo",
            "spfstest_regression_case_fail_foxtrot",
        ] {
            assert!(
                combined.contains(&format!("spfstest regression marker: {case_name}")),
                "missing marker output for {case_name}\n{combined}"
            );
        }

        for failed in ["echo", "foxtrot"] {
            let case_name = format!("spfstest_regression_case_fail_{failed}");
            assert!(
                combined.contains(&format!("    tests::{case_name}")),
                "missing failing case entry for {case_name}\n{combined}"
            );
        }

        assert!(
            combined.contains("test result: FAILED. 4 passed; 2 failed; 0 ignored;"),
            "missing expected summary counts\n{combined}"
        );

        let mut entries = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        entries.sort();
        assert_eq!(
            entries,
            vec![
                "spfstest_regression_case_fail_echo".to_string(),
                "spfstest_regression_case_fail_foxtrot".to_string(),
                "spfstest_regression_case_pass_alpha".to_string(),
                "spfstest_regression_case_pass_bravo".to_string(),
                "spfstest_regression_case_pass_charlie".to_string(),
                "spfstest_regression_case_pass_delta".to_string(),
            ]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[spfstest]
    #[test]
    #[ignore = "helper case for subprocess regression coverage"]
    fn spfstest_regression_case_pass_alpha() {
        run_regression_case("spfstest_regression_case_pass_alpha", false);
    }

    #[spfstest]
    #[test]
    #[ignore = "helper case for subprocess regression coverage"]
    fn spfstest_regression_case_pass_bravo() {
        run_regression_case("spfstest_regression_case_pass_bravo", false);
    }

    #[spfstest]
    #[test]
    #[ignore = "helper case for subprocess regression coverage"]
    fn spfstest_regression_case_pass_charlie() {
        run_regression_case("spfstest_regression_case_pass_charlie", false);
    }

    #[spfstest]
    #[test]
    #[ignore = "helper case for subprocess regression coverage"]
    fn spfstest_regression_case_pass_delta() {
        run_regression_case("spfstest_regression_case_pass_delta", false);
    }

    #[spfstest]
    #[test]
    #[ignore = "helper case for subprocess regression coverage"]
    fn spfstest_regression_case_fail_echo() {
        run_regression_case("spfstest_regression_case_fail_echo", true);
    }

    #[spfstest]
    #[test]
    #[ignore = "helper case for subprocess regression coverage"]
    fn spfstest_regression_case_fail_foxtrot() {
        run_regression_case("spfstest_regression_case_fail_foxtrot", true);
    }
}
