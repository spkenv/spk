// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

pub trait IsDefault {
    /// Returns true if the value equivalent to the default value.
    fn is_default(&self) -> bool;
}
