// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

fn main() -> miette::Result<()> {
    std::process::exit(cmd_fuse_macos::main()?)
}

mod cmd_fuse_macos;
