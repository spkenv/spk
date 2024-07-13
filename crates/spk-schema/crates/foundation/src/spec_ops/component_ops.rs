// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use super::FileMatcher;

pub trait ComponentOps {
    fn files(&self) -> &FileMatcher;
}
