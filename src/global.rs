// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{convert::TryInto, sync::Arc};

use crate::{api, storage, Error, Result};

#[cfg(test)]
#[path = "./global_test.rs"]
mod global_test;
