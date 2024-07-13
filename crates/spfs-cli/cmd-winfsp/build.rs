// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

fn main() {
    #[cfg(windows)]
    winfsp::build::winfsp_link_delayload();
}
