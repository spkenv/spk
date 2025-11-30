// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;

use rstest::rstest;
use spk_schema_foundation::ident::{
    BuildIdent,
    PinnableRequest,
    PinnedRequest,
    PkgRequest,
    RequestedBy,
    parse_ident_range,
};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::pkg_name;
use spk_schema_foundation::spec_ops::{HasBuildIdent, HasVersion, Versioned};
use spk_schema_foundation::version::{BINARY_STR, Compat, Version};

use super::EmbeddedRecipeInstallSpec;
use crate::RequirementsList;

#[rstest]
fn test_render_all_pins_renders_requirements_in_components() {
    let mut recipe_install_spec = EmbeddedRecipeInstallSpec::default();
    let mut requirements = RequirementsList::default();
    requirements.insert_or_replace({
        PinnableRequest::Pkg(
            PkgRequest::new(
                parse_ident_range("test").unwrap(),
                RequestedBy::SpkInternalTest,
            )
            .with_pin(Some(BINARY_STR.to_string())),
        )
    });
    recipe_install_spec
        .components
        .iter_mut()
        .find(|c| c.name == Component::Run)
        .unwrap()
        .requirements = requirements;

    // Expected value before pinning.
    let PinnableRequest::Pkg(req) = &recipe_install_spec
        .components
        .iter()
        .find(|c| c.name == Component::Run)
        .unwrap()
        .requirements[0]
    else {
        panic!("Expected a Pkg request");
    };
    assert_eq!(req.to_string(), "test");

    struct FakeBuild {
        compat: Compat,
        build_ident: BuildIdent,
    }

    impl HasBuildIdent for FakeBuild {
        fn build_ident(&self) -> &BuildIdent {
            &self.build_ident
        }
    }

    impl HasVersion for FakeBuild {
        fn version(&self) -> &Version {
            self.build_ident.version()
        }
    }

    impl Versioned for FakeBuild {
        fn compat(&self) -> &Compat {
            &self.compat
        }
    }

    let install_spec = recipe_install_spec
        .render_all_pins(
            &OptionMap::default(),
            &HashMap::from([(
                pkg_name!("test"),
                FakeBuild {
                    compat: Compat::default(),
                    build_ident: "test/1.2.3/GMTG3CXY".parse::<BuildIdent>().unwrap(),
                },
            )]),
        )
        .unwrap();

    // Now the install requirement inside the run component should be pinned to
    // version 1.2.3.
    let PinnedRequest::Pkg(req) = &install_spec
        .components
        .iter()
        .find(|c| c.name == Component::Run)
        .unwrap()
        .requirements[0]
    else {
        panic!("Expected a Pkg request");
    };
    assert_eq!(req.to_string(), "test/Binary:1.2.3");
}
