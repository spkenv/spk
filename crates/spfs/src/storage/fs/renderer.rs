// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg_attr(unix, path = "./renderer_unix.rs")]
#[cfg_attr(windows, path = "./renderer_win.rs")]
mod os;

pub use os::*;
