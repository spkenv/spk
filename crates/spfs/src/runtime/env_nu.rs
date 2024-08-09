// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
// Warning Nuhshell version >=0.96

use std::fs;

pub fn source<T>(_tmpdir: Option<&T>) -> String
where
    T: AsRef<str>,
{
    fs::read_to_string("/home/philippe.llerena/workspace/github.com/doubleailes/spk/crates/spfs/src/runtime/env.nu").unwrap()
}