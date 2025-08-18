// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=schema/spfs.fbs");
    println!("cargo:rerun-if-changed=ast-grep/rules/replace-digest-bytes-debug.yaml");

    let cmd = match std::env::var_os("FLATC") {
        Some(exe) => flatc_rust::Flatc::from_path(exe),
        None => flatc_rust::Flatc::from_env_path(),
    };

    let out_dir = env::var("OUT_DIR").unwrap();

    cmd.run(flatc_rust::Args {
        lang: "rust",
        inputs: &[Path::new("schema/spfs.fbs")],
        out_dir: &PathBuf::from(&out_dir),
        ..Default::default()
    })
    .expect("schema compiler command");

    let generated_file = PathBuf::from(out_dir).join("spfs_generated.rs");
    let ast_grep_cmd = std::process::Command::new("ast-grep")
        .arg("scan")
        .arg("--rule")
        .arg("ast-grep/rules/replace-digest-bytes-debug.yaml")
        .arg("--update-all")
        .arg(generated_file)
        .status()
        .expect("Failed to run ast-grep command");
    assert!(
        ast_grep_cmd.success(),
        "ast-grep command failed with status: {ast_grep_cmd}",
    );
}
