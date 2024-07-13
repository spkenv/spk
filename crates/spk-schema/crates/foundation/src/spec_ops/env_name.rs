// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use crate::spec_ops::Named;

/// Has a name that can be used as a valid environment variable
pub trait EnvName {
    /// A valid environment variable name for this item
    fn env_name(&self) -> String;
}

impl<T> EnvName for T
where
    T: Named,
{
    fn env_name(&self) -> String {
        self.name().replace('-', "_")
    }
}
