// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::os::unix::process::ExitStatusExt;
use std::process::Command;

use futures::StreamExt;

use super::*;
use crate::{DecisionFormatter, DecisionFormatterBuilder, MultiSolverKind, StepSolver};

/// Isolate process-wide signal handling from the rest of the test runner.
#[test]
fn interrupt_lifecycle() {
    const CASE_ENV: &str = "SPK_TEST_SOLVE_INTERRUPT_CASE";
    if let Ok(case) = std::env::var(CASE_ENV) {
        let first = SolveInterruptGuard::new().unwrap();
        assert!(!first.cancelled());
        signal_hook::low_level::raise(SIGINT).unwrap();
        assert!(first.cancelled());
        let second = SolveInterruptGuard::new().unwrap();
        drop(first);
        signal_hook::low_level::raise(SIGINT).unwrap();
        assert!(second.cancelled());
        drop(second);
        match case.as_str() {
            "finished" => signal_hook::low_level::raise(SIGINT).unwrap(),
            "reset" => {
                let next = SolveInterruptGuard::new().unwrap();
                assert!(!next.cancelled());
                signal_hook::low_level::raise(SIGINT).unwrap();
                assert!(next.cancelled());
            }
            _ => panic!("unexpected interrupt test case: {case}"),
        }
        return;
    }

    for case in ["finished", "reset"] {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "interrupt::interrupt_test::interrupt_lifecycle",
                "--nocapture",
            ])
            .env(CASE_ENV, case)
            .status()
            .unwrap();
        if case == "finished" {
            assert_eq!(status.signal(), Some(SIGINT));
        } else {
            assert!(status.success());
        }
    }
}

/// Completed solver entry points must not swallow a later build interrupt.
#[test]
fn interrupt_after_solve() {
    const CASE_ENV: &str = "SPK_TEST_FINISHED_SOLVE";
    if let Ok(case) = std::env::var(CASE_ENV) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let solver = StepSolver::default();
            if case == "direct" {
                DecisionFormatter::new_testing()
                    .run_and_print_decisions(&mut solver.run())
                    .await
                    .unwrap();
            } else if case == "interrupted" {
                let decisions = futures::stream::iter([Err(Error::String("test decision".into()))]);
                let mut formatted =
                    DecisionFormatter::new_testing().formatted_decisions_iter(decisions);
                let iter = formatted.iter();
                tokio::pin!(iter);
                assert!(matches!(iter.next().await, Some(Err(Error::String(_)))));
                signal_hook::low_level::raise(SIGINT).unwrap();
                assert!(matches!(
                    iter.next().await,
                    Some(Err(Error::SolverInterrupted(_)))
                ));
            } else {
                let kind = match case.as_str() {
                    "cli" => MultiSolverKind::Unchanged,
                    "checks" => MultiSolverKind::AllImpossibleChecks,
                    "all" => MultiSolverKind::All,
                    _ => panic!("unexpected solver test case: {case}"),
                };
                DecisionFormatterBuilder::default()
                    .with_solver_to_run(kind)
                    .with_status_bar(false)
                    .build()
                    .run_and_print_resolve(&solver)
                    .await
                    .unwrap();
            }
            signal_hook::low_level::raise(SIGINT).unwrap();
            panic!("SIGINT did not terminate after solving");
        });
        return;
    }

    for case in ["cli", "checks", "all", "direct", "interrupted"] {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "interrupt::interrupt_test::interrupt_after_solve",
                "--nocapture",
            ])
            .env(CASE_ENV, case)
            .status()
            .unwrap();
        assert_eq!(status.signal(), Some(SIGINT), "solver: {case}");
    }
}
