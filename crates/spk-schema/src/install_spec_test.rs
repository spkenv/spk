// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::version::BINARY_STR;
use spk_schema_ident::{parse_ident_range, PkgRequest, Request, RequestedBy};

use crate::{InstallSpec, Package, RequirementsList};

#[rstest]
fn test_render_all_pins_renders_requirements_in_components() {
    let mut install_spec = InstallSpec::default();
    let mut requirements = RequirementsList::default();
    requirements.insert_or_replace({
        Request::Pkg(
            PkgRequest::new(
                parse_ident_range("test").unwrap(),
                RequestedBy::SpkInternalTest,
            )
            .with_pin(Some(BINARY_STR.to_string())),
        )
    });
    install_spec
        .components
        .iter_mut()
        .find(|c| c.name == Component::Run)
        .unwrap()
        .requirements = requirements;

    // Expected value before pinning.
    let Request::Pkg(req) = &install_spec
        .components
        .iter()
        .find(|c| c.name == Component::Run)
        .unwrap()
        .requirements[0]
    else {
        panic!("Expected a Pkg request");
    };
    assert_eq!(req.to_string(), "test");

    install_spec
        .render_all_pins(
            &OptionMap::default(),
            ["test/1.2.3/GMTG3CXY".parse().unwrap()].iter(),
        )
        .unwrap();

    // Now the install requirement inside the run component should be pinned to
    // version 1.2.3.
    let Request::Pkg(req) = &install_spec
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

#[rstest]
fn test_embedded_components_defaults() {
    // By default, embedded components will embed matching components from the
    // defined embedded packages.
    let install = serde_yaml::from_str::<InstallSpec>(
        r#"
embedded:
  - pkg: "embedded/1.0.0"
        "#,
    )
    .unwrap();

    assert_eq!(
        install.components.len(),
        2,
        "expecting two default components: build and run"
    );

    assert_eq!(install.embedded.len(), 1, "expecting one embedded package");

    assert_eq!(
        install.embedded[0].components().len(),
        2,
        "expecting two default components: build and run"
    );

    for component in install.components.iter() {
        assert_eq!(
            component.embedded_components.len(),
            1,
            "expecting each host component to embed one component"
        );
    }
}
