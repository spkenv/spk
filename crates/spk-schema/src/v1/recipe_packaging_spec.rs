// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::{Conditional, TestScript};
use crate::{ComponentSpec, ComponentSpecList, EnvOp, ValidationSpec};

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct RecipePackagingSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<EnvOp>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: ComponentSpecList<super::Package, Conditional<ComponentSpec<super::Package>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,
    #[serde(default, skip_serializing_if = "ValidationSpec::is_default")]
    pub validation: ValidationSpec,

    /// reserved to help avoid common mistakes in production
    #[serde(
        default,
        deserialize_with = "super::source_spec::no_tests_field",
        skip_serializing
    )]
    tests: PhantomData<()>,
}
