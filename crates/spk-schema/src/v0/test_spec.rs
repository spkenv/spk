// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::{RequestedBy, VersionIdent};

use crate::ident::Request;
use crate::{Script, TestStage};

#[cfg(test)]
#[path = "./test_spec_test.rs"]
mod test_spec_test;

/// A set of structured inputs used to build a package.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct TestSpec {
    pub stage: TestStage,
    pub script: Script,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<super::VariantSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Request>,
}

impl TestSpec {
    /// Add the given requester to any package requirements present in this
    /// test spec.
    pub fn add_requester(&mut self, requester: &VersionIdent) {
        for requirement in self.requirements.iter_mut() {
            if let Request::Pkg(pkg_request) = requirement {
                match self.stage {
                    TestStage::Sources => pkg_request
                        .add_requester(RequestedBy::SourceTest(requester.to_any_ident(None))),
                    TestStage::Build => pkg_request
                        .add_requester(RequestedBy::BuildTest(requester.to_any_ident(None))),
                    TestStage::Install => {
                        pkg_request.add_requester(RequestedBy::InstallTest(requester.clone()))
                    }
                }
            }
        }
    }
}

impl crate::Test for TestSpec {
    fn script(&self) -> String {
        self.script.join("\n")
    }

    fn additional_requirements(&self) -> Vec<Request> {
        self.requirements.clone()
    }
}
