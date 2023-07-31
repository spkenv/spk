// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::EnvOp;

#[rstest]
#[case("{comment: This is a test}")]
#[case("{append: SPK_TEST_VAR, value: simple}")]
#[case("{prepend: SPK_TEST_VAR, value: simple}")]
#[case("{set: SPK_TEST_VAR, value: simple}")]
fn test_valid_bash(#[case] op: &str) {
    let op: EnvOp = serde_yaml::from_str(op).unwrap();
    println!("source:\n{}", op.tcsh_source());

    let mut bash = std::process::Command::new("bash");
    bash.arg("--norc");
    bash.arg("-xe"); // echo commands, fail on error
    bash.arg("-c");
    bash.arg(op.bash_source());
    bash.stdin(std::process::Stdio::piped());
    bash.stderr(std::process::Stdio::piped());
    bash.stdout(std::process::Stdio::piped());
    let out = bash.output().unwrap();
    println!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(out.stdout.as_slice()),
        String::from_utf8_lossy(out.stderr.as_slice())
    );
    assert!(out.status.success(), "failed to execute bash source");
}

#[rstest]
#[case("{comment: This is a test}")]
#[case("{append: SPK_TEST_VAR, value: simple}")]
#[case("{prepend: SPK_TEST_VAR, value: simple}")]
#[case("{set: SPK_TEST_VAR, value: simple}")]
fn test_valid_tcsh(#[case] op: &str) {
    let op: EnvOp = serde_yaml::from_str(op).unwrap();
    println!("source:\n{}", op.tcsh_source());

    let mut tcsh = std::process::Command::new("tcsh");
    tcsh.arg("-xef"); // echo commands, fail on error, skip startup
    tcsh.arg("-c");
    tcsh.arg(op.tcsh_source());
    tcsh.stdin(std::process::Stdio::piped());
    tcsh.stderr(std::process::Stdio::piped());
    tcsh.stdout(std::process::Stdio::piped());
    let out = tcsh.output().unwrap();
    println!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(out.stdout.as_slice()),
        String::from_utf8_lossy(out.stderr.as_slice())
    );
    assert!(out.status.success(), "failed to execute tcsh source");
}

#[rstest]
#[case("{append: SPK_TEST_VAR, value: simple}")]
#[case("{prepend: SPK_TEST_VAR, value: simple}")]
#[case("{set: SPK_TEST_VAR, value: simple}")]
#[case("{append: SPK_TEST_VAR, value: simple, separator: '+'}")]
fn test_yaml_round_trip(#[case] op: &str) {
    let op: EnvOp = serde_yaml::from_str(op).unwrap();
    let yaml = serde_yaml::to_string(&op).unwrap();
    let op2: EnvOp = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(op2, op, "should be the same after sending through yaml");
}

#[rstest]
#[case("{set: SPK_TEST_VAR, value: '${PWD}'}")]
#[case("{set: SPK_TEST_VAR2, value: '$${PWD}}'}")]
fn test_variable_substitution(#[case] op: &str) {
    let op: EnvOp = serde_yaml::from_str(op).unwrap();
    println!("source:\n{}", op.tcsh_source());

    let mut tcsh = std::process::Command::new("tcsh");
    tcsh.arg("-xef"); // echo commands, fail on error, skip startup
    tcsh.arg("-c");
    tcsh.arg(op.tcsh_source());
    tcsh.stdin(std::process::Stdio::piped());
    tcsh.stderr(std::process::Stdio::piped());
    tcsh.stdout(std::process::Stdio::piped());

    let out = tcsh.output().unwrap();
    println!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(out.stdout.as_slice()),
        String::from_utf8_lossy(out.stderr.as_slice())
    );
    assert!(out.status.success(), "failed to execute tcsh source");
}
