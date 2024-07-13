// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=schema/spfs.fbs");

    let cmd = match std::env::var_os("FLATC") {
        Some(exe) => flatc_rust::Flatc::from_path(exe),
        None => flatc_rust::Flatc::from_env_path(),
    };

    let out_dir = env::var("OUT_DIR").unwrap();

    cmd.run(flatc_rust::Args {
        lang: "rust",
        inputs: &[Path::new("schema/spfs.fbs")],
        out_dir: &PathBuf::from(out_dir),
        ..Default::default()
    })
    .expect("schema compiler command");
}
